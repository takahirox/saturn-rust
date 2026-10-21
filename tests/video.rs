//! Independent literal vectors derived from Sega ST-013 §§4,6,7 and ST-058
//! §§3–6,8–12. Expected pixels/registers are authored here, not renderer output.
use saturn_rust::{
    bus::{Bus, Memory},
    devices::{VDP2, WIDTH},
    vdp1::{FRAMEBUFFER, REGISTERS, VRAM, Vdp1},
    vdp2::{CRAM, Vdp2},
};

fn command(v: &mut Vdp1, at: usize, words: &[(usize, u16)]) {
    v.vram[at..at + 32].fill(0);
    for &(offset, value) in words {
        v.vram[at + offset..at + offset + 2].copy_from_slice(&value.to_be_bytes());
    }
}
fn setup() -> Vdp1 {
    let mut v = Vdp1::default();
    command(&mut v, 0, &[(0, 9), (20, 511), (22, 255)]);
    command(&mut v, 64, &[(0, 0x8000)]);
    v
}
fn draw(v: &mut Vdp1) {
    v.write16(REGISTERS + 4, 1).unwrap();
    let mut completed = false;
    for _ in 0..2000 {
        if v.advance(v.clocks_to_event()).unwrap() {
            completed = true;
            break;
        }
    }
    assert!(completed, "VDP1 must fetch END");
    assert_eq!(v.read16(REGISTERS + 0x10).unwrap() & 2, 2);
}
fn pixel(v: &Vdp1, x: usize, y: usize) -> u16 {
    let at = (y * 512 + x) * 2;
    u16::from_be_bytes(v.draw_buffer()[at..at + 2].try_into().unwrap())
}

#[test]
fn vdp1_bus_widths_aliases_and_atomic_register_writes() {
    let mut b = Bus::default();
    for base in [VRAM, FRAMEBUFFER] {
        b.write32(base, 0x801F83E0).unwrap();
        assert_eq!(b.read16(base + 0x20000000).unwrap(), 0x801F);
        b.write8(base + 3, 0xAA).unwrap();
        assert_eq!(b.read32(base).unwrap(), 0x801F83AA);
        assert!(b.write32(base + 1, 0).is_err());
    }
    assert!(b.read16(REGISTERS).is_err());
    assert!(b.write16(REGISTERS + 0x10, 1).is_err());
    assert!(b.write32(REGISTERS, 0x00000008).is_err());
    assert_eq!(b.read16(REGISTERS + 0x16).unwrap(), 0x1000);
    b.write16(CRAM, 0x801F).unwrap();
    assert_eq!(b.read16(CRAM + 0x800).unwrap(), 0x801F); // mode-0 mirrors
    b.write16(VDP2 + 0xE, 0x1000).unwrap();
    b.write16(CRAM + 0x800, 0xFC00).unwrap();
    assert_eq!(b.read16(CRAM).unwrap(), 0x801F);
    assert_eq!(b.read16(CRAM + 0x800).unwrap(), 0xFC00);
}

