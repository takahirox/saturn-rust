//! SH7604 §§6.2–6.3, tables 6.1–6.4: user-break comparison logic.
//! Comparisons observe architectural CPU/DMAC accesses, including cache hits.
use crate::bus::BusError;

pub const BASE: u32 = 0xFFFF_FF40;

#[derive(Default)]
pub(super) struct Ubc {
    address: [u32; 2],
    mask: [u32; 2],
    cycle: [u16; 2],
    data: u32,
    data_mask: u32,
    control: u16,
    sequence: bool,
    pending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Master {
    Cpu,
    Dma,
    External,
}

impl Ubc {
    fn error(address: u32, width: usize) -> BusError {
        BusError::UnsupportedDevice { address, width }
    }

    pub fn read(&self, address: u32, width: usize) -> Result<u32, BusError> {
        let offset = address - BASE;
        let long = match offset & !3 {
            0 => self.address[0],
            4 => self.mask[0],
            8 => {
                return (offset == 8)
                    .then_some(u32::from(self.cycle[0]))
                    .ok_or(Self::error(address, width));
            }
            0x20 => self.address[1],
            0x24 => self.mask[1],
            0x28 => {
                return (offset == 0x28)
                    .then_some(u32::from(self.cycle[1]))
                    .ok_or(Self::error(address, width));
            }
            0x30 => self.data,
            0x34 => self.data_mask,
            0x38 => {
                return (offset == 0x38)
                    .then_some(u32::from(self.control))
                    .ok_or(Self::error(address, width));
            }
            _ => return Err(Self::error(address, width)),
        };
        Ok(if width == 4 {
            long
        } else if offset & 2 == 0 {
            long >> 16
        } else {
            long & 0xFFFF
        })
    }

    pub fn write(&mut self, address: u32, width: usize, value: u32) -> Result<(), BusError> {
        let offset = address - BASE;
        let register = match offset & !3 {
            0 => &mut self.address[0],
            4 => &mut self.mask[0],
            0x20 => &mut self.address[1],
            0x24 => &mut self.mask[1],
            0x30 => &mut self.data,
            0x34 => &mut self.data_mask,
            8 | 0x28 if offset & 3 == 0 => {
                self.cycle[usize::from(offset == 0x28)] = value as u16 & 255;
                return Ok(());
            }
            0x38 if offset & 3 == 0 => {
                self.control = value as u16 & 0xF4DC;
                // A cleared channel-A match restarts sequential comparison.
                if self.control & 0xC000 == 0 {
                    self.sequence = false;
                }
                return Ok(());
            }
            _ => return Err(Self::error(address, width)),
        };
        *register = if width == 4 {
            value
        } else if offset & 2 == 0 {
            (*register & 0xFFFF) | (value << 16)
        } else {
            (*register & 0xFFFF_0000) | (value & 0xFFFF)
        };
        Ok(())
    }

    pub fn pending(&self) -> bool {
        self.pending
    }
    pub fn acknowledge(&mut self) {
        self.pending = false;
    }

