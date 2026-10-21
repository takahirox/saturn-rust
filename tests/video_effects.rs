//! Independent ST-058-R2 vectors: §5.3 figs.5.4–5.8, §6.1 p.147,
//! §6.3 figs.6.2–6.4, §6.4 fig.6.7 and §8.1 figs.8.2–8.4.
use saturn_rust::{
    devices::{VDP2, WIDTH},
    vdp2::Vdp2,
};
fn reg(v: &mut Vdp2, offset: u32, value: u16) {
    v.write16(VDP2 + offset, value).unwrap();
}
fn long_reg(v: &mut Vdp2, offset: u32, value: u32) {
    reg(v, offset, (value >> 16) as u16);
    reg(v, offset + 2, value as u16);
}
fn word(v: &mut Vdp2, offset: usize, value: u16) {
    v.vram[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
}
fn long(v: &mut Vdp2, offset: usize, value: u32) {
    v.vram[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}
fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
    &frame[(y * WIDTH + x) * 4..][..4]
}
const RED: &[u8] = &[255, 0, 0, 255];
const GREEN: &[u8] = &[0, 255, 0, 255];
const BLUE: &[u8] = &[0, 0, 255, 255];
const BLACK: &[u8] = &[0, 0, 0, 255];
fn bitmap() -> Vdp2 {
    let mut v = Vdp2::default();
    for (at, value) in [
        (0, 0x8000),
        (0x20, 0x101),
        (0x28, 0x32),
        (0x78, 1),
        (0x7C, 1),
        (0xF8, 3),
        (0xAC, 3),
        (0xAE, 0x8000),
    ] {
        reg(&mut v, at, value);
    }
    for (x, y, color) in [
        (0, 0, 0x801F),
        (1, 0, 0x83E0),
        (2, 0, 0xFC00),
        (0, 1, 0xFC00),
        (1, 1, 0x801F),
        (2, 1, 0x83E0),
    ] {
        word(&mut v, (y * 512 + x) * 2, color);
    }
    v
}
#[test]
fn line_scroll_uses_packed_relative_fields_fractional_zoom_and_read_intervals() {
    let mut v = bitmap();
    long_reg(&mut v, 0xA0, 0x20000); // byte address 40000, not 80000
    reg(&mut v, 0x9A, 0x1E); // X,Y,zoom every two lines
    long(&mut v, 0x40000, 0x00008000); // X=0.5
    long(&mut v, 0x40004, 0); // Y=0
    long(&mut v, 0x40008, 0x8000); // dx=0.5
    long(&mut v, 0x4000C, 0x00020000); // next group X=2
    long(&mut v, 0x40010, 0); // relative Y=0, not Y=2
    long(&mut v, 0x40014, 0x10000);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), RED);
    assert_eq!(pixel(&f, 1, 0), GREEN); // fractional sum is retained
    assert_eq!(pixel(&f, 2, 0), GREEN);
    assert_eq!(pixel(&f, 0, 1), BLUE); // held Y plus vertical increment
    assert_eq!(pixel(&f, 0, 2), BLUE); // next table Y restarts at relative zero
    long_reg(&mut v, 0x70, 0x10000); // base X is added, never replaced
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
}
#[test]
fn all_line_table_field_subsets_have_no_padding_and_both_layers_are_independent() {
    for layer in 0..2 {
        for fields in [2, 4, 6, 8, 10, 12, 14] {
            let mut v = bitmap();
            reg(&mut v, 0x20, 0x100 << layer | 1 << layer);
            reg(&mut v, 0x28, 0x32 << (layer * 8));
            reg(&mut v, 0xF8, 3 << (layer * 8));
            long_reg(&mut v, 0x78 + layer * 16, 0x10000);
            long_reg(&mut v, 0x7C + layer * 16, 0); // isolate table Y
            long_reg(&mut v, 0xA0 + layer * 4, 0x20000);
            reg(&mut v, 0x9A, fields << (layer * 8));
            let mut at = 0x40000;
            for _ in 0..2 {
                for (bit, value) in [(2, 0x10000), (4, 0), (8, 0x10000)] {
                    if fields & bit != 0 {
                        long(&mut v, at, value);
                        at += 4;
                    }
                }
            }
            let f = v.framebuffer().unwrap();
            assert_eq!(
                pixel(&f, 0, 1),
                if fields & 2 != 0 { GREEN } else { RED },
                "layer={layer},fields={fields}"
            );
        }
    }
}
#[test]
fn vertical_cell_tables_interleave_and_mosaic_disables_cell_scroll() {
    let mut v = bitmap();
    reg(&mut v, 0x20, 0x303);
    reg(&mut v, 0x28, 0x3232);
    for at in [0x88, 0x8C] {
        reg(&mut v, at, 1);
    }
    reg(&mut v, 0x9A, 0x101);
    long_reg(&mut v, 0x9C, 0x20000);
    long(&mut v, 0x40000, 0x10000); // NBG0 first cell -> blue
    long(&mut v, 0x40004, 0); // NBG1 first cell -> red
    long(&mut v, 0x40008, 0);
    long(&mut v, 0x4000C, 0x10000);
    word(&mut v, 16, 0x801F);
    word(&mut v, 1040, 0xFC00);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLUE);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 8, 0), RED);
    reg(&mut v, 0xF8, 0x403);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), RED);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 8, 0), BLUE);
    reg(&mut v, 0x22, 2); // mosaic on NBG1 has priority even at size 1
    assert_eq!(pixel(&v.framebuffer().unwrap(), 8, 0), RED);
}
#[test]
fn line_windows_have_inclusive_edges_vertical_bounds_inversion_and_vram_wrap() {
    let mut v = bitmap();
    reg(&mut v, 0xD0, 2); // W0 inside
    reg(&mut v, 0xC2, 0);
    reg(&mut v, 0xC6, 1);
    long_reg(&mut v, 0xD8, 0x8003FFFE); // first line at 7FFFC; next wraps to 0
    word(&mut v, 0x7FFFC, 2);
    word(&mut v, 0x7FFFE, 4);
    // Second line window values are also ordinary bitmap pixels at VRAM zero.
    word(&mut v, 0, 0);
    word(&mut v, 2, 0);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), BLACK);
    assert_eq!(pixel(&f, 2, 0), BLUE); // inclusive end, horizontal coordinates /2
    assert_eq!(pixel(&f, 0, 1), BLUE);
    assert_eq!(pixel(&f, 1, 1), BLACK);
    reg(&mut v, 0xD0, 3); // outside
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 1), BLACK);
    word(&mut v, 0x7FFFC, 6); // reversed start/end -> empty inside
    reg(&mut v, 0xD0, 2);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 2, 0), BLACK);
}
fn rotation() -> Vdp2 {
    let mut v = bitmap();
    reg(&mut v, 0x20, 0x1010);
    reg(&mut v, 0x2A, 0x3200);
    reg(&mut v, 0xFC, 4);
    long_reg(&mut v, 0xBC, 0x20040); // bit6 ignored: A=40000 B=40080
    for at in [0x40000, 0x40080] {
        for off in [0x10, 0x14, 0x1C, 0x2C, 0x4C, 0x50] {
            long(&mut v, at + off, 0x10000);
        }
    }
    v
}
#[test]
fn rotation_identity_signed_quarter_turn_translation_and_fractional_scaling() {
    let mut v = rotation();
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), RED);
    assert_eq!(pixel(&f, 1, 0), GREEN);
    assert_eq!(pixel(&f, 0, 1), BLUE);
    // A=0 B=1 D=-1 E=0, My=1: (x,y)->(y,1-x).
    long(&mut v, 0x4001C, 0);
    long(&mut v, 0x40020, 0x10000);
    long(&mut v, 0x40028, 0xFFFF0000);
    long(&mut v, 0x4002C, 0);
    long(&mut v, 0x40048, 0x10000);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), BLUE);
    assert_eq!(pixel(&f, 1, 0), RED);
    assert_eq!(pixel(&f, 0, 1), RED);
    // Retain fraction through start + horizontal increments, including negative start.
    let mut v = rotation();
    long(&mut v, 0x40000, 0xFFFF8000);
    long(&mut v, 0x40014, 0x8000);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 1, 0), RED);
    assert_eq!(pixel(&f, 2, 0), RED);
    assert_eq!(pixel(&f, 3, 0), GREEN);
}
#[test]
fn rotation_center_viewpoint_and_z_matrix_components_participate_in_equations() {
    let mut v = rotation();
    // X=2(Sx-Px)+2(Px-Cx)+Cx+Mx, Y=Sy+3(Zst-Pz)+3(Pz-Cz)+My.
    long(&mut v, 0x4001C, 0x20000);
    long(&mut v, 0x40030, 0x30000);
    word(&mut v, 0x40034, 7);
    word(&mut v, 0x40038, 9);
    word(&mut v, 0x4003C, 3);
    word(&mut v, 0x40040, 2);
    long(&mut v, 0x40008, 0x20000);
    long(&mut v, 0x40044, 0x40000); // X=2x+1, Y=y
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
}
#[test]
fn rotation_coefficients_cover_both_widths_all_modes_sign_and_transparency() {
    for short in [false, true] {
        for mode in 0..4 {
            let mut v = rotation();
            reg(&mut v, 0xB4, 1 | if short { 2 } else { 0 } | mode << 2);
            reg(&mut v, 0xB6, 0);
            long(&mut v, 0x40054, 0x80000000); // KAst=2: coefficient at 4/8
            if short {
                word(&mut v, 0x10000, if mode == 3 { 4 } else { 0x800 });
            } else {
                long(&mut v, 0x20000, if mode == 3 { 0x100 } else { 0x20000 });
            }
            let f = v.framebuffer().unwrap();
            if mode == 0 || mode == 1 {
                assert_eq!(pixel(&f, 1, 0), BLUE);
            }
            if mode == 2 {
                assert_eq!(pixel(&f, 1, 0), GREEN);
            }
            if mode == 3 {
                assert_eq!(pixel(&f, 0, 0), GREEN);
            }
            if short {
                word(&mut v, 0x10000, 0x8000);
            } else {
                long(&mut v, 0x20000, 0x80000000);
            }
            assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLACK);
        }
    }
    // Negative coefficient -1 reverses X, with Mx=1 to stay in range.
    let mut v = rotation();
    reg(&mut v, 0xB4, 7);
    long(&mut v, 0x40054, 0x80000000);
    word(&mut v, 0x10000, 0x7C00);
    long(&mut v, 0x40044, 1 << 16);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 1, 0), RED);
}
#[test]
fn rotation_parameter_b_window_switch_and_coefficient_switch_use_independent_tables() {
    let mut v = rotation();
    long(&mut v, 0x400C4, 1 << 16); // B Mx=1
    reg(&mut v, 0xB0, 1);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
    reg(&mut v, 0xB0, 3);
    reg(&mut v, 0xD6, 2);
    reg(&mut v, 0xC0, 0);
    reg(&mut v, 0xC4, 0);
    reg(&mut v, 0xC6, 223);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), RED);
    assert_eq!(pixel(&f, 1, 0), BLUE);
    reg(&mut v, 0xB0, 2);
    reg(&mut v, 0xB4, 3);
    long(&mut v, 0x40054, 0x80000000);
    long(&mut v, 0x4005C, 1 << 16);
    word(&mut v, 0x10000, 0x400);
    word(&mut v, 0x10002, 0x8000);
    let f = v.framebuffer().unwrap();
    assert_eq!(pixel(&f, 0, 0), RED);
    assert_eq!(pixel(&f, 1, 0), BLUE);
}
#[test]
fn rotation_screen_over_modes_repeat_transparent_and_fixed_512_area() {
    let mut v = rotation();
    long(&mut v, 0x40000, 512 << 16);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), RED); // wrapping
    reg(&mut v, 0x3A, 2 << 10);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLACK);
    long(&mut v, 0x40000, 0);
    long(&mut v, 0x40004, 256 << 16);
    reg(&mut v, 0x3A, 3 << 10); // fixed 512x512 repeats 256-high bitmap
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), RED);
    long(&mut v, 0x40004, 512 << 16);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLACK);
}
#[test]
fn rotation_tile_sixteen_plane_map_pattern_names_palette_and_screen_over_character() {
    let mut v = rotation();
    reg(&mut v, 0x20, 0x10);
    reg(&mut v, 0x2A, 0);
    reg(&mut v, 0x38, 0x8000);
    reg(&mut v, 0x50, 8);
    reg(&mut v, 0x52, 0x0B0A);
    word(&mut v, 0x10000, 0x100);
    v.vram[0x2000] = 0x10;
    v.cram[2..4].copy_from_slice(&0x1Fu16.to_be_bytes());
    v.cram[4..6].copy_from_slice(&0x3E0u16.to_be_bytes());
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), RED);
    long(&mut v, 0x40000, 1024 << 16); // third of 4 horizontal planes
    word(&mut v, 0x14000, 0x101);
    v.vram[0x2020] = 0x20;
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
    long(&mut v, 0x40000, 2048 << 16);
    reg(&mut v, 0x3A, 1 << 10);
    reg(&mut v, 0xB8, 0x101);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), GREEN);
}
#[test]
fn unimplemented_rotation_reload_and_line_color_modes_are_not_inert_latches() {
    for (offset, value) in [(0xB2, 1), (0xB4, 0x11), (0x2E, 0x20)] {
        let mut v = rotation();
        reg(&mut v, offset, value);
        assert!(v.framebuffer().is_err());
    }
}