#[test]
fn normal_sprite_dimensions_flip_transparency_end_codes_and_all_texture_formats() {
    for mode in 0..=5 {
        let mut v = setup();
        let color = if mode == 1 { 0x200 } else { 0x120 };
        command(
            &mut v,
            32,
            &[
                (0, 0),
                (4, mode << 3),
                (6, color),
                (8, 0x100),
                (10, 0x102),
                (12, 10),
                (14, 20),
            ],
        );
        match mode {
            0 | 1 => v.vram[0x800..0x808]
                .copy_from_slice(&[0x01, 0xF2, 0xF3, 0x45, 0x67, 0x89, 0xAB, 0xCD]),
            2..=4 => v.vram[0x800..0x810]
                .copy_from_slice(&[0, 1, 255, 2, 255, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13]),
            _ => {
                for (i, word) in [
                    0, 0x801F, 0x7FFF, 0x83E0, 0x7FFF, 0xFC00, 0x8000, 0xFFFF, 1, 2, 3, 4, 5, 6, 7,
                    8,
                ]
                .iter()
                .enumerate()
                {
                    v.vram[0x800 + i * 2..0x802 + i * 2].copy_from_slice(&u16::to_be_bytes(*word));
                }
            }
        }
        // LUT entry 1 and 2 map independently to RGB555.
        v.vram[0x100C..0x100E].copy_from_slice(&0xFFFFu16.to_be_bytes());
        v.vram[0x1002..0x1006].copy_from_slice(&[0x80, 0x1F, 0x83, 0xE0]);
        draw(&mut v);
        assert_eq!(pixel(&v, 10, 20), 0);
        let (first, second) = match mode {
            0 => (0x121, 0x122),
            1 | 5 => (0x801F, 0x83E0),
            _ => (0x101, 0x102),
        };
        if mode == 3 || mode == 4 {
            assert_eq!(pixel(&v, 11, 20), 0x101);
        } else {
            assert_eq!(pixel(&v, 11, 20), first);
        }
        assert_eq!(pixel(&v, 12, 20), 0);
        assert_eq!(pixel(&v, 13, 20), second);
        assert_eq!(pixel(&v, 15, 20), 0); // second END terminates remaining row
        assert_ne!(pixel(&v, 10, 21), 0); // reset end-code count on next row
        assert_eq!(pixel(&v, 18, 21), 0); // width eight
    }
    let mut v = setup();
    command(
        &mut v,
        32,
        &[
            (0, 0x30),
            (4, 0xE8),
            (8, 0x100),
            (10, 0x102),
            (12, 1),
            (14, 1),
        ],
    );
    for i in 0..16 {
        v.vram[0x800 + i * 2..0x802 + i * 2].copy_from_slice(&(0x8000 + i as u16).to_be_bytes());
    }
    draw(&mut v);
    assert_eq!(pixel(&v, 1, 1), 0x800F);
    assert_eq!(pixel(&v, 8, 2), 0x8000);
}

#[test]
fn command_calls_skips_return_local_and_user_system_clip_are_executed() {
    let mut v = setup();
    command(&mut v, 32, &[(0, 0x200A), (2, 0x20), (12, 10), (14, 20)]); // CALL local
    command(&mut v, 64, &[(0, 0x5004), (2, 0x10), (6, 0xFFFF)]); // skip assign END at 128
    command(&mut v, 128, &[(0, 0x8000)]);
    command(
        &mut v,
        256,
        &[(0, 8), (12, 12), (14, 22), (20, 14), (22, 24)],
    );
    command(
        &mut v,
        288,
        &[
            (0, 0x3004),
            (4, 0x480),
            (6, 0x801F),
            (12, 0),
            (14, 0),
            (16, 8),
            (18, 0),
            (20, 8),
            (22, 8),
            (24, 0),
            (26, 8),
        ],
    );
    draw(&mut v);
    assert_eq!(pixel(&v, 11, 22), 0);
    assert_eq!(pixel(&v, 12, 22), 0x801F);
    assert_eq!(pixel(&v, 14, 24), 0x801F);
    assert_eq!(pixel(&v, 15, 24), 0);
    assert_eq!(v.read16(REGISTERS + 0x14).unwrap(), 16);
    v.write16(REGISTERS + 2, 3).unwrap();
    v.vblank_out();
    assert_eq!(v.display_pixel(12, 22), 0x801F);
    assert_eq!(v.read16(REGISTERS + 0x12).unwrap(), 16);
    assert_eq!(v.read16(REGISTERS + 0x10).unwrap(), 1);
}

