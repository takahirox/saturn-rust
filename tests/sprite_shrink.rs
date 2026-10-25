//! Original guest-drawn §6.3 Figure 6.5 / §4.2 HSS/EOS vectors and controls.
use saturn_rust::{Boot, Machine, bus::Memory};

#[test]
fn guest_draws_scaled_distorted_flipped_sprites_with_both_source_parities() {
    for (hex, tsv, reds, ordinary) in [
        (
            include_str!("fixtures/sprite-shrink.hex"),
            include_str!("fixtures/sprite-shrink.tsv"),
            [8, 24, 41, 57],
            false,
        ),
        (
            include_str!("fixtures/sprite-shrink-changed.hex"),
            include_str!("fixtures/sprite-shrink-changed.tsv"),
            [74, 90, 107, 123],
            false,
        ),
        (
            include_str!("fixtures/sprite-shrink-omitted.hex"),
            include_str!("fixtures/sprite-shrink-omitted.tsv"),
            [8, 24, 41, 57],
            false,
        ),
        (
            include_str!("fixtures/sprite-shrink.hex"),
            include_str!("fixtures/sprite-shrink.tsv"),
            [8, 24, 41, 57],
            true,
        ),
    ] {
        let mut rom = vec![0; 524288];
        for line in hex.lines().filter(|l| !l.starts_with('#')) {
            let mut words = line.split_whitespace();
            let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
            for (i, word) in words.enumerate() {
                rom[at + i * 2..at + i * 2 + 2]
                    .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            }
        }
        let mut expected: Vec<_> = tsv
            .lines()
            .map(|l| u32::from_str_radix(l, 16).unwrap())
            .collect();
        if ordinary {
            // Retain the historical HSS-clear operand control. Only guest
            // mode operands change; EOS must not affect ordinary sampling.
            let mut patches = 0;
            for word in rom.as_chunks_mut::<4>().0 {
                if *word == [0, 0, 0x10, 0x68] {
                    word[2] = 0;
                    patches += 1;
                }
            }
            assert_eq!(patches, 7);
            let rows: &[&[u32]] = &[
                &[1, 3, 5, 7],
                &[1, 3, 5, 7],
                &[1, 3, 5, 7],
                &[1, 3, 5, 7],
                &[1, 3, 5, 7],
                &[6, 4, 2, 0],
                &[6, 4, 2, 0],
                &[1, 4, 6],
                &[1, 4, 6],
                &[0, 2, 4, 5, 7],
                &[0, 1, 2, 2, 3, 4, 5, 5, 6, 7],
                &[1, 3, 5, 7],
            ];
            expected = rows
                .iter()
                .flat_map(|r| r.iter().map(|i| 0x8000 + i))
                .collect();
            let sum = expected.iter().sum();
            expected.extend([0x1010, sum, 0x600d]);
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            for _ in 0..1200 {
                m.run_steps(5000).unwrap();
                if m.bus().read32(0x0600dffc) == Ok(0x600d) {
                    break;
                }
            }
            assert_eq!(m.bus().read32(0x0600dffc), Ok(0x600d));
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0600d000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected);
            // Final ordinary eight-to-four command must expose [1,3,5,7].
            m.advance_frame().unwrap();
            m.advance_frame().unwrap();
            let f = m.framebuffer().unwrap();
            for (x, red) in reds.into_iter().chain([0]).enumerate() {
                assert_eq!(&f[(20 * 320 + 10 + x) * 4..][..4], [red, 0, 0, 255]);
            }
        }
    }
}
