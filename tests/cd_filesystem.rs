//! Original guest evidence: ISO9660 §9.1, ECMA-130 annex A, ST-162 §§6.2/6.8.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};

#[test]
fn guest_reads_xa_metadata_interleaved_payload_offsets_and_filesystem_filter_effects() {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-filesystem.hex")
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
    let expected: Vec<_> = include_str!("fixtures/cd-filesystem.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for offset in [0, 1] {
        let mut rom = rom.clone();
        let positions: Vec<_> = rom
            .windows(2)
            .enumerate()
            .filter_map(|(i, b)| (b == [0xE6, 0]).then_some(i))
            .collect();
        assert_eq!(positions.len(), 1);
        rom[positions[0] + 1] = offset;
        let mut expected = expected.clone();
        if offset == 1 {
            expected[3] = 2;
            expected[4] -= 0x1111 * 1024;
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.mount_media(
                Media::cue(
                    include_str!("fixtures/cd-filesystem.cue"),
                    &[(
                        "cd-filesystem.bin",
                        include_bytes!("fixtures/cd-filesystem.bin"),
                    )],
                )
                .unwrap(),
            );
            for _ in 0..100 {
                m.run_steps(10000).unwrap();
                if m.bus().read32(0x0600B01C) == Ok(0x600D) {
                    break;
                }
            }
            for (i, expected) in expected.iter().enumerate() {
                assert_eq!(
                    m.bus().read32(0x0600B000 + i as u32 * 4).unwrap(),
                    *expected,
                    "offset={offset}, record={i}"
                );
            }
            assert_eq!(m.bus().read32(0x0600A020).unwrap() & 0x5F, 0x41);
            assert_eq!(m.bus().read32(0x0600A034), Ok(0x03FF));
            assert_eq!(m.bus().read32(0x0600A048), Ok(0x0300));
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}
