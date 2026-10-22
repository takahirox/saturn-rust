//! ST-058-R2 §3.3, figs.3.2–3.7 and tables3.2–3.5: display/CPU VRAM grants.
use super::Vdp2;

impl Vdp2 {
    fn display_bank(&self, address: usize) -> usize {
        let bank = (address & 0x7FFFF) >> 17;
        if self.word(0xE) & (0x100 << (bank / 2)) == 0 {
            bank & !1
        } else {
            bank
        }
    }
    fn command_slots(&self, bank: usize, command: u16) -> u8 {
        if self.rotation_bank(bank) {
            return 0;
        }
        let count = if self.word(0) & 6 == 0 { 8 } else { 4 };
        (0..count).fold(0, |mask, slot| {
            mask | (u8::from(self.access_command(bank, slot) == command) << slot)
        })
    }
    fn reduction(&self, layer: usize) -> u32 {
        if layer >= 2 {
            return 1;
        }
        match self.word(0x98) >> (layer * 8) & 3 {
            1 => 2,
            3 => 4,
            _ => 1,
        }
    }
    /// ST-058-R2 §3.3 table3.2: the addressed name bank needs
    /// 1/2/4 reads at x1/x2/x4 reduction.
    fn name_slots(&self, layer: usize, address: usize) -> u8 {
        let bank = self.display_bank(address);
        let slots = self.command_slots(bank, layer as u16);
        if slots.count_ones() < self.reduction(layer) {
            return 0;
        }
        slots
    }
    pub(super) fn name_access(&self, layer: usize, address: usize, rotation: bool) -> bool {
        if rotation {
            return self.rotation_access(layer, address, 2);
        }
        if !self.beam_rendering {
            return true;
        }
        self.name_slots(layer, address) != 0
    }
    /// ST-058-R2 §6.2: rotation owns an entire bank, independent of
    /// the normal-screen cycle commands. RBG1's two banks are hardwired.
    pub(super) fn rotation_access(&self, layer: usize, address: usize, role: u16) -> bool {
        if layer == 0 {
            return (address & 0x60000) == if role == 2 { 0x60000 } else { 0x40000 };
        }
        if !self.beam_rendering {
            return true;
        }
        let bank = self.display_bank(address);
        !(self.word(0x20) & 0x20 != 0 && bank >= 2) && self.word(0xE) >> (bank * 2) & 3 == role
    }
    /// ST-058-R2 tables3.3/3.4: each format consumes 1/2/4/4/8
    /// reads per eight source dots. Name-to-character timing excludes
    /// the unavailable pipeline slots, including in four-timing modes.
    pub(super) fn character_access(
        &self,
        layer: usize,
        address: usize,
        format: u16,
        name: Option<usize>,
        rotation: bool,
    ) -> bool {
        if rotation {
            return self.rotation_access(layer, address, 3);
        }
        if !self.beam_rendering {
            return true;
        }
        let slots = self.command_slots(self.display_bank(address), 4 + layer as u16);
        let reads = [1u32, 2, 4, 4, 8][usize::from(format)];
        let reduction = self.reduction(layer);
        if slots.count_ones() < reads * reduction {
            return false;
        }
        let Some(name) = name else {
            return true;
        };
        let names = self.name_slots(layer, name);
        if names == 0 {
            return false;
        }
        let permitted: [u8; 8] = if self.word(0) & 6 == 0 {
            [0xF7, 0xEF, 0xCF, 0x8F, 0x0F, 0x0E, 0x0C, 0x08]
        } else {
            [0x07, 0x0E, 0x0D, 0x0B, 0, 0, 0, 0]
        };
        // Each required name fetch must feed enough character reads.
        (0..8)
            .filter(|slot| names & (1 << slot) != 0)
            .all(|slot| (slots & permitted[slot]).count_ones() >= reads)
    }
    pub(super) fn cell_access(&self, layer: usize, address: usize) -> bool {
        if !self.beam_rendering {
            return true;
        }
        let bank = self.display_bank(address);
        let slots = self.command_slots(bank, 12 + layer as u16);
        if layer == 0 {
            slots & 3 != 0
        } else {
            let other = self.command_slots(bank, 12) & 3;
            slots & 7 != 0
                && (self.word(0x9A) & 1 == 0
                    || (0..3).any(|s| slots & (1 << s) != 0 && other & ((1 << s) - 1) != 0))
        }
    }
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
    fn display_formats_consume_the_authored_number_of_reads_in_the_addressed_bank() {
        for mode in [0x8000, 0x8002, 0x8004] {
            for layer in 0..4 {
                for (format, required) in [1, 2, 4, 4, 8].into_iter().enumerate() {
                    for bank in 0..4 {
                        for count in 0..=8 {
                            let mut v = Vdp2::default();
                            reg(&mut v, 0, mode);
                            reg(&mut v, 0xE, 0x300);
                            reg(&mut v, 0x20, 1 << layer);
                            let mut commands = [15u16; 8];
                            commands[..count].fill(4 + layer as u16);
                            let word = |slice: &[u16]| slice.iter().fold(0, |n, c| n << 4 | c);
                            patterns(&mut v, bank, word(&commands[..4]), word(&commands[4..]));
                            v.beam_rendering = true;
                            for addressed in 0..4 {
                                assert_eq!(
                                    v.character_access(
                                        layer,
                                        addressed << 17,
                                        format as u16,
                                        None,
                                        false
                                    ),
                                    addressed == bank
                                        && count.min(if mode == 0x8000 { 8 } else { 4 })
                                            >= required,
                                    "mode={mode:X}, layer={layer}, format={format}, count={count}, bank={bank}->{addressed}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn unpartitioned_display_uses_bank_zero_words_and_rotation_overrides_only_selected_banks() {
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 0x11);
        patterns(&mut v, 0, 0x4444, 0xFFFF);
        patterns(&mut v, 1, 0xFFFF, 0xFFFF);
        patterns(&mut v, 2, 0xFFFF, 0xFFFF);
        patterns(&mut v, 3, 0x4444, 0xFFFF);
        v.beam_rendering = true;
        assert!(v.character_access(0, 0x20000, 3, None, false));
        assert!(!v.character_access(0, 0x60000, 3, None, false));
        reg(&mut v, 0xE, 0x300);
        assert!(!v.character_access(0, 0x20000, 3, None, false));
        assert!(v.character_access(0, 0x60000, 3, None, false));
        for bank in 0..4 {
            for role in 0..4 {
                reg(&mut v, 0xE, 0x300 | role << (bank * 2));
                assert_eq!(v.rotation_access(4, bank << 17, 2), role == 2);
                assert_eq!(v.rotation_access(4, bank << 17, 3), role == 3);
                assert_eq!(
                    v.character_access(0, bank << 17, 3, None, false),
                    role == 0 && (bank == 0 || bank == 3)
                );
            }
        }
    }
    #[test]
    fn name_to_character_pipeline_uses_table_three_four_and_reduction_enable() {
        for (mode, allowed) in [
            (0x8000, [0xF7u8, 0xEF, 0xCF, 0x8F, 0x0F, 0x0E, 0x0C, 0x08]),
            (0x8002, [0x07, 0x0E, 0x0D, 0x0B, 0, 0, 0, 0]),
        ] {
            for (name, &mask) in allowed
                .iter()
                .enumerate()
                .take(if mode == 0x8000 { 8 } else { 4 })
            {
                for character in 0..8 {
                    let mut v = Vdp2::default();
                    reg(&mut v, 0, mode);
                    reg(&mut v, 0x20, 1);
                    reg(&mut v, 0xE, 0x300);
                    let name_pattern = !(15u32 << ((7 - name) * 4));
                    let char_pattern = !(11u32 << ((7 - character) * 4));
                    patterns(&mut v, 0, (name_pattern >> 16) as u16, name_pattern as u16);
                    patterns(&mut v, 2, (char_pattern >> 16) as u16, char_pattern as u16);
                    v.beam_rendering = true;
                    assert_eq!(
                        v.character_access(0, 0x40000, 0, Some(0), false),
                        mask & (1 << character) != 0
                    );
                }
            }
        }
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 1);
        patterns(&mut v, 0, 0x00FF, 0x4444);
        v.beam_rendering = true;
        reg(&mut v, 0x98, 1); // x1/2, two names and two 4-bit character reads
        assert!(v.character_access(0, 0, 0, Some(0), false));
        reg(&mut v, 0x98, 3); // x1/4 requires four names, not just more character slots
        assert!(!v.character_access(0, 0, 0, Some(0), false));
        patterns(&mut v, 0, 0x0000, 0x4444);
        assert!(v.character_access(0, 0, 0, Some(0), false));
    }
    #[test]
    fn cell_scroll_ports_require_early_slots_and_nbg0_before_nbg1_in_the_same_bank() {
        let mut v = Vdp2::default();
        reg(&mut v, 0, 0x8000);
        reg(&mut v, 0x20, 3);
        reg(&mut v, 0xE, 0x300);
        reg(&mut v, 0x9A, 0x101);
        v.beam_rendering = true;
        for (pattern, n0, n1) in [
            (0xCDFF, true, true),
            (0xDCFF, true, false),
            (0xFFCD, false, false),
            (0xFCDF, true, true),
            (0xFDCF, false, false),
        ] {
            patterns(&mut v, 0, pattern, 0xFFFF);
            assert_eq!(v.cell_access(0, 0), n0, "{pattern:X}");
            assert_eq!(v.cell_access(1, 0), n1, "{pattern:X}");
        }
        patterns(&mut v, 0, 0xCFFF, 0xFFFF);
        patterns(&mut v, 1, 0xFDFF, 0xFFFF);
        assert!(!v.cell_access(1, 0x20000));
        reg(&mut v, 0x9A, 0x100);
        assert!(v.cell_access(1, 0x20000)); // NBG0 no longer participates
    }
    #[test]
    fn beam_pixels_disappear_for_starved_or_misbanked_fetches_and_recover_on_grant() {
        use crate::vdp1::Vdp1;
        let mut v = Vdp2::default();
        let s = Vdp1::default();
        for (at, value) in [
            (0, 0x8000),
            (0x20, 0x101),
            (0x28, 0x32),
            (0xF8, 1),
            (0xAC, 3),
        ] {
            reg(&mut v, at, value);
        }
        v.vram[..2].copy_from_slice(&0x801Fu16.to_be_bytes());
        for (line, a, b, expected) in [
            (0, 0x4444, 0xFFFF, [255, 0, 0, 255]),
            (1, 0x4FFF, 0xFFFF, [0, 0, 0, 255]),
            (2, 0xFFFF, 0x4444, [0, 0, 0, 255]),
            (3, 0x4444, 0xFFFF, [255, 0, 0, 255]),
        ] {
            patterns(&mut v, 0, a, 0xFFFF);
            patterns(&mut v, 2, b, 0xFFFF);
            v.begin_scanline(line);
            v.scan_to(&s, line, 8);
            assert_eq!(&v.scanning[(line * 320) * 4..][..4], expected);
        }
        // Rotation claims all slots even if its cycle words request CPU/NBG.
        reg(&mut v, 0x20, 0x1111);
        reg(&mut v, 0x2A, 0x3200);
        reg(&mut v, 0xFC, 2);
        reg(&mut v, 0xE, 3);
        v.beam_rendering = true;
        assert!(!v.character_access(0, 0, 3, None, false));
        assert!(v.rotation_access(4, 0, 3));
        assert!(!v.rotation_access(4, 0x40000, 3));
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
