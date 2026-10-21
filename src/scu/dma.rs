//! ST-097 §§2.1/3.2; ST-210 #1/2/4/15–25 supersedes obsolete register/table descriptions.
use super::ScuError;
use crate::bus::{BusError, Memory};
use std::collections::VecDeque;

pub(super) fn b_bus(address: u32) -> bool {
    (0x05A0_0000..0x05FC_0000).contains(&address)
}
pub(super) fn valid_source(address: u32) -> bool {
    (0x0200_0000..0x0590_0000).contains(&address)
        || (0x05A0_0000..0x05E0_0000).contains(&address)
        || (0x0600_0000..0x0610_0000).contains(&address)
}
pub(super) fn valid_destination(address: u32) -> bool {
    b_bus(address) || (0x0600_0000..0x0610_0000).contains(&address)
}

pub(super) struct Channel {
    source: u32,
    destination: u32,
    count: u32,
    add: u32,
    mode: u32,
    enabled: bool,
    requested: bool,
    retrigger: bool,
    active: Option<Transfer>,
}
struct Transfer {
    source: u32,
    destination: u32,
    unread: u32,
    unwritten: u32,
    table: u32,
    last: bool,
    buffer: VecDeque<u8>,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            source: 0,
            destination: 0,
            count: 0,
            add: 0x101,
            mode: 7,
            enabled: false,
            requested: false,
            retrigger: false,
            active: None,
        }
    }
}
impl Channel {
    pub(super) fn read(&self, offset: u32, address: u32) -> Result<u32, BusError> {
        match offset {
            0 => Ok(self.source),
            4 => Ok(self.destination),
            _ => Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
    }
    pub(super) fn write(&mut self, offset: u32, value: u32, address: u32) -> Result<(), BusError> {
        if self.pending() {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        match offset {
            0 => self.source = value & 0x07FF_FFFF,
            4 => self.destination = value & 0x07FF_FFFF,
            8 => self.count = value & if address & 0x60 == 0 { 0xF_FFFF } else { 0xFFF },
            12 => self.add = value & 0x107,
            16 => {
                self.enabled = value & 0x100 != 0;
                if self.mode & 7 == 7 && value & 1 != 0 && self.enabled {
                    self.requested = true;
                }
            }
            20 => self.mode = value & 0x0101_0107,
            _ => return Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
        Ok(())
    }
    pub(super) fn armed(&self) -> bool {
        (self.enabled && self.mode & 7 != 7) || self.pending()
    }
    pub(super) fn pending(&self) -> bool {
        self.requested || self.active.is_some()
    }
    pub(super) fn indirect(&self) -> bool {
        self.mode & (1 << 24) != 0
    }
    pub(super) fn trigger(&mut self, trigger: u8) {
        if self.enabled && self.mode & 7 == u32::from(trigger) {
            if self.pending() {
                self.retrigger = true;
            } else {
                self.requested = true;
            }
        }
    }
    pub(super) fn cancel(&mut self) {
        self.active = None;
        self.requested = false;
        self.retrigger = false;
        self.enabled = false;
    }
    fn load_table(memory: &impl Memory, table: u32, index: usize) -> Result<Transfer, ScuError> {
        // ST-210 #25: count, write, read; end marker in read bit 31.
        if table & 3 != 0 || !(0x0600_0000..=0x060F_FFF4).contains(&table) {
            return Err(ScuError::DmaParameters { channel: index });
        }
        let count = memory.read32(table)? & 0xF_FFFF;
        let destination = memory.read32(table + 4)? & 0x07FF_FFFF;
        let source = memory.read32(table + 8)?;
        let count = if count == 0 { 0x10_0000 } else { count };
        Ok(Transfer {
            source: source & 0x07FF_FFFF,
            destination,
            unread: count,
            unwritten: count,
            table: table + 12,
            last: source & 0x8000_0000 != 0,
            buffer: VecDeque::with_capacity(8),
        })
    }
    pub(super) fn beat(
        &mut self,
        memory: &mut impl Memory,
        index: usize,
    ) -> Result<bool, ScuError> {
        if self.active.is_none() {
            self.requested = false;
            let transfer = if self.indirect() {
                Self::load_table(memory, self.destination, index)?
            } else {
                let count = if self.count != 0 {
                    self.count
                } else if index == 0 {
                    0x10_0000
                } else {
                    0x1000
                };
                Transfer {
                    source: self.source,
                    destination: self.destination,
                    unread: count,
                    unwritten: count,
                    table: 0,
                    last: true,
                    buffer: VecDeque::with_capacity(8),
                }
            };
            self.active = Some(transfer);
            return Ok(false);
        }
        let t = self.active.as_mut().unwrap();
        let illegal = || ScuError::DmaParameters { channel: index };
        if !valid_source(t.source)
            || !valid_destination(t.destination)
            || (self.add & 0x100 == 0 && !(0x0580_0000..0x0590_0000).contains(&t.source))
            || (!b_bus(t.destination) && self.add & 7 != 2)
        {
            return Err(illegal());
        }
        let write_width = width(
            t.destination,
            t.unwritten,
            if b_bus(t.destination) { 2 } else { 4 },
        );
        if t.buffer.len() < write_width && t.unread != 0 {
            let read_width = width(t.source, t.unread, 4);
            if !valid_source(t.source + read_width as u32 - 1) {
                return Err(illegal());
            }
            match read_width {
                4 => t.buffer.extend(memory.read32(t.source)?.to_be_bytes()),
                2 => t.buffer.extend(memory.read16(t.source)?.to_be_bytes()),
                _ => t.buffer.push_back(memory.read8(t.source)?),
            }
            if self.add & 0x100 != 0 {
                t.source = t.source.wrapping_add(read_width as u32);
            }
            t.unread -= read_width as u32;
            return Ok(false);
        }
        if !valid_destination(t.destination + write_width as u32 - 1) {
            return Err(illegal());
        }
        let mut bytes = [0; 4];
        for byte in &mut bytes[..write_width] {
            *byte = t.buffer.pop_front().unwrap();
        }
        match write_width {
            4 => memory.write32(t.destination, u32::from_be_bytes(bytes))?,
            2 => memory.write16(t.destination, u16::from_be_bytes([bytes[0], bytes[1]]))?,
            _ => memory.write8(t.destination, bytes[0])?,
        }
        let stride = match self.add & 7 {
            0 => 0,
            n => 1 << n,
        };
        t.destination = t
            .destination
            .wrapping_add(if b_bus(t.destination) && write_width != 1 {
                stride
            } else {
                write_width as u32
            });
        t.unwritten -= write_width as u32;
        if t.unwritten != 0 {
            return Ok(false);
        }
        let t = self.active.take().unwrap();
        if self.indirect() {
            if !t.last {
                self.active = Some(Self::load_table(memory, t.table, index)?);
                return Ok(false);
            }
            if self.mode & 0x100 != 0 {
                self.destination = t.table;
            }
        } else {
            if self.mode & 0x10000 != 0 {
                self.source = t.source;
            }
            if self.mode & 0x100 != 0 {
                self.destination = t.destination;
            }
        }
        // External factors remain armed for repeated transfers; GO is a one-shot.
        self.requested = std::mem::take(&mut self.retrigger);
        Ok(true)
    }
}
fn width(address: u32, remaining: u32, maximum: usize) -> usize {
    if maximum >= 4 && address & 3 == 0 && remaining >= 4 {
        4
    } else if maximum >= 2 && address & 1 == 0 && remaining >= 2 {
        2
    } else {
        1
    }
}