#[test]
fn scaled_anchor_distorted_polygon_polyline_and_line_have_independent_coverage() {
    for kind in [1, 2, 4, 5, 6] {
        let mut v = setup();
        command(
            &mut v,
            32,
            &[
                (0, kind),
                (4, 0xC8),
                (6, 0x801F),
                (8, 0x100),
                (10, 0x101),
                (12, 10),
                (14, 10),
                (16, 14),
                (18, 10),
                (20, 14),
                (22, 14),
                (24, 10),
                (26, 14),
            ],
        );
        // One row of 8 lookup indices, all entry one.
        v.vram[0x800..0x804].fill(0x11);
        v.vram[0x400FA..0x400FC].copy_from_slice(&0x83E0u16.to_be_bytes()); // LUT base 0x801F*8 + 2
        draw(&mut v);
        let color = if kind <= 2 { 0x83E0 } else { 0x801F };
        assert_eq!(pixel(&v, 10, 10), color);
        assert_eq!(pixel(&v, 14, 10), color);
        assert_eq!(pixel(&v, 12, 12), if kind <= 4 { color } else { 0 });
        assert_eq!(pixel(&v, 10, 14), if kind == 6 { 0 } else { color });
    }
    let mut v = setup();
    command(
        &mut v,
        32,
        &[
            (0, 0xA01),
            (4, 0xC0),
            (6, 0x120),
            (8, 0x100),
            (10, 0x101),
            (12, 20),
            (14, 20),
            (16, 4),
            (18, 4),
        ],
    ); // center zoom anchor
    v.vram[0x800..0x804].fill(0x11);
    draw(&mut v);
    assert_eq!(pixel(&v, 18, 18), 0x121);
    assert_eq!(pixel(&v, 22, 22), 0x121);
    assert_eq!(pixel(&v, 17, 20), 0);
}

#[test]
fn color_calculation_gouraud_mesh_and_msb_shadow_match_literal_vectors() {
    for (mode, old, expected) in [
        (0, 0xFC00u16, 0x801F),
        (1, 0xFFFF, 0xBDEF),
        (1, 0x1234, 0x1234),
        (2, 0, 0x800F),
        (3, 0xFC00, 0xBC0F),
        (3, 0, 0x801F),
        (4, 0, 0x801E),
        (6, 0, 0x800F),
        (7, 0xFC00, 0xBC0F),
        (0x8000, 0x1234, 0x9234),
    ] {
        let mut v = setup();
        v.draw_buffer_mut()[0..2].copy_from_slice(&old.to_be_bytes());
        command(
            &mut v,
            32,
            &[
                (0, 6),
                (4, 0xC0 | mode),
                (6, 0x801F),
                (12, 0),
                (14, 0),
                (16, 0),
                (18, 0),
                (28, 0x100),
            ],
        );
        v.vram[0x800..0x808].copy_from_slice(&[0x42, 0x0F, 0x42, 0x0F, 0x42, 0x0F, 0x42, 0x0F]); // R -1, G/B neutral
        draw(&mut v);
        assert_eq!(pixel(&v, 0, 0), expected, "mode {mode:X}");
    }
    let mut v = setup();
    command(&mut v, 32, &[(0, 6), (4, 0x1C0), (6, 0x801F), (16, 7)]);
    draw(&mut v);
    for x in 0..8 {
        assert_eq!(pixel(&v, x, 0), if x % 2 == 0 { 0x801F } else { 0 });
    }
}

#[test]
fn command_deadlines_forced_stop_and_cyclic_lists_are_bounded() {
    let mut v = setup();
    command(&mut v, 32, &[(0, 0x5000), (2, 4)]); // skip assign self
    v.write16(REGISTERS + 4, 1).unwrap();
    assert!(!v.advance(31).unwrap());
    assert_eq!(v.read16(REGISTERS + 0x14).unwrap(), 0);
    assert!(!v.advance(1).unwrap());
    for _ in 0..1000 {
        assert!(!v.advance(32).unwrap());
    }
    assert_eq!(v.read16(REGISTERS + 0x10).unwrap(), 0);
    v.write16(REGISTERS + 12, 0).unwrap();
    assert_eq!(v.clocks_to_event(), u32::MAX);
    assert!(!v.advance(1000).unwrap());
    command(&mut v, 32, &[(0, 7)]);
    v.write16(REGISTERS + 4, 1).unwrap();
    v.advance(32).unwrap();
    assert!(v.advance(32).is_err());
    assert_eq!(v.read16(REGISTERS + 0x10).unwrap(), 0);
}

