//! Independently authored §4.11 coordinates, addresses and literal RGB vectors.
use super::*;

fn reg(v: &mut Vdp2, at: u32, value: u16) {
    v.write16(VDP2 + at, value).unwrap();
}
fn word(v: &mut Vdp2, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn long(v: &mut Vdp2, at: usize, value: u32) {
    v.vram[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn normal(layer: usize, bitmap: bool) -> Vdp2 {
    let mut v = Vdp2::default();
    for (at, value) in [
        (0, 0x8000),
        (0xE, 0x300),
        (0x20, 1 << layer),
        (0xF8 + (layer as u32 / 2) * 2, 2 << ((layer & 1) * 8)),
        (0x10, (layer as u16) << 12 | 0xFFF),
        (0x12, ((4 + layer as u16) * 0x1100) | 0xEE),
        (0x14, (layer as u16) << 12 | 0xFFF),
        (0x16, ((4 + layer as u16) * 0x1100) | 0xEE),
    ] {
        reg(&mut v, at, value);
    }
    if layer < 2 {
        reg(
            &mut v,
            0x28,
            (0x10 | if bitmap { 2 } else { 0 }) << (layer * 8),
        );
        reg(&mut v, 0x78 + layer as u32 * 16, 1);
        reg(&mut v, 0x7C + layer as u32 * 16, 1);
    } else {
        reg(&mut v, 0x2A, 2 << ((layer - 2) * 4));
    }
    if bitmap {
        reg(&mut v, 0x3C, 1 << (layer * 4));
    }
    for y in 0..128 {
        for x in 0..64 {
            let index = (y * 17 + x) % 255 + 1;
            let at = if bitmap {
                0x20000 + y * 512 + x
            } else {
                0x20000 + ((y / 8) * 8 + x / 8) * 64 + (y % 8) * 8 + x % 8
            };
            v.vram[at] = index as u8;
        }
    }
    for cell in 0..128 {
        word(
            &mut v,
            (cell / 8 * 64 + cell % 8) * 4 + 2,
            0x1000 + cell as u16 * 2,
        );
    }
    for index in 1..256 {
        v.cram[index * 2..index * 2 + 2].copy_from_slice(&(index as u16).to_be_bytes());
    }
    v
}
fn rgb(index: usize) -> [u8; 3] {
    let r = (index & 31) as u8;
    let g = (index >> 5 & 31) as u8;
    [r << 3 | r >> 2, g << 3 | g >> 2, 0]
}
#[test]
fn every_size_axis_layer_density_scroll_zoom_and_block_boundary_has_independent_addresses() {
    for layer in 0..4 {
        for bitmap in [false, true] {
            if bitmap && layer > 1 {
                continue;
            }
            let mut v = normal(layer, bitmap);
            for tv in [0x8000, 0x8080, 0x80C0] {
                reg(&mut v, 0, tv);
                for enabled in [false, true] {
                    for axis in 0..2 {
                        for code in 0..16u16 {
                            let size = usize::from(code + 1);
                            reg(
                                &mut v,
                                0x22,
                                code << (8 + axis * 4) | if enabled { 1 << layer } else { 0 },
                            );
                            for (scroll, zoom) in [(0, 0x10000), (3, 0x8000), (5, 0x20000)] {
                                if layer < 2 {
                                    for (off, value) in [
                                        (0x70, scroll << 16),
                                        (0x74, scroll << 16),
                                        (0x78, zoom),
                                        (0x7C, zoom),
                                    ] {
                                        let at = off + layer * 16;
                                        reg(&mut v, at as u32, (value >> 16) as u16);
                                        reg(&mut v, at as u32 + 2, value as u16);
                                    }
                                } else {
                                    reg(&mut v, 0x90 + (layer as u32 - 2) * 4, scroll as u16);
                                    reg(&mut v, 0x92 + (layer as u32 - 2) * 4, scroll as u16);
                                }
                                for parity in 0..2 {
                                    for l in [0, size - 1, size, size + 1] {
                                        for x in [0, size - 1, size, size + 1] {
                                            let output_y =
                                                if tv == 0x80C0 { 2 * l + parity } else { l };
                                            let mut sx = x;
                                            let mut sy = output_y;
                                            if enabled {
                                                if tv == 0x80C0 {
                                                    sy = l;
                                                }
                                                if axis == 0 {
                                                    sx = x / size * size;
                                                } else {
                                                    sy = sy / size * size;
                                                }
                                            }
                                            let increment = if layer < 2 { zoom } else { 0x10000 };
                                            sx = scroll + sx * increment / 65536;
                                            sy = scroll + sy * increment / 65536;
                                            let sample =
                                                v.normal_sample(layer, x, output_y).unwrap();
                                            let expected = if bitmap {
                                                0x20000 + sy * 512 + sx
                                            } else {
                                                0x20000
                                                    + ((sy / 8) * 8 + sx / 8) * 64
                                                    + sy % 8 * 8
                                                    + sx % 8
                                            };
                                            assert_eq!(
                                                sample.0 + sample.1,
                                                expected,
                                                "layer={layer}, bitmap={bitmap}, tv={tv:x}, code={code}, axis={axis}, enabled={enabled}, x={x}, l={l}, parity={parity}"
                                            );
                                            let pixel =
                                                v.background(layer, x, output_y, false).unwrap();
                                            assert_eq!(pixel.rgb, rgb((sy * 17 + sx) % 255 + 1));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn scheduled_normal_layers_use_selected_rows_retain_words_and_exclude_cell_scroll() {
    let sprites = Vdp1::default();
    for layer in 0..4 {
        for bitmap in [false, true] {
            if bitmap && layer > 1 {
                continue;
            }
            for field in 0..2 {
                for code in 0..16u16 {
                    let mut v = normal(layer, bitmap);
                    reg(&mut v, 0, 0x80C0);
                    reg(&mut v, 0x22, code << 12 | code << 8 | 1 << layer);
                    // A denied cell port must not prevent mosaic image fetches.
                    if layer < 2 {
                        reg(&mut v, 0x9A, 1 << (layer * 8));
                    }
                    v.scan_field(field == 0);
                    let line = code + 2;
                    for phase in (0..160).step_by(4) {
                        v.acquire_display(field, line, phase, (phase as usize / 4) % 8);
                    }
                    let size = usize::from(code + 1);
                    let sy = usize::from(line) / size * size;
                    let expected_address = if bitmap {
                        0x20000 + sy * 512
                    } else {
                        0x20000 + sy / 8 * 8 * 64 + sy % 8 * 8
                    };
                    assert!(
                        v.display_fetch
                            .borrow()
                            .trace
                            .iter()
                            .any(|r| r.path == 1 && r.address == expected_address),
                        "layer={layer}, bitmap={bitmap}, field={field}, code={code}, expected={expected_address:x}, trace={:?}",
                        v.display_fetch.borrow().trace
                    );
                    assert!(!v.display_fetch.borrow().trace.iter().any(|r| r.path == 2));
                    v.vram.fill(0); // Acquired bytes, not late RAM or duplicated rows.
                    v.begin_scanline(usize::from(line));
                    v.scan_to(&sprites, usize::from(line), 8);
                    let y = usize::from(line) * 2 + field as usize;
                    assert_eq!(&v.scanning[(y * 320) * 4..][..3], rgb((sy * 17) % 255 + 1));
                    assert_eq!(v.word(0), 0x80C0);
                    assert_eq!(v.dimensions(), (320, 448));
                }
            }
        }
    }
}
fn rotation(dual: bool, bitmap: bool) -> Vdp2 {
    let mut v = Vdp2::clocked();
    for (at, value) in [
        (0, 0x80C0),
        (0xE, if dual { 0x30E } else { 0x3BE }),
        (0x20, if dual { 0x30 } else { 0x10 }),
        (0x28, 0x20),
        (0x2A, if bitmap { 0x3200 } else { 0x2000 }),
        (0x3E, if bitmap { 0x11 } else { 0x10 }),
        (0x60, 0x1818),
        (0xBC, 3),
        (0xBE, 0x8000),
        (0xFC, 2),
        (0xF8, 3),
    ] {
        reg(&mut v, at, value);
    }
    for parameter in 0..2 {
        let table = 0x70000 + parameter * 0x80;
        for off in [0x10, 0x14, 0x1C, 0x2C, 0x4C, 0x50] {
            long(&mut v, table + off, 0x10000);
        }
    }
    for base in [0x20000, 0x40000] {
        for y in 0..64 {
            for x in 0..64 {
                let offset = if bitmap && base == 0x20000 {
                    y * 512 + x
                } else {
                    ((y / 8) * 8 + x / 8) * 64 + y % 8 * 8 + x % 8
                };
                word(
                    &mut v,
                    base + offset * 2,
                    0x8000 | ((y * 17 + x) % 1023 + 1) as u16,
                );
            }
        }
    }
    for cell in 0..64 {
        for (base, character) in [(0, 0x1000), (0x60000, 0x2000)] {
            word(
                &mut v,
                base + (cell / 8 * 64 + cell % 8) * 4 + 2,
                character + cell as u16 * 4,
            );
        }
    }
    for index in 1..1024 {
        v.cram[index * 2..index * 2 + 2].copy_from_slice(&(index as u16).to_be_bytes());
    }
    v
}
#[test]
fn rotation_sizes_parameters_fields_and_independent_rbg1_density_have_literal_pixels() {
    for dual in [false, true] {
        for bitmap in [false, true] {
            for parameter in 0..2 {
                if dual && parameter == 1 {
                    continue;
                }
                for selected in [0, 1, 0x10, 0x11] {
                    for code in 0..16u16 {
                        let mut v = rotation(dual, bitmap);
                        reg(&mut v, 0xB0, parameter);
                        reg(&mut v, 0x22, code << 12 | code << 8 | selected);
                        for field in 0..2u64 {
                            v.scan_field(field == 0);
                            for line in 0..4u16 {
                                v.begin_scanline(line as usize);
                                for x in 0..35 {
                                    v.acquire_rotation_display(
                                        &Vdp1::default(),
                                        field,
                                        line,
                                        x as u32 * 4,
                                        x,
                                    );
                                    for (rbg1, id) in [(false, 4), (true, 0)] {
                                        if rbg1 && !dual {
                                            continue;
                                        }
                                        let y = line as usize * 2 + field as usize;
                                        let mosaic = selected & (1 << id) != 0;
                                        let sx = if mosaic {
                                            x / usize::from(code + 1) * usize::from(code + 1)
                                        } else {
                                            x
                                        };
                                        let sy = if mosaic { line as usize } else { y };
                                        v.beam_rendering = true;
                                        let sample = v
                                            .scheduled_rotation_sample(rbg1)
                                            .flatten()
                                            .map(|s| s.image);
                                        v.beam_rendering = false;
                                        // Check a standalone snapshot independently of retained dot operands.
                                        let live = v.rotation_sample(rbg1, x, y, false).unwrap();
                                        let offset = if bitmap && !rbg1 {
                                            sy * 512 + sx
                                        } else {
                                            ((sy / 8) * 8 + sx / 8) * 64 + sy % 8 * 8 + sx % 8
                                        };
                                        assert_eq!(
                                            live.image.0 + live.image.1 * 2,
                                            (if rbg1 || (!bitmap && parameter == 1) {
                                                0x40000
                                            } else {
                                                0x20000
                                            }) + offset * 2
                                        );
                                        assert_eq!(sample, Some(live.image));
                                    }
                                }
                                v.scan_to(&Vdp1::default(), line as usize, 35);
                                let y = line as usize * 2 + field as usize;
                                let id = if dual { 0 } else { 4 };
                                let sy = if selected & (1 << id) != 0 {
                                    line as usize
                                } else {
                                    y
                                };
                                assert_eq!(&v.scanning[(y * 320) * 4..][..3], rgb(sy * 17 + 1));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn rotation_mosaic_precedes_translation_scaling_and_coefficient_coordinates() {
    for bitmap in [false, true] {
        for parameter in 0..2 {
            for (kx, ky) in [(0x8000u32, 0x20000u32), (0x20000, 0x8000)] {
                let mut v = rotation(false, bitmap);
                reg(&mut v, 0xB0, parameter);
                reg(&mut v, 0x22, 0xF310); // V size 16 must not quantize rotation Y.
                let table = 0x70000 + parameter as usize * 0x80;
                long(&mut v, table + 0x44, 3 << 16);
                long(&mut v, table + 0x48, 5 << 16);
                long(&mut v, table + 0x4C, kx);
                long(&mut v, table + 0x50, ky);
                for field in 0..2 {
                    v.scan_field(field == 0);
                    for line in 0..5u16 {
                        for x in 0..20 {
                            let sy = 5 + usize::from(line) * ky as usize / 65536;
                            let sx = 3 + (x / 4 * 4) * kx as usize / 65536;
                            let y = usize::from(line) * 2 + field as usize;
                            let sample = v.rotation_sample(false, x, y, false).unwrap();
                            let offset = if bitmap {
                                sy * 512 + sx
                            } else {
                                (sy / 8 * 8 + sx / 8) * 64 + sy % 8 * 8 + sx % 8
                            };
                            let base = if !bitmap && parameter == 1 {
                                0x40000
                            } else {
                                0x20000
                            };
                            assert_eq!(sample.image.0 + sample.image.1 * 2, base + offset * 2);
                            v.acquire_rotation_display(
                                &Vdp1::default(),
                                field,
                                line,
                                x as u32 * 4,
                                x,
                            );
                            v.beam_rendering = true;
                            assert_eq!(
                                v.scheduled_rotation_sample(false).flatten().unwrap().image,
                                sample.image
                            );
                            v.beam_rendering = false;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn field_boundary_size_enable_and_scroll_updates_refetch_normal_image_rows() {
    let mut v = normal(0, true);
    reg(&mut v, 0, 0x80C0);
    for (field, mosaic, scroll, expected_y) in [
        (0u64, 1, 0, 0usize),
        (1, 0x3001, 5, 5),
        (2, 0, 9, 9),
        (3, 1, 1, 1),
    ] {
        reg(&mut v, 0x22, mosaic);
        reg(&mut v, 0x74, scroll);
        v.scan_field(field.is_multiple_of(2));
        for phase in (0..64).step_by(4) {
            v.acquire_display(field, 0, phase, phase as usize / 4 % 8);
        }
        assert!(
            v.display_fetch
                .borrow()
                .trace
                .iter()
                .any(|r| r.path == 1 && r.address == 0x20000 + expected_y * 512)
        );
        v.begin_scanline(0);
        v.scan_to(&Vdp1::default(), 0, 4);
        let y = field as usize % 2;
        assert_eq!(
            &v.scanning[(y * 320) * 4..][..3],
            rgb(expected_y * 17 % 255 + 1)
        );
    }
}

#[test]
fn mosaic_line_scroll_latches_selected_image_row_and_keeps_other_layers_independent() {
    let mut v = normal(0, true);
    reg(&mut v, 0, 0x80C0);
    reg(&mut v, 0x22, 0x1001);
    reg(&mut v, 0x9A, 0x202);
    reg(&mut v, 0xA0, 3);
    reg(&mut v, 0xA4, 3);
    reg(&mut v, 0xA6, 0x1000);
    for row in 0..16 {
        long(&mut v, 0x60000 + row * 4, (row as u32 + 1) << 16);
        long(&mut v, 0x62000 + row * 4, (row as u32 + 20) << 16);
    }
    v.read_line_tables(10); // field line 5 -> mosaic upper-left field line 4
    v.beam_rendering = true;
    let sample = v.normal_sample(0, 0, 10).unwrap();
    assert_eq!(sample.0 + sample.1, 0x20000 + 4 * 512 + 5);
    // Unselected NBG1 retains output-density line-table index 10.
    assert_eq!(v.normal_coordinates(1, 0, 10).unwrap().0, 30);
    long(&mut v, 0x60010, 63 << 16);
    let sample = v.normal_sample(0, 0, 10).unwrap();
    assert_eq!(sample.0 + sample.1, 0x20000 + 4 * 512 + 5);
}

#[test]
fn minimum_mosaic_selects_source_row_before_each_applicable_color_decoder() {
    // At output (2,6), minimum mosaic must decode source (2,3).
    // Each format has a separately authored physical address and red literal.
    for layer in 0..4 {
        for bitmap in [false, true] {
            if bitmap && layer > 1 {
                continue;
            }
            for format in 0..=if layer == 0 {
                4
            } else if layer == 1 {
                3
            } else {
                1
            } {
                let mut v = normal(layer, bitmap);
                reg(&mut v, 0, 0x80C0);
                reg(&mut v, 0x22, 1 << layer);
                if layer < 2 {
                    reg(
                        &mut v,
                        0x28,
                        (format << 4 | if bitmap { 2 } else { 0 }) << (layer * 8),
                    );
                } else {
                    reg(&mut v, 0x2A, format << (1 + (layer - 2) * 4));
                }
                let pixel = if bitmap { 3 * 512 + 2 } else { 26 };
                let address = 0x20000
                    + match format {
                        0 => pixel / 2,
                        1 => pixel,
                        2 | 3 => pixel * 2,
                        _ => pixel * 4,
                    };
                let data: &[u8] = match format {
                    0 => &[0x11],
                    1 => &[1],
                    2 => &[0, 1],
                    3 => &[0x80, 0x1F],
                    _ => &[0x80, 0, 0, 0xFF],
                };
                v.vram[address..address + data.len()].copy_from_slice(data);
                v.cram[2..4].copy_from_slice(&0x801Fu16.to_be_bytes());
                assert_eq!(v.background(layer, 2, 6, false).unwrap().rgb, [255, 0, 0]);
                // A dedicated character bank can grant all eight timings even
                // when the independent name bank needs its own T0 transfer.
                reg(&mut v, 0x14, (4 + layer as u16) * 0x1111);
                reg(&mut v, 0x16, (4 + layer as u16) * 0x1111);
                for phase in (0..64).step_by(4) {
                    v.acquire_display(0, 3, phase, phase as usize / 4 % 8);
                }
                assert!(
                    v.display_fetch
                        .borrow()
                        .trace
                        .iter()
                        .any(|r| r.path == 1 && r.address <= address && address < r.address + 4)
                );
                v.begin_scanline(3);
                v.scan_to(&Vdp1::default(), 3, 3);
                assert_eq!(&v.scanning[(6 * 320 + 2) * 4..][..3], [255, 0, 0]);
            }
        }
    }
    for rbg1 in [false, true] {
        for bitmap in [false, true] {
            if bitmap && rbg1 {
                continue;
            }
            for format in 0..=4 {
                let mut v = rotation(rbg1, bitmap);
                reg(&mut v, 0x22, 0xF011);
                reg(&mut v, 0x2A, format << 12 | if bitmap { 0x200 } else { 0 });
                reg(&mut v, 0x28, format << 4);
                let pixel = if bitmap { 3 * 512 + 2 } else { 26 };
                let address = (if rbg1 { 0x40000 } else { 0x20000 })
                    + match format {
                        0 => pixel / 2,
                        1 => pixel,
                        2 | 3 => pixel * 2,
                        _ => pixel * 4,
                    };
                let data: &[u8] = match format {
                    0 => &[0x11],
                    1 => &[1],
                    2 => &[0, 1],
                    3 => &[0x80, 0x1F],
                    _ => &[0x80, 0, 0, 0xFF],
                };
                v.vram[address..address + data.len()].copy_from_slice(data);
                v.cram[2..4].copy_from_slice(&0x801Fu16.to_be_bytes());
                v.acquire_rotation_display(&Vdp1::default(), 0, 3, 8, 2);
                v.scan_field(true);
                v.begin_scanline(3);
                v.scan_to(&Vdp1::default(), 3, 3);
                assert_eq!(
                    &v.scanning[(6 * 320 + 2) * 4..][..3],
                    [255, 0, 0],
                    "rbg1={rbg1}, bitmap={bitmap}, format={format}"
                );
            }
        }
    }
}
