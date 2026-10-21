//! ST-013 §6.3/fig6.5 and §4.2, original guest-drawn HSS/EOS vectors.
use saturn_rust::{Boot, Machine, bus::Memory};
#[test]
fn guest_draws_scaled_distorted_flipped_sprites_with_both_source_parities() {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/sprite-shrink.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let expected: Vec<_> = include_str!("fixtures/sprite-shrink.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.run_steps(10000).unwrap();
        let actual: Vec<_> = (0..expected.len())
            .map(|i| m.bus().read32(0x0600d000 + i as u32 * 4).unwrap())
            .collect();
        assert_eq!(actual, expected);
        m.advance_frame().unwrap();
        m.advance_frame().unwrap(); // show a full field after guest drawing/swap
        let f = m.framebuffer().unwrap();
        for (x, red) in [57, 41, 24, 8].into_iter().enumerate() {
            assert_eq!(&f[(20 * 320 + 10 + x) * 4..][..4], [red, 0, 0, 255]);
        }
        assert_eq!(&f[(20 * 320 + 14) * 4..][..4], [0, 0, 0, 255]);
    }
    let mut patched = 0;
    for word in rom.as_chunks_mut::<4>().0 {
        if *word == [0, 0, 0x10, 0x68] {
            word[2] = 0;
            patched += 1;
        }
    }
    assert_eq!(patched, 6);
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.run_steps(10000).unwrap();
    let expected_changed = [
        0x8000, 0x8002, 0x8004, 0x8007, 0x8000, 0x8002, 0x8004, 0x8007, 0x8000, 0x8002, 0x8004,
        0x8007, 0x8000, 0x8002, 0x8004, 0x8007, 0x8000, 0x8002, 0x8004, 0x8007, 0x8007, 0x8005,
        0x8003, 0x8000, 0x8007, 0x8005, 0x8003, 0x8000, 0x1090, 0x600d,
    ];
    let actual: Vec<_> = (0..expected_changed.len())
        .map(|i| m.bus().read32(0x0600d000 + i as u32 * 4).unwrap())
        .collect();
    assert_eq!(actual, expected_changed);
}
