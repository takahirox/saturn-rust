//! Original MIT SH-2 guests; expected colors follow Sega §§2.2/4.11 directly.
use saturn_rust::{Boot, Machine, bus::Memory};
const GUESTS: [(&str, &str); 3] = [
    ("normal", include_str!("fixtures/mosaic-normal.hex")),
    ("rotation", include_str!("fixtures/mosaic-rotation.hex")),
    ("rbg1", include_str!("fixtures/mosaic-rbg1.hex")),
];
fn rom(text: &str, tv: u16, seed: u16, mosaic: u16, parameter: u16) -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut fields = line.split_whitespace();
        let at = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
        for (i, field) in fields.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(field, 16).unwrap().to_be_bytes());
        }
    }
    for (at, value) in [
        (0x100, tv),
        (0x102, seed),
        (0x104, mosaic),
        (0x106, parameter),
    ] {
        bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    bytes
}
fn execute(m: &mut Machine, bytes: &[u8], chunk: u32, inspect: bool) -> (Vec<u32>, Vec<Vec<u8>>) {
    m.reset(Boot::ExternalBios(bytes)).unwrap();
    for i in 0..3_000_000u32.div_ceil(chunk) {
        m.run_steps(chunk).unwrap();
        if inspect && i % (10000 / chunk).max(1) == 0 {
            let _ = m.framebuffer();
            let _ = m.cpu_state();
            let _ = m.bus().read16(0x05f800b2);
        }
        if m.bus().read32(0x0600dc1c) == Ok(0x600d) {
            break;
        }
    }
    assert_eq!(
        m.bus().read32(0x0600dc1c),
        Ok(0x600d),
        "pc={:x}",
        m.cpu_state().pc
    );
    assert_eq!(m.cpu_state().exceptions, 0);
    let records = (0..8)
        .map(|i| m.bus().read32(0x0600dc00 + i * 4).unwrap())
        .collect();
    m.advance_frame().unwrap();
    m.advance_frame().unwrap();
    let mut frames = vec![];
    for _ in 0..2 {
        m.advance_frame().unwrap();
        frames.push(m.framebuffer().unwrap());
    }
    (records, frames)
}
fn check(
    mode: &str,
    result: &(Vec<u32>, Vec<Vec<u8>>),
    tv: u16,
    seed: u16,
    mosaic: u16,
    parameter: u16,
) {
    let checksum = if mode == "normal" {
        3072 * u32::from(seed) + 329728
    } else if mode == "rbg1" {
        4096 * u32::from(seed) + 450560
    } else {
        2048 * u32::from(seed) + 67334144
    };
    assert_eq!(
        &result.0[..5],
        [
            checksum,
            u32::from(tv),
            u32::from(mosaic),
            if mode == "normal" {
                3
            } else if mode == "rbg1" {
                0x30
            } else {
                0x10
            },
            u32::from(parameter)
        ],
        "{mode}"
    );
    assert_ne!(
        result.0[5] & 2,
        result.0[6] & 2,
        "guest observed both fields"
    );
    assert_eq!(result.0[7], 0x600d);
    let double = tv == 0x80c0;
    let height = if double { 448 } else { 224 };
    let selected = mosaic & if mode == "rotation" { 0x10 } else { 1 } != 0;
    for frame in &result.1 {
        assert_eq!(frame.len(), 320 * height * 4);
        for y in 0..height {
            for x in 0..320 {
                let mut sx = x;
                let mut sy = y;
                if selected {
                    sx = x / (usize::from(mosaic >> 8 & 15) + 1)
                        * (usize::from(mosaic >> 8 & 15) + 1);
                    if double {
                        sy = y / 2;
                    }
                    if mode == "normal" {
                        sy = sy / (usize::from(mosaic >> 12) + 1) * (usize::from(mosaic >> 12) + 1);
                    }
                }
                if mode != "rbg1" {
                    sy %= 256;
                }
                let companion_y = y % 256;
                let color = if mode == "normal" && !(sx < 16 && sy < 64) {
                    if x < 32 && companion_y < 64 {
                        3 * companion_y + x + usize::from(seed)
                    } else {
                        0
                    }
                } else if sx < 32 && sy < 64 {
                    3 * sy + sx + usize::from(seed)
                } else {
                    0
                };
                let expand = |n: usize| ((n & 31) << 3 | (n & 31) >> 2) as u8;
                assert_eq!(
                    &frame[(y * 320 + x) * 4..][..4],
                    [expand(color), expand(color >> 5), 0, 255],
                    "{mode}, tv={tv:x}, seed={seed}, mosaic={mosaic:x}, parameter={parameter}, ({x},{y})"
                );
            }
        }
    }
}
#[test]
fn mosaic_guest_fields_pixels_controls_and_repeated_cold_boots() {
    let mut m = Machine::new();
    for (mode, source) in GUESTS {
        let bit = if mode == "rotation" { 0x10 } else { 1 };
        for _ in 0..2 {
            for tv in [0x8080, 0x80c0] {
                for (seed, mosaic, parameter) in [
                    (1, bit, 0),
                    (9, bit, 0),
                    (1, 0x3300 | bit, 0),
                    (9, 0x3300 | bit, if mode == "rotation" { 1 } else { 0 }),
                    (1, 0x3300, 0),
                ] {
                    let result = execute(
                        &mut m,
                        &rom(source, tv, seed, mosaic, parameter),
                        10000,
                        false,
                    );
                    check(mode, &result, tv, seed, mosaic, parameter);
                }
            }
        }
    }
}
#[test]
fn mosaic_guest_acquisition_is_chunk_and_inspection_invariant() {
    for (mode, source) in GUESTS {
        let mosaic = if mode == "rotation" { 0x10 } else { 1 };
        let bytes = rom(source, 0x80c0, 1, mosaic, 0);
        let mut reference = None;
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let result = execute(&mut Machine::new(), &bytes, chunk, inspect);
                check(mode, &result, 0x80c0, 1, mosaic, 0);
                if let Some(expected) = &reference {
                    assert_eq!(&result, expected);
                } else {
                    reference = Some(result);
                }
            }
        }
    }
}
