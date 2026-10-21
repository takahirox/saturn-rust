//! Original pixel/name vectors: ST-058 §§4.4–4.8, figs.4.9/4.10,
//! table4.8 and §§5.1/8/11–13. These literal pixels are independent of renderer code.
use saturn_rust::{
    devices::{VDP2, WIDTH},
    vdp2::Vdp2,
};
fn reg(v: &mut Vdp2, off: u32, n: u16) {
    v.write16(VDP2 + off, n).unwrap();
}
fn word(v: &mut Vdp2, at: usize, n: u16) {
    v.vram[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
fn color(v: &mut Vdp2, index: usize, n: u16) {
    v.cram[index * 2..index * 2 + 2].copy_from_slice(&n.to_be_bytes());
}
fn at(frame: &[u8], x: usize, y: usize) -> &[u8] {
    &frame[(y * WIDTH + x) * 4..(y * WIDTH + x) * 4 + 4]
}
fn base(layer: usize, format: u16, size: bool, one: bool, aux: bool) -> Vdp2 {
    let mut v = Vdp2::default();
    reg(&mut v, 0, 0x8000);
    reg(&mut v, 0xAC, 3);
    reg(&mut v, 0xAE, 0xF800);
    reg(&mut v, 0x20, 1 << layer);
    if layer < 2 {
        reg(&mut v, 0x28, (format << 4 | u16::from(size)) << (layer * 8));
        reg(&mut v, 0x78 + layer as u32 * 16, 1);
        reg(&mut v, 0x7C + layer as u32 * 16, 1);
    } else {
        reg(
            &mut v,
            0x2A,
            (format << 1 | u16::from(size)) << ((layer - 2) * 4),
        );
    }
    reg(
        &mut v,
        0x30 + layer as u32 * 2,
        (u16::from(one) * 0x8000) | (u16::from(aux) * 0x4000),
    );
    reg(
        &mut v,
        0xF8 + (layer / 2) as u32 * 2,
        3 << ((layer & 1) * 8),
    );
    v
}
#[test]
fn every_normal_background_one_and_two_word_names_palette_and_transparency() {
    for layer in 0..4 {
        for one in [false, true] {
            for format in 0..=1 {
                let mut v = base(layer, format, false, one, false);
                if one {
                    word(&mut v, 0, 0x2020);
                } else {
                    word(&mut v, 0, if format == 0 { 2 } else { 0x20 });
                    word(&mut v, 2, 0x20);
                }
                v.vram[0x400] = if format == 0 { 0x10 } else { 1 };
                // Color 16 uses palette32; color256 ignores low palette bits.
                color(&mut v, if format == 0 { 33 } else { 513 }, 0x1F);
                let f = v.framebuffer().unwrap();
                assert_eq!(
                    at(&f, 0, 0),
                    [255, 0, 0, 255],
                    "layer={layer},one={one},fmt={format}"
                );
                assert_eq!(at(&f, 1, 0), [0, 0, 0, 255]);
                // Changing actual character RAM must change sampled output.
                v.vram[0x400] = if format == 0 { 0x20 } else { 2 };
                color(&mut v, if format == 0 { 34 } else { 514 }, 0x3E0);
                assert_eq!(at(&v.framebuffer().unwrap(), 0, 0), [0, 255, 0, 255]);
            }
        }
    }
}
#[test]
fn name_supplements_auxiliary_modes_flip_and_sixteen_pixel_cell_layout() {
    for one in [false, true] {
        for aux in [false, true] {
            for size in [false, true] {
                let mut v = base(0, 0, size, one, aux);
                let character = if size { 0x24 } else { 0x20 };
                let name = if one && size { 0x9 } else { character };
                if one {
                    word(&mut v, 0, name);
                } else {
                    word(&mut v, 0, 0);
                    word(&mut v, 2, character);
                }
                v.vram[character as usize * 32] = 0x12;
                if size {
                    v.vram[character as usize * 32 + 32] = 0x34;
                    v.vram[character as usize * 32 + 64] = 0x56;
                    v.vram[character as usize * 32 + 96] = 0x78;
                }
                for i in 1..=8 {
                    color(&mut v, i, if i & 1 == 1 { 0x1F } else { 0x3E0 });
                }
                let f = v.framebuffer().unwrap();
                assert_eq!(at(&f, 0, 0), [255, 0, 0, 255]);
                assert_eq!(at(&f, 1, 0), [0, 255, 0, 255]);
                if size {
                    for (x, y) in [(8, 0), (0, 8), (8, 8)] {
                        assert_eq!(at(&f, x, y), [255, 0, 0, 255]);
                    }
                }
                if !one || !aux {
                    word(&mut v, 0, if one { name | 0xC00 } else { 0xC000 });
                    let f = v.framebuffer().unwrap();
                    let n = if size { 16 } else { 8 };
                    assert_eq!(at(&f, n - 1, n - 1), [255, 0, 0, 255]);
                    assert_eq!(at(&f, n - 2, n - 1), [0, 255, 0, 255]);
                }
            }
        }
    }
    // One-word 8x8 aux0 supplies character14..10 and palette6..4.
    let mut v = base(0, 0, false, true, false);
    reg(&mut v, 0x30, 0x8021);
    word(&mut v, 0, 0x1020);
    v.vram[0x8400] = 0x10; // character0x420 * 32
    color(&mut v, 273, 0x7C00); // palette supplement256 + name16 + dot1
    assert_eq!(at(&v.framebuffer().unwrap(), 0, 0), [0, 0, 255, 255]);
}
#[test]
fn planes_pages_and_wrapping_follow_table_four_eight() {
    for one in [false, true] {
        for large in [false, true] {
            let mut v = base(2, 0, large, one, false);
            // Plane A uses selection0, B selection4, C selection8, D selection12.
            reg(&mut v, 0x48, 0x0400);
            reg(&mut v, 0x4A, 0x0C08);
            let page_bytes = match (one, large) {
                (true, false) => 0x2000,
                (true, true) => 0x800,
                (false, false) => 0x4000,
                (false, true) => 0x1000,
            };
            let charnum = 0x3000;
            v.vram[charnum * 32] = 0x10;
            color(&mut v, 1, 0x1F);
            for selection in [0, 4, 8, 12] {
                let p = selection * page_bytes;
                if one {
                    word(&mut v, p, 0);
                } else {
                    word(&mut v, p, 0);
                    word(&mut v, p + 2, charnum as u16);
                }
            }
            // Use 2-word names here; one-word supplement picks upper character bits.
            if one {
                reg(&mut v, 0x34, 0x800C);
            }
            for (x, y) in [(0, 0), (512, 0), (0, 512), (512, 512), (1024, 1024)] {
                reg(&mut v, 0x90, x);
                reg(&mut v, 0x92, y);
                assert_eq!(
                    at(&v.framebuffer().unwrap(), 0, 0),
                    [255, 0, 0, 255],
                    "{one} {large} {x} {y}"
                );
            }
        }
    }
}
#[test]
fn tile_layers_participate_in_window_priority_and_color_calculation() {
    let mut v = base(2, 0, false, true, false);
    reg(&mut v, 0x20, 0xC);
    reg(&mut v, 0x36, 0x8000); // NBG2 + NBG3
    reg(&mut v, 0xFA, 0x0203);
    reg(&mut v, 0x4C, 1); // NBG3 plane starts at0x2000
    word(&mut v, 0, 0x20);
    word(&mut v, 0x2000, 0x1030);
    v.vram[0x400..0x420].fill(0x11);
    v.vram[0x600..0x620].fill(0x11);
    color(&mut v, 1, 0x1F);
    color(&mut v, 17, 0x7C00);
    reg(&mut v, 0xC0, 0);
    reg(&mut v, 0xC2, 0);
    reg(&mut v, 0xC4, 6);
    reg(&mut v, 0xC6, 7);
    reg(&mut v, 0xD2, 2); // NBG2 only inside window0 (x0..3)
    reg(&mut v, 0xEC, 4);
    reg(&mut v, 0x10A, 15);
    let f = v.framebuffer().unwrap();
    assert_eq!(at(&f, 0, 0), [127, 0, 127, 255]); // ratio16/32 red over blue
    assert_eq!(at(&f, 4, 0), [0, 0, 255, 255]);
}

fn guest_rom() -> Vec<u8> {
    let mut rom = vec![0; saturn_rust::bus::BIOS_SIZE];
    for line in include_str!("fixtures/tiles.hex")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let mut a = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[a..a + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            a += 2;
        }
    }
    rom
}
#[test]
fn guest_programs_all_four_layers_and_mutated_guest_palette_changes_real_pixels() {
    use saturn_rust::{Boot, Machine, bus::Memory};
    let rom = guest_rom();
    for mutated in [false, true] {
        let mut image = rom.clone();
        if mutated {
            // Change only the original guest's literal red palette value. The
            // SH-2 still performs every MMIO write; no host video-state writes.
            let pattern = 0x0000001Fu32.to_be_bytes();
            let at = image.windows(4).position(|b| b == pattern).unwrap();
            image[at..at + 4].copy_from_slice(&0x000003FFu32.to_be_bytes());
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&image)).unwrap();
        m.run_steps(3000).unwrap();
        assert_eq!(m.bus().read32(0x06009000).unwrap(), 0x600D);
        m.advance_frame().unwrap();
        m.advance_frame().unwrap();
        let frame = m.framebuffer().unwrap();
        for (x, color) in [
            (
                0,
                if mutated {
                    [255, 255, 0, 255]
                } else {
                    [255, 0, 0, 255]
                },
            ),
            (8, [0, 255, 0, 255]),
            (16, [0, 0, 255, 255]),
            (24, [0, 0, 255, 255]),
            (32, [255, 255, 255, 255]),
            (36, [0, 0, 0, 255]),
            (40, [0, 0, 0, 255]),
        ] {
            assert_eq!(at(&frame, x, 0), color, "{x} {mutated}");
        }
        assert_eq!(m.cpu_state().exceptions, 0);
        assert!(m.cpu_state().instructions > 400);
    }
}

