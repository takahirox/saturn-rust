//! Independent ch.4/§4.1 sample ordering and §4.4 aggregate capacities.
use super::*;
use crate::vdp1::{FRAMEBUFFER, REGISTERS};

fn reg(b: &mut Bus, offset: u32, value: u16) {
    b.write16(REGISTERS + offset, value).unwrap();
}
fn advance(b: &mut Bus, clocks: u32, chunk: u32) {
    let mut left = clocks;
    while left != 0 {
        let n = left.min(chunk);
        b.advance_main_clocks(n, 0).unwrap();
        left -= n;
    }
}
fn board(mode: u16, pal: bool, fast: bool, display: u16) -> Bus {
    let mut b = Bus::default();
    b.raster.pal = pal;
    b.raster.select_clock(fast);
    b.raster.configure(display);
    reg(&mut b, 0, mode);
    reg(&mut b, 6, 0x1234);
    reg(&mut b, 8, 0);
    reg(&mut b, 10, 64 << 9 | 255);
    b.vdp1.draw_buffer_mut().fill(0xEE);
    reg(&mut b, 2, 3);
    b.vdp1.vblank_out(); // capture A and display physical buffer zero
    b.vdp1.draw_buffer_mut().fill(0xDD);
    b
}
#[test]
fn legal_vbe_window_samples_last_completed_value_without_premature_publication() {
    for mode in [0, 1] {
        for fast in [false, true] {
            for pal in [false, true] {
                for chunk in [1, 3, 17, 10000] {
                    let l = if fast { 1820 } else { 1708 };
                    let s = if fast { 1408 } else { 1280 };
                    for writes in [&[8][..], &[8, 0][..], &[8, 0, 8][..]] {
                        let mut b = board(mode, pal, fast, 0);
                        advance(&mut b, 224 * l, chunk);
                        for &vbe in writes {
                            reg(&mut b, 0, mode | vbe);
                            advance(&mut b, 20, chunk);
                            assert_eq!(
                                b.vdp1.display_pixel(0, 0),
                                if mode == 0 { 0xEEEE } else { 0xEE }
                            );
                            assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 15, mode | vbe);
                        }
                        advance(&mut b, s - 20 * writes.len() as u32, chunk);
                        assert_eq!(
                            b.vdp1.display_pixel(0, 0),
                            if mode == 0 { 0xEEEE } else { 0xEE }
                        );
                        advance(&mut b, 2, chunk); // first integer opportunity, independently derived
                        assert_eq!(
                            b.vdp1.display_pixel(0, 0),
                            if writes.last() == Some(&0) {
                                if mode == 0 { 0xEEEE } else { 0xEE }
                            } else if mode == 0 {
                                0x1234
                            } else {
                                0x12
                            }
                        );
                        assert_eq!(b.read16(FRAMEBUFFER), Ok(0xDDDD));
                        assert_eq!(b.read16(REGISTERS + 0x10), Ok(0));
                        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn sampled_erase_preserves_all_standard_tv_capacity_and_end_cancellation() {
    // Literal table4.5 capacities, not values obtained from the implementation.
    for (pal, display, lines, slow, fast_capacity) in [
        (false, 0, 224, 58812usize, 63180usize),
        (false, 0x10, 240, 34684, 37260),
        (true, 0, 224, 134212, 144180),
        (true, 0x10, 240, 110084, 118260),
        (true, 0x20, 256, 85956, 92340),
    ] {
        for fast in [false, true] {
            for mode in [0, 1] {
                let l = if fast { 1820 } else { 1708 };
                let capacity = if fast { fast_capacity } else { slow };
                let mut b = board(mode, pal, fast, display);
                advance(&mut b, lines * l, 10000);
                reg(&mut b, 0, mode | 8);
                // Observe the original display buffer through the CPU aperture
                // after the requested manual change at blank end.
                reg(&mut b, 2, 3);
                let blank = (if pal { 313 } else { 263 }) - lines;
                advance(&mut b, blank * l - 1, 17);
                let last = capacity.min(131072) - 1;
                if capacity <= 131072 {
                    let x = if mode == 0 {
                        last % 512
                    } else {
                        (last % 512) * 2
                    };
                    let y = last / 512;
                    assert_eq!(
                        b.vdp1.display_pixel(x, y),
                        if mode == 0 { 0xEEEE } else { 0xEE }
                    );
                }
                advance(&mut b, 1, 1);
                for word in [0, last, (last + 1).min(131071), 131071] {
                    assert_eq!(
                        b.read16(FRAMEBUFFER + word as u32 * 2),
                        Ok(if word < capacity { 0x1234 } else { 0xEEEE })
                    );
                }
                advance(&mut b, 10000, 3);
                assert_eq!(
                    b.read16(FRAMEBUFFER + 262142),
                    Ok(if capacity >= 131072 { 0x1234 } else { 0xEEEE })
                );
                assert_eq!(b.read16(REGISTERS + 0x10), Ok(0));
                assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
            }
        }
    }
}

#[test]
fn completed_sh2_words_define_the_sample_and_equal_edge_is_only_an_emulator_probe() {
    const SAMPLE: u32 = 224 * 1708 + 1280;
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                // -1 is the permitted window. 0/+1 deliberately probe the
                // prohibited window; no hardware conformance is inferred.
                for offset in [-1i32, 0, 1] {
                    for initial in [0, 8] {
                        let mut b = board(initial, false, false, 0);
                        b.set_slave_enabled(true);
                        let completion = SAMPLE.wrapping_add_signed(offset);
                        advance(&mut b, completion - 8, chunk);
                        assert_eq!(
                            b.replay_cpu(p).write16(REGISTERS | 0x20000000, initial ^ 8),
                            Err(BusError::Pending)
                        );
                        b.grant_bus_transaction();
                        for _ in 0..7 {
                            advance(&mut b, 1, chunk);
                            if inspect {
                                assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 8, initial);
                                let _ = b.read16(FRAMEBUFFER).unwrap();
                                let _ = b.vdp2.framebuffer().unwrap();
                            }
                        }
                        assert_eq!(b.transactions.active.as_ref().unwrap().remaining, 1);
                        assert_eq!(
                            b.replay_cpu(p).write16(REGISTERS | 0x20000000, initial ^ 8),
                            Ok(())
                        );
                        b.finish_cpu_transaction(p);
                        assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 8, initial);
                        advance(&mut b, SAMPLE + 40 - (completion - 1), chunk);
                        assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 8, initial ^ 8);
                        let enabled = if offset < 0 {
                            initial == 0
                        } else {
                            initial == 8
                        };
                        assert_eq!(
                            b.vdp1.display_pixel(0, 0),
                            if enabled { 0x1234 } else { 0xEEEE }
                        );
                        assert_eq!(b.read16(FRAMEBUFFER), Ok(0xDDDD));
                        assert!(b.transactions.writes[p as usize].is_none());
                        assert_eq!(b.read16(REGISTERS + 0x10), Ok(0));
                        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn vbe_reset_and_real_pll_chunking_have_no_stale_words() {
    for pal in [false, true] {
        for fast in [false, true] {
            let mut expected = None;
            for chunk in [1, 3, 17, 10000] {
                let mut b = board(8, pal, fast, 0);
                b.system_clock.set_pal(pal);
                b.system_clock.select(fast);
                // Fixed reference-clock horizon crosses sample and field end.
                let mut left = 700000;
                while left != 0 {
                    let n = left.min(chunk);
                    b.advance_clocks(n).unwrap();
                    left -= n;
                }
                let state = (
                    b.main_clocks,
                    b.raster.line,
                    b.raster.fields,
                    b.vdp1.draw_buffer().to_vec(),
                    (0..131072)
                        .map(|pixel| b.vdp1.display_pixel(pixel % 512, pixel / 512))
                        .collect::<Vec<_>>(),
                );
                if let Some(expected) = &expected {
                    assert_eq!(&state, expected);
                } else {
                    expected = Some(state);
                }
            }
        }
    }
    for at in [224 * 1708 + 100, 224 * 1708 + 1300] {
        for chunk in [1, 3, 17, 10000] {
            let mut b = board(8, false, false, 0);
            advance(&mut b, at, chunk);
            assert_eq!(
                b.replay_cpu(Processor::Master)
                    .write16(REGISTERS | 0x20000000, 8),
                Err(BusError::Pending)
            );
            advance(&mut b, 4, chunk);
            b.system_reset();
            b.vdp1.draw_buffer_mut().fill(0xAA);
            advance(&mut b, 3000, chunk);
            assert!(b.vdp1.draw_buffer().iter().all(|&v| v == 0xAA));
            assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 8, 0);
            assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
        }
    }
}

#[test]
fn blank_erase_keeps_the_in_flight_drawing_cursor_and_never_fabricates_end_irq() {
    let mut b = board(0, false, false, 0);
    for (at, v) in [
        (0, 9),
        (20, 511),
        (22, 255),
        (32, 4),
        (36, 0x8C0),
        (38, 0x801F),
        (48, 4095),
        (52, 4095),
        (54, 4095),
        (58, 4095),
        (64, 0x8000),
    ] {
        b.write16(crate::vdp1::VRAM + at, v).unwrap();
    }
    reg(&mut b, 4, 1);
    advance(&mut b, 224 * 1708, 17);
    reg(&mut b, 0, 8);
    reg(&mut b, 2, 3);
    advance(&mut b, 39 * 1708, 3);
    assert!(b.vdp1.busy());
    assert_eq!(b.read16(REGISTERS + 0x10), Ok(0));
    assert_eq!(b.read16(REGISTERS + 0x12), Ok(4));
    assert_eq!(b.read16(REGISTERS + 0x14), Ok(4));
    assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
    assert_eq!(b.read16(FRAMEBUFFER), Ok(0x1234)); // old display erase result
    advance(&mut b, 100, 17);
    assert_eq!(b.read16(FRAMEBUFFER), Ok(0x1234)); // drawing never restarts at origin
}
