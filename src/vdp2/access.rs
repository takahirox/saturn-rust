//! ST-058-R2 §3.3, figs.3.2/3.6/3.7 and table3.5: CPU VRAM grants.
use super::Vdp2;

impl Vdp2 {
    fn access_command(&self, bank: usize, slot: usize) -> u16 {
        let word = self.word(0x10 + bank * 4 + slot / 4 * 2);
        word >> ((3 - slot % 4) * 4) & 15
    }
    fn cpu_command(&self, command: u16) -> bool {
        match command {
            0..=7 => self.word(0x20) & (1 << (command & 3)) == 0,
            12..=13 => self.word(0x20) & (1 << (command - 12)) == 0,
            14..=15 => true,
            _ => false,
        }
    }
    fn rotation_bank(&self, bank: usize) -> bool {
        // RBG1 owns B0/B1 independently of the programmable cycle words.
        (self.word(0x20) & 0x20 != 0 && bank >= 2)
            || (self.word(0x20) & 0x10 != 0 && self.word(0xE) >> (bank * 2) & 3 != 0)
    }
    /// Each bit grants one access timing. On split VRAM both halves share
    /// the CPU port: a slot in only the addressed half cannot grant a cycle.
    pub(crate) fn cpu_access_slots(&self, address: u32) -> u8 {
        if self.word(0) & 0x8000 == 0 {
            return 255;
        }
        let side = ((address as usize >> 18) & 1) * 2;
        let split = self.word(0xE) & (0x100 << (side / 2)) != 0;
        let banks = if split { 2 } else { 1 };
        if (side..side + banks).any(|bank| self.rotation_bank(bank)) {
            return 0;
        }
        let slots = if self.word(0) & 6 == 0 { 8 } else { 4 };
        let mut available = 0u8;
        let mut idle = 0u8;
        for slot in 0..slots {
            if (side..side + banks).all(|bank| self.cpu_command(self.access_command(bank, slot))) {
                available |= 1 << slot;
            }
            if (side..side + banks).all(|bank| self.access_command(bank, slot) == 15) {
                idle |= 1 << slot;
            }
        }
        if !split || available == ((1u16 << slots) - 1) as u8 {
            return available;
        }
        // A chain of CPU slots begins after a no-access timing in both
        // halves. Subsequent linked CPU timings need no additional gap.
        let mut granted = 0;
        for start in 0..slots {
            if idle & (1 << start) == 0 {
                continue;
            }
            for distance in 1..=slots {
                let slot = (start + distance) % slots;
                if available & (1 << slot) == 0 {
                    break;
                }
                granted |= 1 << slot;
            }
        }
        granted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::VDP2;
    fn reg(v: &mut Vdp2, at: u32, value: u16) {
        v.write16(VDP2 + at, value).unwrap();
    }
    fn patterns(v: &mut Vdp2, bank: usize, lower: u16, upper: u16) {
        reg(v, 0x10 + bank as u32 * 4, lower);
        reg(v, 0x12 + bank as u32 * 4, upper);
    }
    #[test]
    fn every_cycle_word_controls_its_own_bank_and_unused_layers_release_slots() {
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 15);
        reg(&mut v, 0xE, 0x300);
        for bank in 0..4 {
            patterns(&mut v, bank, 0x0123, 0x4567);
        }
        for command in 0..16 {
            if (8..12).contains(&command) {
                continue;
            }
            for bank in 0..4 {
                patterns(&mut v, bank, 0xF000 | command << 8, 0);
            }
            let granted = v.cpu_access_slots(0x05E00000);
            assert_eq!(granted & 2 != 0, command >= 14);
            reg(&mut v, 0x20, 0);
            assert_eq!(v.cpu_access_slots(0x05E00000), 255);
            reg(&mut v, 0x20, 15);
        }
    }
    #[test]
    fn split_banks_require_matching_slots_and_an_idle_lead_but_allow_linked_grants() {
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 1);
        reg(&mut v, 0xE, 0x300);
        patterns(&mut v, 0, 0x00FE, 0xE000);
        patterns(&mut v, 1, 0x00FE, 0xE000);
        assert_eq!(v.cpu_access_slots(0x05E00000), 0b00011000);
        patterns(&mut v, 1, 0x000E, 0x0000); // no common lead
        assert_eq!(v.cpu_access_slots(0x05E20000), 0);
        patterns(&mut v, 2, 0x00FE, 0xE000);
        patterns(&mut v, 3, 0x00FE, 0xE000);
        assert_eq!(v.cpu_access_slots(0x05E40000), 0b00011000);
        reg(&mut v, 0, 0x8002); // T4–T7 no longer participate
        assert_eq!(v.cpu_access_slots(0x05E40000), 0b00001000);
        reg(&mut v, 0xE, 0); // A1/B1 patterns are ignored when not split
        assert_eq!(v.cpu_access_slots(0x05E20000), 0b00001100);
    }
    #[test]
    fn rotation_ownership_blanking_and_display_disable_override_cycle_patterns() {
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 0x10);
        reg(&mut v, 0xE, 0x300 | 3);
        assert_eq!(v.cpu_access_slots(0x05E00000), 0);
        assert_eq!(v.cpu_access_slots(0x05E40000), 255);
        reg(&mut v, 0x20, 0x20);
        assert_eq!(v.cpu_access_slots(0x05E40000), 0);
        // Raster blanking releases this mask at the scheduled access time,
        // including when earlier beats already queued a wait into HBlank.
        reg(&mut v, 0, 0);
        assert_eq!(v.cpu_access_slots(0x05E40000), 255);
    }
}
