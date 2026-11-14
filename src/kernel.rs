//! Experimental dual-SH-2 continuation kernel, independent of `Machine`.
//!
//! The initial board is an explicit *serial test-bus contract*, not Saturn timing:
//! fetch, operand transfer, then ISA execution states, without cache or IF/MA overlap.
//! Its purpose is to validate ownership, interruption and completion semantics before
//! attaching the real cache, backplane and peripherals. It cannot boot Saturn media.
//! The separate ReadPort now uses the actual cache and board arbiter/BSC; CPU
//! IF/MA, writes and interrupt integration are still pending.
pub use crate::cpu::resumable::Executor;
mod read_port;
use crate::cpu::{Cpu, CpuState};
pub use read_port::{
    Read, ReadError, ReadPoll, ReadPort, ReadTicket, ReadTransfer,
    verify_board as verify_read_port_board,
};
use std::fmt;

pub const RAM_BASE: u32 = 0x0600_0000;
pub const RAM_SIZE: usize = 65536;
/// Synthetic side-effecting register, not a Saturn device address assignment.
pub const PROBE: u32 = 0x0100_0000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unsupported { pc: u32, opcode: u16 },
    Address { address: u32, width: u32 },
    InvalidTime,
    Protocol,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "experimental kernel: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Fetch {
        address: u32,
    },
    Read {
        address: u32,
        width: u32,
    },
    Write {
        address: u32,
        width: u32,
        value: u32,
    },
}
impl Operation {
    fn address_width(self) -> (u32, u32) {
        match self {
            Self::Fetch { address } => (address, 2),
            Self::Read { address, width } | Self::Write { address, width, .. } => (address, width),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ticket {
    owner: usize,
    epoch: u64,
    operation: Operation,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Transfer {
    ticket: Ticket,
    end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trace {
    Requested {
        at: u64,
        owner: usize,
        epoch: u64,
        operation: Operation,
    },
    Granted {
        at: u64,
        end: u64,
        owner: usize,
        epoch: u64,
        operation: Operation,
    },
    Completed {
        at: u64,
        owner: usize,
        epoch: u64,
        operation: Operation,
        value: u32,
    },
}

pub(crate) struct Fabric {
    ram: Vec<u8>,
    epochs: [u64; 2],
    requests: [Option<Ticket>; 2],
    replies: [Option<u32>; 2],
    active: Option<Transfer>,
    next_owner: usize,
    latency: u32,
    probe_value: u32,
    probe_reads: u64,
    probe_writes: u64,
    pub retired: [u64; 2],
    trace: Option<Vec<Trace>>,
}
impl Fabric {
    fn validate(address: u32, width: u32) -> Result<(), Error> {
        if !matches!(width, 1 | 2 | 4) || address & (width - 1) != 0 {
            return Err(Error::Address { address, width });
        }
        if address == PROBE
            || address
                .checked_sub(RAM_BASE)
                .is_some_and(|v| v as usize <= RAM_SIZE - width as usize)
        {
            Ok(())
        } else {
            Err(Error::Address { address, width })
        }
    }
    fn trace(&mut self, event: Trace) {
        if let Some(trace) = &mut self.trace {
            trace.push(event);
        }
    }
    pub fn request(&mut self, owner: usize, at: u64, operation: Operation) -> Result<(), Error> {
        let (address, width) = operation.address_width();
        Self::validate(address, width)?;
        if self.requests[owner].is_some() || self.replies[owner].is_some() {
            return Err(Error::Protocol);
        }
        let epoch = self.epochs[owner];
        self.requests[owner] = Some(Ticket {
            owner,
            epoch,
            operation,
        });
        self.trace(Trace::Requested {
            at,
            owner,
            epoch,
            operation,
        });
        Ok(())
    }
    pub fn reply(&mut self, owner: usize) -> Option<u32> {
        self.replies[owner].take()
    }
    fn read(&self, address: u32, width: u32) -> u32 {
        if address == PROBE {
            return self.probe_value & (u32::MAX >> ((4 - width) * 8));
        }
        let offset = (address - RAM_BASE) as usize;
        self.ram[offset..offset + width as usize]
            .iter()
            .fold(0, |v, b| v << 8 | u32::from(*b))
    }
    fn write(&mut self, address: u32, width: u32, value: u32) {
        if address == PROBE {
            self.probe_value = value & (u32::MAX >> ((4 - width) * 8));
            return;
        }
        let offset = (address - RAM_BASE) as usize;
        let bytes = value.to_be_bytes();
        self.ram[offset..offset + width as usize].copy_from_slice(&bytes[4 - width as usize..]);
    }
    fn complete(&mut self, now: u64) {
        let Some(transfer) = self.active.filter(|v| v.end == now) else {
            return;
        };
        self.active = None;
        let Ticket {
            owner,
            epoch,
            operation,
        } = transfer.ticket;
        let (address, width) = operation.address_width();
        let value = match operation {
            Operation::Write { value, .. } => {
                self.write(address, width, value);
                if address == PROBE {
                    self.probe_writes += 1;
                }
                0
            }
            _ => {
                let value = self.read(address, width);
                if address == PROBE {
                    self.probe_reads += 1;
                }
                value
            }
        };
        self.trace(Trace::Completed {
            at: now,
            owner,
            epoch,
            operation,
            value,
        });
        // Reset invalidates delivery, never an already granted physical transfer.
        if epoch == self.epochs[owner] {
            self.replies[owner] = Some(value);
        }
    }
    fn grant(&mut self, now: u64) -> Result<(), Error> {
        if self.active.is_some() {
            return Ok(());
        }
        for owner in [self.next_owner, self.next_owner ^ 1] {
            if let Some(ticket) = self.requests[owner].take() {
                let end = now
                    .checked_add(u64::from(self.latency))
                    .ok_or(Error::InvalidTime)?;
                self.active = Some(Transfer { ticket, end });
                self.next_owner = owner ^ 1;
                self.trace(Trace::Granted {
                    at: now,
                    end,
                    owner,
                    epoch: ticket.epoch,
                    operation: ticket.operation,
                });
                return Ok(());
            }
        }
        Ok(())
    }
    fn reset(&mut self, owner: usize) {
        self.epochs[owner] += 1;
        self.requests[owner] = None;
        self.replies[owner] = None;
    }
}

/// External events occur before a bus completion at the same timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Write {
        address: u32,
        width: u32,
        value: u32,
    },
    Reset {
        owner: usize,
        pc: u32,
        sp: u32,
    },
}

pub struct Kernel {
    cpus: [Executor; 2],
    fabric: Fabric,
    now: u64,
    events: Vec<(u64, Event)>,
    failure: Option<Error>,
}
impl Kernel {
    pub fn new(states: [CpuState; 2], transfer_clocks: u32) -> Self {
        Self {
            cpus: states.map(Executor::new),
            now: 0,
            events: Vec::new(),
            failure: None,
            fabric: Fabric {
                ram: vec![0; RAM_SIZE],
                epochs: [0; 2],
                requests: [None; 2],
                replies: [None; 2],
                active: None,
                next_owner: 0,
                latency: transfer_clocks,
                probe_value: 0,
                probe_reads: 0,
                probe_writes: 0,
                retired: [0; 2],
                trace: None,
            },
        }
    }
    pub fn now(&self) -> u64 {
        self.now
    }
    pub fn states(&self) -> [&CpuState; 2] {
        [self.cpus[0].state(), self.cpus[1].state()]
    }
    pub fn retired(&self) -> [u64; 2] {
        self.fabric.retired
    }
    pub fn probe_counts(&self) -> [u64; 2] {
        [self.fabric.probe_reads, self.fabric.probe_writes]
    }
    pub fn ram(&self) -> &[u8] {
        &self.fabric.ram
    }
    pub fn trace(&self) -> &[Trace] {
        self.fabric.trace.as_deref().unwrap_or(&[])
    }
    pub fn enable_trace(&mut self) {
        self.fabric.trace = Some(Vec::new());
    }
    /// Setup-only access; runtime mutations must be timestamped events.
    pub fn initialize(&mut self, address: u32, width: u32, value: u32) -> Result<(), Error> {
        if self.now != 0 || self.fabric.active.is_some() {
            return Err(Error::Protocol);
        }
        Fabric::validate(address, width)?;
        self.fabric.write(address, width, value);
        Ok(())
    }
    pub fn schedule(&mut self, at: u64, event: Event) -> Result<(), Error> {
        if at < self.now {
            return Err(Error::InvalidTime);
        }
        match event {
            Event::Write { address, width, .. } => Fabric::validate(address, width)?,
            Event::Reset { owner, .. } if owner > 1 => return Err(Error::Protocol),
            _ => {}
        }
        let index = self.events.partition_point(|(time, _)| *time <= at);
        self.events.insert(index, (at, event));
        Ok(())
    }
    /// Run the half-open interval [now, end). Splitting an interval is equivalent.
    /// Every transfer and retirement exits at the earliest external event.
    pub fn run_until(&mut self, end: u64) -> Result<(), Error> {
        self.run(end, None)
    }
    #[cfg(feature = "kernel-experiment")]
    fn run_instructions(&mut self, count: u64) -> Result<(), Error> {
        // Benchmark-only drain at retirement; no external-event timing claim.
        if !self.events.is_empty() {
            return Err(Error::Protocol);
        }
        let [first, second] = self
            .fabric
            .retired
            .map(|v| v.checked_add(count).ok_or(Error::InvalidTime));
        self.run(u64::MAX - 32, Some([first?, second?]))
    }
    fn run(&mut self, end: u64, targets: Option<[u64; 2]>) -> Result<(), Error> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if end < self.now {
            return Err(Error::InvalidTime);
        }
        while self.now < end {
            while self.events.first().is_some_and(|(at, _)| *at == self.now) {
                let (_, event) = self.events.remove(0);
                match event {
                    Event::Write {
                        address,
                        width,
                        value,
                    } => self.fabric.write(address, width, value),
                    Event::Reset { owner, pc, sp } => {
                        self.fabric.reset(owner);
                        self.cpus[owner] = Executor::new(Cpu::start(pc, sp).state().clone());
                    }
                }
            }
            self.fabric.complete(self.now);
            for owner in 0..2 {
                if let Err(error) = self.cpus[owner].poll(
                    owner,
                    self.now,
                    &mut self.fabric,
                    targets.map(|t| t[owner]),
                ) {
                    self.failure = Some(error.clone());
                    return Err(error);
                }
            }
            if targets == Some(self.fabric.retired) {
                return Ok(());
            }
            if let Err(error) = self.fabric.grant(self.now) {
                self.failure = Some(error.clone());
                return Err(error);
            }
            let mut next = end;
            if let Some(transfer) = self.fabric.active {
                next = next.min(transfer.end);
            }
            for cpu in &self.cpus {
                if let Some(at) = cpu.deadline() {
                    next = next.min(at);
                }
            }
            if let Some((at, _)) = self.events.first() {
                next = next.min(*at);
            }
            if targets.is_some() && next == end {
                return Err(Error::Protocol);
            }
            self.now = next;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(feature = "kernel-experiment")]
pub mod experiment;
