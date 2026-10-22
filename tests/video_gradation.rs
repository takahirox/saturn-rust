//! Source-authored ST-058-R2 §12.2/fig.12.4 and 1997 §12.1 blur vectors.
use saturn_rust::{Boot, Machine, bus::Memory, devices::VDP2, vdp2::Vdp2};
fn reg(v: &mut Vdp2, off: u32, value: u16) {
    v.write16(VDP2 + off, value).unwrap();
}
fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
    &frame[(y * 320 + x) * 4..][..4]
}
fn screen(id: usize) -> Vdp2 {
    let mut v = Vdp2::default();
    reg(&mut v, 0, 0x8000);
    reg(&mut v, 0x20, 1 << id);
    reg(&mut v, 0x30 + id as u32 * 2, 0x8000); // One-word name
    reg(&mut v, 0x40 + id as u32 * 4, 8); // Page at 0x10000
    v.vram[0x10000..0x10002].copy_from_slice(&0x100u16.to_be_bytes());
    for y in 0..8 {
        v.vram[0x2000 + y * 4..0x2004 + y * 4].copy_from_slice(&[0x12, 0x34, 0x12, 0x34]);
    }
    for (i, c) in [0x1Fu16, 0x3E0, 0x7C00, 0x7FFF].into_iter().enumerate() {
        v.cram[(i + 1) * 2..(i + 2) * 2].copy_from_slice(&c.to_be_bytes());
    }
    if id < 2 {
        reg(&mut v, 0x78 + id as u32 * 16, 1);
        reg(&mut v, 0x7C + id as u32 * 16, 1);
    }
    reg(&mut v, 0xF8 + (id / 2) as u32 * 2, 4 << (id % 2 * 8));
    reg(&mut v, 0x108 + (id / 2) as u32 * 2, 15 << (id % 2 * 8));
    let selector = [2, 4, 5, 6][id];
    reg(&mut v, 0xEC, 0x8000 | selector << 12 | (1 << id));
    v
}
#[test]
fn all_normal_screens_use_horizontal_one_one_two_weights_and_top_calculation_enable() {
    for id in 0..4 {
        let mut v = screen(id);
        let f = v.framebuffer().unwrap();
        for (x, expected) in [
            (2, [31, 31, 191, 255]),
            (3, [191, 223, 223, 255]),
            (4, [223, 31, 63, 255]),
            (5, [63, 223, 31, 255]),
        ] {
            assert_eq!(pixel(&f, x, 0), expected, "screen{id} x{x}");
            assert_eq!(pixel(&f, x, 1), expected);
        }
        // The selected top remains the top; its enable, ratio and post-blend
        // offset still control final output. Without CCEN it stays unfiltered.
        let control = v.read16(VDP2 + 0xEC).unwrap();
        reg(&mut v, 0xEC, control & !(1 << id));
        assert_eq!(pixel(&v.framebuffer().unwrap(), 2, 0), [0, 0, 255, 255]);
        reg(&mut v, 0xEC, control);
        reg(&mut v, 0x110, 1 << id);
        reg(&mut v, 0x114, 10);
        assert_eq!(pixel(&v.framebuffer().unwrap(), 2, 0), [41, 31, 191, 255]);
        // Left-border seeds are an explicit functional policy; no history
        // survives a scanline or framebuffer inspection.
        assert_eq!(pixel(&f, 0, 0), pixel(&f, 0, 1));
        reg(&mut v, 0x110, 0);
        assert_eq!(v.framebuffer().unwrap(), f);
    }
}
#[test]
fn prohibited_selectors_and_cram_modes_reject_but_disabled_selector_bits_are_inert() {
    let mut v = screen(0);
    for selector in [3, 7] {
        reg(&mut v, 0xEC, 0x8000 | selector << 12);
        assert!(v.framebuffer().is_err());
        reg(&mut v, 0xEC, selector << 12);
        assert!(v.framebuffer().is_ok());
    }
    reg(&mut v, 0xEC, 0xA001);
    for mode in [0x1000, 0x2000] {
        reg(&mut v, 0xE, mode);
        assert!(v.framebuffer().is_err());
    }
}
fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/gradation.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    image
}
#[test]
fn original_guest_filters_top_and_second_screens_and_blocks_line_and_extended_calculation() {
    let expected = [
        [31, 31, 191, 255],
        [191, 223, 223, 255],
        [223, 159, 191, 255],
        [191, 223, 159, 255],
        [159, 159, 191, 255],
        [191, 223, 223, 255],
    ];
    for (control, values) in [
        (0xA003u32, expected),
        (0xA403, expected), // Extended color calculation cannot intervene.
        (
            3,
            [
                [0, 127, 127, 255],
                [127, 255, 127, 255],
                [127, 255, 127, 255],
                [127, 255, 127, 255],
                [127, 255, 127, 255],
                [127, 255, 127, 255],
            ],
        ),
    ] {
        let mut image = rom();
        let literal = image
            .windows(4)
            .position(|w| w == 0xA003u32.to_be_bytes())
            .unwrap();
        assert!(
            image[literal + 4..]
                .windows(4)
                .all(|w| w != 0xA003u32.to_be_bytes())
        );
        image[literal..literal + 4].copy_from_slice(&control.to_be_bytes());
        let mut m = Machine::new();
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            assert_eq!(m.bus().read32(0x0600A400), Ok(0x600D));
            let f = m.framebuffer().unwrap();
            for y in 0..2 {
                for (i, expected) in values.iter().enumerate() {
                    assert_eq!(
                        pixel(&f, i + 2, y),
                        expected,
                        "control{control:04X} x{} y{y}",
                        i + 2
                    );
                }
            }
            if let Some(previous) = previous {
                assert_eq!(f, previous);
            }
            previous = Some(f);
        }
    }
}

