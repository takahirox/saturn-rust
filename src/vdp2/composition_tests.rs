//! Limited priority stacks versus the original four-image compositor.
use super::*;
fn reg(v: &mut Vdp2, at: u32, value: u16) {
    v.write16(VDP2 + at, value).unwrap();
}
fn word(v: &mut Vdp2, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn limited_stacks_preserve_ties_blending_line_color_gradation_offsets_and_sprite_shadow() {
    let mut v = Vdp2::default();
    for (at, value) in [
        (0, 0x8000),
        (0xE, 0x300),
        (0x20, 0xF0F),
        (0x28, 0x1212),
        (0x2A, 0x22),
        (0x3C, 0x21),
        (0x78, 1),
        (0x7C, 1),
        (0x88, 1),
        (0x8C, 1),
        (0xE4, 0x3210),
        (0x108, 0x1703),
        (0x10A, 0x0B1D),
        (0x10E, 0x1F0D),
        (0x114, 13),
        (0x116, 0x1F9),
        (0x118, 21),
        (0x11A, 0x1F2),
        (0x11C, 17),
        (0x11E, 0x1E3),
    ] {
        reg(&mut v, at, value);
    }
    // The table addresses are word addresses in two register halves.
    reg(&mut v, 0xAC, 3);
    reg(&mut v, 0xAE, 0x8000);
    reg(&mut v, 0xA8, 3);
    reg(&mut v, 0xAA, 0x8800);
    for i in 0..2048 {
        v.cram[i * 2..i * 2 + 2]
            .copy_from_slice(&((i as u16).wrapping_mul(37) | 0x8000).to_be_bytes());
    }
    for i in 0..1024 {
        word(&mut v, i * 4, 0);
        word(&mut v, i * 4 + 2, 0x3000);
    }
    for i in 0usize..4096 {
        v.vram[0x20000 + i] = (i.wrapping_mul(13) as u8) | 1;
        v.vram[0x40000 + i] = (i.wrapping_mul(7) as u8) | 1;
        v.vram[0x60000 + i] = (i.wrapping_mul(3) as u8) | 1;
    }
    word(&mut v, 0x70000, 0x7C00);
    word(&mut v, 0x71000, 3);
    reg(&mut v, 0x2A, 0x1022);
    reg(&mut v, 0x2C, 0x3030);
    reg(&mut v, 0xE6, 1);
    reg(&mut v, 0x26, 0xAAAA);
    reg(&mut v, 0xBC, 2);
    reg(&mut v, 0xBE, 0x8000);
    for base in [0x50000, 0x50080] {
        for off in [0x10, 0x14, 0x1C, 0x2C, 0x4C, 0x50] {
            v.vram[base + off..base + off + 4].copy_from_slice(&0x10000u32.to_be_bytes());
        }
    }
    // RBG1's names/characters use the documented fixed B1/B0 banks.
    reg(&mut v, 0x3E, 0x10);
    reg(&mut v, 0x60, 0x1818);
    for i in 0..1024 {
        word(&mut v, 0x60000 + i * 4, 0);
        word(&mut v, 0x60000 + i * 4 + 2, 0x2000);
    }
    reg(&mut v, 0x20, 0x1130);
    reg(&mut v, 0xF8, 3);
    reg(&mut v, 0xFC, 3);
    assert!(v.rotation(3, 0, false).is_some());
    assert!(v.rotation1(3, 0, false).is_some());
    let mut sprites = Vdp1::default();
    for y in 0..4 {
        for x in 0..32 {
            let data = [0x801F, 0x7FFE, 0x4001, 0xC002, 0, 0x7FFF][(x + y) % 6];
            let at = (y * 512 + x) * 2;
            sprites.draw_buffer_mut()[at..at + 2].copy_from_slice(&u16::to_be_bytes(data));
        }
    }
    sprites.vblank_out();
    let mut compared = 0;
    for enabled in [0xF0F, 0x1F1F, 0x1130] {
        reg(&mut v, 0x20, enabled);
        for special in [0, 0x155, 0x2AA] {
            reg(&mut v, 0xEA, special);
            reg(&mut v, 0xEE, if special == 0x2AA { 0x2AA } else { 0 });
            for kind in 0..16 {
                reg(&mut v, 0xE0, kind | 0x20);
                for priorities in [0x0707, 0x0303, 0x0701, 0x0107] {
                    for at in [0xF0, 0xF2, 0xF4, 0xF6, 0xF8, 0xFA] {
                        reg(&mut v, at, priorities);
                    }
                    reg(&mut v, 0xFC, priorities & 7);
                    for control in [
                        0, 0xF, 0x40, 0x7F, 0x17F, 0x27F, 0x47F, 0x77F, 0xA00F, 0x8040,
                    ] {
                        reg(&mut v, 0xEC, control);
                        for effects in [false, true] {
                            reg(&mut v, 0xE0, kind | if effects { 0x10 } else { 0x20 });
                            for at in [0xD0, 0xD2, 0xD4] {
                                reg(&mut v, at, if effects { 0x3030 } else { 0 });
                            }
                            reg(&mut v, 0xD6, if effects { 0x3000 } else { 0 });
                            reg(&mut v, 0xE8, if effects { 0x2F } else { 0 });
                            reg(&mut v, 0xE2, if effects { 0x5F } else { 0 });
                            reg(&mut v, 0x110, if effects { 0x5F } else { 0 });
                            reg(&mut v, 0x112, if effects { 0x55 } else { 0 });
                            let mut fast_history = [[1, 27, 93], [255, 0, 171]];
                            let mut literal_history = fast_history;
                            let fast = v
                                .render_region::<true>(
                                    Some(&sprites),
                                    0,
                                    4,
                                    3..19,
                                    &mut fast_history,
                                )
                                .unwrap();
                            let literal = v
                                .render_region::<false>(
                                    Some(&sprites),
                                    0,
                                    4,
                                    3..19,
                                    &mut literal_history,
                                )
                                .unwrap();
                            assert_eq!(
                                fast, literal,
                                "kind={kind} priority={priorities:X} control={control:X} effects={effects}"
                            );
                            assert_eq!(fast_history, literal_history);
                            assert!(fast.as_chunks::<4>().0.iter().any(|p| p[..3] != [0, 0, 0]));
                            compared += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(compared, 11520);
}

#[test]
fn scanout_reuses_pixels_across_display_disable_and_preserves_error_prefixes() {
    let mut v = Vdp2::default();
    let sprites = Vdp1::default();
    reg(&mut v, 0, 0x8000);
    word(&mut v, 0, 0x001F);
    v.scan_to(&sprites, 0, 4);
    assert_eq!(&v.scanning[..16], &[255, 0, 0, 255].repeat(4));
    let original = v.scanning.clone();
    // The same line buffer still contains the previous red field. Disabling
    // both display and border must overwrite RGB, not just the alpha byte.
    reg(&mut v, 0, 0);
    v.begin_scanline(0);
    for end in [1, 3, 4] {
        v.scan_to(&sprites, 0, end);
        assert_eq!(&v.scanning[..end * 4], &[0, 0, 0, 255].repeat(end));
        assert_eq!(&v.scanning[end * 4..], &original[end * 4..]);
    }
    let before_error = v.scanning.clone();
    reg(&mut v, 0, 0x8008); // Reserved display-mode bit.
    v.begin_scanline(0);
    v.scan_to(&sprites, 0, 4);
    assert!(
        v.scan_error
            .as_deref()
            .unwrap()
            .contains("Unsupported VDP2")
    );
    assert_eq!(v.scanning, before_error);
    reg(&mut v, 0, 0x8000);
    v.begin_scanline(0);
    v.scan_to(&sprites, 0, 4);
    assert_eq!(&v.scanning[..16], &[255, 0, 0, 255].repeat(4));
}
