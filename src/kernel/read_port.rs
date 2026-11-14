//! Owned SH7604 read transport for the continuation-kernel integration.
//!
//! This slice covers canonical WRAM and its cache-through aliases. Transfers
//! carry the real cache-fill order and grouping; the board must grant/complete
//! them using its BSC and shared-bus timeline. This module invents no latency.
use super::Error;
use crate::bus::{Bus, BusError, HIGH_WRAM, LOW_WRAM, Processor, WRAM_SIZE};
use crate::cache::Cache;

mod fixture;
#[cfg(test)]
mod tests;
pub use fixture::verify_board;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read {
    Fetch16(u32),
    Data { address: u32, width: u32 },
}
impl Read {
    fn parts(self) -> Result<(u32, usize, bool), Error> {
        let (address, width, instruction) = match self {
            Self::Fetch16(address) => (address, 2, true),
            Self::Data { address, width } => (address, width, false),
        };
        let physical = address & 0x1FFF_FFFF;
        if !matches!(width, 1 | 2 | 4)
            || !address.is_multiple_of(width)
            || address >= 0x4000_0000
            || ![LOW_WRAM, HIGH_WRAM]
                .into_iter()
                .any(|base| (base..base + WRAM_SIZE as u32).contains(&physical))
        {
            return Err(Error::Address { address, width });
        }
        Ok(if instruction {
            (address & !3, 4, true)
        } else {
            (address, width as usize, false)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadTicket {
    owner: Processor,
    sequence: u64,
}
impl ReadTicket {
    pub fn owner(self) -> Processor {
        self.owner
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadTransfer {
    pub ticket: ReadTicket,
    pub address: u32,
    pub width: usize,
    pub instruction: bool,
    pub first: bool,
    pub last: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadPoll {
    Ready(u32),
    Pending,
    /// A reset or an already consumed completion invalidates delivery only.
    Discarded,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReadError {
    State(Error),
    Bus(Box<BusError>),
}

struct Fill {
    bytes: [u8; 16],
    beat: usize,
}
struct Pending {
    operation: Read,
    address: u32,
    width: usize,
    instruction: bool,
    ticket: ReadTicket,
    fill: Option<Fill>,
}

/// Owns a CPU's physical cache, retained IF halfword and one read continuation.
/// There is deliberately no Clone implementation or instruction replay log.
pub struct ReadPort {
    owner: Processor,
    cache: Cache,
    retained: Option<(u32, u16)>,
    sequence: u64,
    pending: Option<Pending>,
    outgoing: Option<ReadTransfer>,
}
impl ReadPort {
    pub fn new(owner: Processor) -> Self {
        Self {
            owner,
            cache: Cache::default(),
            retained: None,
            sequence: 0,
            pending: None,
            outgoing: None,
        }
    }

    pub fn set_control(&mut self, value: u8) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Protocol);
        }
        self.cache.set_control(value);
        Ok(())
    }

    pub fn discard_prefetch(&mut self) {
        self.retained = None;
    }

    pub fn reset(&mut self) {
        self.pending = None;
        self.outgoing = None;
        self.retained = None;
        self.cache.reset_control();
        // Already granted physical work belongs to the board. Its old ticket
        // cannot deliver into a later request, whose sequence is never reused.
    }

    pub fn request(&mut self, operation: Read) -> Result<ReadPoll, Error> {
        if self.pending.is_some() {
            return Err(Error::Protocol);
        }
        let (address, width, instruction) = operation.parts()?;
        if let Read::Fetch16(address) = operation
            && self.retained.is_some_and(|(at, _)| at == address)
        {
            return Ok(ReadPoll::Ready(u32::from(self.retained.take().unwrap().1)));
        }
        if self.cache.read_hit(address) {
            let value = self.cached_value(address, width, instruction, None);
            return Ok(ReadPoll::Ready(self.finish(operation, value)));
        }
        let allocate =
            address < 0x2000_0000 && self.cache.control() & (if instruction { 3 } else { 5 }) == 1;
        self.queue(Pending {
            operation,
            address,
            width,
            instruction,
            ticket: ReadTicket {
                owner: self.owner,
                sequence: 0,
            },
            fill: allocate.then_some(Fill {
                bytes: [0; 16],
                beat: 0,
            }),
        })?;
        Ok(ReadPoll::Pending)
    }

    pub fn take_transfer(&mut self) -> Option<ReadTransfer> {
        self.outgoing.take()
    }

    /// Resume this unit on the production board's real arbiter and BSC.
    /// Retain `transfer` across Pending while the board advances. A queued
    /// next unit can be taken immediately at the same board edge.
    pub fn resume(&mut self, bus: &mut Bus, transfer: ReadTransfer) -> Result<ReadPoll, ReadError> {
        if !self.current(transfer.ticket).map_err(ReadError::State)? {
            return Ok(ReadPoll::Discarded);
        }
        if transfer != Self::transfer(self.pending.as_ref().unwrap()) {
            return Err(ReadError::State(Error::Protocol));
        }
        match bus.resume_kernel_read(transfer) {
            Ok(value) => self
                .complete(transfer.ticket, value)
                .map_err(ReadError::State),
            Err(BusError::Pending) => Ok(ReadPoll::Pending),
            Err(error) => Err(ReadError::Bus(Box::new(error))),
        }
    }

    fn current(&self, ticket: ReadTicket) -> Result<bool, Error> {
        if ticket.owner != self.owner || ticket.sequence > self.sequence {
            return Err(Error::Protocol);
        }
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| pending.ticket != ticket)
        {
            return Ok(false);
        }
        if self.outgoing.is_some() {
            return Err(Error::Protocol);
        }
        Ok(true)
    }

    /// Call only after the corresponding board transfer physically completes.
    /// Values have the same CPU-facing byte order as ordinary read32/read16.
    pub fn complete(&mut self, ticket: ReadTicket, value: u32) -> Result<ReadPoll, Error> {
        if !self.current(ticket)? {
            return Ok(ReadPoll::Discarded);
        }
        let mut pending = self.pending.take().unwrap();
        if let Some(fill) = &mut pending.fill {
            let offset = ((pending.address & 12) as usize + (fill.beat + 1) * 4) & 15;
            fill.bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
            fill.beat += 1;
            if fill.beat != 4 {
                self.queue(pending)?;
                return Ok(ReadPoll::Pending);
            }
            // Reuse the actual cache's victim/tag/LRU implementation. These
            // callbacks read already completed local bytes, never the bus.
            let value = self.cached_value(
                pending.address,
                pending.width,
                pending.instruction,
                Some(&fill.bytes),
            );
            return Ok(ReadPoll::Ready(self.finish(pending.operation, value)));
        }
        let value = value & (u32::MAX >> ((4 - pending.width) * 8));
        Ok(ReadPoll::Ready(self.finish(pending.operation, value)))
    }

    fn queue(&mut self, mut pending: Pending) -> Result<(), Error> {
        self.sequence = self.sequence.checked_add(1).ok_or(Error::Protocol)?;
        pending.ticket = ReadTicket {
            owner: self.owner,
            sequence: self.sequence,
        };
        self.outgoing = Some(Self::transfer(&pending));
        self.pending = Some(pending);
        Ok(())
    }

    fn transfer(pending: &Pending) -> ReadTransfer {
        let (address, width, first, last) = if let Some(fill) = &pending.fill {
            let offset = ((pending.address & 12) + (fill.beat as u32 + 1) * 4) & 15;
            (
                (pending.address & !15) | offset,
                4,
                fill.beat == 0,
                fill.beat == 3,
            )
        } else {
            (pending.address, pending.width, true, true)
        };
        ReadTransfer {
            ticket: pending.ticket,
            address: address & 0x1FFF_FFFF,
            width,
            instruction: pending.instruction,
            first,
            last,
        }
    }

    fn cached_value(
        &mut self,
        address: u32,
        width: usize,
        instruction: bool,
        fill: Option<&[u8; 16]>,
    ) -> u32 {
        let read = |at: u32| {
            let bytes = fill.expect("resident hit or completed owned refill");
            let offset = (at & 15) as usize;
            Ok(bytes[offset..offset + 4].try_into().unwrap())
        };
        match width {
            1 => u32::from(
                self.cache
                    .read::<1>(address, instruction, read)
                    .unwrap()
                    .unwrap()[0],
            ),
            2 => u32::from(u16::from_be_bytes(
                self.cache
                    .read(address, instruction, read)
                    .unwrap()
                    .unwrap(),
            )),
            4 => u32::from_be_bytes(
                self.cache
                    .read(address, instruction, read)
                    .unwrap()
                    .unwrap(),
            ),
            _ => unreachable!("validated read width"),
        }
    }

    fn finish(&mut self, operation: Read, value: u32) -> u32 {
        if let Read::Fetch16(address) = operation {
            if address & 2 == 0 {
                self.retained = Some((address + 2, value as u16));
                value >> 16
            } else {
                self.retained = None;
                value & 0xFFFF
            }
        } else {
            value
        }
    }
}
