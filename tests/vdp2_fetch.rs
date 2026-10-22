//! Original guest display-bus/window evidence from ST-058-R2 §§3.3/8.1.
use saturn_rust::{Boot, Machine, bus::Memory};
fn load(text: &str) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
#[test]
fn guest_fetch_grants_color_depth_banks_and_line_window_writes_determine_actual_scanout() {
    let original = load(include_str!("fixtures/vdp2-fetch.hex"));
    let mut m = Machine::new();
    for changed in [false, false, true] {
        let mut image = original.clone();
        if changed {
            let locations: Vec<_> = image
                .windows(4)
                .enumerate()
                .filter_map(|(i, b)| (b == 0x801Fu32.to_be_bytes()).then_some(i))
                .collect();
            assert_eq!(locations.len(), 5);
            for at in locations {
                image[at..at + 4].copy_from_slice(&0x83E0u32.to_be_bytes());
            }
        }
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..500 {
            m.run_steps(500).unwrap();
            if m.bus().read32(0x0600DF00) == Ok(0x600D) {
                break;
            }
        }
        assert_eq!(m.bus().read32(0x0600DF00), Ok(0x600D));
        assert_eq!(m.cpu_state().exceptions, 0);
        let f = m.framebuffer().unwrap();
        for y in [
            0, 15, 17, 31, 33, 47, 49, 63, 65, 79, 81, 95, 97, 111, 113, 127, 128, 129, 130, 144,
            145, 160, 161, 176, 177, 191, 193, 207, 209, 223,
        ] {
            let visible = !matches!(y,17..=47|65..=79|97..=111|129);
            let color = if visible {
                if changed || matches!(y,145..=160|193..=207) {
                    [0, 255, 0, 255]
                } else {
                    [255, 0, 0, 255]
                }
            } else {
                [0, 0, 0, 255]
            };
            for x in [159, 319] {
                assert_eq!(
                    &f[(y * 320 + x) * 4..][..4],
                    color,
                    "({x},{y}), changed={changed}"
                );
            }
        }
    }
}

#[test]
fn guest_vram_writes_cannot_replace_packed_dots_from_a_retained_display_word() {
    let original = load(include_str!("fixtures/vdp2-word-retention.hex"));
    let mut m = Machine::new();
    for initial in [0x1111u16, 0x1111, 0x3333, 0x3333] {
        let mut rom = original.clone();
        rom[0x100..0x102].copy_from_slice(&initial.to_be_bytes());
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..100 {
            m.run_steps(5000).unwrap();
            if m.bus().read32(0x0600DF04) == Ok(0x600D) {
                break;
            }
        }
        assert_eq!(m.bus().read32(0x0600DF04), Ok(0x600D));
        assert_eq!(m.cpu_state().exceptions, 0);
        let f = m.framebuffer().unwrap();
        for y in [0, 15, 16, 17, 223] {
            let color = if y > 16 {
                [0, 255, 0, 255]
            } else if initial == 0x1111 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            for x in 0..320 {
                assert_eq!(
                    &f[(y * 320 + x) * 4..][..4],
                    color,
                    "({x},{y}), {initial:X}"
                );
            }
        }
    }
}
