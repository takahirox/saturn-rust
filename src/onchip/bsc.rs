//! SH7604 §§7.1.4, 7.2 and 7.5.7: keyed BSC registers and refresh timer.
//! Register addresses are the longword addresses; word reads use address + 2.
use crate::bus::BusError;
use std::cell::Cell;

pub const BSC: u32 = 0xFFFF_FFE0;
pub(super) struct Bsc {
    registers: [u16; 7],
    slave: bool,
    observed: Cell<bool>,
    phase: u32,
}
impl Default for Bsc {
    fn default() -> Self {
        Self {
            registers: [0x03F0, 0x00FC, 0xAAFF, 0, 0, 0, 0],
            slave: false,
            observed: Cell::new(false),
            phase: 0,
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
        // External memory-interface programming needs bus-cycle support. Do not
        // accept changed configurations as inert register storage. MASTER is RO.
        if i < 4 && word != self.registers[i] {
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
    pub fn interrupt(&self) -> bool {
        self.registers[4] & 0xC0 == 0xC0
    }
    pub fn can_request_interrupt(&self) -> bool {
        self.registers[4] & 0x40 != 0 && self.registers[4] & 0x38 != 0
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
