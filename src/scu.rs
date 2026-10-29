//! Independently authored SCU control. ST-097 §§2–4 and ST-210 final-spec
//! corrections are the behavioral references; see docs/hardware-conformance.md.
mod abus;
mod dma;
mod dsp;
mod transactions;
pub(crate) use transactions::DmaCycle;
#[cfg(test)]
mod tests;

use crate::bus::{BusError, Memory};
use std::cell::Cell;
use std::fmt;

pub const SCU: u32 = 0x05FE_0000;
pub(crate) const INTERRUPT_LEVELS: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 8, 6, 6, 5, 3, 2, 0, 0, 7, 7, 7, 7, 4, 4, 4, 4, 1, 1, 1, 1, 1, 1,
    1, 1,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScuError {
    Bus(BusError),
    IllegalDsp { pc: u8, instruction: u32 },
    DspDmaConflict { pc: u8, bank: u8 },
    DmaParameters { channel: usize },
}
impl From<BusError> for ScuError {
    fn from(error: BusError) -> Self {
        Self::Bus(error)
    }
}
impl fmt::Display for ScuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => write!(f, "SCU: {error}"),
            Self::IllegalDsp { pc, instruction } => write!(
                f,
                "SCU DSP illegal instruction {instruction:08X} at {pc:02X}"
            ),
            Self::DspDmaConflict { pc, bank } => write!(
                f,
                "SCU DSP at {pc:02X}: data bank {bank} is owned by DMA (SOA #8 §4.1)"
            ),
            Self::DmaParameters { channel } => write!(
                f,
                "SCU DMA {channel}: prohibited transfer parameters (ST-210)"
            ),
        }
    }
}
impl std::error::Error for ScuError {}

pub struct Scu {
    channels: [dma::Channel; 3],
    dsp: dsp::Dsp,
    mask: u16,
    status: u32,
    abus_ack: bool,
    timer0_compare: u16,
    timer0: u16,
    timer1_set: u16,
    timer_mode: u16,
    timer1_remaining: Option<u32>,
    dsp_phase: u32,
    sdram: bool,
    abus: abus::Abus,
}
impl Default for Scu {
    fn default() -> Self {
        Self {
            channels: std::array::from_fn(|_| dma::Channel::default()),
            dsp: dsp::Dsp::default(),
            mask: 0xBFFF,
            status: 0,
            abus_ack: false,
            timer0_compare: 0,
            timer0: 0,
            timer1_set: 0,
            timer_mode: 0,
            timer1_remaining: None,
            dsp_phase: 0,
            sdram: false,
            abus: abus::Abus::default(),
        }
    }
}

