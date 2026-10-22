//! SH7604 §§13.2–13.3: register-level serial engine; no implicit loopback.
use std::cell::Cell;
use std::collections::VecDeque;

pub(super) struct Serial {
    mode: u8,
    rate: u8,
    control: u8,
    transmit: u8,
    receive: u8,
    status: u8,
    observed: Cell<u8>,
    shift: Option<(u8, u32)>,
    output: VecDeque<u8>,
}
impl Default for Serial {
    fn default() -> Self {
        Self {
            mode: 0,
            rate: 0xFF,
            control: 0,
            transmit: 0xFF,
            receive: 0,
            status: 0x84,
            observed: Cell::new(0),
            shift: None,
            output: VecDeque::new(),
        }
    }
}
impl Serial {
    pub fn read(&self, offset: u32) -> Option<u8> {
        Some(match offset {
            0 => self.mode,
            1 => self.rate,
            2 => self.control,
            3 => self.transmit,
            4 => {
                self.observed.set(self.status & 0xF8);
                self.status
            }
            5 => self.receive,
            _ => return None,
        })
    }
    pub fn write(&mut self, offset: u32, value: u8) -> bool {
        match offset {
            0 => self.mode = value,
            1 => self.rate = value,
            2 => {
                self.control = value;
                if value & 0x20 == 0 {
                    self.shift = None;
                    self.status |= 0x84;
                }
            }
            3 => self.transmit = value,
            4 => {
                self.status &= !(self.observed.get() & !value);
                self.status = (self.status & !1) | (value & 1);
                self.observed.set(0);
                if self.control & 0x20 == 0 {
                    self.status |= 0x84;
                } else if self.status & 0x80 == 0 {
                    self.status &= !4;
                }
            }
            _ => return false,
        }
        true
    }
    fn frame_clocks(&self) -> u32 {
        let synchronous = self.mode & 0x80 != 0;
        let data_bits = if synchronous || self.mode & 0x40 == 0 {
            8
        } else {
            7
        };
        let bits = if synchronous {
            8
        } else {
            1 + data_bits
                + u32::from(self.mode & 0x24 != 0)
                + if self.mode & 8 == 0 { 1 } else { 2 }
        };
        bits * (if synchronous { 4 } else { 32 })
            * (1 << (2 * (self.mode & 3)))
            * (u32::from(self.rate) + 1)
    }
    pub fn clocks_to_event(&self) -> u32 {
        if self.control & 0x20 == 0 || self.control & 2 != 0 {
            u32::MAX
        } else if let Some((_, remaining)) = self.shift {
            remaining
        } else if self.status & 0x80 == 0 {
            1 // TDR transfers to the idle shifter; TDRE can request DMA/IRQ.
        } else {
            u32::MAX
        }
    }
    pub fn advance(&mut self, mut clocks: u32) {
        if self.control & 0x20 == 0 || self.control & 2 != 0 {
            return;
        }
        loop {
            if self.shift.is_none() && self.status & 0x80 == 0 {
                let byte = if self.mode & 0xC0 == 0x40 {
                    self.transmit & 0x7F
                } else {
                    self.transmit
                };
                self.shift = Some((byte, self.frame_clocks()));
                self.status |= 0x80;
            }
            let Some((byte, remaining)) = self.shift else {
                return;
            };
            if clocks < remaining {
                self.shift = Some((byte, remaining - clocks));
                return;
            }
            clocks -= remaining;
            self.shift = None;
            // Bound host diagnostics when the serial output is unconnected.
            if self.output.len() == 256 {
                self.output.pop_front();
            }
            self.output.push_back(byte);
            if self.status & 0x80 != 0 {
                self.status |= 4;
                return;
            }
        }
    }
    /// Supply one completed serial character from an external pin-level adapter.
    /// Error bits are FER/PER; no host write targets guest memory.
    pub fn receive(&mut self, byte: u8, errors: u8, multiprocessor: bool) {
        if self.control & 0x10 == 0 || self.status & 0x38 != 0 {
            return;
        }
        if self.status & 0x40 != 0 {
            self.status |= 0x20;
            return;
        }
        if self.mode & 4 != 0 && self.control & 8 != 0 && !multiprocessor {
            return;
        }
        self.receive = if self.mode & 0xC0 == 0x40 {
            byte & 0x7F
        } else {
            byte
        };
        self.status = (self.status & !2) | (u8::from(multiprocessor) << 1) | (errors & 0x18);
        if errors & 0x18 == 0 {
            self.status |= 0x40;
        }
        if multiprocessor {
            self.control &= !8;
        }
    }
    pub fn take_transmitted(&mut self) -> Option<u8> {
        self.output.pop_front()
    }
    pub fn irq(&self) -> Option<usize> {
        if self.control & 0x40 != 0 && self.status & 0x38 != 0 {
            Some(0)
        } else if self.control & 0x40 != 0 && self.status & 0x40 != 0 {
            Some(1)
        } else if self.control & 0x80 != 0 && self.status & 0x80 != 0 {
            Some(2)
        } else if self.control & 4 != 0 && self.status & 4 != 0 {
            Some(3)
        } else {
            None
        }
    }
    pub fn can_wake(&self) -> bool {
        self.irq().is_some()
            || (self.control & 0x26 == 0x24 && (self.shift.is_some() || self.status & 0x80 == 0))
    }
    pub fn dma_request(&self, source: u8) -> bool {
        match source {
            1 => self.control & 0x10 != 0 && self.status & 0x40 != 0,
            2 => self.control & 0x20 != 0 && self.status & 0x80 != 0,
            _ => false,
        }
    }
    pub fn dma_acknowledge(&mut self, source: u8) {
        match source {
            1 => self.status &= !0x40,
            2 => self.status &= !0x84,
            _ => (),
        }
    }
}
