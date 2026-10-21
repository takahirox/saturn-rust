//! SH7604 §§7.1.4, 7.2 and 7.5.7: keyed BSC registers and refresh timer.
//! Register addresses are the longword addresses; word reads use address + 2.
use crate::bus::BusError;
use std::cell::Cell;

pub const BSC: u32 = 0xFFFF_FFE0;
#[derive(Clone, Copy)]
struct Access {
    area: u32,
    read: bool,
    busy: u32,
    gap: u32,
}
pub(super) struct Bsc {
    registers: [u16; 7],
    slave: bool,
    observed: Cell<bool>,
    phase: u32,
    access: Cell<Option<Access>>,
}
impl Default for Bsc {
    fn default() -> Self {
        Self {
            registers: [0x03F0, 0x00FC, 0xAAFF, 0, 0, 0, 0],
            slave: false,
            observed: Cell::new(false),
            phase: 0,
            access: Cell::new(None),
        }
    }
}
impl Bsc {
    pub fn set_slave(&mut self, slave: bool) {
        self.slave = slave;
    }
    pub fn read(&self, address: u32, width: usize) -> Result<u32, BusError> {
        let offset = address.wrapping_sub(BSC);
        if offset >= 0x1C {
            return Err(BusError::Unmapped { address, width });
        }
        if offset > 0x1A || !matches!((width, offset & 3), (4, 0) | (2, 2)) {
            return Err(BusError::UnsupportedDevice { address, width });
        }
        let i = (offset / 4) as usize;
        let mut value = self.registers[i];
        if i == 0 && self.slave {
            value |= 0x8000;
        }
        if i == 4 {
            self.observed.set(value & 0x80 != 0);
        }
        Ok(u32::from(value))
    }
    pub fn write(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        let offset = address.wrapping_sub(BSC);
        if offset >= 0x1C {
            return Err(BusError::Unmapped { address, width: 4 });
        }
        if offset > 0x18 || offset & 3 != 0 {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        // Invalid key has no effect, including on the read-before-clear latch.
        if value >> 16 != 0xA55A {
            return Ok(());
        }
        let i = (offset / 4) as usize;
        let mask = [0x1FF7, 0x00FC, 0xFFFF, 0xFEFC, 0x00F8, 0x00FF, 0x00FF][i];
        let mut word = value as u16 & mask;
        // Ordinary-space software waits have real external-access effects.
        // Keep other interface changes explicit until their bus protocols exist:
        // BCR1 memory/arbitration and MCR remain separate bus protocols.
        let implemented = [0x13F0, 0x00FC, 0xFFFF, 0][i.min(3)];
        if i < 4 && (word ^ self.registers[i]) & !implemented != 0 {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 1 && [2, 4, 6].iter().any(|shift| word >> shift & 3 == 0) {
            // H §7.2.2: 00 is reserved, not an alternate bus width.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 2 && [8, 10, 12, 14].iter().any(|shift| word >> shift & 3 == 3) {
            // H §7.2.3: IWn=11 is reserved, not three idle cycles.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        let bcr1 = if i == 0 { word } else { self.registers[0] };
        let bcr2 = if i == 1 { word } else { self.registers[1] };
        if i <= 1 && bcr1 & 0x1000 != 0 && bcr2 >> 4 & 3 == 1 {
            // H §7.3.2: little-endian mode does not support an 8-bit bus.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 4 {
            let flag = self.registers[4] & 0x80;
            // CMF cannot be set by software, and clears only after a flagged read.
            word = (word & !0x80)
                | (flag
                    & if self.observed.replace(false) {
                        word
                    } else {
                        0x80
                    });
        }
        self.registers[i] = word;
        Ok(())
    }
    /// H §§7.2.1/7.2.3/7.4.2, fig.7.11: inserted Tw clocks per
    /// ordinary-space bus transaction, independently for CS0-CS3. These are
    /// additional waits, not a claim about instruction/pipeline base cycles.
    pub fn software_waits(&self, address: u32) -> u32 {
        if address >= 0x0800_0000 {
            return 0;
        }
        let area = address >> 25;
        let select = (self.registers[2] >> (area * 2)) & 3;
        if select != 3 {
            u32::from(select)
        } else {
            let shift = match area {
                0 => 4,
                1 => 6,
                _ => 8,
            };
            3 + u32::from((self.registers[0] >> shift) & 3)
        }
    }
    /// H §7.9, fig.7.47: the preceding READ's area selects IWn.
    /// Writes never require turnaround on their own. An immediate read/write
    /// pair needs at least one cycle even with IWn=0. Existing empty bus cycles
    /// satisfy the requirement. Only actual external beats enter this history.
    pub fn access_waits(&self, address: u32, write: bool) -> u32 {
        if address >= 0x0800_0000 {
            return 0;
        }
        let area = address >> 25;
        let prior = self.access.get();
        let idle = prior.map_or(0, |last| {
            if last.read && (last.area != area || write) {
                let specified = u32::from(self.registers[2] >> (8 + last.area * 2) & 3);
                specified.max(u32::from(write)).saturating_sub(last.gap)
            } else {
                0
            }
        });
        let waits = self.software_waits(address) + idle;
        self.access.set(Some(Access {
            area,
            read: !write,
            // H fig.7.7: ordinary beats occupy T1/T2 plus waits. The
            // interpreter accounts for base cycles separately; keep their bus
            // occupancy here so time spent transferring is never empty time.
            busy: prior.map_or(0, |last| last.busy).saturating_add(2 + waits),
            gap: 0,
        }));
        waits
    }
    pub fn advance_bus(&self, clocks: u32) {
        if let Some(mut last) = self.access.get() {
            let empty = clocks.saturating_sub(last.busy);
            last.busy = last.busy.saturating_sub(clocks);
            last.gap = last.gap.saturating_add(empty).min(2);
            self.access.set(Some(last));
        }
    }
    pub fn reset_bus(&self) {
        self.access.set(None);
    }
    /// H §§7.2.2/7.3.1: ordinary CS1-CS3 narrow buses divide larger
    /// operand/refill requests into separate external byte/word transactions.
    /// CS0 retains this board model's existing 32-bit external adapter.
    pub fn bus_width(&self, address: u32) -> usize {
        match address >> 25 {
            area @ 1..=3 => 1 << (((self.registers[1] >> (area * 2)) & 3) - 1),
            _ => 4,
        }
    }
    pub fn little_endian(&self, address: u32) -> bool {
        address >> 25 == 2 && self.registers[0] & 0x1000 != 0
    }
    pub fn interrupt(&self) -> bool {
        self.registers[4] & 0xC0 == 0xC0
    }
    pub fn can_request_interrupt(&self) -> bool {
        self.registers[4] & 0x40 != 0 && self.registers[4] & 0x38 != 0
    }
    pub fn clocks_to_event(&self) -> u32 {
        let divisor =
            [0, 4, 16, 64, 256, 1024, 2048, 4096][((self.registers[4] >> 3) & 7) as usize];
        if divisor == 0 {
            return u32::MAX;
        }
        let ticks = self.registers[6].wrapping_sub(self.registers[5]) & 255;
        (u32::from(if ticks == 0 { 256 } else { ticks }) * divisor)
            .saturating_sub(self.phase)
            .max(1)
    }
    pub fn advance(&mut self, clocks: u32) {
        let select = (self.registers[4] >> 3) as usize & 7;
        let divisor = [0, 4, 16, 64, 256, 1024, 2048, 4096][select];
        if divisor == 0 {
            return;
        }
        let total = u64::from(self.phase) + u64::from(clocks);
        self.phase = (total % divisor) as u32;
        for _ in 0..total / divisor {
            // §7.5.7: RTCOR-1 causes refresh on the next count edge.
            self.registers[5] = (self.registers[5] + 1) & 255;
            if self.registers[5] == self.registers[6] {
                self.registers[5] = 0;
                self.registers[4] |= 0x80;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyed_registers_processor_pins_and_word_lanes() {
        for slave in [false, true] {
            let mut b = Bsc::default();
            b.set_slave(slave);
            assert_eq!(
                b.read(BSC, 4).unwrap(),
                0x3F0 | if slave { 0x8000 } else { 0 }
            );
            for (i, reset) in [0x3F0, 0xFC, 0xAAFF, 0, 0, 0, 0].into_iter().enumerate() {
                let a = BSC + i as u32 * 4;
                let pin = if slave && i == 0 { 0x8000 } else { 0 };
                assert_eq!(b.read(a + 2, 2), Ok(reset | pin));
                b.write(a, 0xFFFF).unwrap();
                assert_eq!(b.read(a, 4), Ok(reset | pin));
                assert!(b.read(a, 2).is_err());
                assert!(b.read(a + 1, 1).is_err());
            }
            b.write(BSC, 0xA55A_83F0).unwrap();
            assert_eq!(b.read(BSC, 4), Ok(0x3F0 | if slave { 0x8000 } else { 0 }));
            assert!(b.write(BSC, 0xA55A_FFFF).is_err());
        }
    }
    #[test]
    fn ordinary_software_waits_decode_every_area_and_long_wait_selection() {
        // H §7.2.3: W=00/01/10 -> 0/1/2 Tw; W=11 selects
        // 3/4/5/6 Tw in the appropriate BCR1 AnLW field.
        for area in 0..4 {
            let mut b = Bsc::default();
            let address = area << 25;
            assert_eq!(b.software_waits(address), 6);
            for select in 0..3 {
                b.write(BSC + 8, 0xA55A_AA00 | select << (area * 2))
                    .unwrap();
                assert_eq!(b.software_waits(address), select);
                for other in 0..4 {
                    if other != area {
                        assert_eq!(b.software_waits(other << 25), 0);
                    }
                }
            }
            b.write(BSC + 8, 0xA55A_AAFF).unwrap();
            let shift = [4, 6, 8, 8][area as usize];
            for long in 0..4 {
                let word = (0x3F0 & !(3 << shift)) | long << shift;
                b.write(BSC, 0xA55A_0000 | word).unwrap();
                assert_eq!(b.software_waits(address), 3 + long);
                // Areas 2 and 3 share AHLW, unlike their WCR fields.
                if area >= 2 {
                    assert_eq!(b.software_waits((5 - area) << 25), 3 + long);
                }
            }
            assert_eq!(b.software_waits(0x0800_0000), 0);
        }
    }
    #[test]
    fn unsupported_interface_changes_and_wrong_keys_are_atomic() {
        let mut b = Bsc::default();
        b.write(BSC, 0xA55A_0120).unwrap();
        b.write(BSC + 8, 0xA55A_AA39).unwrap();
        b.write(BSC, 0x1234_03F0).unwrap();
        b.write(BSC + 8, 0x1234_AAFF).unwrap();
        assert_eq!(b.read(BSC, 4), Ok(0x120));
        assert_eq!(b.read(BSC + 8, 4), Ok(0xAA39));
        for (address, value) in [
            (BSC, 0xA55A_0920),      // burst ROM
            (BSC, 0xA55A_0121),      // SDRAM
            (BSC + 4, 0xA55A_00F0),  // reserved bus width
            (BSC + 8, 0xA55A_AB39),  // reserved idle encoding
            (BSC + 12, 0xA55A_8000), // memory-interface timing
        ] {
            assert!(b.write(address, value).is_err());
        }
        assert_eq!(b.read(BSC, 4), Ok(0x120));
        assert_eq!(b.read(BSC + 8, 4), Ok(0xAA39));
    }
    #[test]
    fn turnaround_uses_prior_read_area_minimum_write_gap_and_existing_empty_cycles() {
        // H §§7.2.3/7.9, fig.7.47: independent direction/CS/IW/gap
        // cross-product, with zero software waits to isolate turnaround.
        for prior_area in 0..4 {
            for next_area in 0..4 {
                for prior_write in [false, true] {
                    for next_write in [false, true] {
                        for idle in 0u32..=2 {
                            for empty in 0u32..=4 {
                                let mut b = Bsc::default();
                                let wcr = (0xAA00 & !(3 << (8 + prior_area * 2)))
                                    | idle << (8 + prior_area * 2);
                                b.write(BSC + 8, 0xA55A_0000 | wcr).unwrap();
                                assert_eq!(b.access_waits(prior_area << 25, prior_write), 0);
                                b.advance_bus(2 + empty); // T1/T2, then empty cycles.
                                let required =
                                    if !prior_write && (prior_area != next_area || next_write) {
                                        idle.max(u32::from(next_write))
                                    } else {
                                        0
                                    };
                                assert_eq!(
                                    b.access_waits(next_area << 25, next_write),
                                    required.saturating_sub(empty),
                                    "CS{prior_area} write={prior_write} -> CS{next_area} write={next_write}, IW={idle}, empty={empty}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn software_waits_and_prior_beat_occupancy_are_not_empty_time_and_reset_drops_history() {
        for chunk in [1, 2, 3, 8] {
            let b = Bsc::default();
            assert_eq!(b.access_waits(0, false), 6);
            let mut clocks = 8; // T1/T2 plus six Tw; still no empty cycle.
            while clocks != 0 {
                let n = clocks.min(chunk);
                b.advance_bus(n);
                clocks -= n;
            }
            assert_eq!(b.access_waits(0x0600_0000, false), 8);
            // Two back-to-back beats queue their full occupancy.
            assert_eq!(b.access_waits(0x0600_0004, true), 8);
            b.advance_bus(19); // 10+10 clocks queued; one busy clock remains.
            assert_eq!(b.access_waits(0, false), 6); // write->read, no idle
            b.reset_bus();
            assert_eq!(b.access_waits(0x0600_0000, true), 6);
        }
    }
    #[test]
    fn ordinary_bus_widths_are_independent_and_reserved_selections_do_not_publish() {
        let mut b = Bsc::default();
        for area in 1..=3 {
            for (select, width) in [(1, 1), (2, 2), (3, 4)] {
                b.write(
                    BSC + 4,
                    0xA55A_0000 | (0xFC & !(3 << (area * 2))) | select << (area * 2),
                )
                .unwrap();
                assert_eq!(b.bus_width(area << 25), width);
                for other in 1..=3 {
                    if area != other {
                        assert_eq!(b.bus_width(other << 25), 4);
                    }
                }
            }
            assert!(
                b.write(BSC + 4, 0xA55A_0000 | (0xFC & !(3 << (area * 2))))
                    .is_err()
            );
            assert_eq!(b.read(BSC + 4, 4), Ok(0xFC));
        }
    }
    #[test]
    fn endian_is_cs2_only_and_byte_bus_combinations_are_rejected_in_either_order() {
        let mut b = Bsc::default();
        b.write(BSC, 0xA55A_13F0).unwrap();
        for area in 0..4 {
            assert_eq!(b.little_endian(area << 25), area == 2);
        }
        assert!(b.write(BSC + 4, 0xA55A_00DC).is_err());
        assert_eq!(b.read(BSC + 4, 4), Ok(0xFC));
        b.write(BSC, 0xA55A_03F0).unwrap();
        b.write(BSC + 4, 0xA55A_00DC).unwrap();
        assert!(b.write(BSC, 0xA55A_13F0).is_err());
        assert_eq!(b.read(BSC, 4), Ok(0x3F0));
    }
    #[test]
    fn refresh_compare_interrupt_and_read_before_clear_across_split_clocks() {
        for chunk in [1, 3, 4, 7, 32] {
            let mut b = Bsc::default();
            b.write(BSC + 24, 0xA55A_0002).unwrap();
            b.write(BSC + 16, 0xA55A_0048).unwrap(); // /4, CMIE
            let mut left = 7;
            while left != 0 {
                let n = left.min(chunk);
                b.advance(n);
                left -= n;
            }
            assert!(!b.interrupt());
            b.advance(1);
            assert!(b.interrupt());
            b.write(BSC + 16, 0xA55A_0048).unwrap();
            assert!(b.interrupt()); // not read yet
            assert_eq!(b.read(BSC + 16, 4), Ok(0xC8));
            b.write(BSC + 16, 0x1234_0048).unwrap(); // wrong key cannot consume latch
            b.write(BSC + 16, 0xA55A_0048).unwrap();
            assert!(!b.interrupt());
        }
    }
}
