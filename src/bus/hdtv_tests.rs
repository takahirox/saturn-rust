//! ST-013 §§4.1/4.4 literal mode and aggregate blank-erase vectors.
use super::*;
use crate::vdp1::{FRAMEBUFFER, REGISTERS, VRAM as VDP1_RAM};

fn advance(b: &mut Bus, mut clocks: u32, chunk: u32, inspect: bool) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_main_clocks(n, 0).unwrap();
        clocks -= n;
        if inspect {
            let _ = b.read16(REGISTERS + 0x16).unwrap();
            let _ = b.vdp1.display_pixel(511, 255);
        }
    }
    if inspect {
        let _ = b.vdp2.framebuffer().unwrap();
    }
}
fn reg(b: &mut Bus, off: u32, value: u16) {
    b.write16(REGISTERS + off, value).unwrap();
}
#[test]
fn hdtv_scheduled_words_on_both_sh2s_preserve_mode_readback_geometry_and_widths() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut b = Bus::default();
                b.set_slave_enabled(true);
                // Second HBlank after IN: start of the permitted TVM window.
                advance(&mut b, 225 * 1708 + 1280, 10000, false);
                assert_eq!(
                    b.replay_cpu(p).write16(REGISTERS | 0x20000000, 4),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
                advance(&mut b, 7, chunk, inspect);
                assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1000));
                advance(&mut b, 1, chunk, inspect);
                assert_eq!(b.replay_cpu(p).write16(REGISTERS | 0x20000000, 4), Ok(()));
                b.finish_cpu_transaction(p);
                assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1004));
                for (at, v) in [
                    (0, 0x801F),
                    (1022, 0x83E0),
                    (1024, 0xFC00),
                    (262142, 0xABCD),
                ] {
                    b.write16(FRAMEBUFFER + at, v).unwrap();
                }
                reg(&mut b, 2, 3);
                advance(&mut b, 263 * 1708 - (225 * 1708 + 1288), chunk, inspect);
                for (x, y, v) in [
                    (0, 0, 0x801F),
                    (511, 0, 0x83E0),
                    (0, 1, 0xFC00),
                    (511, 255, 0xABCD),
                    (512, 0, 0),
                    (0, 256, 0),
                ] {
                    assert_eq!(b.vdp1.display_pixel(x, y), v);
                }
                for bad in [5, 6, 7, 13, 14, 15, 0x14] {
                    assert_eq!(
                        b.replay_cpu(p).write16(REGISTERS, bad),
                        Err(BusError::Pending)
                    );
                    advance(&mut b, 8, chunk, inspect);
                    assert_eq!(b.replay_cpu(p).write16(REGISTERS, bad), Ok(()));
                    b.finish_cpu_transaction(p);
                    assert!(matches!(
                        b.take_cpu_write_error(p),
                        Some(crate::cpu::CpuError::Bus {
                            source: BusError::UnsupportedDevice { width: 2, .. },
                            ..
                        })
                    ));
                }
                assert!(b.write8(REGISTERS, 4).is_err());
                // Preserve the checked host API's atomic pair validation;
                // hardware guests use word stores (ST-210 item3).
                b.write32(REGISTERS, 0x00040003).unwrap();
                assert!(b.write32(REGISTERS, 0x00000004).is_err());
                assert!(b.write16(REGISTERS + 1, 4).is_err());
                assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1014));
            }
        }
    }
}

#[test]
fn hdtv_automatic_blank_erase_has_literal_capacity_no_display_erase_and_cancels_suffix() {
    // Independent table4.4 arithmetic and table4.5 values. Both VDP2
    // widths for each monitor share one physical raster and erase budget.
    for (mode, line, total, capacity) in [
        (4, 852, 525, 29340usize),
        (5, 848, 562, 53136),
        (6, 852, 525, 29340),
        (7, 848, 562, 53136),
    ] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut b = Bus::default();
                b.raster.configure(mode);
                reg(&mut b, 0, 4);
                for (o, v) in [(6, 0x1234), (8, 0), (10, 64 << 9 | 255), (2, 0)] {
                    reg(&mut b, o, v);
                }
                b.vdp1.draw_buffer_mut().fill(0xEE);
                b.vdp1.vblank_out(); // automatic change captures A
                b.vdp1.draw_buffer_mut().fill(0xDD);
                // B is pending, not captured; manual change is deferred.
                reg(&mut b, 6, 0x5678);
                advance(&mut b, 480 * line, chunk, inspect);
                for word in [0, capacity - 1, capacity, 131071] {
                    assert_eq!(b.vdp1.display_pixel(word % 512, word / 512), 0xEEEE);
                }
                reg(&mut b, 2, 3);
                let blank = (total - 480) * line;
                advance(&mut b, blank - 1, chunk, inspect);
                assert_eq!(
                    b.vdp1
                        .display_pixel((capacity - 1) % 512, (capacity - 1) / 512),
                    0xEEEE
                );
                assert_eq!(
                    b.vdp1
                        .display_pixel((capacity - 2) % 512, (capacity - 2) / 512),
                    0x1234
                );
                advance(&mut b, 1, 1, inspect);
                for word in [0, capacity - 1, capacity, 131071] {
                    assert_eq!(
                        b.read16(FRAMEBUFFER + word as u32 * 2),
                        Ok(if word < capacity { 0x1234 } else { 0xEEEE })
                    );
                }
                advance(&mut b, 10000, chunk, inspect);
                assert_eq!(b.read16(FRAMEBUFFER + capacity as u32 * 2), Ok(0xEEEE));
                assert_eq!(b.vdp1.display_pixel(511, 255), 0xDDDD);
                assert_eq!(b.read16(REGISTERS + 0x10), Ok(0));
                assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
                // New blank uses captured B, on the other physical buffer.
                advance(&mut b, 480 * line - 10000 + blank, chunk, inspect);
                assert_eq!(
                    b.vdp1
                        .display_pixel((capacity - 1) % 512, (capacity - 1) / 512),
                    0x5678
                );
                assert_eq!(b.vdp1.display_pixel(capacity % 512, capacity / 512), 0xDDDD);
            }
        }
    }
}

