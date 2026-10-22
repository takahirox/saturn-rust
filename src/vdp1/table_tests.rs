//! Independent ST-013-R3 §3.1 step5 / §5.2 table-load vectors.
use super::*;

fn word(v: &mut Vdp1, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn lookup_sprite(flip: u16, shading: bool) -> Vdp1 {
    let mut v = Vdp1::default();
    for (at, value) in [
        (0, 9),
        (20, 511),
        (22, 255),
        (32, flip),
        (36, 0xC8 | if shading { 4 } else { 0 }),
        (38, 0x200),
        (40, 0x100),
        (42, 0x102),
        (60, 0x300),
        (64, 0x8000),
        (0x800, 0x0123),
        (0x802, 0x4567),
        (0x804, 0x89AB),
        (0x806, 0xCDEF),
    ] {
        word(&mut v, at, value);
    }
    for entry in 0..16 {
        word(&mut v, 0x1000 + entry * 2, 0x8000 | entry as u16);
    }
    for entry in 0..4 {
        word(&mut v, 0x1800 + entry * 2, 0x4210); // zero shade adjustment
    }
    v.write16(REGISTERS + 2, 2).unwrap();
    v.write16(REGISTERS + 4, 1).unwrap();
    v
}
fn advance(v: &mut Vdp1, clocks: u32, chunk: u32, vram: bool, framebuffer: bool) -> bool {
    let mut left = clocks;
    let mut ended = false;
    while left != 0 {
        let delta = left.min(chunk);
        ended |= v.advance_with_bus(delta, vram, framebuffer).unwrap();
        left -= delta;
    }
    ended
}
fn pixel(v: &Vdp1, x: usize, y: usize) -> u16 {
    let at = (y * 512 + x) * 2;
    u16::from_be_bytes(v.draw_buffer()[at..at + 2].try_into().unwrap())
}

#[test]
fn lookup_words_are_retained_at_separate_deadlines_before_any_texture_pixel() {
    for chunk in [1, 3, 17, 10000] {
        for captured in 0..=16 {
            for flip in [0, 0x10, 0x20, 0x30] {
                for shading in [false, true] {
                    let mut v = lookup_sprite(flip, shading);
                    let start = 64 + if shading { 8 } else { 0 };
                    assert!(!advance(&mut v, start + captured * 2, chunk, false, false));
                    assert!(v.draw_buffer().iter().all(|&b| b == 0));
                    for entry in 0..16 {
                        word(&mut v, 0x1000 + entry * 2, 0xFC00 | entry as u16);
                    }
                    // Complete table, sixteen pixels, then stop just before END.
                    assert!(!advance(
                        &mut v,
                        (16 - captured) * 2 + 8 + 16 + 31,
                        chunk,
                        false,
                        false
                    ));
                    assert_eq!(v.edsr, 0);
                    for y in 0..2 {
                        for x in 0..8 {
                            let sx = if flip & 0x10 == 0 { x } else { 7 - x };
                            let sy = if flip & 0x20 == 0 { y } else { 1 - y };
                            let index = sy * 8 + sx;
                            let base = if index < captured as usize {
                                0x8000
                            } else {
                                0xFC00
                            };
                            assert_eq!(
                                pixel(&v, x, y),
                                base | index as u16,
                                "captured={captured}, flip={flip:X}, shade={shading}, ({x},{y})"
                            );
                        }
                    }
                    assert!(advance(&mut v, 1, chunk, false, false));
                    assert_eq!(v.edsr, 2);
                }
            }
        }
    }
}

#[test]
fn lookup_fetch_uses_vram_ownership_and_drawing_uses_its_retained_table() {
    for chunk in [1, 3, 17, 10000] {
        let mut v = lookup_sprite(0, false);
        advance(&mut v, 64, chunk, false, false);
        // VRAM ownership pauses at even a partial word; framebuffer does not.
        advance(&mut v, 1, chunk, false, true);
        advance(&mut v, 100, chunk, true, false);
        word(&mut v, 0x1000, 0x83E0);
        advance(&mut v, 31, chunk, false, true);
        assert_eq!(v.drawing.as_ref().unwrap().lookup_read, 16);
        assert_eq!(pixel(&v, 0, 0), 0);
        // Table can change while the framebuffer is blocked, without resampling.
        word(&mut v, 0x1000, 0xFC00);
        advance(&mut v, 100, chunk, false, true);
        assert_eq!(pixel(&v, 0, 0), 0);
        advance(&mut v, 1, chunk, false, false);
        assert_eq!(pixel(&v, 0, 0), 0x83E0);
        assert_eq!(pixel(&v, 1, 0), 0);
        // VRAM contention cannot block the remaining nibbles of a sampled word.
        advance(&mut v, 100, chunk, true, false);
        assert_eq!(pixel(&v, 1, 0), 0x8001);
        assert_eq!(pixel(&v, 3, 0), 0x8003);
        assert_eq!(pixel(&v, 4, 0), 0);
        assert!(advance(&mut v, 50, chunk, false, false));
    }
}

#[test]
fn stop_and_restart_discard_every_partial_lookup_load_without_publishing_pixels() {
    for captured in 0..16 {
        let mut v = lookup_sprite(0, false);
        advance(&mut v, 64 + captured * 2 + 1, 3, false, false);
        v.write16(REGISTERS + 12, 0).unwrap();
        assert!(!advance(&mut v, 29, 17, true, false));
        assert!(v.busy());
        assert!(!advance(&mut v, 1, 1, true, false));
        assert!(!v.busy());
        assert!(v.drawing.is_none());
        assert!(v.draw_buffer().iter().all(|&b| b == 0));
        for entry in 0..16 {
            word(&mut v, 0x1000 + entry * 2, 0x83E0);
        }
        v.write16(REGISTERS + 4, 1).unwrap();
        assert!(advance(&mut v, 152, 17, false, false));
        assert_eq!(pixel(&v, 0, 0), 0x83E0);
        assert_eq!(pixel(&v, 7, 1), 0x83E0);
        // PTMR also abandons an unfinished table without retaining old entries.
        v.draw_buffer_mut().fill(0);
        v.write16(REGISTERS + 4, 1).unwrap();
        advance(&mut v, 64 + captured * 2, 3, false, false);
        for entry in 0..16 {
            word(&mut v, 0x1000 + entry * 2, 0xFC00);
        }
        v.write16(REGISTERS + 4, 1).unwrap();
        assert!(advance(&mut v, 152, 17, false, false));
        assert_eq!(pixel(&v, 0, 0), 0xFC00);
        assert_eq!(pixel(&v, 7, 1), 0xFC00);
    }
}

#[test]
fn physical_texture_words_retain_all_nibbles_bytes_and_rgb_dots_in_both_directions() {
    // ST-013 §§2.1/5.1, fig5.1: a 16-bit VRAM read carries four
    // 4-bit dots, two 8-bit dots, or one RGB dot. Mutation after a word
    // deadline cannot change any of its not-yet-published components.
    for mode in 0..=5 {
        let dots_per_word = match mode {
            0 | 1 => 4,
            2..=4 => 2,
            _ => 1,
        };
        let words = 16 / dots_per_word;
        for flip in [0, 0x10, 0x20, 0x30] {
            for captured in 0..=words {
                for chunk in [1, 3, 17, 10000] {
                    let mut v = lookup_sprite(flip, false);
                    word(&mut v, 36, 0xC0 | mode << 3);
                    if mode != 1 {
                        word(&mut v, 38, 0x8000);
                    }
                    match mode {
                        2..=4 => {
                            for i in 0..16 {
                                v.vram[0x800 + i] = (if mode == 2 {
                                    64
                                } else if mode == 3 {
                                    128
                                } else {
                                    224
                                }) + i as u8;
                            }
                        }
                        5 => {
                            for i in 0..16 {
                                word(&mut v, 0x800 + i * 2, 0x8010 + i as u16);
                            }
                        }
                        _ => (),
                    }
                    let setup = 64 + if mode == 1 { 32 } else { 0 };
                    let elapsed = if captured == 0 {
                        0
                    } else {
                        captured * 2 + (captured - 1) * dots_per_word
                    };
                    advance(&mut v, setup + elapsed, chunk, false, false);
                    v.vram[0x800..0x820].fill(0);
                    let remaining = (words - captured) * 2 + 16
                        - if captured == 0 {
                            0
                        } else {
                            (captured - 1) * dots_per_word
                        }
                        + 31;
                    assert!(!advance(&mut v, remaining, chunk, false, false));
                    for y in 0..2 {
                        for x in 0..8 {
                            let sx = if flip & 0x10 == 0 { x } else { 7 - x };
                            let sy = if flip & 0x20 == 0 { y } else { 1 - y };
                            let index = (sy * 8 + sx) as u16;
                            let retained = ((y * 8 + x) as u32 / dots_per_word) < captured;
                            let expected = if retained {
                                match mode {
                                    0..=3 => 0x8000 | index,
                                    4 => 0x80E0 | index,
                                    _ => 0x8010 + index,
                                }
                            } else if mode == 5 {
                                0
                            } else {
                                0x8000
                            };
                            assert_eq!(
                                pixel(&v, x, y),
                                expected,
                                "mode={mode}, flip={flip:X}, captured={captured}, ({x},{y})"
                            );
                        }
                    }
                    assert!(advance(&mut v, 1, chunk, false, false));
                }
            }
        }
    }
}

#[test]
fn texture_fetch_completion_and_framebuffer_calculation_use_separate_owners() {
    for chunk in [1, 3, 17, 10000] {
        let mut v = lookup_sprite(0, false);
        word(&mut v, 36, 0xEB); // RGB, translucent, no Gouraud
        word(&mut v, 0x800, 0x801F);
        v.draw_buffer_mut()[..2].copy_from_slice(&0xFC00u16.to_be_bytes());
        advance(&mut v, 65, chunk, false, false); // partial RGB fetch
        advance(&mut v, 100, chunk, true, false);
        assert_eq!(pixel(&v, 0, 0), 0xFC00);
        word(&mut v, 0x800, 0x83E0);
        advance(&mut v, 1, chunk, false, true); // word completes on VRAM
        word(&mut v, 0x800, 0x801F); // must not replace the sampled green
        advance(&mut v, 100, chunk, true, true);
        advance(&mut v, 1, chunk, true, false); // RMW samples framebuffer
        v.draw_buffer_mut()[..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        advance(&mut v, 100, chunk, false, true); // framebuffer ownership stalls RMW
        assert_eq!(pixel(&v, 0, 0), 0xFFFF);
        advance(&mut v, 4, chunk, true, false);
        assert_eq!(pixel(&v, 0, 0), 0xFFFF);
        advance(&mut v, 1, chunk, true, false);
        assert_eq!(pixel(&v, 0, 0), 0xBDE0); // half green plus blue, retained
        assert_eq!(pixel(&v, 1, 0), 0); // next RGB fetch is still blocked
    }
}