fn guest_rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/effects.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let at = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for (i, w) in parts.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
#[test]
fn original_guest_creates_rotation_coefficient_line_scroll_and_line_window_tables() {
    use saturn_rust::{Boot, Machine, bus::Memory};
    let rom = guest_rom();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.run_steps(10000).unwrap();
        assert_eq!(m.bus().read32(0x0600A000).unwrap(), 0x600D);
        let f = m.framebuffer().unwrap();
        for (x, y, color) in [
            (0, 0, &[255, 255, 255, 255][..]),
            (1, 0, BLUE),
            (2, 0, &[255, 255, 255, 255][..]),
            (3, 0, RED),
            (0, 1, RED),
            (1, 1, &[255, 255, 255, 255][..]),
            (2, 1, RED),
            (8, 0, RED),
            (9, 0, GREEN),
            (8, 1, BLACK),
            (10, 1, GREEN),
            (13, 1, RED),
            (14, 1, BLACK),
            (0, 8, BLACK),
        ] {
            assert_eq!(pixel(&f, x, y), color, "({x},{y})");
        }
    }
    let mut changed = rom;
    let locations: Vec<_> = changed
        .windows(4)
        .enumerate()
        .filter_map(|(at, b)| (b == [0, 3, 0, 0]).then_some(at))
        .collect();
    assert_eq!(locations.len(), 1);
    changed[locations[0]..locations[0] + 4].copy_from_slice(&[0, 2, 0, 0]);
    m.reset(Boot::ExternalBios(&changed)).unwrap();
    m.run_steps(10000).unwrap();
    assert_eq!(pixel(&m.framebuffer().unwrap(), 0, 0), BLUE);
    assert_eq!(m.bus().read32(0x25E70048).unwrap(), 2 << 16);
}

