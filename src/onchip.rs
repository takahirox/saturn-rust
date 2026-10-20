//! CPU-local SH7604 FRT and its INTC registers. Original implementation from
//! SH7604 §§5.3.2/5.3.6/5.3.7 and §§11.1–11.5; no external emulator core.
use crate::bus::BusError;
use std::cell::Cell;

pub const FRT: u32 = 0xFFFF_FE10;
pub const IPRB: u32 = 0xFFFF_FE60;
pub const VCRC: u32 = 0xFFFF_FE66;
pub const VCRD: u32 = 0xFFFF_FE68;

pub struct OnChip {
    tier: u8,
    status: u8,
    // Reads of FTCSR must precede a software flag clear (§11.2.5).
    observed: Cell<u8>,
    counter: u16,
    compare: [u16; 2],
    capture: u16,
    temp: Cell<u8>,
    control: u8,
    output_control: u8,
    outputs: [bool; 2],
    divider: u32,
    iprb: u16,
    vcrc: u16,
    vcrd: u16,
}

impl Default for OnChip {
    fn default() -> Self {
        Self {
            tier: 1,
            status: 0,
            observed: Cell::new(0),
            counter: 0,
            compare: [0xFFFF; 2],
            capture: 0,
            temp: Cell::new(0),
            control: 0,
            output_control: 0xE0,
            outputs: [false; 2],
            divider: 0,
            iprb: 0,
            vcrc: 0,
            vcrd: 0,
        }
    }
}

impl OnChip {
    fn unsupported(address: u32, width: usize) -> BusError {
        if (FRT..FRT + 10).contains(&address) || matches!(address & !1, IPRB | VCRC | VCRD) {
            BusError::UnsupportedDevice { address, width }
        } else {
            BusError::Unmapped { address, width }
        }
    }

    fn selected(&self) -> usize {
        usize::from(self.output_control & 0x10 != 0)
    }

    fn intc(&self, address: u32) -> Option<(u16, u16)> {
        match address {
            IPRB => Some((self.iprb, 0x0F00)), // SCI is still unsupported.
            VCRC => Some((self.vcrc, 0x7F7F)),
            VCRD => Some((self.vcrd, 0x7F00)),
            _ => None,
        }
    }

