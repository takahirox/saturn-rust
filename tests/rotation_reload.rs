//! Independent §6.3/6.4 bitmap oracle for original MIT SH-2 A/B guests.
use saturn_rust::{Boot, Machine, bus::Memory};

fn rom(text: &str) -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}

fn expected(x: usize, y: usize, delta: usize) -> [u8; 4] {
    // Authored from the guest's explicit line schedule, without core helpers.
    let (sx, sy, k) = match y {
        0..128 => (10 + delta * y, 0, 2 + y),
        128..160 => (100 + delta * (y - 128), 4, 8 + y - 128),
        160..192 => (100 + delta * 31 + 3 * (y - 159), 4, 39 + 2 * (y - 159)),
        192 => (20, 8, 16),
        193 => (30, 10, 20),
        _ => (40 + 3 * (y - 194), 12 + y - 194, 24 + 2 * (y - 194)),
    };
    let column = (sx + x + k % 4) % 512;
    let rgb = [column % 32, sy % 32, column / 32];
    [
        ((rgb[0] << 3) | (rgb[0] >> 2)) as u8,
        ((rgb[1] << 3) | (rgb[1] >> 2)) as u8,
        ((rgb[2] << 3) | (rgb[2] >> 2)) as u8,
        255,
    ]
}

#[test]
fn guest_a_b_origins_coefficients_one_request_no_request_and_reissued_requests() {
    for (source, trace) in [
        (
            include_str!("fixtures/rotation-reload-0.hex"),
            include_str!("fixtures/rotation-reload-0.tsv"),
        ),
        (
            include_str!("fixtures/rotation-reload-1.hex"),
            include_str!("fixtures/rotation-reload-1.tsv"),
        ),
    ] {
        let mut image = rom(source);
        let records: Vec<_> = trace
            .lines()
            .map(|l| u32::from_str_radix(l, 16).unwrap())
            .collect();
        let mut machine = Machine::new();
        for delta in [1, 1, 2] {
            image[0x100..0x104].copy_from_slice(&((delta as u32) << 16).to_be_bytes());
            machine.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..2000 {
                machine.run_steps(500).unwrap();
                if machine.bus().read32(0x0600dd24) == Ok(0x600d) {
                    break;
                }
            }
            let actual: Vec<_> = (0..10)
                .map(|i| machine.bus().read32(0x0600dd00 + i * 4).unwrap())
                .collect();
            assert_eq!(actual, records, "delta={delta}");
            let frame = machine.framebuffer().unwrap();
            for y in 0..224 {
                for x in 0..320 {
                    assert_eq!(
                        &frame[(y * 320 + x) * 4..][..4],
                        expected(x, y, delta),
                        "({x},{y}) delta={delta}"
                    );
                }
            }
        }
    }
}
