//! Original MIT guest bytes; §3.3/6.2 literal acquisition/scanout oracle.
use saturn_rust::{Boot, Machine, bus::Memory};
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
const SOURCES: [(&str, &str); 3] = [
    ("tile", include_str!("fixtures/rotation-fetch-tile.hex")),
    ("bitmap", include_str!("fixtures/rotation-fetch-bitmap.hex")),
    ("rbg1", include_str!("fixtures/rotation-fetch-rbg1.hex")),
];
fn execute(m: &mut Machine, image: &[u8], chunk: u32) -> (Vec<u32>, Vec<u8>) {
    m.reset(Boot::ExternalBios(image)).unwrap();
    for _ in 0..2_000_000 / chunk {
        m.run_steps(chunk).unwrap();
        if m.bus().read32(0x0600dc0c) == Ok(0x600d) {
            break;
        }
    }
    assert_eq!(m.bus().read32(0x0600dc0c), Ok(0x600d));
    assert_eq!(m.cpu_state().exceptions, 0);
    let records = (0..4)
        .map(|i| m.bus().read32(0x0600dc00 + i * 4).unwrap())
        .collect();
    m.advance_frame().unwrap();
    (records, m.framebuffer().unwrap())
}
fn check(frame: &[u8], initial: u16, early: bool, write: bool) {
    // Authored from the transaction contract, not core timing/address helpers:
    // repeated source(0,0); write publishes at/just after line16's first-dot edge.
    // Display acquisition wins an equal edge. An early write precedes the line;
    // an omitted write leaves every dot unchanged. Palettes are guest stores.
    assert_eq!(frame.len(), 320 * 224 * 4);
    for y in 0..224 {
        for x in 0..320 {
            let new = write && (y > 16 || y == 16 && (early || x != 0));
            let expected = if new {
                [0, 255, 0, 255]
            } else if initial == 0x1111 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            assert_eq!(
                &frame[(y * 320 + x) * 4..][..4],
                expected,
                "({x},{y}), initial={initial:X}, early={early}, write={write}"
            );
        }
    }
}
fn check_guest(mode: &str, source: &str) {
    let mut m = Machine::new();
    for _ in 0..2 {
        // independent cold boots, never a warmed guest state
        for initial in [0x1111u16, 0x3333] {
            for (threshold, write) in [(842u16, true), (700, true), (842, false)] {
                let mut image = rom(source);
                image[0x100..0x102].copy_from_slice(&initial.to_be_bytes());
                image[0x102..0x104].copy_from_slice(&threshold.to_be_bytes());
                image[0x104..0x106].copy_from_slice(&u16::from(write).to_be_bytes());
                let (records, frame) = execute(&mut m, &image, 5000);
                assert!(
                    (u32::from(threshold)..854).contains(&records[0]),
                    "{mode}: {records:?}"
                );
                if threshold == 842 && write {
                    assert_eq!(records[0], 844, "{mode}");
                }
                let value = if write { 0x2222 } else { u32::from(initial) };
                assert_eq!(
                    &records[1..],
                    [value, u32::from(initial) + value, 0x600d],
                    "{mode}"
                );
                check(&frame, initial, threshold == 700, write);
            }
        }
    }
}
#[test]
fn guest_rbg0_tile_image_words_are_acquired_before_consumption() {
    check_guest(SOURCES[0].0, SOURCES[0].1);
}
#[test]
fn guest_rbg0_bitmap_image_words_are_acquired_before_consumption() {
    check_guest(SOURCES[1].0, SOURCES[1].1);
}
#[test]
fn guest_rbg1_tile_image_words_are_acquired_before_consumption() {
    check_guest(SOURCES[2].0, SOURCES[2].1);
}
#[test]
fn guest_records_and_frames_are_identical_across_execution_chunks() {
    for (_, source) in SOURCES {
        let mut reference = None;
        for chunk in [1, 3, 17, 10000] {
            let actual = execute(&mut Machine::new(), &rom(source), chunk);
            assert_eq!(actual.0, [844, 0x2222, 0x3333, 0x600d]);
            check(&actual.1, 0x1111, false, true);
            if let Some(expected) = &reference {
                assert_eq!(&actual, expected);
            } else {
                reference = Some(actual);
            }
        }
    }
}