#[test]
fn hidden_source_history_is_sampled_and_third_image_never_replaces_second() {
    let mut v = screen(0);
    reg(&mut v, 0x20, 7);
    // NBG1 white bitmap; NBG2 white tile except transparent at x3.
    reg(&mut v, 0x28, 0x3200);
    reg(&mut v, 0x3C, 0x20);
    reg(&mut v, 0x88, 1);
    reg(&mut v, 0x8C, 1);
    for x in 0..8 {
        v.vram[0x40000 + x * 2..0x40002 + x * 2].copy_from_slice(&0xFFFFu16.to_be_bytes());
    }
    reg(&mut v, 0x34, 0x8000);
    reg(&mut v, 0x48, 0x18);
    v.vram[0x30000..0x30002].copy_from_slice(&0x200u16.to_be_bytes());
    v.vram[0x4000..0x4004].copy_from_slice(&[0x44, 0x40, 0x44, 0x44]);
    reg(&mut v, 0xF8, 0x0403);
    reg(&mut v, 0xFA, 5);
    reg(&mut v, 0x108, 0x0F0F);
    reg(&mut v, 0x10A, 15);
    reg(&mut v, 0xEC, 0xA007);
    let hidden = v.framebuffer().unwrap();
    // NBG0 is third (or fourth) so regular top/second composition applies.
    assert_eq!(pixel(&hidden, 2, 0), [255, 255, 255, 255]);
    assert_eq!(pixel(&hidden, 3, 0), [191, 223, 223, 255]);
    reg(&mut v, 0xF8, 0x0405); // Now NBG0 wins priority ties and is top.
    let visible = v.framebuffer().unwrap();
    assert_eq!(pixel(&visible, 3, 0), [191, 223, 223, 255]);
    // Expose NBG0 only as second at x3; its earlier concealed samples must
    // remain red/green/blue, not the visible white top from x0..2.
    reg(&mut v, 0xFA, 6);
    reg(&mut v, 0xF8, 0x0605);
    let second = v.framebuffer().unwrap();
    assert_eq!(pixel(&second, 3, 0), [191, 223, 223, 255]);
    assert_eq!(pixel(&second, 2, 0), [255, 255, 255, 255]);
    // A color-calculation window disables blending but never shifts history.
    reg(&mut v, 0xC0, 16);
    reg(&mut v, 0xC4, 16);
    reg(&mut v, 0xC6, 1);
    reg(&mut v, 0xD6, 0x0200);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 3, 0), [255, 255, 255, 255]);
}

#[test]
fn sprite_and_both_rotation_screen_selectors_filter_device_samples() {
    use saturn_rust::vdp1::{REGISTERS, Vdp1};
    // Identical RGB555 source sequence through VDP1 display RAM, RBG0
    // bitmap and RBG1 character RAM. No renderer-specific image replacement.
    let colors = [0x801Fu16, 0x83E0, 0xFC00, 0xFFFF];
    for selector in [0, 1, 2] {
        let mut v = screen(0);
        let mut sprites = Vdp1::default();
        reg(&mut v, 0x20, 0);
        reg(
            &mut v,
            0xEC,
            0x8000
                | selector << 12
                | match selector {
                    0 => 0x40,
                    1 => 0x10,
                    _ => 1,
                },
        );
        for (i, color) in colors.into_iter().enumerate() {
            sprites.draw_buffer_mut()[i * 2..i * 2 + 2].copy_from_slice(&color.to_be_bytes());
            v.vram[i * 2..i * 2 + 2].copy_from_slice(&color.to_be_bytes());
        }
        // Make the authored framebuffer the display buffer; retain draw RAM.
        sprites.write16(REGISTERS + 2, 3).unwrap();
        sprites.vblank_out();
        reg(&mut v, 0xE0, 0x720); // mixed RGB, sprite priority <=7 calculates
        reg(&mut v, 0xF0, 4);
        reg(&mut v, 0x100, 15);
        if selector != 0 {
            reg(&mut v, 0x20, if selector == 1 { 0x10 } else { 0x30 });
            reg(&mut v, 0x2A, 0x3200);
            reg(&mut v, 0xFC, if selector == 1 { 4 } else { 0 });
            reg(&mut v, 0x10C, 15);
            reg(&mut v, 0xBC, 1); // Tables at0x20000/0x20080
            reg(&mut v, 0xBE, 0);
            for base in [0x20000, 0x20080] {
                for off in [0x10, 0x14, 0x1C, 0x2C, 0x4C, 0x50] {
                    v.vram[base + off..base + off + 4].copy_from_slice(&0x10000u32.to_be_bytes());
                }
            }
            if selector == 2 {
                reg(&mut v, 0xE, 0x200);
                reg(&mut v, 0x30, 0x8008);
                reg(&mut v, 0x60, 0x30);
                v.vram[0x60000..0x60002].fill(0);
                v.vram[0x40000..0x40004].copy_from_slice(&[0x12, 0x34, 0x12, 0x34]);
            }
        }
        let frame = if selector == 0 {
            v.composite(&sprites)
        } else {
            v.framebuffer()
        }
        .unwrap();
        assert_eq!(
            pixel(&frame, 2, 0),
            [31, 31, 191, 255],
            "selector{selector}"
        );
        assert_eq!(
            pixel(&frame, 3, 0),
            [191, 223, 223, 255],
            "selector{selector}"
        );
    }
}