#[test]
fn tile_high_color_formats_decode_palette_rgb555_and_rgb888() {
    for format in 2..=4 {
        for large in [false, true] {
            let mut v = base(0, format, large, false, false);
            word(&mut v, 0, 0);
            word(&mut v, 2, 0x100);
            let cell_bytes = if format == 4 { 256 } else { 128 };
            for cell in 0..if large { 4 } else { 1 } {
                let a = 0x2000 + cell * cell_bytes;
                if format == 2 {
                    word(&mut v, a, 0x321);
                    color(&mut v, 0x321, 0x3E0);
                } else if format == 3 {
                    word(&mut v, a, 0x83E0);
                } else {
                    word(&mut v, a, 0x8000);
                    word(&mut v, a + 2, 0xFF00);
                }
            }
            let f = v.framebuffer().unwrap();
            assert_eq!(at(&f, 0, 0), [0, 255, 0, 255]);
            assert_eq!(at(&f, 1, 0), [0, 0, 0, 255]);
            if large {
                for (x, y) in [(8, 0), (0, 8), (8, 8)] {
                    assert_eq!(at(&f, x, y), [0, 255, 0, 255]);
                }
            }
        }
    }
}
#[test]
fn plane_page_layout_and_ignored_low_selection_bits_are_independent_of_name_size() {
    for one in [false, true] {
        for large in [false, true] {
            for plane in [0, 1, 3] {
                // 1x1, 2x1, 2x2 pages
                let mut v = base(3, 0, large, one, false);
                reg(&mut v, 0x3A, plane << 6);
                let page_bytes = match (one, large) {
                    (true, false) => 0x2000,
                    (true, true) => 0x800,
                    (false, false) => 0x4000,
                    (false, true) => 0x1000,
                };
                let pages = match plane {
                    0 => 1,
                    1 => 2,
                    _ => 4,
                };
                // Selector3's low plane-alignment bits are ignored.
                reg(&mut v, 0x4C, 3);
                let plane_base = (3 & !(pages - 1)) * page_bytes;
                for page in 0..pages {
                    let a = plane_base + page * page_bytes;
                    let character = 0x2000 + page * 4;
                    if one {
                        // Supplemental character13=1; name supplies the rest.
                        reg(&mut v, 0x36, 0x8008);
                        word(
                            &mut v,
                            a,
                            if large {
                                (page as u16) & 0x3FF
                            } else {
                                (page as u16) * 4
                            },
                        );
                    } else {
                        word(&mut v, a, 0);
                        word(&mut v, a + 2, character as u16);
                    }
                    v.vram[character * 32] = ((page + 1) as u8) << 4;
                    color(&mut v, page + 1, [0x1F, 0x3E0, 0x7C00, 0x7FFF][page]);
                }
                for page in 0..pages {
                    reg(
                        &mut v,
                        0x94,
                        ((page % if plane == 0 { 1 } else { 2 }) * 512) as u16,
                    );
                    reg(
                        &mut v,
                        0x96,
                        ((page / if plane == 0 { 1 } else { 2 }) * 512) as u16,
                    );
                    let f = v.framebuffer().unwrap();
                    assert_eq!(
                        at(&f, 0, 0),
                        [
                            [255, 0, 0, 255],
                            [0, 255, 0, 255],
                            [0, 0, 255, 255],
                            [255, 255, 255, 255]
                        ][page],
                        "{one} {large} {plane} {page}"
                    );
                }
            }
        }
    }
}