    /// `instruction` is Some(before/after) only at an executed fetch boundary.
    /// An external cycle has only A26–A0, with no size/data qualification (§6.3.4).
    pub fn observe(
        &mut self,
        address: u32,
        width: usize,
        value: u32,
        write: bool,
        master: Master,
        instruction: Option<bool>,
    ) {
        let native = self.control & 0x1000 != 0;
        if master == Master::External && (!native || self.control & 0x2000 == 0) {
            return;
        }
        let previous_sequence = self.sequence;
        for channel in 0..if native { 2 } else { 1 } {
            let bbr = self.cycle[channel];
            let cpu = master == Master::Cpu;
            let fetch = instruction.is_some();
            if bbr & if cpu { 0x40 } else { 0x80 } == 0
                || bbr & if fetch { 0x10 } else { 0x20 } == 0
                || bbr & if write { 8 } else { 4 } == 0
            {
                continue;
            }
            if let Some(after) = instruction {
                let select = native && self.control & if channel == 0 { 0x400 } else { 4 } != 0;
                if after != select {
                    continue;
                }
            }
            let size = [0, 1, 2, 4][usize::from(bbr & 3)];
            if master != Master::External && size != 0 && size != width {
                continue;
            }
            let low_mask = if native && !fetch && master != Master::External {
                width as u32 - 1
            } else {
                0
            };
            if (address ^ self.address[channel]) & !(self.mask[channel] | low_mask) != 0 {
                continue;
            }
            if channel == 1 && self.control & 8 != 0 && !fetch && master != Master::External {
                let mask = match width {
                    1 => 0xFFFF,
                    2 => 0xFFFF,
                    _ => u32::MAX,
                };
                let data = if width == 1 {
                    (value & 255) * 0x101
                } else {
                    value
                };
                if (data ^ self.data) & !self.data_mask & mask != 0 {
                    continue;
                }
            }
            if native {
                self.control |= if channel == 0 {
                    if cpu { 0x8000 } else { 0x4000 }
                } else if cpu {
                    0x80
                } else {
                    0x40
                };
            }
            if channel == 0 {
                self.sequence = true;
            }
            if !native || self.control & 0x10 == 0 || (channel == 1 && previous_sequence) {
                self.pending = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_examples_mask_sizes_data_and_master_selection() {
        let mut u = Ubc::default();
        // H §6.3.6: word A512 written to any even address ABC00..ABCFE.
        for (offset, value) in [
            (0x20, 0xABCDE),
            (0x24, 255),
            (0x28, 0x6A),
            (0x30, 0xA512),
            (0x38, 0x1008),
        ] {
            u.write(BASE + offset, 4, value).unwrap();
        }
        for (address, width, value, write, master, matches) in [
            (0xABC00, 2, 0xA512, true, Master::Cpu, true),
            (0xABCFE, 2, 0xA512, true, Master::Cpu, true),
            (0xABC00, 2, 0xA513, true, Master::Cpu, false),
            (0xABC00, 2, 0xA512, false, Master::Cpu, false),
            (0xABD00, 2, 0xA512, true, Master::Cpu, false),
            (0xABC00, 4, 0xA512, true, Master::Cpu, false),
            (0xABC00, 2, 0xA512, true, Master::Dma, false),
        ] {
            u.acknowledge();
            u.observe(address, width, value, write, master, None);
            assert_eq!(u.pending(), matches);
        }
        assert_eq!(u.read(BASE + 0x38, 2), Ok(0x1088));
        u.write(BASE + 0x38, 2, 0x1008).unwrap();
        assert_eq!(u.read(BASE + 0x38, 4), Ok(0x1008));
    }
    #[test]
    fn sequence_requires_a_previous_cycle_and_flags_survive_acknowledgement() {
        let mut u = Ubc::default();
        for (offset, value) in [
            (0, 0x37226),
            (0x20, 0x3722E),
            (8, 0x56),
            (0x28, 0x56),
            (0x38, 0x1010),
        ] {
            u.write(BASE + offset, 4, value).unwrap();
        }
        u.observe(0x3722E, 2, 0, false, Master::Cpu, Some(false));
        assert!(!u.pending());
        u.observe(0x37226, 2, 0, false, Master::Cpu, Some(false));
        assert!(!u.pending());
        u.observe(0x3722E, 2, 0, false, Master::Cpu, Some(false));
        assert!(u.pending());
        u.acknowledge();
        assert_eq!(u.read(BASE + 0x38, 2), Ok(0x9090));
    }
    #[test]
    fn data_address_alignment_compatibility_and_external_bus_rules() {
        let mut u = Ubc::default();
        u.write(BASE, 4, 0x123457).unwrap();
        u.write(BASE + 8, 2, 0xE4).unwrap();
        u.observe(0x123454, 4, 0, false, Master::Cpu, None);
        assert!(!u.pending()); // Compatibility mode compares every address bit.
        u.write(BASE + 0x38, 2, 0x1000).unwrap();
        u.observe(0x123454, 4, 0, false, Master::Cpu, None);
        assert!(u.pending());
        u.acknowledge();
        u.observe(0x123457, 1, 0, false, Master::External, None);
        assert!(!u.pending());
        u.write(BASE + 0x38, 2, 0x3000).unwrap();
        u.observe(0x123457, 1, 0, false, Master::External, None);
        assert!(u.pending());
        assert_eq!(u.read(BASE + 0x38, 2), Ok(0x7000));
    }
    #[test]
    fn register_halfwords_and_reserved_bits_follow_table_six_one() {
        let mut u = Ubc::default();
        for offset in [0, 4, 0x20, 0x24, 0x30, 0x34] {
            u.write(BASE + offset, 2, 0x1234).unwrap();
            u.write(BASE + offset + 2, 2, 0x5678).unwrap();
            assert_eq!(u.read(BASE + offset, 4), Ok(0x12345678));
            assert_eq!(u.read(BASE + offset, 2), Ok(0x1234));
            assert_eq!(u.read(BASE + offset + 2, 2), Ok(0x5678));
        }
        u.write(BASE + 8, 4, u32::MAX).unwrap();
        assert_eq!(u.read(BASE + 8, 4), Ok(255));
        assert!(u.read(BASE + 10, 2).is_err());
    }
}