    pub fn read8(&self, address: u32) -> Result<u8, BusError> {
        if let Some((word, _)) = self.intc(address & !1) {
            return Ok(if address & 1 == 0 {
                (word >> 8) as u8
            } else {
                word as u8
            });
        }
        let high = |word: u16| {
            self.temp.set(word as u8);
            (word >> 8) as u8
        };
        Ok(match address.wrapping_sub(FRT) {
            0 => self.tier,
            1 => {
                self.observed.set(self.status & 0x8E);
                self.status
            }
            2 => high(self.counter),
            3 | 9 => self.temp.get(),
            4 => (self.compare[self.selected()] >> 8) as u8,
            5 => self.compare[self.selected()] as u8,
            6 => self.control,
            7 => self.output_control,
            8 => high(self.capture),
            _ => return Err(Self::unsupported(address, 1)),
        })
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if self.intc(address & !1).is_some() {
            let (old, _) = self.intc(address & !1).unwrap();
            let new = if address & 1 == 0 {
                (old & 0xFF) | (u16::from(value) << 8)
            } else {
                (old & 0xFF00) | u16::from(value)
            };
            return self.write16(address & !1, new);
        }
        match address.wrapping_sub(FRT) {
            0 => self.tier = (value & 0x8E) | 1,
            1 => {
                self.status &= !(self.observed.get() & !value);
                self.status = (self.status & 0x8E) | (value & 1);
                self.observed.set(0);
            }
            2 | 4 => self.temp.set(value),
            3 => self.counter = u16::from_be_bytes([self.temp.get(), value]),
            5 => self.compare[self.selected()] = u16::from_be_bytes([self.temp.get(), value]),
            6 => self.control = value & 0x83,
            7 => self.output_control = (value & 0x13) | 0xE0,
            _ => return Err(Self::unsupported(address, 1)),
        }
        Ok(())
    }

    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        self.intc(address)
            .map(|(v, _)| v)
            .ok_or(Self::unsupported(address, 2))
    }

    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        let (_, mask) = self.intc(address).ok_or(Self::unsupported(address, 2))?;
        // Reject SCI priority configuration instead of silently accepting a
        // nonfunctional serial interrupt controller.
        if address == IPRB && value & 0xF000 != 0 {
            return Err(Self::unsupported(address, 2));
        }
        let word = value & mask;
        match address {
            IPRB => self.iprb = word,
            VCRC => self.vcrc = word,
            VCRD => self.vcrd = word,
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Counts peripheral reference clocks even while the CPU is in SLEEP.
    /// Pin synchronization, access wait states and clock-switch glitches are
    /// outside this functional model (SH7604 §§11.4/11.7).
    pub fn advance_clocks(&mut self, clocks: u32) {
        let divisor = match self.control & 3 {
            0 => 8,
            1 => 32,
            2 => 128,
            _ => return,
        };
        let clocks = u64::from(self.divider) + u64::from(clocks);
        self.divider = (clocks % divisor) as u32;
        for _ in 0..clocks / divisor {
            self.increment();
        }
    }

    fn increment(&mut self) {
        // Compare the value held BEFORE this count edge. The match flags,
        // outputs and CCLRA clear take effect on the edge leaving that value
        // (SH7604 §§11.4.2/11.4.3/11.4.6, figs. 11.6/11.7/11.11).
        let a = self.counter == self.compare[0];
        let b = self.counter == self.compare[1];
        if a {
            self.status |= 8;
            self.outputs[0] = self.output_control & 2 != 0;
        }
        if b {
            self.status |= 4;
            self.outputs[1] = self.output_control & 1 != 0;
        }
        // OVF describes FFFF -> 0000, not a clear from any other count
        // (§§11.2.5/11.4.7, fig. 11.12). Both compares use the old value.
        if self.counter == 0xFFFF {
            self.status |= 2;
        }
        self.counter = if a && self.status & 1 != 0 {
            0
        } else {
            self.counter.wrapping_add(1)
        };
    }

    /// Saturn's 16-bit communication strobe contains both edges, so either
    /// IEDG selection captures. The strobe data has no data-transfer meaning.
    pub(crate) fn capture_strobe(&mut self) {
        self.capture = self.counter;
        self.status |= 0x80;
    }

    /// Level-sensitive until guest software reads/clears FTCSR. Exception
    /// acceptance itself does not acknowledge the peripheral flag (§11.5).
    pub fn interrupt(&self) -> Option<(u8, u8)> {
        let level = ((self.iprb >> 8) & 15) as u8;
        let pending = self.tier & self.status & 0x8E;
        if level == 0 || pending == 0 {
            return None;
        }
        let vector = if pending & 0x80 != 0 {
            self.vcrc >> 8
        } else if pending & 0x0C != 0 {
            self.vcrc & 0x7F
        } else {
            self.vcrd >> 8
        };
        Some((level, vector as u8))
    }

    pub(crate) fn can_request_interrupt(&self) -> bool {
        self.iprb & 0x0F00 != 0 && self.tier & 0x8E != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pair(chip: &mut OnChip, offset: u32, word: u16) {
        chip.write8(FRT + offset, (word >> 8) as u8).unwrap();
        chip.write8(FRT + offset + 1, word as u8).unwrap();
    }
    fn read_pair(chip: &OnChip, offset: u32) -> u16 {
        u16::from_be_bytes([
            chip.read8(FRT + offset).unwrap(),
            chip.read8(FRT + offset + 1).unwrap(),
        ])
    }

    #[test]
    fn reset_values_reserved_bits_widths_and_read_only_capture() {
        let mut chip = OnChip::default();
        assert_eq!(
            (chip.read8(FRT).unwrap(), chip.read8(FRT + 7).unwrap()),
            (1, 0xE0)
        );
        assert_eq!(read_pair(&chip, 2), 0);
        assert_eq!(read_pair(&chip, 4), 0xFFFF);
        assert_eq!(read_pair(&chip, 8), 0);
        assert!(chip.read16(FRT).is_err());
        assert!(chip.write16(FRT + 2, 0x1234).is_err());
        assert!(chip.write8(FRT + 8, 1).is_err());
        chip.write8(FRT, 0xFF).unwrap();
        chip.write8(FRT + 6, 0xFF).unwrap();
        chip.write8(FRT + 7, 0xFF).unwrap();
        assert_eq!(chip.read8(FRT).unwrap(), 0x8F);
        assert_eq!(chip.read8(FRT + 6).unwrap(), 0x83);
        assert_eq!(chip.read8(FRT + 7).unwrap(), 0xF3);
        chip.write16(VCRC, 0xFFFF).unwrap();
        chip.write16(VCRD, 0xFFFF).unwrap();
        assert_eq!(chip.read16(VCRC).unwrap(), 0x7F7F);
        assert_eq!(chip.read16(VCRD).unwrap(), 0x7F00);
        assert!(chip.write16(IPRB, 0xF000).is_err()); // Unsupported SCI priority.
        chip.write8(IPRB, 0x0B).unwrap();
        assert_eq!(chip.read16(IPRB).unwrap(), 0x0B00);
        assert!(matches!(
            chip.read16(IPRB + 1),
            Err(BusError::Unaligned { .. })
        ));
    }

    #[test]
    fn temp_register_latches_reads_and_commits_only_on_lower_byte_write() {
        let mut chip = OnChip::default();
        write_pair(&mut chip, 2, 0x12FF);
        assert_eq!(chip.read8(FRT + 2).unwrap(), 0x12);
        chip.advance_clocks(8);
        assert_eq!(chip.read8(FRT + 3).unwrap(), 0xFF); // Latch, not live 0x00.
        assert_eq!(read_pair(&chip, 2), 0x1300);
        chip.write8(FRT + 2, 0x56).unwrap();
        assert_eq!(chip.counter, 0x1300);
        chip.write8(FRT + 5, 0x78).unwrap(); // All pairs share TEMP.
        assert_eq!(read_pair(&chip, 4), 0x5678);
        chip.write8(FRT + 7, 0xF0).unwrap();
        assert_eq!(read_pair(&chip, 4), 0xFFFF); // OCRB independently reset.
        write_pair(&mut chip, 4, 0x2345);
        chip.write8(FRT + 7, 0xE0).unwrap();
        assert_eq!(read_pair(&chip, 4), 0x5678);
    }

    #[test]
    fn clock_divisors_split_advances_and_external_clock_selection() {
        for (select, divisor) in [(0, 8), (1, 32), (2, 128)] {
            let mut chip = OnChip::default();
            chip.write8(FRT + 6, select).unwrap();
            chip.advance_clocks(divisor - 1);
            assert_eq!(read_pair(&chip, 2), 0);
            chip.advance_clocks(1);
            assert_eq!(read_pair(&chip, 2), 1);
            chip.advance_clocks(divisor * 7);
            assert_eq!(read_pair(&chip, 2), 8);
        }
        let mut chip = OnChip::default();
        chip.write8(FRT + 6, 3).unwrap();
        chip.advance_clocks(10_000);
        assert_eq!(read_pair(&chip, 2), 0); // No external clock wire driven.
    }

    #[test]
    fn compare_flags_outputs_and_clear_use_the_value_before_the_count_edge() {
        let mut chip = OnChip::default();
        write_pair(&mut chip, 4, 3); // OCRA=3
        chip.write8(FRT + 7, 0xF3).unwrap();
        write_pair(&mut chip, 4, 2); // OCRB=2
        chip.write8(FRT + 1, 1).unwrap(); // CCLRA
        // SH7604 figs. 11.6/11.7/11.11: reaching N does not yet signal
        // a match; the next count edge signals it and optionally clears FRC.
        for (counter, status, outputs) in [
            (1, 1, [false, false]),
            (2, 1, [false, false]),
            (3, 5, [false, true]),
            (0, 13, [true, true]),
        ] {
            chip.advance_clocks(8);
            assert_eq!(
                (chip.counter, chip.status, chip.outputs),
                (counter, status, outputs)
            );
        }
        chip.read8(FRT + 1).unwrap();
        chip.write8(FRT + 1, 1).unwrap();
        chip.advance_clocks(24);
        assert_eq!((chip.counter, chip.status), (3, 5));
        chip.advance_clocks(8);
        assert_eq!((chip.counter, chip.status), (0, 13)); // Four-edge period.
    }

    #[test]
    fn compare_zero_and_overflow_boundary_vectors() {
        // Independently specified one-edge vectors from SH7604 §§11.2.5,
        // 11.4.3/11.4.6/11.4.7, figs. 11.7/11.11/11.12.
        for (initial, a, b, clear, counter, flags) in [
            (0, 0, 0, false, 1, 0x0C),
            (0, 0, 0, true, 0, 0x0C),
            (0xFFFE, 0xFFFF, 0xFFFF, false, 0xFFFF, 0),
            (0xFFFF, 0, 0, false, 0, 2),
            (0xFFFF, 0, 0, true, 0, 2),
            (0xFFFF, 0xFFFF, 0xFFFF, false, 0, 0x0E),
            (0xFFFF, 0xFFFF, 0xFFFF, true, 0, 0x0E),
            (3, 3, 0, true, 0, 8),    // Clearing to OCRB is not a B match.
            (3, 3, 3, true, 0, 0x0C), // A clear preserves simultaneous B.
        ] {
            let mut chip = OnChip::default();
            write_pair(&mut chip, 2, initial);
            write_pair(&mut chip, 4, a);
            chip.write8(FRT + 7, 0xF3).unwrap();
            write_pair(&mut chip, 4, b);
            chip.write8(FRT + 1, u8::from(clear)).unwrap();
            chip.advance_clocks(7);
            assert_eq!((chip.counter, chip.status), (initial, u8::from(clear)));
            chip.advance_clocks(1);
            assert_eq!(
                (chip.counter, chip.status, chip.outputs),
                (
                    counter,
                    flags | u8::from(clear),
                    [flags & 8 != 0, flags & 4 != 0]
                ),
                "FRC={initial:04X}, OCRA={a:04X}, OCRB={b:04X}, CCLRA={clear}"
            );
        }
        let mut chip = OnChip::default();
        write_pair(&mut chip, 4, 0);
        chip.write8(FRT + 1, 1).unwrap();
        for _ in 0..3 {
            chip.advance_clocks(8);
            assert_eq!((chip.counter, chip.read8(FRT + 1).unwrap()), (0, 9));
            chip.write8(FRT + 1, 1).unwrap(); // Compare=0 repeats every edge.
        }
    }

    #[test]
    fn read_then_zero_flag_acknowledgment_and_irq_priority_are_independent() {
        let mut chip = OnChip::default();
        chip.write8(FRT, 0x8F).unwrap();
        chip.write16(VCRC, 0x4546).unwrap();
        chip.write16(VCRD, 0x4700).unwrap();
        write_pair(&mut chip, 2, 0xFFFE);
        chip.advance_clocks(16); // Compare A/B at FFFF; overflow at 0000.
        chip.capture_strobe();
        assert_eq!(chip.interrupt(), None); // Priority zero masks all requests.
        chip.write16(IPRB, 0x0B00).unwrap();
        assert_eq!(chip.interrupt(), Some((11, 0x45)));
        chip.write8(FRT + 1, 0).unwrap(); // Without read, no flag may clear.
        assert_eq!(chip.interrupt(), Some((11, 0x45)));
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0x8E);
        chip.write8(FRT + 1, 0x0E).unwrap(); // Clear capture only.
        assert_eq!(chip.interrupt(), Some((11, 0x46)));
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0x0E);
        chip.write8(FRT + 1, 2).unwrap(); // Clear both compare flags.
        assert_eq!(chip.interrupt(), Some((11, 0x47)));
        chip.read8(FRT + 1).unwrap();
        chip.write8(FRT + 1, 0).unwrap();
        assert_eq!(chip.interrupt(), None);
        chip.write8(FRT + 1, 0x8E).unwrap(); // Software cannot set flags.
        assert_eq!(chip.read8(FRT + 1).unwrap(), 0);
    }
}
