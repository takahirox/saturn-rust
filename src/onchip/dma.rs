//! SH7604 §§9.2–9.3. Two independent channels with bounded transfer quanta.
use crate::bus::{BusError, Memory};
use std::cell::Cell;

struct Channel {
    source: u32,
    destination: u32,
    count: u32,
    control: u32,
    vector: u32,
    request: u8,
    observed: Cell<bool>,
    external_level: bool,
    external_edge: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            source: 0,
            destination: 0,
            count: 0,
            control: 0,
            vector: 0,
            request: 0,
            observed: Cell::new(false),
            external_level: true,
            external_edge: false,
        }
    }
}

pub(crate) struct Dma {
    channels: [Channel; 2],
    operation: u32,
    observed: Cell<u32>,
    next: usize,
    pub address_error: bool,
}
impl Default for Dma {
    fn default() -> Self {
        Self {
            channels: std::array::from_fn(|_| Channel::default()),
            operation: 0,
            observed: Cell::new(0),
            next: 1,
            address_error: false,
        }
    }
}
impl Dma {
    pub fn read(&self, offset: u32) -> Option<u32> {
        Some(match offset {
            0..=0x1F => {
                let c = &self.channels[(offset / 16) as usize];
                match offset & 15 {
                    0 => c.source,
                    4 => c.destination,
                    8 => c.count,
                    12 => {
                        c.observed.set(c.control & 2 != 0);
                        c.control
                    }
                    _ => return None,
                }
            }
            0x20 => self.channels[0].vector,
            0x28 => self.channels[1].vector,
            0x30 => {
                self.observed.set(self.operation & 6);
                self.operation
            }
            _ => return None,
        })
    }
    pub fn write(&mut self, offset: u32, value: u32) -> bool {
        match offset {
            0..=0x1F => {
                let c = &mut self.channels[(offset / 16) as usize];
                match offset & 15 {
                    0 => c.source = value,
                    4 => c.destination = value,
                    8 => c.count = value & 0xFF_FFFF,
                    12 => {
                        if value & 8 != 0 {
                            return false;
                        } // No single-address DACK pin adapter yet.
                        let te = c.control & 2 & if c.observed.get() { value } else { u32::MAX };
                        c.control = (value & 0xFFFD) | te;
                        c.observed.set(false);
                    }
                    _ => return false,
                }
            }
            0x20 => self.channels[0].vector = value & 0x7F,
            0x28 => self.channels[1].vector = value & 0x7F,
            0x30 => {
                let flags = self.operation & 6 & !(self.observed.get() & !value);
                self.operation = (value & 9) | flags;
                self.observed.set(0);
            }
            _ => return false,
        }
        true
    }
    pub fn read_request(&self, channel: usize) -> u8 {
        self.channels[channel].request
    }
    pub fn write_request(&mut self, channel: usize, value: u8) {
        self.channels[channel].request = value & 3;
    }
    pub fn set_request_pin(&mut self, channel: usize, level: bool) {
        let c = &mut self.channels[channel];
        let rising = !c.external_level && level;
        let falling = c.external_level && !level;
        if (c.control & 0x20 != 0 && rising) || (c.control & 0x20 == 0 && falling) {
            c.external_edge = true;
        }
        c.external_level = level;
    }
    pub fn nmi(&mut self) {
        self.operation |= 2;
    }
    pub fn interrupt(&self) -> Option<u8> {
        self.channels
            .iter()
            .find(|c| c.control & 6 == 6)
            .map(|c| c.vector as u8)
    }
    pub fn active(&self) -> bool {
        self.operation & 7 == 1 && self.channels.iter().any(|c| c.control & 3 == 1)
    }
    fn requested(c: &Channel, serial: [bool; 2]) -> bool {
        c.control & 0x200 != 0
            || match c.request {
                0 => {
                    if c.control & 0x40 != 0 {
                        c.external_edge
                    } else {
                        c.external_level == (c.control & 0x20 != 0)
                    }
                }
                1 => serial[0],
                2 => serial[1],
                _ => false,
            }
    }
    /// Returns the SCI request acknowledged, if any. Errors abort DMA and latch
    /// AE; they are not propagated as a host CPU bus failure.
    pub fn transfer(&mut self, memory: &mut impl Memory, serial: [bool; 2]) -> Option<u8> {
        if self.operation & 7 != 1 {
            return None;
        }
        let order = if self.operation & 8 == 0 {
            [0, 1]
        } else {
            [self.next, 1 - self.next]
        };
        let selected = order.into_iter().find(|&i| {
            let c = &self.channels[i];
            c.control & 3 == 1 && Self::requested(c, serial)
        })?;
        let c = &mut self.channels[selected];
        let size = match (c.control >> 10) & 3 {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => 16,
        };
        let count = if size == 16 { 4 } else { 1 };
        let width = size.min(4);
        // Single-address external DACK endpoints require a pin adapter, not
        // memory-to-memory substitution. Reserved address modes are illegal.
        let valid = c.control & 8 == 0
            && (c.control >> 12) & 3 != 3
            && (c.control >> 14) & 3 != 3
            && c.source.is_multiple_of(if size == 16 { 16 } else { width })
            && c.destination.is_multiple_of(width);
        let result = if !valid {
            Err(BusError::Unaligned {
                address: c.source,
                width: width as usize,
            })
        } else {
            (|| {
                for i in 0..count {
                    let src = c.source.wrapping_add(i * width);
                    let dst = c.destination.wrapping_add(i * width);
                    match width {
                        1 => {
                            let value = memory.read8(src)?;
                            memory.write8(dst, value)?;
                        }
                        2 => {
                            let value = memory.read16(src)?;
                            memory.write16(dst, value)?;
                        }
                        _ => {
                            let value = memory.read32(src)?;
                            memory.write32(dst, value)?;
                        }
                    }
                }
                Ok(())
            })()
        };
        if result.is_err() {
            self.operation |= 4;
            self.address_error = true;
            return None;
        }
        let update = |address: u32, mode: u32| match mode {
            1 => address.wrapping_add(size),
            2 => address.wrapping_sub(size),
            _ => address,
        };
        c.source = if size == 16 {
            c.source.wrapping_add(16)
        } else {
            update(c.source, (c.control >> 12) & 3)
        };
        c.destination = update(c.destination, (c.control >> 14) & 3);
        c.count = c.count.wrapping_sub(count) & 0xFF_FFFF;
        if c.count == 0 {
            c.control |= 2;
        }
        c.external_edge = false;
        self.next = 1 - selected;
        Some(if c.control & 0x200 != 0 { 0 } else { c.request })
    }
}
