//! Independent ST-058-R2 §§4.6/4.9/10.3/11.2/12.3 pixel vectors.
use saturn_rust::{devices::VDP2, vdp2::Vdp2};
fn reg(v: &mut Vdp2, off: u32, value: u16) {
    v.write16(VDP2 + off, value).unwrap();
}
fn word(v: &mut Vdp2, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn color(v: &mut Vdp2, index: usize, value: u16) {
    v.cram[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
}
fn pixel(f: &[u8], x: usize) -> &[u8] {
    &f[x * 4..x * 4 + 4]
}
const RED: &[u8] = &[255, 0, 0, 255];
const BLUE: &[u8] = &[0, 0, 255, 255];
const BLACK: &[u8] = &[0, 0, 0, 255];
const PURPLE: &[u8] = &[127, 0, 127, 255];
fn screen(id: usize, bitmap: bool, one: bool, format: u16) -> Vdp2 {
    let mut v = Vdp2::default();
    reg(&mut v, 0, 0x8000);
    reg(&mut v, 0x20, 1 << id);
    if id < 2 {
        reg(
            &mut v,
            0x28,
            (format << 4 | if bitmap { 2 } else { 0 }) << (id * 8),
        );
        reg(&mut v, 0x78 + id as u32 * 16, 1);
        reg(&mut v, 0x7C + id as u32 * 16, 1);
    } else {
        reg(&mut v, 0x2A, format << ((id - 2) * 4 + 1));
    }
    reg(&mut v, 0xF8 + (id / 2) as u32 * 2, 3 << ((id % 2) * 8));
    reg(&mut v, 0x108 + (id / 2) as u32 * 2, 15 << ((id % 2) * 8));
    reg(&mut v, 0xEC, 1 << id);
    reg(&mut v, 0xAC, 3);
    reg(&mut v, 0xAE, 0x8000);
    word(&mut v, 0x70000, 0x7C00);
    if !bitmap {
        reg(&mut v, 0x30 + id as u32 * 2, if one { 0x8000 } else { 0 });
        reg(&mut v, 0x40 + id as u32 * 4, if one { 8 } else { 4 });
        word(&mut v, 0x10000, if one { 0x100 } else { 0 });
        if !one {
            word(&mut v, 0x10002, 0x100);
        }
    }
    let at = if bitmap { 0 } else { 0x2000 };
    match format {
        0 => v.vram[at..at + 2].copy_from_slice(&[0x12, 0x34]),
        1 => v.vram[at..at + 4].copy_from_slice(&[1, 2, 3, 4]),
        2 => {
            for (i, c) in [0x101, 0x102, 0x103, 0x104].into_iter().enumerate() {
                word(&mut v, at + i * 2, c);
            }
        }
        3 => {
            for i in 0..4 {
                word(&mut v, at + i * 2, 0x801F);
            }
        }
        _ => {
            for i in 0..4 {
                word(&mut v, at + i * 4, 0x8000);
                word(&mut v, at + i * 4 + 2, 0xFF);
            }
        }
    }
    for index in [1, 2, 3, 4, 0x101, 0x102, 0x103, 0x104] {
        color(&mut v, index, 0x1F);
    }
    v
}
fn flags(v: &mut Vdp2, id: usize, bitmap: bool, one: bool, value: u16) {
    if bitmap {
        reg(v, 0x2C, (value << 4) << (id * 8));
    } else if one {
        reg(v, 0x30 + id as u32 * 2, 0x8000 | value << 8);
    } else {
        word(v, 0x10000, value << 12);
    }
}
#[test]
fn every_normal_layer_and_both_name_widths_apply_character_priority_and_zero_priority() {
    for id in 0..4 {
        for one in [false, true] {
            let mut v = screen(id, false, one, 0);
            reg(&mut v, 0xEC, 0);
            reg(&mut v, 0xEA, 1 << (id * 2));
            reg(&mut v, 0xF8 + (id / 2) as u32 * 2, 0); // Special bit can raise register0.
            flags(&mut v, id, false, one, 2);
            assert_eq!(
                pixel(&v.framebuffer().unwrap(), 0),
                RED,
                "layer{id},one{one}"
            );
            flags(&mut v, id, false, one, 1); // CC flag must not set priority.
            assert_eq!(pixel(&v.framebuffer().unwrap(), 0), BLUE);
            reg(&mut v, 0xEA, 0); // Mode0 uses unmodified register priority0.
            flags(&mut v, id, false, one, 3);
            assert_eq!(pixel(&v.framebuffer().unwrap(), 0), BLUE);
        }
    }
}
#[test]
fn dot_priority_uses_low_four_code_bits_before_palette_offsets_and_independent_a_b_selectors() {
    for (format, bitmap, one) in [
        (0, false, false),
        (0, false, true),
        (1, true, false),
        (2, true, false),
    ] {
        let mut v = screen(0, bitmap, one, format);
        flags(&mut v, 0, bitmap, one, 2);
        reg(&mut v, 0xEA, 2);
        reg(&mut v, 0xF8, 0);
        reg(&mut v, 0xEC, 0);
        reg(&mut v, 0x26, 0x0201); // A admits0/1; B admits2/3.
        let f = v.framebuffer().unwrap();
        assert_eq!(pixel(&f, 0), RED);
        assert_eq!(pixel(&f, 1), BLUE);
        assert_eq!(pixel(&f, 2), BLUE);
        reg(&mut v, 0x24, 1);
        let f = v.framebuffer().unwrap();
        assert_eq!(pixel(&f, 0), BLUE);
        assert_eq!(pixel(&f, 1), RED);
        assert_eq!(pixel(&f, 2), RED);
        flags(&mut v, 0, bitmap, one, 0);
        assert_eq!(pixel(&v.framebuffer().unwrap(), 1), BLUE);
    }
}
#[test]
fn special_color_screen_character_dot_and_msb_modes_gate_only_the_top_screen_enable() {
    for (bitmap, one) in [(true, false), (false, false), (false, true)] {
        for mode in 0..4 {
            let mut v = screen(0, bitmap, one, 0);
            reg(&mut v, 0xEE, mode);
            reg(&mut v, 0x26, 2); // codes2/3
            flags(&mut v, 0, bitmap, one, 1);
            color(&mut v, 3, 0x801F); // Only code3 has palette MSB.
            let f = v.framebuffer().unwrap();
            let expected = match mode {
                0 | 1 => [PURPLE, PURPLE, PURPLE, PURPLE],
                2 => [RED, PURPLE, PURPLE, RED],
                _ => [RED, RED, PURPLE, RED],
            };
            for (x, c) in expected.into_iter().enumerate() {
                assert_eq!(pixel(&f, x), c, "mode{mode},bitmap{bitmap},one{one},x{x}");
            }
            flags(&mut v, 0, bitmap, one, 0);
            if mode == 1 || mode == 2 {
                assert_eq!(pixel(&v.framebuffer().unwrap(), 2), RED);
            }
            reg(&mut v, 0xEC, 0);
            assert_eq!(pixel(&v.framebuffer().unwrap(), 2), RED);
        }
    }
}
#[test]
fn rgb_formats_allow_character_flags_and_msb_mode_but_reject_dot_and_reserved_priority_modes() {
    for format in [3, 4] {
        let mut v = screen(0, true, false, format);
        reg(&mut v, 0xEE, 3); // RGB mode3 always allows calculation.
        assert_eq!(pixel(&v.framebuffer().unwrap(), 0), PURPLE);
        reg(&mut v, 0xEE, 1);
        assert_eq!(pixel(&v.framebuffer().unwrap(), 0), RED);
        flags(&mut v, 0, true, false, 1);
        assert_eq!(pixel(&v.framebuffer().unwrap(), 0), PURPLE);
        reg(&mut v, 0xEE, 2);
        assert!(v.framebuffer().is_err());
        reg(&mut v, 0xEE, 0);
        for mode in [2, 3] {
            reg(&mut v, 0xEA, mode);
            assert!(v.framebuffer().is_err());
        }
    }
}
#[test]
fn special_color_msb_reads_mode2_palette_metadata_after_color_offset_addressing() {
    let mut v = screen(0, true, false, 0);
    reg(&mut v, 0xE, 0x2000);
    reg(&mut v, 0xE4, 1); // Palette entries257+.
    reg(&mut v, 0xEE, 3);
    v.cram[0x404..0x40C].copy_from_slice(&[0x80, 0, 0, 0xFF, 0, 0, 0, 0xFF]);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0), PURPLE);
    assert_eq!(pixel(&f, 1), RED);
    // Transparent code0 stays transparent even if palette entry0 has its MSB.
    v.vram[0] = 0;
    v.cram[0x400..0x404].copy_from_slice(&[0x80, 0, 0, 0xFF]);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0), BLUE);
    reg(&mut v, 0x20, 0x101);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0), PURPLE);
    assert_ne!(pixel(&f, 0), BLACK);
}

