//! SH7604 §§12.2–12.3, table 12.2. Keyed writes and read-before-clear flags.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetRequest {
    PowerOn,
    Manual,
}

pub(super) struct Watchdog {
    control: u8,
    counter: u8,
    reset: u8,
    observed: Cell<u8>,
    reset_observed: Cell<bool>,
    divider: u32,
    pub request: Option<ResetRequest>,
    reset_hold: Option<(u32, ResetRequest)>,
}
impl Default for Watchdog {
    fn default() -> Self {
        Self {
            control: 0x18,
            counter: 0,
            reset: 0x1F,
            observed: Cell::new(0),
            reset_observed: Cell::new(false),
            divider: 0,
            request: None,
            reset_hold: None,
        }
    }
}
impl Watchdog {
    pub fn read(&self, offset: u32) -> Option<u8> {
        match offset {
            0 => {
                self.observed.set(self.control & 0x80);
                Some(self.control)
            }
            1 => Some(self.counter),
            3 => {
                self.reset_observed.set(self.reset & 0x80 != 0);
                Some(self.reset)
            }
            _ => None,
        }
    }
    pub fn write(&mut self, offset: u32, value: u16) {
        let data = value as u8;
        match (offset, value >> 8) {
            (0, 0x5A) => self.counter = data,
            (0, 0xA5) => {
                let flag = self.control & 0x80 & !(self.observed.get() & !data);
                self.control = (data & 0x67) | 0x18 | flag;
                self.observed.set(0);
                if self.control & 0x20 == 0 {
                    self.counter = 0;
                    self.divider = 0;
                }
            }
            (2, 0x5A) => self.reset = (self.reset & 0x80) | (data & 0x60) | 0x1F,
            (2, 0xA5) if data == 0 && self.reset_observed.get() => {
                self.reset &= 0x7F;
                self.reset_observed.set(false);
            }
            _ => (), // Wrong key is ignored by hardware, not a new register write.
        }
    }
    pub fn advance(&mut self, clocks: u32) {
        if let Some((remaining, request)) = self.reset_hold {
            if clocks < remaining {
                self.reset_hold = Some((remaining - clocks, request));
            } else {
                self.reset_hold = None;
                self.request = Some(request);
            }
            return;
        }
        if self.control & 0x20 == 0 {
            return;
        }
        let divisor = [2, 64, 128, 256, 512, 1024, 4096, 8192][(self.control & 7) as usize];
        let total = u64::from(self.divider) + u64::from(clocks);
        self.divider = (total % divisor) as u32;
        for edge in 0..total / divisor {
            self.counter = self.counter.wrapping_add(1);
            if self.counter != 0 {
                continue;
            }
            if self.control & 0x40 == 0 {
                self.control |= 0x80;
            } else {
                self.reset |= 0x80;
                if self.reset & 0x40 != 0 {
                    let request = if self.reset & 0x20 != 0 {
                        ResetRequest::Manual
                    } else {
                        ResetRequest::PowerOn
                    };
                    // H §12.3.1: internal reset is asserted for 512 phi clocks.
                    let after_overflow = total - (edge + 1) * divisor;
                    if after_overflow >= 512 {
                        self.request = Some(request);
                    } else {
                        self.reset_hold = Some((512 - after_overflow as u32, request));
                    }
                }
                self.control = 0x18;
                self.counter = 0;
                self.divider = 0;
                break;
            }
        }
    }
    pub fn reset_active(&self) -> bool {
        self.reset_hold.is_some()
    }
    pub fn irq(&self) -> bool {
        self.control & 0xC0 == 0x80
    }
    pub fn can_reset(&self) -> bool {
        self.reset_active() || (self.control & 0x60 == 0x60 && self.reset & 0x40 != 0)
    }
    pub fn enabled(&self) -> bool {
        self.control & 0x20 != 0
    }
    pub fn reset_preserving_cause(&mut self) {
        let cause = self.reset;
        *self = Self::default();
        self.reset = cause;
    }
}