#[test]
fn framebuffer_change_and_erase_window_are_separate_from_draw_trigger() {
    let mut v = Vdp1::default();
    v.draw_buffer_mut().fill(0xFF);
    v.write16(REGISTERS + 2, 3).unwrap();
    v.vblank_out();
    assert_eq!(v.display_pixel(0, 0), 0xFFFF);
    v.draw_buffer_mut().fill(0xFF);
    for (off, val) in [(6, 0x801F), (8, 2 << 9 | 3), (10, 3 << 9 | 4), (2, 2)] {
        v.write16(REGISTERS + off, val).unwrap();
    }
    v.vblank_out();
    assert_eq!(v.display_pixel(16, 3), 0x801F);
    assert_eq!(v.display_pixel(23, 4), 0x801F);
    assert_eq!(pixel(&v, 15, 3), 0xFFFF);
    assert_eq!(pixel(&v, 24, 4), 0xFFFF);
    assert_eq!(pixel(&v, 16, 3), 0xFFFF);
    assert_eq!(v.clocks_to_event(), u32::MAX);
}

#[test]
fn automatic_start_preserves_previous_end_status_and_persistent_vblank_erase() {
    let mut v = setup();
    command(&mut v, 32, &[(0, 0x8000)]);
    draw(&mut v);
    v.write16(REGISTERS + 4, 2).unwrap();
    v.vblank_out();
    assert_eq!(v.read16(REGISTERS + 0x10).unwrap(), 1); // BEF survives automatic start
    assert_eq!(v.read16(REGISTERS + 0x16).unwrap(), 0x1100);
    let mut v = Vdp1::default();
    for (o, n) in [(6, 0x801F), (8, 0), (10, 0x200), (0, 8), (2, 3)] {
        v.write16(REGISTERS + o, n).unwrap();
    }
    v.vblank_out();
    assert_eq!(pixel(&v, 0, 0), 0x801F); // old display is now draw
    v.draw_buffer_mut()[..2].copy_from_slice(&0xFC00u16.to_be_bytes());
    v.vblank_out();
    assert_eq!(pixel(&v, 0, 0), 0xFC00); // no repeated manual change
    assert_eq!(v.display_pixel(0, 0), 0x801F); // VBE erases display every field
}

#[test]
fn malformed_commands_and_links_never_panic_or_report_false_completion() {
    let mut seed = 0x713CA5E9u32;
    for _ in 0..128 {
        let mut v = setup();
        let mut words = [(0, 0); 16];
        for (i, word) in words.iter_mut().enumerate() {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *word = (i * 2, (seed >> 16) as u16);
        }
        words[0].1 &= 0x7FFF; // no END at first randomly authored command
        command(&mut v, 32, &words);
        v.write16(REGISTERS + 4, 1).unwrap();
        for _ in 0..4 {
            match v.advance(v.clocks_to_event()) {
                Err(_) => {
                    assert_eq!(v.read16(REGISTERS + 0x10).unwrap() & 2, 0);
                    break;
                }
                Ok(true) => break,
                Ok(false) => (),
            }
        }
    }
}

fn bitmap() -> Vdp2 {
    let mut v = Vdp2::default();
    for (o, n) in [
        (0, 0x8000),
        (0x20, 0x101),
        (0x28, 0x32),
        (0x78, 1),
        (0x7C, 1),
        (0xF8, 1),
    ] {
        v.write16(VDP2 + o, n).unwrap();
    }
    v
}
fn rgb(v: &Vdp2, x: usize, y: usize) -> Vec<u8> {
    v.framebuffer().unwrap()[(y * WIDTH + x) * 4..][..3].to_vec()
}