#[test]
fn original_guest_special_function_register_mutations_change_composed_pixels_and_reset_repeats() {
    use saturn_rust::{Boot, Machine, bus::Memory};
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/special-video.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    let mut m = Machine::new();
    for (address, old, new, expected) in [
        (0, 0, 0, [BLUE, PURPLE, PURPLE, BLUE]),
        (0x25F80024, 0, 1, [PURPLE, BLUE, BLUE, BLUE]),
        (0x25F8002C, 0x30, 0x20, [BLUE, RED, RED, BLUE]),
        (0x25F8002C, 0x30, 0x10, [BLUE, BLUE, BLUE, BLUE]),
    ] {
        let mut image = rom.clone();
        if address != 0 {
            // Each original address literal is followed by the load of its
            // value. Patch that value pool entry, leaving the store executable.
            let mut hits = Vec::new();
            for at in (0x200..0x10000).step_by(2) {
                let op = u16::from_be_bytes(image[at + 4..at + 6].try_into().unwrap());
                if image[at..at + 4] != u32::to_be_bytes(address) || op & 0xFF00 != 0xD000 {
                    continue;
                }
                let literal = ((at + 4) & !3) + 4 + usize::from(op & 255) * 4;
                if image[literal..literal + 4] == u32::to_be_bytes(old)
                    && image[literal + 4..literal + 6] == [0x21, 0x01]
                {
                    hits.push(literal);
                }
            }
            assert_eq!(hits.len(), 1);
            image[hits[0]..hits[0] + 4].copy_from_slice(&u32::to_be_bytes(new));
        }
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            assert_eq!(m.bus().read32(0x0600A300).unwrap(), 0x600D);
            let frame = m.framebuffer().unwrap();
            for x in 0..8 {
                assert_eq!(
                    pixel(&frame, x),
                    expected[x % 4],
                    "address{address:08X} x{x}"
                );
            }
            if let Some(previous) = previous {
                assert_eq!(frame, previous);
            }
            previous = Some(frame);
        }
    }
}
