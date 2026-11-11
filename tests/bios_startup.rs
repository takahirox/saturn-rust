//! Original guest checks for private-BIOS initialization findings. No Sega bytes.
use saturn_rust::{
    Boot, Machine,
    bus::{Bus, Memory},
    devices::VDP2,
};

#[test]
fn initialization_guest_observes_ist_clear_reassertion_and_vdp2_registers() {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/bios-startup.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    for chunk in [1, 127, 10000] {
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            for i in (0..30000).step_by(chunk as usize) {
                m.run_steps(chunk.min(30000 - i)).unwrap();
            }
            let records: Vec<_> = (0..25)
                .map(|i| m.bus().read32(0x0600d000 + i * 4).unwrap())
                .collect();
            assert_eq!(
                records,
                [
                    4, 4, 0, 4, 0, 0, 0, 0, 0x1a, 0x40, 0xc0, 0x20, 0x24, 0, 0x12345678,
                    0x9abcdef0, 0x800, 5, 10, 19, 0x12345678, 0x5ab8, 0, 0, 0x600d
                ]
            );
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.sound_state().exceptions, 0);
            assert!(!m.sound_state().halted);
        }
    }
}

#[test]
fn vram_size_version_reserved_words_and_neighboring_restrictions() {
    let mut b = Bus::default();
    for base in [VDP2, VDP2 + 0x20000000] {
        assert_eq!(b.read16(base + 6), Ok(0));
        b.write16(base + 0xE, 0x200).unwrap();
        for value in [0, 15, 0x1234, 0x7fff] {
            b.write16(base + 6, value).unwrap();
            assert_eq!(b.read16(base + 6), Ok(0));
        }
        for offset in [0xC, 0xFE] {
            for value in [0, 0x1234, 0xffff] {
                b.write16(base + offset, value).unwrap();
                assert_eq!(b.read16(base + offset), Ok(0));
            }
        }
        assert_eq!(b.read16(base + 0xE), Ok(0x200));
        assert!(b.write16(base + 6, 0x8000).is_err());
        assert_eq!(b.read16(base + 6), Ok(0));
        for offset in [4, 8, 0xA] {
            let before = b.read16(base + offset).unwrap();
            b.write16(base + offset, 0xffff).unwrap();
            assert_eq!(b.read16(base + offset), Ok(before));
        }
        assert!(b.write16(base + 0x120, 0).is_err());
        assert!(b.write8(base + 6, 0).is_err());
    }
}