#[test]
fn vdp2_bitmap_scroll_zoom_wrap_and_mosaic_use_vram_coordinates() {
    let mut v = bitmap();
    for (x, color) in [(0, 0x801Fu16), (1, 0x83E0), (2, 0xFC00), (511, 0xFFFF)] {
        v.vram[x * 2..x * 2 + 2].copy_from_slice(&color.to_be_bytes());
    }
    assert_eq!(rgb(&v, 0, 0), [255, 0, 0]);
    v.write16(VDP2 + 0x70, 511).unwrap();
    assert_eq!(rgb(&v, 0, 0), [255, 255, 255]);
    assert_eq!(rgb(&v, 1, 0), [255, 0, 0]);
    v.write16(VDP2 + 0x70, 0).unwrap();
    v.write16(VDP2 + 0x78, 0).unwrap();
    v.write16(VDP2 + 0x7A, 0x8000).unwrap();
    assert_eq!(rgb(&v, 1, 0), [255, 0, 0]);
    assert_eq!(rgb(&v, 2, 0), [0, 255, 0]);
    v.write16(VDP2 + 0x22, 0x101).unwrap(); // horizontal 2-dot mosaic
    assert_eq!(rgb(&v, 3, 0), [0, 255, 0]);
}

#[test]
fn vdp2_palette_rgb24_transparency_back_screen_and_color_offsets() {
    let mut v = bitmap();
    v.write16(VDP2 + 0x28, 0x12).unwrap(); // 256-color bitmap
    v.vram[0] = 1;
    v.cram[2..4].copy_from_slice(&0x83E0u16.to_be_bytes());
    assert_eq!(rgb(&v, 0, 0), [0, 255, 0]);
    v.write16(VDP2 + 0xE, 0x2000).unwrap();
    v.cram[4..8].copy_from_slice(&[0, 30, 20, 10]);
    assert_eq!(rgb(&v, 0, 0), [10, 20, 30]);
    v.write16(VDP2 + 0x28, 0x42).unwrap();
    v.vram[..8].copy_from_slice(&[0x80, 30, 20, 10, 0, 1, 2, 3]);
    v.write16(VDP2 + 0x20, 1).unwrap(); // enable transparency
    v.write16(VDP2 + 0xAE, 0x4000).unwrap();
    v.vram[0x8000..0x8002].copy_from_slice(&0xFC00u16.to_be_bytes());
    assert_eq!(rgb(&v, 0, 0), [10, 20, 30]);
    assert_eq!(rgb(&v, 1, 0), [0, 0, 255]);
    for (o, n) in [(0x110, 1), (0x114, 0x1EC), (0x116, 250), (0x118, 1)] {
        v.write16(VDP2 + o, n).unwrap();
    }
    assert_eq!(rgb(&v, 0, 0), [0, 255, 31]);
}

