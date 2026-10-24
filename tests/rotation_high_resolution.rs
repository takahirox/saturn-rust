//! Original MIT guest, §2.1/6.3 full-image oracle independent of core helpers.
use saturn_rust::{Boot, Machine, bus::Memory};
const SOURCES: [(&str, &str); 4] = [
    ("tile", include_str!("fixtures/rotation-hi-tile.hex")),
    ("bitmap", include_str!("fixtures/rotation-hi-bitmap.hex")),
    ("rbg1", include_str!("fixtures/rotation-hi-rbg1.hex")),
    (
        "coefficient",
        include_str!("fixtures/rotation-hi-coefficient.hex"),
    ),
];
fn rom(text: &str) -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    image
}
fn execute(m: &mut Machine, image: &[u8], chunk: u32) -> (Vec<u32>, Vec<u8>) {
    m.reset(Boot::ExternalBios(image)).unwrap();
    for _ in 0..3_000_000u32.div_ceil(chunk) {
        m.run_steps(chunk).unwrap();
        if m.bus().read32(0x0600de1c) == Ok(0x600d) {
            break;
        }
    }
    assert_eq!(
        m.bus().read32(0x0600de1c),
        Ok(0x600d),
        "guest pc={:x}",
        m.cpu_state().pc
    );
    assert_eq!(m.cpu_state().exceptions, 0);
    let records = (0..8)
        .map(|i| m.bus().read32(0x0600de00 + i * 4).unwrap())
        .collect();
    m.advance_frame().unwrap();
    (records, m.framebuffer().unwrap())
}
fn check(mode: &str, result: &(Vec<u32>, Vec<u8>), tv: u16, seed: u16, shift: u16, enabled: bool) {
    let bitmap = mode == "bitmap" || mode == "coefficient";
    let base = if bitmap { 0x8000u32 } else { 0 };
    assert_eq!(
        result.0,
        [
            512 * (base + u32::from(seed)) + 130816,
            base + u32::from(seed),
            base + 511 + u32::from(seed),
            u32::from(shift) << 16,
            u32::from(tv),
            if enabled {
                if mode == "rbg1" { 0x30 } else { 0x10 }
            } else {
                0
            },
            if mode == "coefficient" { 0x10000 } else { 0 },
            0x600d
        ],
        "mode={mode}"
    );
    let width = if tv & 1 == 0 { 640 } else { 704 };
    assert_eq!(result.1.len(), width * 224 * 4);
    for y in 0..224 {
        for x in 0..width {
            let color = if enabled {
                (x / 2 + usize::from(seed) + usize::from(shift)) as u16
            } else {
                0
            };
            let expand = |c: u16| {
                let b = (c & 31) as u8;
                b << 3 | b >> 2
            };
            let expected = [expand(color), expand(color >> 5), expand(color >> 10), 255];
            assert_eq!(
                &result.1[(y * width + x) * 4..][..4],
                expected,
                "mode={mode}, tv={tv:x}, seed={seed}, shift={shift}, enabled={enabled}, x={x}, y={y}"
            );
        }
    }
}
#[test]
fn guest_high_resolution_rotation_full_images_mutations_and_cold_boots() {
    let mut m = Machine::new();
    for (mode, source) in SOURCES {
        for _ in 0..2 {
            for tv in [0x8002u16, 0x8003] {
                for (seed, shift, enabled, parameter) in
                    [(1u16, 0u16, true, 0u16), (17, 7, true, 1), (1, 0, false, 0)]
                {
                    let mut image = rom(source);
                    for (at, value) in [
                        (0x100, tv),
                        (0x102, seed),
                        (0x104, shift),
                        (
                            0x106,
                            if enabled {
                                if mode == "rbg1" { 0x30 } else { 0x10 }
                            } else {
                                0
                            },
                        ),
                        (0x108, if mode == "rbg1" { 0 } else { parameter }),
                    ] {
                        image[at..at + 2].copy_from_slice(&value.to_be_bytes());
                    }
                    let result = execute(&mut m, &image, 10000);
                    check(mode, &result, tv, seed, shift, enabled);
                }
            }
        }
    }
}
#[test]
fn guest_high_resolution_records_and_images_ignore_advancement_chunk_size() {
    for (mode, source) in SOURCES {
        let mut reference = None;
        for chunk in [1, 3, 17, 10000] {
            let result = execute(&mut Machine::new(), &rom(source), chunk);
            check(mode, &result, 0x8002, 1, 0, true);
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
        }
    }
}
