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
    /// H table 14.3: address/count/vector registers survive software standby;
    /// CHCR0/1 and DMAOR initialize, aborting any outstanding request.
    pub fn standby(&mut self) {
        for channel in &mut self.channels {
            channel.control = 0;
            channel.observed.set(false);
            channel.external_edge = false;
        }
        self.operation = 0;
        self.observed.set(0);
        self.address_error = false;
    }
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
        let single = c.control & 8 != 0;
        let input = c.control & 0x100 != 0;
        let address = if input { c.destination } else { c.source };
        // H §§9.2.4/9.3.4 table9.7: single-address transfers use external
        // DREQ, no SCI/auto requests, no 16-byte bursts or on-chip endpoints.
        // The register/address mode on the DACK side is completely ignored.
        let valid = if single {
            c.control & 0x200 == 0
                && c.request == 0
                && size != 16
                && address < 0x4000_0000
                && address.is_multiple_of(width)
                && (c.control >> if input { 14 } else { 12 }) & 3 != 3
        } else {
            (c.control >> 12) & 3 != 3
                && (c.control >> 14) & 3 != 3
                && c.source.is_multiple_of(if size == 16 { 16 } else { width })
                && c.destination.is_multiple_of(width)
        };
        let result = if !valid {
            Err(BusError::Unaligned {
                address: c.source,
                width: width as usize,
            })
        } else if single {
            let active_high = c.control & 0x80 != 0;
            if input {
                memory
                    .dack_read(selected, width as usize, active_high)
                    .and_then(|value| match width {
                        1 => memory.write8(address, value as u8),
                        2 => memory.write16(address, value as u16),
                        _ => memory.write32(address, value),
                    })
            } else {
                let value = match width {
                    1 => memory.read8(address).map(u32::from),
                    2 => memory.read16(address).map(u32::from),
                    _ => memory.read32(address),
                };
                value.and_then(|value| {
                    memory.dack_write(selected, width as usize, active_high, value)
                })
            }
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
        c.source = if single && input {
            c.source
        } else if size == 16 {
            c.source.wrapping_add(16)
        } else {
            update(c.source, (c.control >> 12) & 3)
        };
        if !single || input {
            c.destination = update(c.destination, (c.control >> 14) & 3);
        }
        c.count = c.count.wrapping_sub(count) & 0xFF_FFFF;
        if c.count == 0 {
            c.control |= 2;
        }
        c.external_edge = false;
        self.next = 1 - selected;
        Some(if c.control & 0x200 != 0 { 0 } else { c.request })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Pins {
        memory: Vec<u8>,
        input: VecDeque<u32>,
        output: Vec<(usize, usize, bool, u32)>,
        reads: Vec<(usize, usize, bool)>,
    }
    impl Default for Pins {
        fn default() -> Self {
            Self {
                memory: vec![0; 128],
                input: VecDeque::from([0x12345678, 0x89ABCDEF]),
                output: Vec::new(),
                reads: Vec::new(),
            }
        }
    }
    impl Memory for Pins {
        fn read8(&self, a: u32) -> Result<u8, BusError> {
            self.memory
                .get(a as usize)
                .copied()
                .ok_or(BusError::Unmapped {
                    address: a,
                    width: 1,
                })
        }
        fn read16(&self, a: u32) -> Result<u16, BusError> {
            Ok(u16::from_be_bytes([self.read8(a)?, self.read8(a + 1)?]))
        }
        fn read32(&self, a: u32) -> Result<u32, BusError> {
            Ok(u32::from(self.read16(a)?) << 16 | u32::from(self.read16(a + 2)?))
        }
        fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
            *self.memory.get_mut(a as usize).ok_or(BusError::Unmapped {
                address: a,
                width: 1,
            })? = v;
            Ok(())
        }
        fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
            self.write8(a, (v >> 8) as u8)?;
            self.write8(a + 1, v as u8)
        }
        fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
            self.write16(a, (v >> 16) as u16)?;
            self.write16(a + 2, v as u16)
        }
        fn dack_read(&mut self, channel: usize, width: usize, high: bool) -> Result<u32, BusError> {
            self.reads.push((channel, width, high));
            Ok(self.input.pop_front().unwrap())
        }
        fn dack_write(
            &mut self,
            channel: usize,
            width: usize,
            high: bool,
            value: u32,
        ) -> Result<(), BusError> {
            self.output.push((channel, width, high, value));
            Ok(())
        }
    }

    #[test]
    fn single_address_dack_uses_one_memory_endpoint_and_selected_pin_polarity() {
        // H §9.2.4 and table9.7. Independent DACK data, not an invented second
        // memory-mapped endpoint; the unused address is deliberately unmapped.
        for channel in 0..2 {
            for input in [false, true] {
                for size in 0..3 {
                    for high in [false, true] {
                        for mode in 0..3u32 {
                            let width = 1usize << size;
                            let mut pins = Pins::default();
                            pins.memory[0x18..0x30]
                                .copy_from_slice(&[0x12, 0x34, 0x56, 0x78].repeat(6));
                            let mut d = Dma::default();
                            let base = channel as u32 * 16;
                            d.write(base, if input { u32::MAX } else { 0x20 });
                            d.write(base + 4, if input { 0x20 } else { u32::MAX });
                            d.write(base + 8, 2);
                            d.write(0x20 + channel as u32 * 8, 0x44);
                            let modes = if input {
                                (3 << 12) | (mode << 14)
                            } else {
                                (mode << 12) | (3 << 14)
                            };
                            let control = modes
                                | (size << 10)
                                | (u32::from(input) * 0x100)
                                | (u32::from(high) * 0x80)
                                | 0x4D;
                            d.write(base + 12, control);
                            d.write(0x30, 1);
                            assert_eq!(d.transfer(&mut pins, [true, true]), None); // SCI cannot request DACK
                            for n in 0..2 {
                                d.set_request_pin(channel, false); // active-low DREQ falling edge
                                assert_eq!(d.transfer(&mut pins, [false, false]), Some(0));
                                assert_eq!(d.read(base + 8), Some(1 - n));
                                assert_eq!(d.transfer(&mut pins, [false, false]), None); // consume one edge
                                d.set_request_pin(channel, true);
                            }
                            assert_eq!(d.interrupt(), Some(0x44));
                            let address = match mode {
                                0 => 0x20,
                                1 => 0x20 + width as u32 * 2,
                                _ => 0x20 - width as u32 * 2,
                            };
                            assert_eq!(d.read(base + if input { 0 } else { 4 }), Some(u32::MAX));
                            assert_eq!(d.read(base + if input { 4 } else { 0 }), Some(address));
                            if input {
                                assert_eq!(pins.reads, vec![(channel, width, high); 2]);
                                assert!(pins.output.is_empty());
                                let at = match mode {
                                    0 => 0x20,
                                    1 => 0x20 + width,
                                    _ => 0x20 - width,
                                };
                                assert_eq!(
                                    &pins.memory[at..at + width],
                                    &0x89ABCDEFu32.to_be_bytes()[4 - width..]
                                );
                            } else {
                                assert!(pins.reads.is_empty());
                                let first = [0x12, 0x1234, 0x12345678][size as usize];
                                let second = match (size, mode) {
                                    (0, 1) => 0x34,
                                    (0, 2) => 0x78,
                                    (1, 1 | 2) => 0x5678,
                                    _ => first,
                                };
                                assert_eq!(
                                    pins.output,
                                    vec![
                                        (channel, width, high, first),
                                        (channel, width, high, second)
                                    ]
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn illegal_single_address_modes_latch_error_without_counting_or_acknowledging() {
        for control in [0x020D, 0x0C0D, 0x010D] {
            // auto request, sixteen bytes, on-chip DAR
            let mut d = Dma::default();
            let mut pins = Pins::default();
            d.write(0, 0x20);
            d.write(4, 0xFFFF_FE00);
            d.write(8, 1);
            d.write(12, control);
            d.write(0x30, 1);
            d.set_request_pin(0, false);
            assert_eq!(d.transfer(&mut pins, [false, false]), None);
            assert!(d.address_error);
            assert_eq!(d.read(8), Some(1));
            assert!(pins.output.is_empty() && pins.reads.is_empty());
        }
    }
}