#[test]
fn vdp2_two_bitmaps_windows_priorities_blending_and_vdp1_composition() {
    let mut v = bitmap();
    v.write16(VDP2 + 0x20, 0x303).unwrap();
    v.write16(VDP2 + 0x28, 0x3232).unwrap();
    v.write16(VDP2 + 0x3C, 0x20).unwrap(); // NBG1 starts bank 2
    for o in [0x88, 0x8C] {
        v.write16(VDP2 + o, 1).unwrap();
    }
    v.write16(VDP2 + 0xF8, 0x201).unwrap();
    v.vram[0..2].copy_from_slice(&0x801Fu16.to_be_bytes());
    v.vram[0x40000..0x40002].copy_from_slice(&0xFC00u16.to_be_bytes());
    assert_eq!(rgb(&v, 0, 0), [0, 0, 255]);
    v.write16(VDP2 + 0xEC, 2).unwrap();
    v.write16(VDP2 + 0x108, 0xF00).unwrap();
    assert_eq!(rgb(&v, 0, 0), [127, 0, 127]);
    v.write16(VDP2 + 0x110, 2).unwrap();
    v.write16(VDP2 + 0x114, 20).unwrap();
    assert_eq!(rgb(&v, 0, 0), [147, 0, 127]); // §13.1: offset after blending
    v.write16(VDP2 + 0x110, 1).unwrap();
    assert_eq!(rgb(&v, 0, 0), [127, 0, 127]); // second screen's offset does not apply
    v.write16(VDP2 + 0x110, 0).unwrap();
    // Normal windows use half-resolution horizontal register units.
    for (o, n) in [(0xD0, 0x200), (0xC0, 2), (0xC4, 4), (0xC2, 0), (0xC6, 0)] {
        v.write16(VDP2 + o, n).unwrap();
    }
    assert_eq!(rgb(&v, 0, 0), [255, 0, 0]);
    let mut s = setup();
    command(&mut s, 32, &[(0, 6), (4, 0xC0), (6, 0x83E0)]);
    draw(&mut s);
    s.write16(REGISTERS + 2, 3).unwrap();
    s.vblank_out();
    v.write16(VDP2 + 0xE0, 0x20).unwrap();
    v.write16(VDP2 + 0xF0, 3).unwrap();
    assert_eq!(&v.composite(&s).unwrap()[..3], &[0, 255, 0]);
    v.write16(VDP2 + 0xF0, 0).unwrap();
    assert_eq!(&v.composite(&s).unwrap()[..3], &[255, 0, 0]);
}

