//! Literal source indices from the 1997 VDP1 §6.3 Figure 6.5, all eight rows.
//! The tables are independent of production coordinate/address generation.
use saturn_rust::vdp1::{REGISTERS, Vdp1};

const ORDINARY: [&[u16]; 8] = [
    &[1, 4, 6],
    &[1, 3, 5, 7],
    &[0, 2, 4, 5, 7],
    &[0, 2, 3, 4, 6, 7],
    &[0, 1, 2, 4, 5, 6, 7],
    &[0, 1, 2, 3, 4, 5, 6, 7],
    &[0, 1, 2, 3, 3, 4, 5, 6, 7],
    &[0, 1, 2, 2, 3, 4, 5, 5, 6, 7],
];
const EVEN: [&[u16]; 8] = [
    &[0, 4, 6],
    &[0, 2, 4, 6],
    &[0, 2, 2, 4, 6],
    &[0, 2, 2, 4, 4, 6],
    &[0, 0, 2, 2, 4, 4, 6],
    &[0, 1, 2, 3, 4, 5, 6, 7],
    &[0, 1, 2, 3, 3, 4, 5, 6, 7],
    &[0, 1, 2, 2, 3, 4, 5, 5, 6, 7],
];
const ODD: [&[u16]; 8] = [
    &[1, 5, 7],
    &[1, 3, 5, 7],
    &[1, 3, 3, 5, 7],
    &[1, 3, 3, 5, 5, 7],
    &[1, 1, 3, 3, 5, 5, 7],
    &[0, 1, 2, 3, 4, 5, 6, 7],
    &[0, 1, 2, 3, 3, 4, 5, 6, 7],
    &[0, 1, 2, 2, 3, 4, 5, 5, 6, 7],
];
fn word(v: &mut Vdp1, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn sprite(
    width: u16,
    span: u16,
    kind: u16,
    mode: u16,
    reverse: bool,
    flip: bool,
    eos: bool,
) -> Vdp1 {
    let mut v = Vdp1::default();
    let (a, b) = if reverse {
        (30 + span - 1, 30)
    } else {
        (30, 30 + span - 1)
    };
    for (at, value) in [
        (0, 9),
        (20, 511),
        (22, 255),
        (64, 0x8000),
        (32, kind | if flip { 0x10 } else { 0 }),
        (36, mode),
        (38, if mode >> 3 & 7 == 1 { 0x200 } else { 0x100 }),
        (40, 0x100),
        (42, (width / 8) << 8 | 1),
        (44, a),
        (46, 20),
        (48, b),
        (50, 20),
        (52, b),
        (54, 20),
        (56, a),
        (58, 20),
    ] {
        word(&mut v, at, value);
    }
    v.write16(REGISTERS + 2, 3 | if eos { 16 } else { 0 })
        .unwrap();
    v.vblank_out();
    v
}
fn finish(v: &mut Vdp1) {
    v.write16(REGISTERS + 4, 1).unwrap();
    for _ in 0..2000 {
        if v.advance(v.clocks_to_event()).unwrap() {
            return;
        }
    }
    panic!("END was not fetched");
}
fn pixels(v: &Vdp1, span: usize, y: usize) -> Vec<u16> {
    (30..30 + span)
        .map(|x| {
            let at = (y * 512 + x) * 2;
            u16::from_be_bytes(v.draw_buffer()[at..at + 2].try_into().unwrap())
        })
        .collect()
}
#[test]
fn every_figure_6_5_row_reaches_the_framebuffer_in_both_directions_and_flips() {
    for kind in [1, 2] {
        for reverse in [false, true] {
            for flip in [false, true] {
                for (hss, eos, table) in [
                    (false, false, ORDINARY),
                    (false, true, ORDINARY),
                    (true, false, EVEN),
                    (true, true, ODD),
                ] {
                    for (row, &indices) in table.iter().enumerate() {
                        let mut v = sprite(
                            8,
                            row as u16 + 3,
                            kind,
                            0xE8 | if hss { 0x1000 } else { 0 },
                            reverse,
                            flip,
                            eos,
                        );
                        for i in 0..8 {
                            word(&mut v, 0x800 + i * 2, 0x8000 + i as u16);
                        }
                        finish(&mut v);
                        let mut expected: Vec<_> = indices
                            .iter()
                            .map(|&i| {
                                // EOS names original source parity even with a read flip.
                                let i = if flip {
                                    if hss && row < 5 {
                                        (6 - (i & !1)) | u16::from(eos)
                                    } else {
                                        7 - i
                                    }
                                } else {
                                    i
                                };
                                0x8000 + i
                            })
                            .collect();
                        if reverse {
                            expected.reverse();
                        }
                        assert_eq!(
                            pixels(&v, row + 3, 20),
                            expected,
                            "kind={kind} ratio={}/8 HSS={hss} EOS={eos} reverse={reverse} flip={flip}",
                            row + 3
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn additional_legal_widths_use_the_same_reduction_and_enlargement_contract() {
    // Independently calculated extension of the documented sampling rule.
    for (width, indices) in [
        (16, &[2, 6, 10, 14][..]),
        (24, &[3, 9, 15, 21][..]),
        (504, &[63, 189, 315, 441][..]),
        (16, &[0, 2, 4, 6, 8, 9, 11, 13, 15][..]),
        (8, &[4][..]),
        (8, &[0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7][..]),
    ] {
        for kind in [1, 2] {
            let mut v = sprite(width, indices.len() as u16, kind, 0xE8, false, false, false);
            for i in 0..width as usize {
                word(&mut v, 0x800 + i * 2, 0x8000 + i as u16);
            }
            finish(&mut v);
            assert_eq!(
                pixels(&v, indices.len(), 20),
                indices.iter().map(|i| 0x8000 + i).collect::<Vec<_>>()
            );
        }
    }
}
#[test]
fn scaled_acquisition_decodes_all_six_formats_across_packed_words_and_lookup_entries() {
    for mode in 0..=5 {
        for (span, indices) in [(3, ORDINARY[0]), (4, ORDINARY[1]), (10, ORDINARY[7])] {
            let mut v = sprite(8, span, 1, 0xC0 | mode << 3, false, false, false);
            match mode {
                0 | 1 => {
                    word(&mut v, 0x800, 0x1234);
                    word(&mut v, 0x802, 0x5678);
                }
                2..=4 => {
                    for (i, w) in [0x0102, 0x0304, 0x0506, 0x0708].into_iter().enumerate() {
                        word(&mut v, 0x800 + i * 2, w);
                    }
                }
                _ => {
                    for (i, w) in [
                        0x801F, 0x83E0, 0xFC00, 0xFFFF, 0x8421, 0x8842, 0x8C63, 0x9084,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        word(&mut v, 0x800 + i * 2, w);
                    }
                }
            }
            for (i, w) in [
                0x801F, 0x83E0, 0xFC00, 0xFFFF, 0x8421, 0x8842, 0x8C63, 0x9084,
            ]
            .into_iter()
            .enumerate()
            {
                word(&mut v, 0x1002 + i * 2, w);
            }
            finish(&mut v);
            let expected = indices
                .iter()
                .map(|&i| {
                    if mode == 1 || mode == 5 {
                        [
                            0x801F, 0x83E0, 0xFC00, 0xFFFF, 0x8421, 0x8842, 0x8C63, 0x9084,
                        ][i as usize]
                    } else {
                        0x100 | (i + 1)
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(
                pixels(&v, span as usize, 20),
                expected,
                "mode={mode},span={span}"
            );
        }
    }
}
#[test]
fn distorted_rows_choose_their_own_span_and_vertical_flip_keeps_row_selection() {
    for flip in [false, true] {
        let mut v = sprite(8, 3, 2, 0xE8, false, false, false);
        word(&mut v, 32, 2 | if flip { 0x20 } else { 0 });
        word(&mut v, 42, 0x103);
        word(&mut v, 52, 34);
        word(&mut v, 54, 22);
        word(&mut v, 58, 22);
        for row in 0..3 {
            for x in 0..8 {
                word(
                    &mut v,
                    0x800 + (row * 8 + x) * 2,
                    0x8000 + (row * 16 + x) as u16,
                );
            }
        }
        finish(&mut v);
        for (row, indices) in [&[1, 4, 6][..], &[1, 3, 5, 7][..], &[0, 2, 4, 5, 7][..]]
            .into_iter()
            .enumerate()
        {
            let source_row = if flip { 2 - row } else { row };
            assert_eq!(
                pixels(&v, indices.len(), 20 + row),
                indices
                    .iter()
                    .map(|i| 0x8000 + source_row as u16 * 16 + i)
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn selected_pixels_preserve_transparency_end_code_rules_mesh_and_gouraud() {
    for mode in 0..=5 {
        for hss in [false, true] {
            for eos in [false, true] {
                for span in [4, 8, 10] {
                    let mut v = sprite(
                        8,
                        span,
                        1,
                        mode << 3 | if hss { 0x1000 } else { 0 },
                        false,
                        false,
                        eos,
                    );
                    let end = match mode {
                        0 | 1 => 15,
                        2..=4 => 255,
                        _ => 0x7fff,
                    };
                    let raw = [0, end, 1, 2, 3, end, 0, 0];
                    match mode {
                        0 | 1 => {
                            word(&mut v, 0x800, 0x0F12);
                            word(&mut v, 0x802, 0x3F00);
                        }
                        2..=4 => {
                            for (i, w) in [0x00FF, 0x0102, 0x03FF, 0].into_iter().enumerate() {
                                word(&mut v, 0x800 + i * 2, w);
                            }
                        }
                        _ => {
                            for (i, w) in raw.into_iter().enumerate() {
                                word(&mut v, 0x800 + i * 2, w);
                            }
                        }
                    }
                    for i in 0..16 {
                        word(&mut v, 0x1000 + i * 2, 0x9000 + i as u16);
                    }
                    finish(&mut v);
                    let indices = match (span, hss, eos) {
                        (4, true, false) => EVEN[1],
                        (4, true, true) => ODD[1],
                        (4, _, _) => ORDINARY[1],
                        (8, _, _) => ORDINARY[5],
                        _ => ORDINARY[7],
                    };
                    let mut codes = 0;
                    let expected = indices
                        .iter()
                        .map(|&i| {
                            let r = raw[i as usize];
                            if !hss && r == end {
                                codes += 1;
                                return 0;
                            }
                            if r == 0 || codes == 2 {
                                return 0;
                            }
                            match mode {
                                0 => 0x100 | r,
                                1 => 0x9000 | r,
                                2 => 0x100 | (r & 63),
                                3 => 0x100 | (r & 127),
                                4 => 0x100 | r,
                                _ => r,
                            }
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        pixels(&v, span as usize, 20),
                        expected,
                        "mode={mode} HSS={hss} EOS={eos} span={span}"
                    );
                }
            }
        }
    }
    let mut v = sprite(8, 5, 1, 0x1EC, false, false, false); // mesh + Gouraud + ECD/SPD
    word(&mut v, 60, 0x300);
    for i in 0..4 {
        word(&mut v, 0x1800 + i * 2, 0x4213);
    } // +3 red, neutral G/B
    for i in 0..8 {
        word(&mut v, 0x800 + i * 2, 0x8000 + i as u16);
    }
    finish(&mut v);
    assert_eq!(pixels(&v, 5, 20), [0x8003, 0, 0x8007, 0, 0x800A]);
}

#[test]
fn preclipping_reversal_and_user_clip_keep_the_original_sampling_phase() {
    for hss in [false, true] {
        for disable in [false, true] {
            let mut v = sprite(
                8,
                3,
                1,
                0xE8 | if hss { 0x1000 } else { 0 } | if disable { 0x800 } else { 0 },
                false,
                false,
                false,
            );
            word(&mut v, 44, (-1i16) as u16);
            word(&mut v, 52, 1);
            for i in 0..8 {
                word(&mut v, 0x800 + i * 2, 0x8000 + i as u16);
            }
            finish(&mut v);
            let at = (20 * 512) * 2;
            assert_eq!(&v.draw_buffer()[at..at + 4], &[0x80, 4, 0x80, 6]);
        }
    }
    // A user-clip boundary must not rescale the five-pixel source phase.
    let mut v = sprite(8, 5, 1, 0x4E8, false, false, false);
    // Insert a user clip between the initialized system clip and sprite.
    let draw = v.vram[32..64].to_vec();
    v.vram[64..96].copy_from_slice(&draw);
    v.vram[32..64].fill(0);
    word(&mut v, 96, 0x8000);
    for (at, value) in [(32, 8), (44, 32), (46, 20), (52, 34), (54, 20)] {
        word(&mut v, at, value);
    }
    for i in 0..8 {
        word(&mut v, 0x800 + i * 2, 0x8000 + i as u16);
    }
    finish(&mut v);
    assert_eq!(pixels(&v, 5, 20), [0, 0, 0x8004, 0x8005, 0x8007]);
}