#[test]
fn rotation_layer_uses_real_priority_color_ratio_offsets_and_transparent_coefficients() {
    let mut v = rotation();
    // Normal bitmap starts in bank 2, while rotation uses bank 0.
    reg(&mut v, 0x20, 0x1111);
    reg(&mut v, 0x3C, 2);
    word(&mut v, 0x40000, 0xFC00); // blue lower screen
    reg(&mut v, 0xEC, 0x10);
    reg(&mut v, 0x10C, 15);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 0, 127, 255]);
    reg(&mut v, 0x110, 0x10);
    reg(&mut v, 0x114, 20);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [147, 0, 127, 255]);
    reg(&mut v, 0xF8, 5); // normal screen above rotation, no rotation offset applied
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLUE);
    reg(&mut v, 0xF8, 3);
    reg(&mut v, 0xB4, 3);
    long(&mut v, 0x40054, 0x80000000);
    word(&mut v, 0x10000, 0x8000);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), BLUE);
}
#[test]
fn rotation_table_ignored_bits_and_extreme_signed_values_are_bounded() {
    let mut v = rotation();
    for off in [0, 4, 8] {
        long(&mut v, 0x40000 + off, 0xFFFFFFFF);
    }
    for off in [0xC, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C, 0x30] {
        long(&mut v, 0x40000 + off, 0xFFFFFFFF);
    }
    for off in [0x34, 0x36, 0x38, 0x3C, 0x3E, 0x40] {
        word(&mut v, 0x40000 + off, 0xFFFF);
    }
    for off in [0x44, 0x48, 0x4C, 0x50] {
        long(&mut v, 0x40000 + off, 0xFFFFFFFF);
    }
    let frame = v.framebuffer().unwrap();
    assert_eq!(frame.len(), 320 * 224 * 4);
    assert!(frame.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    // Reserved high bits and the six unused fractional bits do not change Xst.
    long(&mut v, 0x40000, 0x1FFFFFC0);
    assert_eq!(v.framebuffer().unwrap(), frame);
}