// ST-058 §§9.1/9.2/14.1: expected pixels are literal register/palette
// vectors, including the ordering of blend, offset, then shadow.
fn scanout(kind: u16, data: &[u16]) -> Vdp1 {
    let mut s = Vdp1::default();
    if kind >= 8 {
        s.write16(REGISTERS, 1).unwrap();
    }
    for (i, value) in data.iter().enumerate() {
        if kind >= 8 {
            s.draw_buffer_mut()[i] = *value as u8;
        } else {
            s.draw_buffer_mut()[i * 2..i * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
    }
    s.write16(REGISTERS + 2, 3).unwrap();
    s.vblank_out();
    s
}
#[test]
fn all_sprite_types_select_independent_priority_ratio_and_palette_fields() {
    for (kind, data, ratio, color) in [
        (0, 0xD801, 3, 1),
        (1, 0xD801, 3, 1),
        (2, 0x5801, 3, 1),
        (3, 0x5801, 3, 1),
        (4, 0x5401, 5, 1),
        (5, 0x5801, 1, 1),
        (6, 0x5C01, 3, 1),
        (7, 0x5E01, 7, 1),
        (8, 0x81, 0, 1),
        (9, 0xC1, 1, 1),
        (10, 0x81, 0, 1),
        (11, 0xC1, 3, 1),
        (12, 0x81, 0, 0x81),
        (13, 0xC1, 1, 0xC1),
        (14, 0x81, 0, 0x81),
        (15, 0xC1, 3, 0xC1),
    ] {
        let mut v = bitmap();
        v.vram[..2].copy_from_slice(&0x801Fu16.to_be_bytes());
        v.cram[color * 2..color * 2 + 2].copy_from_slice(&0x83E0u16.to_be_bytes());
        for off in [0xF0, 0xF2, 0xF4, 0xF6] {
            v.write16(VDP2 + off, 0x0707).unwrap();
        }
        // register r holds ratio r*4+3; all eight selectors differ.
        for r in 0..8 {
            let at = 0x100 + r / 2 * 2;
            let old = v.read16(VDP2 + at).unwrap();
            v.write16(VDP2 + at, old | (((r * 4 + 3) << (r % 2 * 8)) as u16))
                .unwrap();
        }
        v.write16(VDP2 + 0xE0, 0x2000 | kind).unwrap();
        v.write16(VDP2 + 0xEC, 0x40).unwrap();
        let s = scanout(kind, &[data]);
        let ratio = ratio * 4 + 3;
        assert_eq!(
            &v.composite(&s).unwrap()[..3],
            &[
                ((255 * (ratio + 1)) >> 5) as u8,
                ((255 * (31 - ratio)) >> 5) as u8,
                0
            ],
            "type {kind:X}"
        );
        // No sprite priority means transparent, including all shared-bit types.
        for off in [0xF0, 0xF2, 0xF4, 0xF6] {
            v.write16(VDP2 + off, 0).unwrap();
        }
        assert_eq!(&v.composite(&s).unwrap()[..3], &[255, 0, 0]);
    }
}
#[test]
fn normal_and_msb_shadows_follow_enable_priority_and_color_offset_order() {
    let mut v = bitmap();
    v.vram[..2].copy_from_slice(&0xC210u16.to_be_bytes()); // RGB132
    v.write16(VDP2 + 0xF0, 3).unwrap();
    let normal = scanout(0, &[0x7FE]);
    assert_eq!(&v.composite(&normal).unwrap()[..3], &[132, 132, 132]);
    v.write16(VDP2 + 0xE2, 1).unwrap();
    assert_eq!(&v.composite(&normal).unwrap()[..3], &[66, 66, 66]);
    v.write16(VDP2 + 0x110, 1).unwrap();
    v.write16(VDP2 + 0x114, 20).unwrap();
    assert_eq!(&v.composite(&normal).unwrap()[..3], &[76, 66, 66]);
    v.write16(VDP2 + 0xF8, 4).unwrap(); // shadow below background
    assert_eq!(&v.composite(&normal).unwrap()[..3], &[152, 132, 132]);
    v.write16(VDP2 + 0xF8, 1).unwrap();
    v.write16(VDP2 + 0xE0, 3).unwrap();
    v.cram[2..4].copy_from_slice(&0x83E0u16.to_be_bytes());
    let shaded = scanout(3, &[0x8001]);
    assert_eq!(&v.composite(&shaded).unwrap()[..3], &[0, 127, 0]);
    let transparent = scanout(3, &[0x8000]);
    assert_eq!(&v.composite(&transparent).unwrap()[..3], &[152, 132, 132]);
    v.write16(VDP2 + 0xE2, 0x101).unwrap();
    assert_eq!(&v.composite(&transparent).unwrap()[..3], &[76, 66, 66]);
}
#[test]
fn sprite_window_masks_normal_layers_and_combines_with_rectangles() {
    let mut v = bitmap();
    v.write16(VDP2 + 0xAE, 0xF000).unwrap();
    for x in 0..4 {
        v.vram[x * 2..x * 2 + 2].copy_from_slice(&0x801Fu16.to_be_bytes());
    }
    v.write16(VDP2 + 0xE0, 0x13).unwrap(); // type3 SD becomes window
    v.write16(VDP2 + 0xD0, 0x20).unwrap(); // inside sprite window
    let s = scanout(3, &[0x8000, 0, 0x8000, 0]);
    let f = v.composite(&s).unwrap();
    assert_eq!(
        &f[..16],
        &[255, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 255]
    );
    v.write16(VDP2 + 0xD0, 0x30).unwrap(); // outside
    assert_eq!(
        &v.composite(&s).unwrap()[..8],
        &[0, 0, 0, 255, 255, 0, 0, 255]
    );
    for (off, value) in [(0xC0, 0), (0xC4, 2), (0xC2, 0), (0xC6, 0), (0xD0, 0xA2)] {
        v.write16(VDP2 + off, value).unwrap();
    } // inside rectangle AND inside sprite window
    assert_eq!(
        &v.composite(&s).unwrap()[..16],
        &[255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255]
    );
}
#[test]
fn palette_msb_calculation_uses_cram_metadata_in_both_color_formats() {
    for mode in [0, 1, 2] {
        let mut v = bitmap();
        v.vram[..2].copy_from_slice(&0x801Fu16.to_be_bytes());
        v.write16(VDP2 + 0xE, mode << 12).unwrap();
        v.write16(VDP2 + 0xE0, 0x3000).unwrap();
        v.write16(VDP2 + 0xEC, 0x40).unwrap();
        v.write16(VDP2 + 0xF0, 3).unwrap();
        v.write16(VDP2 + 0x100, 15).unwrap();
        let at = if mode == 2 { 4 } else { 2 };
        if mode == 2 {
            v.cram[at..at + 4].copy_from_slice(&[0, 0, 255, 0]);
        } else {
            v.cram[at..at + 2].copy_from_slice(&0x03E0u16.to_be_bytes());
        }
        let s = scanout(0, &[1]);
        assert_eq!(&v.composite(&s).unwrap()[..3], &[0, 255, 0]);
        v.cram[at] |= 0x80;
        assert_eq!(&v.composite(&s).unwrap()[..3], &[127, 127, 0]);
    }
}

#[test]
fn high_speed_shrink_selects_original_x_parity_and_ignores_end_codes() {
    // ST-013 §6.3/fig.6.5, §4.2 EOS. Four reduced destination pixels
    // sample literal source coordinates; flip occurs before parity selection.
    for kind in [1, 2] {
        for (hss, eos, flip, indices) in [
            (false, false, false, [0, 2, 4, 7]),
            (true, false, false, [0, 2, 4, 6]),
            (true, true, false, [1, 3, 5, 7]),
            (true, false, true, [6, 4, 2, 0]),
            (true, true, true, [7, 5, 3, 1]),
        ] {
            let mut v = setup();
            v.write16(REGISTERS + 2, 2 | if eos { 16 } else { 0 })
                .unwrap();
            command(
                &mut v,
                32,
                &[
                    (0, kind | if flip { 16 } else { 0 }),
                    (4, 0x68 | if hss { 0x1000 } else { 0 }),
                    (8, 0x100),
                    (10, 0x101),
                    (12, 10),
                    (14, 20),
                    (16, 13),
                    (18, 20),
                    (20, 13),
                    (22, 20),
                    (24, 10),
                    (26, 20),
                ],
            );
            for i in 0..8 {
                v.vram[0x800 + i * 2..0x802 + i * 2]
                    .copy_from_slice(&(0x8000 + i as u16).to_be_bytes());
            }
            draw(&mut v);
            for (x, source) in indices.into_iter().enumerate() {
                assert_eq!(
                    pixel(&v, 10 + x, 20),
                    0x8000 + source,
                    "kind={kind},HSS={hss},EOS={eos},flip={flip}"
                );
            }
        }
    }
    // HSS disables end codes for unchanged size and expansion too, while
    // preserving ordinary coordinates (including odd ones) on these lines.
    for end_x in [17, 19] {
        let mut v = setup();
        command(
            &mut v,
            32,
            &[
                (0, 1),
                (4, 0x1068),
                (8, 0x100),
                (10, 0x101),
                (12, 10),
                (14, 20),
                (20, end_x),
                (22, 20),
            ],
        );
        for i in 0..8 {
            let color = if i == 1 || i == 2 {
                0x7fff
            } else {
                0x8000 + i as u16
            };
            v.vram[0x800 + i * 2..0x802 + i * 2].copy_from_slice(&color.to_be_bytes());
        }
        draw(&mut v);
        assert_eq!(pixel(&v, 10, 20), 0x8000);
        assert_eq!(pixel(&v, end_x as usize, 20), 0x8007);
        assert!((10..=end_x as usize).any(|x| pixel(&v, x, 20) == 0x7fff));
    }
    // EOS is a real readable MODR field and remains distinct from the
    // still-unimplemented double-interlace draw controls.
    let mut v = setup();
    v.write16(REGISTERS + 2, 0x12).unwrap();
    assert_eq!(v.read16(REGISTERS + 0x16).unwrap() & 0x80, 0x80);
    assert!(v.write16(REGISTERS + 2, 0x1c).is_err());
}