#[test]
fn hdtv_ptmr_changes_end_irq_transitions_and_reset_cancel_retained_work() {
    const LINE: u32 = 852;
    const FIELD: u32 = 525 * LINE;
    const WINDOW: u32 = 481 * LINE + 640;
    for automatic in [false, true] {
        let mut b = Bus::default();
        b.write16(VDP2, 0x8004).unwrap();
        // Legal second-HBlank window for both transitions; FBCR.DIE stays0.
        advance(&mut b, WINDOW, 10000, false);
        reg(&mut b, 0, 4);
        for (at, v) in [
            (0, 9),
            (20, 511),
            (22, 255),
            (32, 6),
            (36, 0xC0),
            (38, 0x801F),
            (48, 7),
            (64, 0x8000),
        ] {
            b.write16(VDP1_RAM + at, v).unwrap();
        }
        reg(&mut b, 2, if automatic { 0 } else { 3 });
        reg(&mut b, 4, if automatic { 2 } else { 1 });
        advance(&mut b, FIELD - WINDOW + 200, 17, false);
        assert!(!b.vdp1.busy());
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0x2000);
        assert_eq!(b.read16(REGISTERS + 0x14), Ok(8));
        assert_eq!(
            b.read16(REGISTERS + 0x10),
            Ok(if automatic { 2 } else { 1 })
        );
        if automatic {
            assert_eq!(b.read16(FRAMEBUFFER), Ok(0x801F));
        } else {
            assert_eq!(b.vdp1.display_pixel(0, 0), 0x801F);
        }
        advance(&mut b, WINDOW - 200, 10000, false);
        reg(&mut b, 0, 0);
        assert_eq!(b.read16(REGISTERS + 0x16).unwrap() & 15, 0);
        reg(&mut b, 0, 4);
        reg(&mut b, 6, 0x5678);
        reg(&mut b, 10, 64 << 9 | 255);
        reg(&mut b, 2, 3);
        advance(&mut b, FIELD - WINDOW, 10000, false);
        advance(&mut b, 480 * LINE + 640 + 20, 10000, false);
        b.system_reset();
        b.vdp1.draw_buffer_mut().fill(0xAA);
        advance(&mut b, 10000, 17, true);
        assert!(b.vdp1.draw_buffer().iter().all(|&v| v == 0xAA));
        assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1000));
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
    }
}

#[test]
fn hdtv_reset_cancels_pending_sample_or_partial_erase_and_scheduled_mode_word() {
    for chunk in [1, 3, 17, 10000] {
        for offset in [639, 642] {
            let mut b = Bus::default();
            b.raster.configure(4);
            reg(&mut b, 0, 4);
            for (o, v) in [(6, 0x1234), (8, 0), (10, 64 << 9 | 255), (2, 3)] {
                reg(&mut b, o, v);
            }
            b.vdp1.draw_buffer_mut().fill(0xEE);
            b.vdp1.vblank_out();
            advance(&mut b, 480 * 852 + offset, chunk, true);
            assert_eq!(
                b.vdp1.display_pixel(0, 0),
                if offset == 639 { 0xEEEE } else { 0x1234 }
            );
            b.system_reset();
            b.vdp1.draw_buffer_mut().fill(0xAA);
            advance(&mut b, 10000, chunk, true);
            assert!(b.vdp1.draw_buffer().iter().all(|&v| v == 0xAA));
            assert_eq!(b.vdp1.display_pixel(0, 0), 0);
            assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1000));
        }
        for p in [Processor::Master, Processor::Slave] {
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            advance(&mut b, 225 * 1708 + 1280, chunk, false);
            assert_eq!(
                b.replay_cpu(p).write16(REGISTERS, 4),
                Err(BusError::Pending)
            );
            advance(&mut b, 7, chunk, false);
            b.system_reset();
            advance(&mut b, 100, chunk, true);
            assert_eq!(b.read16(REGISTERS + 0x16), Ok(0x1000));
            assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
        }
    }
}
