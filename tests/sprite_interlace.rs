//! ST-013 §4.2 and ST-058 §2.4: guest even/odd drawing, compressed RAM and scanout.
use saturn_rust::{Boot, Machine, bus::Memory};
#[test]
fn guest_interlaced_sprites_use_both_buffers_and_retain_alternate_historical_rows() {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sprite-interlace.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    let expected: Vec<_> = include_str!("fixtures/sprite-interlace.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    for changed in [false, false, true] {
        if changed {
            image[0x103] = 0xf;
        }
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..1000 {
            m.run_steps(500).unwrap();
            if m.bus().read32(0x0600df34) == Ok(0x600d) {
                break;
            }
        }
        let mut records = expected.clone();
        if changed {
            records[1..5].fill(0x83e0);
        }
        let actual: Vec<_> = (0..records.len())
            .map(|i| m.bus().read32(0x0600df00 + i as u32 * 4).unwrap())
            .collect();
        assert_eq!(actual, records);
        assert_eq!(m.video_dimensions(), (320, 448));
        let f = m.framebuffer().unwrap();
        for y in 0..448 {
            for x in 0..320 {
                let rgb = if (430..=437).contains(&y) && (10..=17).contains(&x) {
                    if changed || y % 2 == 1 {
                        [0, 255, 0, 255]
                    } else {
                        [255, 0, 0, 255]
                    }
                } else {
                    [0, 0, 0, 255]
                };
                assert_eq!(
                    &f[(y * 320 + x) * 4..][..4],
                    rgb,
                    "({x},{y}),changed={changed}"
                );
            }
        }
        m.advance_frame().unwrap();
        assert_eq!(m.framebuffer().unwrap(), f);
        m.advance_frame().unwrap();
        let next = m.framebuffer().unwrap();
        for y in 430..=437 {
            assert_eq!(&next[(y * 320 + 10) * 4..][..4], [0, 255, 0, 255]);
        }
    }
}
