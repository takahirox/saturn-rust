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
