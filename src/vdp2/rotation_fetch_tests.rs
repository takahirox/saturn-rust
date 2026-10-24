//! Literal ST-058-R2 §§4.4–4.9/6.1/6.2 image vectors. No core address oracle.
use super::*;

fn word(v: &mut Vdp2, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn long(v: &mut Vdp2, at: usize, value: u32) {
    v.vram[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn setup(format: u16, one: bool, large: bool, dual: bool) -> Vdp2 {
    let mut v = Vdp2::clocked();
    for (at, value) in [
        (0, 0x8000),
        (0xE, if dual { 0x30E } else { 0x10E }),
        (0x20, if dual { 0x30 } else { 0x10 }),
        (0x2A, (format << 4 | u16::from(large)) << 8),
        (0x28, format << 4 | u16::from(large)),
        (0x38, if one { 0x8004 } else { 0 }),
        (0x30, if one { 0x8008 } else { 0 }),
        (0x3E, if large && one { 0x30 } else { 0x10 }), // B map uses B1 (one-word 8-dot names, map=16 below).
        (
            0x60,
            if large {
                if one { 0 } else { 0x2020 }
            } else if one {
                0x3030
            } else {
                0x1818
            },
        ),
        (0xBC, 3),
        (0xBE, 0x8000),
        (0xFC, 2),
        (0xF8, 3),
    ] {
        v.write16(VDP2 + at, value).unwrap();
    }
    for base in [0x70000, 0x70080] {
        for (off, value) in [
            (0x14, 0x10000),
            (0x1C, 0x10000),
            (0x2C, 0x10000),
            (0x4C, 0x10000),
            (0x50, 0x10000),
        ] {
            long(&mut v, base + off, value);
        }
    }
    for (at, value) in [(2, 0x801F_u16), (4, 0x83E0), (6, 0xFC00)] {
        v.cram[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    for base in [0, 0x60000] {
        for entry in 0..4 {
            if one {
                word(&mut v, base + entry * 2, 0);
            } else {
                word(&mut v, base + entry * 4, 0);
                word(
                    &mut v,
                    base + entry * 4 + 2,
                    if base == 0 { 0x1000 } else { 0x2000 },
                );
            }
        }
    }
    let data: &[u8] = match format {
        0 => &[0x11],
        1 => &[1],
        2 => &[0, 1],
        3 => &[0x80, 0x1F],
        _ => &[0x80, 0, 0, 0xFF],
    };
    for base in [0x20000, 0x40000] {
        for at in (base..base + 1024).step_by(data.len()) {
            v.vram[at..at + data.len()].copy_from_slice(data);
        }
    }
    v
}
fn acquire(v: &mut Vdp2, x: usize) {
    v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 4, x);
}
fn pixel(v: &Vdp2, x: usize) -> [u8; 4] {
    v.scanning[x * 4..x * 4 + 4].try_into().unwrap()
}

#[test]
fn names_cells_formats_tile_boundaries_and_fixed_rbg1_routes_have_literal_transfers() {
    for format in 0..5 {
        for one in [false, true] {
            for large in [false, true] {
                for dual in [false, true] {
                    let mut v = setup(format, one, large, dual);
                    let start = if large { 15 } else { 7 };
                    long(&mut v, 0x70000, start << 16);
                    long(&mut v, 0x70080, start << 16);
                    for x in 0..2 {
                        acquire(&mut v, x);
                    }
                    let traces = v.display_fetch.borrow().trace.clone();
                    for (layer, name, character) in if dual {
                        vec![(4, 0, 0x20000), (0, 0x60000, 0x40000)]
                    } else {
                        vec![(4, 0, 0x20000)]
                    } {
                        let got: Vec<_> = traces
                            .iter()
                            .filter(|r| r.layer == layer)
                            .map(|r| (r.phase, r.path, r.address, r.words))
                            .collect();
                        let offset = match (large, format) {
                            (false, 0) => 0,
                            (false, 1) => 4,
                            (false, 2 | 3) => 12,
                            (false, _) => 28,
                            (true, 0) => 32,
                            (true, 1) => 68,
                            (true, 2 | 3) => 140,
                            (true, _) => 284,
                        };
                        let name_words = if one {
                            [0, 0]
                        } else {
                            [0, if layer == 4 { 0x1000 } else { 0x2000 }]
                        };
                        let data = match format {
                            0 => [0x1111; 2],
                            1 => [0x0101; 2],
                            2 => [1; 2],
                            3 => [0x801F; 2],
                            _ => [0x8000, 0x00FF],
                        };
                        assert_eq!(
                            got,
                            [
                                (0, 0, name, name_words),
                                (0, 1, character + offset, data),
                                (4, 0, name + if one { 2 } else { 4 }, name_words),
                                (4, 1, character, data)
                            ],
                            "format={format}, one={one}, large={large}, dual={dual}"
                        );
                    }
                    // Destroy RAM and address interpretation AFTER acquisition.
                    v.vram.fill(0);
                    for (at, value) in [
                        (0xE, if dual { 0x200 } else { 0 }),
                        (0x28, 0x40),
                        (0x2A, 0x4000),
                        (0x30, 0),
                        (0x38, 0),
                        (0x3E, 0),
                        (0xB0, 0),
                    ] {
                        v.write16(VDP2 + at, value).unwrap();
                    }
                    v.scan_to(&Vdp1::default(), 0, 3);
                    assert_eq!(pixel(&v, 0), [255, 0, 0, 255]);
                    assert_eq!(pixel(&v, 1), [255, 0, 0, 255]);
                    assert_eq!(pixel(&v, 2), [0, 0, 0, 255]); // no live miss repair
                }
            }
        }
    }
}

#[test]
fn nonsequential_rows_and_revisited_addresses_issue_fresh_per_dot_transfers() {
    let mut v = setup(0, true, false, false);
    // x=9,8,7 crosses the tile boundary backwards, then switches to x=9.
    long(&mut v, 0x70000, 9 << 16);
    long(&mut v, 0x70014, 0xFFFF0000);
    for x in 0..3 {
        acquire(&mut v, x);
    }
    // Change the latched matrix translation without re-reading start requests.
    long(&mut v, 0x70044, 3 << 16);
    v.read_line_tables(0);
    word(&mut v, 0x20000, 0x2222);
    acquire(&mut v, 3);
    let got: Vec<_> = v
        .display_fetch
        .borrow()
        .trace
        .iter()
        .map(|r| (r.phase, r.path, r.address, r.words[0]))
        .collect();
    assert_eq!(
        got,
        [
            (0, 0, 2, 0),
            (0, 1, 0x20000, 0x1111),
            (4, 0, 2, 0),
            (4, 1, 0x20000, 0x1111),
            (8, 0, 0, 0),
            (8, 1, 0x20000, 0x1111),
            (12, 0, 2, 0),
            (12, 1, 0x20000, 0x2222)
        ]
    );
    v.scan_to(&Vdp1::default(), 0, 4);
    for x in 0..3 {
        assert_eq!(pixel(&v, x), [255, 0, 0, 255]);
    }
    assert_eq!(pixel(&v, 3), [0, 255, 0, 255]);
}

#[test]
fn bitmap_formats_bank_eligibility_and_normal_companions_are_independent() {
    for format in 0..5 {
        for split in [false, true] {
            let mut v = setup(format, true, false, false);
            v.write16(VDP2 + 0x2A, (format << 4 | 2) << 8).unwrap();
            v.write16(VDP2 + 0x3E, 1).unwrap(); // bitmap base 20000
            v.write16(VDP2 + 0xE, if split { 0x10C } else { 3 })
                .unwrap();
            acquire(&mut v, 0);
            assert_eq!(v.display_fetch.borrow().trace[0].address, 0x20000);
            assert_eq!(v.display_fetch.borrow().trace.len(), 1);
            v.write16(VDP2 + 0xE, 2).unwrap(); // names only, no image port
            acquire(&mut v, 1);
            assert_eq!(v.display_fetch.borrow().trace.len(), 1);
            v.vram.fill(0);
            v.scan_to(&Vdp1::default(), 0, 2);
            assert_eq!(pixel(&v, 0), [255, 0, 0, 255]);
            assert_eq!(pixel(&v, 1), [0, 0, 0, 255]);
        }
    }
    let mut v = setup(0, true, false, false);
    // NBG0 bitmap lives independently in unsplit B, rotation uses split A.
    for (at, value) in [
        (0x20, 0x11),
        (0x28, 2),
        (0x3C, 2),
        (0x18, 0x4444),
        (0x1A, 0xEEEE),
        (0x78, 1),
        (0x7C, 1),
    ] {
        v.write16(VDP2 + at, value).unwrap();
    }
    v.acquire_display(0, 0, 0, 0);
    acquire(&mut v, 0);
    let got: Vec<_> = v
        .display_fetch
        .borrow()
        .trace
        .iter()
        .map(|r| (r.layer, r.path, r.address))
        .collect();
    assert_eq!(got, [(0, 1, 0x40000), (4, 0, 0), (4, 1, 0x20000)]);
    let before = v.display_fetch.borrow().trace.clone();
    for _ in 0..2 {
        v.framebuffer().unwrap();
    }
    assert_eq!(v.display_fetch.borrow().trace, before);
}

#[test]
fn unsplit_tile_banks_and_repeated_line_zero_cannot_reuse_stale_operands() {
    for one in [false, true] {
        let mut v = setup(0, one, false, false);
        v.write16(VDP2 + 0xE, 0x32).unwrap(); // unsplit A names, B characters
        v.write16(VDP2 + 0x38, if one { 0x8008 } else { 0 })
            .unwrap();
        if !one {
            word(&mut v, 2, 0x2000);
        }
        acquire(&mut v, 0);
        let trace = v.display_fetch.borrow().trace.clone();
        assert_eq!(
            trace
                .iter()
                .map(|r| (r.path, r.address))
                .collect::<Vec<_>>(),
            [(0, 0), (1, 0x40000)]
        );
        v.write16(VDP2 + 0xE, 2).unwrap(); // remove character port
        // The same logical line number in a later field is a fresh stream.
        v.acquire_rotation_display(&Vdp1::default(), 2, 0, 0, 0);
        v.scan_to(&Vdp1::default(), 0, 1);
        assert_eq!(pixel(&v, 0), [0, 0, 0, 255]);
        assert!(v.display_fetch.borrow().trace.iter().all(|r| r.path == 0));
    }
}

#[test]
fn high_resolution_uses_two_clock_dots_and_rejects_rgb888_rotation() {
    for dual in [false, true] {
        let mut v = setup(3, true, false, dual);
        v.write16(VDP2, 0x8002).unwrap();
        v.acquire_rotation_display(&Vdp1::default(), 0, 0, 0, 0);
        word(&mut v, 0x20000, 0x83E0);
        word(&mut v, 0x40000, 0x83E0);
        word(&mut v, 0x20002, 0x83E0);
        word(&mut v, 0x40002, 0x83E0);
        v.acquire_rotation_display(&Vdp1::default(), 0, 0, 2, 1);
        v.scan_to(&Vdp1::default(), 0, 2);
        assert_eq!(&v.scanning[..8], [255, 0, 0, 255, 0, 255, 0, 255]);
        v.write16(VDP2 + 0x2A, 0x4000).unwrap();
        assert!(v.validate().unwrap_err().contains("RGB888 rotation"));
        if dual {
            v.write16(VDP2 + 0x2A, 0x3000).unwrap();
            v.write16(VDP2 + 0x28, 0x40).unwrap();
            assert!(v.validate().unwrap_err().contains("RGB888 rotation"));
        }
    }
}

#[test]
fn rbg1_cannot_overwrite_nbg0_operands_already_acquired_before_a_display_switch() {
    let mut v = setup(0, true, false, true);
    for (at, value) in [
        (0x20, 0x11),
        (0x28, 2),
        (0x3C, 2),
        (0x18, 0x4EEE),
        (0x1A, 0xEEEE),
        (0x78, 1),
        (0x7C, 1),
    ] {
        v.write16(VDP2 + at, value).unwrap();
    }
    // NBG0's ahead-of-display pipeline holds red from B0.
    v.acquire_display(0, 0, 0, 0);
    word(&mut v, 0x40000, 0x2222);
    v.write16(VDP2 + 0x28, 0).unwrap(); // RBG1 cell controls
    v.write16(VDP2 + 0x20, 0x30).unwrap();
    acquire(&mut v, 0); // independent RBG1 port reads green at the same address
    v.beam_rendering = true;
    v.display_fetch.get_mut().pixel = 0;
    assert_eq!(v.fetch_word(0, 1, 0x40000), Some(0x1111));
    v.display_fetch.borrow().rotation.set(true);
    assert_eq!(v.fetch_word(0, 1, 0x40000), Some(0x2222));
    v.display_fetch.borrow().rotation.set(false);
    v.beam_rendering = false;
    // Returning to the normal screen consumes its original retained operand.
    v.write16(VDP2 + 0x28, 2).unwrap();
    v.write16(VDP2 + 0x20, 0x11).unwrap();
    v.scan_to(&Vdp1::default(), 0, 1);
    assert_eq!(pixel(&v, 0), [255, 0, 0, 255]);
}

#[test]
fn quarter_turn_sampling_acquires_vertical_source_rows_in_output_dot_order() {
    for dual in [false, true] {
        let mut v = setup(0, true, false, dual);
        for base in [0x70000, 0x70080] {
            long(&mut v, base + 0x14, 0); // dx=0
            long(&mut v, base + 0x18, 0x10000); // dy=1: horizontal output scans source rows
        }
        for base in [0x20000, 0x40000] {
            for (off, value) in [(0, 0x1111), (4, 0x2222), (8, 0x3333), (12, 0x1111)] {
                word(&mut v, base + off, value);
            }
        }
        for x in 0..4 {
            acquire(&mut v, x);
        }
        let reads = v.display_fetch.borrow().trace.clone();
        for (layer, base) in if dual {
            vec![(4, 0x20000), (0, 0x40000)]
        } else {
            vec![(4, 0x20000)]
        } {
            let actual: Vec<_> = reads
                .iter()
                .filter(|r| r.layer == layer && r.path == 1)
                .map(|r| (r.phase, r.address, r.words[0]))
                .collect();
            assert_eq!(
                actual,
                [
                    (0, base, 0x1111),
                    (4, base + 4, 0x2222),
                    (8, base + 8, 0x3333),
                    (12, base + 12, 0x1111)
                ]
            );
        }
        v.vram.fill(0);
        v.scan_to(&Vdp1::default(), 0, 4);
        assert_eq!(
            &v.scanning[..16],
            [
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255
            ]
        );
    }
}

// §2.1 + §6.3: authored source rows, independently addressed in 32-byte
// character units. These vectors never call production address/color helpers.
fn spatial_color(index: usize) -> u16 {
    0x8000 | (index as u16 & 31) | (((index / 32 + 1) as u16 & 31) << 5)
}
fn spatial_rgb(color: u16) -> [u8; 4] {
    let expand = |n: u16| {
        let bits = (n & 31) as u8;
        bits << 3 | bits >> 2
    };
    [expand(color), expand(color >> 5), expand(color >> 10), 255]
}
fn spatial_setup(format: u16, one: bool, large: bool, route: usize, bitmap: bool) -> Vdp2 {
    let mut v = setup(format, one, large, route == 2);
    v.write16(VDP2 + 0xB0, u16::from(route == 1)).unwrap();
    if route == 1 {
        // Parameter B of RBG0 uses the same legal A0 names/A1 characters.
        v.write16(VDP2 + 0x3E, 0).unwrap();
        v.write16(VDP2 + 0x60, 0).unwrap();
    }
    if bitmap {
        v.write16(VDP2 + 0x2A, (format << 4 | 2) << 8).unwrap();
        v.write16(VDP2 + 0x3E, 0x11).unwrap();
    }
    let cell_bytes = [32, 64, 128, 128][format as usize];
    let size = if large { 16 } else { 8 };
    for index in 1..2048 {
        let at = index * 2;
        v.cram[at..at + 2].copy_from_slice(&spatial_color(index).to_be_bytes());
    }
    for (names, image) in [(0, 0x20000), (0x60000, 0x40000)] {
        for sx in 0..512 {
            let tile = sx / size;
            let cell = sx % size / 8;
            let character_offset = tile * if large { 4 } else { 1 } * cell_bytes;
            let number = character_offset / 32;
            if one {
                word(
                    &mut v,
                    names + tile * 2,
                    (number / if large { 4 } else { 1 }) as u16,
                );
            } else {
                word(&mut v, names + tile * 4, 0);
                word(
                    &mut v,
                    names + tile * 4 + 2,
                    ((image + character_offset) / 32) as u16,
                );
            }
            let pixel = if bitmap { sx } else { sx % 8 };
            let base = if bitmap {
                image
            } else {
                image + character_offset + cell * cell_bytes
            };
            let value = if format == 0 { sx % 15 + 1 } else { sx + 1 };
            match format {
                0 => {
                    let at = base + pixel / 2;
                    let shift = if pixel % 2 == 0 { 4 } else { 0 };
                    v.vram[at] = (v.vram[at] & !(15 << shift)) | (value as u8) << shift;
                }
                1 => v.vram[base + pixel] = (sx % 255 + 1) as u8,
                2 => word(&mut v, base + pixel * 2, value as u16),
                _ => word(&mut v, base + pixel * 2, spatial_color(value)),
            }
        }
    }
    v
}
fn spatial_expected(format: u16, sx: usize) -> [u8; 4] {
    spatial_rgb(spatial_color(match format {
        0 => sx % 15 + 1,
        1 => sx % 255 + 1,
        _ => sx + 1,
    }))
}

#[test]
fn high_resolution_full_spans_match_normal_picture_coordinates_on_every_image_path() {
    for (normal, high, width) in [(0x8000, 0x8002, 640), (0x8001, 0x8003, 704)] {
        for format in 0..4 {
            for (bitmap, one, large) in [
                (true, true, false),
                (false, false, false),
                (false, true, false),
                (false, false, true),
                (false, true, true),
            ] {
                for route in 0..3 {
                    if bitmap && route == 2 {
                        continue;
                    }
                    let mut v = spatial_setup(format, one, large, route, bitmap);
                    let rbg1 = route == 2;
                    v.write16(VDP2, normal).unwrap();
                    for sx in 0..width / 2 {
                        let rgb = if rbg1 {
                            v.rotation1(sx, 0, false)
                        } else {
                            v.rotation(sx, 0, false)
                        }
                        .unwrap()
                        .rgb;
                        assert_eq!(rgb, spatial_expected(format, sx)[..3]);
                    }
                    v.write16(VDP2, high).unwrap();
                    for x in 0..width {
                        let rgb = if rbg1 {
                            v.rotation1(x, 0, false)
                        } else {
                            v.rotation(x, 0, false)
                        }
                        .unwrap()
                        .rgb;
                        assert_eq!(
                            rgb,
                            spatial_expected(format, x / 2)[..3],
                            "snapshot format={format}, bitmap={bitmap}, route={route}, x={x}"
                        );
                        v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 2, x);
                    }
                    // Every physical acquisition has a literal independently
                    // expected name/address, including all right-edge dots.
                    let layer = if rbg1 { 0 } else { 4 };
                    let reads = v.display_fetch.borrow().trace.clone();
                    for x in 0..width {
                        let sx = x / 2;
                        let image = if rbg1 { 0x40000 } else { 0x20000 };
                        let size = if large { 16 } else { 8 };
                        let cell_bytes = [32, 64, 128, 128][format as usize];
                        let base = if bitmap {
                            image
                        } else {
                            image
                                + sx / size * if large { 4 } else { 1 } * cell_bytes
                                + sx % size / 8 * cell_bytes
                        };
                        let p = if bitmap { sx } else { sx % 8 };
                        let at = base
                            + match format {
                                0 => p / 2,
                                1 => p,
                                _ => p * 2,
                            };
                        let got: Vec<_> = reads
                            .iter()
                            .filter(|r| r.layer == layer && r.phase == x as u32 * 2)
                            .map(|r| (r.path, r.address))
                            .collect();
                        let mut expected = vec![];
                        if !bitmap {
                            expected.push((
                                0,
                                if rbg1 { 0x60000 } else { 0 }
                                    + sx / size * if one { 2 } else { 4 },
                            ));
                        }
                        expected.push((1, at & !3));
                        assert_eq!(
                            got, expected,
                            "format={format}, bitmap={bitmap}, one={one}, large={large}, route={route}, x={x}"
                        );
                    }
                    v.vram.fill(0); // scheduled consumption cannot repair reads
                    v.scan_to(&Vdp1::default(), 0, width);
                    for x in 0..width {
                        assert_eq!(pixel(&v, x), spatial_expected(format, x / 2));
                    }
                }
            }
        }
    }
}

#[test]
fn high_resolution_fractional_signed_increments_and_horizontal_coefficients_use_picture_x() {
    for route in 0..3 {
        for (start, delta) in [(17i64, -0x8000i64), (0, 0x18000)] {
            let mut v = spatial_setup(3, true, false, route, false);
            v.write16(VDP2, 0x8002).unwrap();
            for base in [0x70000, 0x70080] {
                long(&mut v, base, (start << 16) as u32);
                long(&mut v, base + 0x14, delta as u32);
            }
            for x in 0..32 {
                let sx = ((start << 16) + delta * (x / 2) as i64) >> 16;
                v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 2, x);
                let rgb = if route == 2 {
                    v.rotation1(x, 0, false)
                } else {
                    v.rotation(x, 0, false)
                }
                .unwrap()
                .rgb;
                assert_eq!(rgb, spatial_expected(3, sx as usize)[..3]);
            }
            v.scan_to(&Vdp1::default(), 0, 32);
            for x in 0..32 {
                let sx = ((start << 16) + delta * (x / 2) as i64) >> 16;
                assert_eq!(
                    pixel(&v, x),
                    spatial_expected(3, sx as usize),
                    "route={route}, start={start}, delta={delta}, x={x}, valid={:?}",
                    v.validate()
                );
            }
        }
    }
    for route in 0..3 {
        let parameter = usize::from(route != 0);
        for delta in [0x8000i64, -0x8000] {
            let mut v = spatial_setup(3, true, false, route, route != 2);
            for (at, value) in [
                (0, 0x8002),
                (0xE, if route == 2 { 0x930E } else { 0x10D }),
                (0xB4, 0xD << (parameter * 8)),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            v.validate().unwrap();
            let base = 0x70000 + parameter * 0x80;
            long(&mut v, base + 0x14, 0); // source X comes solely from Xp coefficient
            long(&mut v, base + 0x54, 16 << 16);
            long(&mut v, base + 0x5C, delta as u32);
            for index in 0..32 {
                let value = ((index * 3 + 1) << 8) as u32;
                if route == 2 {
                    let at = 0x800 + index * 4;
                    v.cram[at..at + 4].copy_from_slice(&(value | 0x80000000).to_be_bytes());
                } else {
                    long(&mut v, index * 4, value);
                }
            }
            for x in 0..32 {
                let index = ((16 << 16) + delta * (x / 2) as i64) >> 16;
                let expected = spatial_expected(3, index as usize * 3 + 1);
                assert_eq!(
                    if route == 2 {
                        v.rotation1(x, 0, false)
                    } else {
                        v.rotation(x, 0, false)
                    }
                    .unwrap()
                    .rgb,
                    expected[..3]
                );
                v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 2, x);
            }
            v.scan_to(&Vdp1::default(), 0, 32);
            for x in 0..32 {
                let index = ((16 << 16) + delta * (x / 2) as i64) >> 16;
                assert_eq!(pixel(&v, x), spatial_expected(3, index as usize * 3 + 1));
            }
        }
    }
}

#[test]
fn high_resolution_parameter_and_layer_windows_can_split_an_output_pair() {
    let mut v = spatial_setup(3, true, false, 0, true);
    for (at, value) in [
        (0, 0x8002),
        (0xB0, 3),
        (0xD6, 2),
        (0xC0, 0),
        (0xC4, 2),
        (0xC6, 223),
    ] {
        v.write16(VDP2 + at, value).unwrap();
    }
    long(&mut v, 0x700C4, 32 << 16); // B translation
    for x in 0..8 {
        let expected = spatial_expected(3, x / 2 + if x >= 3 { 32 } else { 0 });
        assert_eq!(v.rotation(x, 0, false).unwrap().rgb, expected[..3]);
        v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 2, x);
    }
    v.scan_to(&Vdp1::default(), 0, 8);
    for x in 0..8 {
        assert_eq!(
            pixel(&v, x),
            spatial_expected(3, x / 2 + if x >= 3 { 32 } else { 0 })
        );
    }
    // A new field discards the previous acquisitions. The layer window itself
    // also ends at output2, independently of the paired image coordinate.
    v.scan_x = 0;
    v.write16(VDP2 + 0xD4, 2).unwrap();
    for x in 0..8 {
        v.acquire_rotation_display(&Vdp1::default(), 1, 0, x as u32 * 2, x);
    }
    v.scan_to(&Vdp1::default(), 0, 8);
    for x in 0..8 {
        assert_eq!(
            pixel(&v, x),
            if x < 3 {
                spatial_expected(3, x / 2)
            } else {
                [0, 0, 0, 255]
            }
        );
    }
}

#[test]
fn high_resolution_missing_second_acquisition_stays_missing_and_new_field_is_fresh() {
    let mut v = spatial_setup(3, true, false, 0, true);
    v.write16(VDP2, 0x8002).unwrap();
    v.acquire_rotation_display(&Vdp1::default(), 0, 0, 0, 0);
    // Disabled character bank at dot1's independent acquisition, even though
    // dot0/dot1 have identical image coordinates. Re-enabling cannot repair it.
    v.write16(VDP2 + 0xE, 0x102).unwrap();
    v.acquire_rotation_display(&Vdp1::default(), 0, 0, 2, 1);
    v.write16(VDP2 + 0xE, 0x10E).unwrap();
    word(&mut v, 0x20000, 0xFC00);
    let before = v.display_fetch.borrow().trace.clone();
    v.framebuffer().unwrap();
    assert_eq!(v.display_fetch.borrow().trace, before);
    v.scan_to(&Vdp1::default(), 0, 3);
    assert_eq!(pixel(&v, 0), spatial_expected(3, 0));
    assert_eq!(pixel(&v, 1), [0, 0, 0, 255]);
    assert_eq!(pixel(&v, 2), [0, 0, 0, 255]); // no deadline at all
    v.scan_x = 0;
    v.acquire_rotation_display(&Vdp1::default(), 1, 0, 0, 0);
    v.scan_to(&Vdp1::default(), 0, 2);
    assert_eq!(pixel(&v, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&v, 1), [0, 0, 0, 255]);
}

#[test]
fn high_resolution_normal_background_and_sprite_composition_keep_output_x() {
    let mut v = spatial_setup(3, true, false, 0, true);
    for (at, value) in [
        (0, 0x8002),
        (0x20, 0x11),
        (0x28, 0x32),
        (0x3C, 2),
        (0x78, 1),
        (0x7C, 1),
        (0xF8, 3),
        (0xE0, 0x20),
        (0xF0, 4),
    ] {
        v.write16(VDP2 + at, value).unwrap();
    }
    // An independent normal RGB555 bitmap in unsplit B advances each dot.
    for x in 0..16 {
        word(&mut v, 0x40000 + x * 2, spatial_color(x + 65));
    }
    let mut sprites = Vdp1::default();
    for x in 0..16 {
        sprites.draw_buffer_mut()[x * 2..x * 2 + 2]
            .copy_from_slice(&spatial_color(x + 129).to_be_bytes());
    }
    sprites.vblank_out();
    for x in 0..16 {
        assert_eq!(
            v.rotation(x, 0, false).unwrap().rgb,
            spatial_expected(3, x / 2)[..3]
        );
        assert_eq!(
            v.background(0, x, 0, false).unwrap().rgb,
            spatial_rgb(spatial_color(x + 65))[..3]
        );
        assert_eq!(
            v.sprite(&sprites, x, 0).layer.unwrap().rgb,
            spatial_rgb(spatial_color(x + 129))[..3]
        );
    }
    // Ordinary render/composition picks the sprite's live output-coordinate
    // priority, then NBG0 when that priority is zero, then the rotation layer.
    let frame = v.composite(&sprites).unwrap();
    for x in 0..16 {
        assert_eq!(
            &frame[x * 4..x * 4 + 4],
            spatial_rgb(spatial_color(x + 129))
        );
    }
    v.write16(VDP2 + 0xF0, 0).unwrap();
    let frame = v.composite(&sprites).unwrap();
    for x in 0..16 {
        assert_eq!(&frame[x * 4..x * 4 + 4], spatial_rgb(spatial_color(x + 65)));
    }
    v.write16(VDP2 + 0xF8, 0).unwrap();
    let frame = v.composite(&sprites).unwrap();
    for x in 0..16 {
        assert_eq!(&frame[x * 4..x * 4 + 4], spatial_expected(3, x / 2));
    }
}

#[test]
fn high_resolution_transformed_addresses_cross_bitmap_and_map_boundaries() {
    for route in 0..3 {
        for bitmap in [false, true] {
            if route == 2 && bitmap {
                continue;
            }
            let mut v = spatial_setup(3, true, false, route, bitmap);
            v.write16(VDP2, 0x8003).unwrap();
            for base in [0x70000, 0x70080] {
                long(&mut v, base, 500 << 16);
            }
            // Adjacent map planes deliberately alias the same authored page;
            // the source coordinate wraps at512 for both that page and bitmap.
            for x in 0..64 {
                let sx = (500 + x / 2) % 512;
                assert_eq!(
                    if route == 2 {
                        v.rotation1(x, 0, false)
                    } else {
                        v.rotation(x, 0, false)
                    }
                    .unwrap()
                    .rgb,
                    spatial_expected(3, sx)[..3]
                );
                v.acquire_rotation_display(&Vdp1::default(), 0, 0, x as u32 * 2, x);
            }
            v.vram.fill(0);
            v.scan_to(&Vdp1::default(), 0, 64);
            for x in 0..64 {
                assert_eq!(pixel(&v, x), spatial_expected(3, (500 + x / 2) % 512));
            }
        }
    }
}