impl Scu {
    /// ST-097 table 2.1. Only the master receives the SCU interrupt output.
    pub fn interrupt(&self) -> Option<(u8, u8)> {
        let unmasked = self.status & !u32::from(self.mask) & 0x3FFF
            | if self.mask & 0x8000 == 0 {
                self.status & 0xFFFF_0000
            } else {
                0
            };
        // Equal-level sources use ascending vector order.
        (0..32)
            .filter(|bit| unmasked & (1 << bit) != 0)
            .map(|bit| (INTERRUPT_LEVELS[bit], 0x40 + bit as u8))
            .max_by_key(|(level, vector)| (*level, 255 - *vector))
    }
    fn acknowledge(&mut self, vector: u8) {
        if (0x40..=0x5F).contains(&vector) {
            self.status &= !(1 << (vector - 0x40));
        }
    }
    /// Functional SCU endpoint for H §5.2.3's IVECF/A3–A0 interface.
    /// ST-097 table2.1 defines the level/vector map, not the clear edge or
    /// equal-level arbitration. This model selects ascending vectors and
    /// consumes the selected source at acquisition completion (H §5.6 allows
    /// fetch-detected clearing). Neither choice proves the SCU silicon rule.
    /// No endpoint call is made for SH-2 autovectors; see the phase contract.
    pub(crate) fn acquire_vector(&mut self, level: u8) -> u8 {
        let unmasked = self.status & !u32::from(self.mask) & 0x3FFF
            | if self.mask & 0x8000 == 0 {
                self.status & 0xFFFF_0000
            } else {
                0
            };
        let source = (0..32)
            .find(|&bit| unmasked & (1 << bit) != 0 && INTERRUPT_LEVELS[bit] == level)
            .map(|bit| 0x40 + bit as u8);
        if let Some(vector) = source {
            self.acknowledge(vector);
        }
        // Undriven data is modeled high; this is not a specified SCU vector.
        source.unwrap_or(0x7F)
    }
    #[cfg(test)]
    pub(crate) fn dma_pending(&self) -> bool {
        self.dsp.dma_pending() || self.channels.iter().any(|c| c.pending())
    }
    pub(crate) fn can_wake(&self) -> bool {
        self.interrupt().is_some()
            || self.dsp.running()
            || self.channels.iter().any(|c| c.armed())
            || self.mask & 7 != 7
            || (self.timer_mode & 1 != 0 && self.mask & 0x18 != 0x18)
    }
    /// Device edges latch irrespective of IMS. DMA triggers are independent of IRQ masking.
    pub fn signal(&mut self, bit: u8) {
        if bit >= 32 || matches!(bit, 14 | 15) {
            return;
        }
        if bit >= 16 {
            if !self.abus_ack {
                return;
            }
            self.abus_ack = false;
        }
        self.status |= 1 << bit;
        let trigger = match bit {
            0..=4 => Some(bit),
            6 => Some(5),
            13 => Some(6),
            _ => None,
        };
        if let Some(trigger) = trigger {
            for channel in &mut self.channels {
                channel.trigger(trigger);
            }
        }
    }
    pub(crate) fn vblank_out(&mut self) {
        self.signal(1);
        self.timer0 = 0;
        if self.timer_mode & 1 != 0 && self.timer0_compare == 0 {
            self.signal(3);
        }
    }
    pub(crate) fn hblank_in(&mut self) {
        self.signal(2);
        self.timer0 = (self.timer0 + 1) & 0x3FF;
        if self.timer_mode & 1 != 0 {
            if self.timer0 == self.timer0_compare {
                self.signal(3);
            }
            // ST-210 #31: a running timer is not reloaded at the next line.
            if self.timer1_remaining.is_none() {
                let count = if self.timer1_set == 0 {
                    512
                } else {
                    u32::from(self.timer1_set)
                };
                self.timer1_remaining = Some(count * 4);
            }
        }
    }
    pub fn read32(&self, address: u32) -> Result<u32, BusError> {
        match address - SCU {
            offset @ 0..=0x5F => self.channels[offset as usize / 32].read(offset & 31, address),
            0x80 => Ok(self.dsp.control()),
            0x8C => self.dsp.read_data(address),
            0xA4 => Ok(self.status),
            0xA8 => Ok(u32::from(self.abus_ack)),
            0xC4 => Ok(u32::from(self.sdram)),
            0xC8 => Ok(4), // Functional model of production SCU revision 4.
            _ => Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
    }
    pub fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        match address - SCU {
            offset @ 0..=0x5F => {
                self.channels[offset as usize / 32].write(offset & 31, value, address)?
            }
            0x80 => self.dsp.set_control(value),
            0x84 => self.dsp.write_program(value, address)?,
            0x88 => self.dsp.set_data_address(value, address)?,
            0x8C => self.dsp.write_data(value, address)?,
            0x90 => self.timer0_compare = value as u16 & 0x3FF,
            0x94 => self.timer1_set = value as u16 & 0x1FF,
            0x98 => {
                self.timer_mode = value as u16 & 0x101;
                if self.timer_mode & 1 == 0 {
                    self.timer1_remaining = None;
                }
            }
            0xA0 => self.mask = value as u16 & 0xBFFF,
            0xA8 => self.abus_ack = value & 1 != 0,
            0xB0 | 0xB4 | 0xB8 => self.abus.write(address - SCU, value),
            0xC4 => self.sdram = value & 1 != 0,
            // ST-210 #6/7/13/14: unused ports, IST writes, DSTA/DSTOP are prohibited.
            // ASR/AREF are write-only and control external bus cycles.
            _ => return Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
        Ok(())
    }
    /// One bounded external beat. High level preempts at beat boundaries.
    #[cfg(test)]
    pub(crate) fn advance_dma(&mut self, memory: &mut impl Memory) -> Result<(), ScuError> {
        if let Some(index) = (0..3).rev().find(|i| self.channels[*i].pending()) {
            match self.channels[index].beat(memory, index) {
                Ok(true) => self.signal(11 - index as u8),
                Ok(false) => (),
                Err(error) => {
                    let indirect = self.channels[index].indirect();
                    self.channels[index].cancel();
                    // ST-210 #24: no illegal IRQ in indirect mode; invalid table
                    // accesses remain an explicit bounded machine diagnostic.
                    if indirect {
                        return Err(error);
                    }
                    self.signal(12);
                }
            }
        } else {
            self.dsp.dma_beat(memory)?;
        }
        Ok(())
    }
    pub(crate) fn abus_access(&self, address: u32, width: usize, write: bool) -> u32 {
        self.abus.access(address, width, write)
    }
    pub(crate) fn bus_width(&self, address: u32) -> Option<usize> {
        self.abus
            .width(address)
            .or_else(|| dma::b_bus(address).then_some(2))
    }
    pub(crate) fn refresh_abus(&self) {
        self.abus.request_refresh();
    }
    pub(crate) fn clocks_to_event(&self) -> u32 {
        self.timer1_remaining
            .unwrap_or(u32::MAX)
            .min(if self.dsp.instruction_pending() {
                2 - self.dsp_phase
            } else {
                u32::MAX
            })
    }
    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Result<(), ScuError> {
        self.abus.advance(clocks);
        if let Some(remaining) = self.timer1_remaining {
            if clocks >= remaining {
                self.timer1_remaining = None;
                if self.timer_mode & 0x100 == 0 || self.timer0 == self.timer0_compare {
                    self.signal(4);
                }
            } else {
                self.timer1_remaining = Some(remaining - clocks);
            }
        }
        let cycles = self.dsp_phase + clocks;
        self.dsp_phase = cycles % 2;
        if self.dsp.running() {
            for _ in 0..cycles / 2 {
                if self.dsp.step()? {
                    self.signal(5);
                }
            }
        }
        Ok(())
    }
}

// Read ports have device side effects even through immutable Memory references.
fn next_data_address(address: &Cell<u8>) {
    let old = address.get();
    address.set((old & 0xC0) | (old.wrapping_add(1) & 63));
}
