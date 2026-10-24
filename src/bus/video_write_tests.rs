//! Literal ST-097 §3.2 memory phases; see docs/sh2-video-writes.md.
use super::*;

const ENDPOINTS: &[u32] = &[
    0x05C01000, 0x05C81000, 0x05CC1000, 0x05E01000, 0x05E81000, 0x05F00100, 0x05F00900,
];
fn board() -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    for p in [Processor::Master, Processor::Slave] {
        b.onchip[p as usize]
            .write32(0xFFFFFFE8, 0xA55A0030)
            .unwrap();
        b.onchip[p as usize]
            .write32(0xFFFFFFEC, 0xA55A0000)
            .unwrap();
    }
    b
}
fn advance(b: &mut Bus, mut clocks: u32, chunk: u32, preview: bool) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_clocks(n).unwrap();
        clocks -= n;
        if preview {
            let _ = b.read32(0x25E01000).unwrap();
            let _ = b.read16(VDP2 + 4).unwrap();
        }
    }
}
fn store(b: &mut Bus, p: Processor, at: u32, width: usize, v: u32) -> Result<(), BusError> {
    let mut m = b.replay_cpu(p);
    match width {
        1 => m.write8(at, v as u8),
        2 => m.write16(at, v as u16),
        _ => m.write32(at, v),
    }
}
fn dma(b: &mut Bus, p: Processor, channel: u32, at: u32, size: u32) {
    let chip = &mut b.onchip[p as usize];
    for (off, v) in [
        (0, HIGH_WRAM + 0x100),
        (4, at),
        (8, if size == 3 { 4 } else { 1 }),
        (12, 0x5205 | size << 10),
    ] {
        chip.write32(0xFFFFFF80 + channel * 16 + off, v).unwrap();
    }
    chip.write32(0xFFFFFFA0 + channel * 8, 0x44).unwrap();
    chip.write16(crate::onchip::IPRA, 0x600).unwrap();
    chip.write32(0xFFFFFFB0, 1).unwrap();
}

#[test]
fn video_longword_publishes_retained_words_at_eight_and_sixteen_without_replay() {
    for &endpoint in ENDPOINTS {
        for p in [Processor::Master, Processor::Slave] {
            for alias in [0, 0x20000000] {
                for chunk in [1, 3, 17, 10000] {
                    for preview in [false, true] {
                        let mut b = board();
                        let at = endpoint | alias;
                        b.write32(at, 0xAABBCCDD).unwrap();
                        assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                        b.grant_bus_transaction();
                        advance(&mut b, 1, chunk, preview);
                        assert_eq!(store(&mut b, p, at, 4, 0x11223344), Ok(()));
                        b.finish_cpu_transaction(p);
                        assert_eq!(b.read32(at), Ok(0xAABBCCDD));
                        // Retire the CPU and change a source: buffered data belongs to BSC.
                        b.write32(HIGH_WRAM + 0x100, 0xDEADBEEF).unwrap();
                        advance(&mut b, 6, chunk, preview);
                        assert_eq!(b.read32(at), Ok(0xAABBCCDD));
                        advance(&mut b, 1, chunk, preview);
                        assert_eq!(b.read32(at), Ok(0x1122CCDD));
                        b.write16(at, 0x7788).unwrap();
                        advance(&mut b, 7, chunk, preview);
                        assert_eq!(b.read32(at), Ok(0x7788CCDD));
                        advance(&mut b, 1, chunk, preview);
                        assert_eq!(b.read32(at), Ok(0x77883344));
                        assert!(b.transactions.writes[p as usize].is_none());
                        advance(&mut b, 100, chunk, preview);
                        assert_eq!(b.read32(at), Ok(0x77883344));
                    }
                }
            }
        }
    }
}

#[test]
fn video_byte_word_endian_bsc_and_alignment_preserve_requested_strobes() {
    for &at in ENDPOINTS {
        for p in [Processor::Master, Processor::Slave] {
            for (lane, width, value, expected) in [
                (0, 1, 0x55, 0x55BBCCDD),
                (1, 1, 0x55, 0xAA55CCDD),
                (2, 1, 0x55, 0xAABB55DD),
                (3, 1, 0x55, 0xAABBCC55),
                (0, 2, 0x5566, 0x5566CCDD),
                (2, 2, 0x5566, 0xAABB5566),
            ] {
                let mut b = board();
                b.write32(at, 0xAABBCCDD).unwrap();
                assert_eq!(
                    store(&mut b, p, at + lane, width, value),
                    Err(BusError::Pending)
                );
                advance(&mut b, 7, 3, true);
                assert_eq!(b.read32(at), Ok(0xAABBCCDD));
                advance(&mut b, 1, 1, true);
                assert_eq!(b.read32(at), Ok(expected));
            }
            for (bcr2, little, first, final_value, duration) in [
                (0xFC, false, 0x11220000, 0x11223344, 16),
                (0xFC, true, 0x44330000, 0x44332211, 16),
                (0xEC, false, 0x11220000, 0x11223344, 16),
                (0xDC, false, 0x11000000, 0x11223344, 32),
            ] {
                let mut b = board();
                b.onchip[p as usize]
                    .write32(0xFFFFFFE4, 0xA55A0000 | bcr2)
                    .unwrap();
                if little {
                    b.onchip[p as usize]
                        .write32(0xFFFFFFE0, 0xA55A13F0)
                        .unwrap();
                }
                assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                advance(&mut b, 8, 3, true);
                assert_eq!(b.read32(at), Ok(first));
                advance(&mut b, duration - 8, 3, true);
                assert_eq!(b.read32(at), Ok(final_value));
            }
            for (lane, width) in [(1, 2), (2, 4)] {
                let mut b = board();
                assert_eq!(
                    store(&mut b, p, at + lane, width, 0),
                    Err(BusError::Unaligned {
                        address: at + lane,
                        width
                    })
                );
                assert!(
                    b.transactions.journals[p as usize]
                        .borrow()
                        .pending_write
                        .is_none()
                );
            }
        }
    }
    // Memory conversion must not split or widen the inherited MMIO request.
    for width in [1, 4] {
        let mut b = board();
        assert_eq!(
            store(&mut b, Processor::Master, 0x25D00000, width, 0),
            Err(BusError::Pending)
        );
        b.grant_bus_transaction();
        assert_eq!(
            b.transactions.active.as_ref().unwrap().transfer.width,
            width
        );
    }
}

#[test]
fn video_both_dmac_channels_retain_longwords_and_sixteen_byte_units_with_early_irq() {
    for &at in ENDPOINTS {
        for p in [Processor::Master, Processor::Slave] {
            for channel in [0, 1] {
                for size in [0, 1, 2, 3] {
                    for chunk in [1, 3, 17, 10000] {
                        let mut b = board();
                        for (i, v) in [0x11223344, 0x55667788, 0x99AABBCC, 0xDDEEFF00]
                            .into_iter()
                            .enumerate()
                        {
                            b.write32(HIGH_WRAM + 0x100 + i as u32 * 4, v).unwrap();
                        }
                        dma(&mut b, p, channel, at, size);
                        if size < 3 {
                            advance(&mut b, 3, chunk, true);
                            assert_eq!(b.onchip[p as usize].interrupt(), Some((6, 0x44)));
                            b.write32(HIGH_WRAM + 0x100, 0xDEADBEEF).unwrap();
                            advance(&mut b, 7, chunk, true);
                            assert_eq!(b.read32(at), Ok(0));
                            advance(&mut b, 1, chunk, true);
                            assert_eq!(
                                b.read32(at),
                                Ok(match size {
                                    0 => 0x11000000,
                                    1 => 0x11220000,
                                    _ => 0x11220000,
                                })
                            );
                            advance(&mut b, 8, chunk, true);
                            assert_eq!(
                                b.read32(at),
                                Ok(match size {
                                    0 => 0x11000000,
                                    1 => 0x11220000,
                                    _ => 0x11223344,
                                })
                            );
                        } else {
                            // Four source reads consume eight clocks, acceptance one;
                            // subsequent single-entry stores accept only after prior16.
                            for (deadline, offset, value) in [
                                (17, 0, 0x1122),
                                (25, 2, 0x3344),
                                (33, 4, 0x5566),
                                (41, 6, 0x7788),
                                (49, 8, 0x99AA),
                                (57, 10, 0xBBCC),
                                (65, 12, 0xDDEE),
                                (73, 14, 0xFF00),
                            ] {
                                let left = deadline - b.main_clocks as u32;
                                advance(&mut b, left - 1, chunk, true);
                                assert_eq!(b.read16(at + offset), Ok(0));
                                advance(&mut b, 1, chunk, true);
                                assert_eq!(b.read16(at + offset), Ok(value));
                                if deadline == 17 {
                                    b.write32(HIGH_WRAM + 0x104, 0xDEADBEEF).unwrap();
                                }
                                if deadline == 57 {
                                    assert_eq!(b.onchip[p as usize].interrupt(), None);
                                    advance(&mut b, 1, chunk, true); // final acceptance at58
                                    assert_eq!(b.onchip[p as usize].interrupt(), Some((6, 0x44)));
                                }
                            }
                        }
                        advance(&mut b, 100, chunk, true);
                        assert_eq!(b.take_dma_errors(), [false; 2]);
                        assert_eq!(
                            b.onchip[p as usize].read32(0xFFFFFF88 + channel * 16),
                            Ok(0)
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn video_second_word_reacquires_vdp2_grants_and_preserves_backpressure_and_read_drain() {
    for p in [Processor::Master, Processor::Slave] {
        for at in [0x25E01000, 0x25E81000] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                b.write16(VDP2, 0x8000).unwrap();
                b.write16(VDP2 + 0x20, 1).unwrap();
                b.write32(VDP2 + 0x10, 0xFFFFFFFF).unwrap();
                assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                advance(&mut b, 1, chunk, true);
                assert_eq!(store(&mut b, p, at, 4, 0x11223344), Ok(()));
                b.finish_cpu_transaction(p);
                // Independent on-chip work finishes while external data is pending.
                assert_eq!(b.replay_cpu(p).read8(0xFFFFFE01), Err(BusError::Pending));
                advance(&mut b, 2, chunk, true);
                assert_eq!(b.replay_cpu(p).read8(0xFFFFFE01), Ok(255));
                b.finish_cpu_transaction(p);
                advance(&mut b, 5, chunk, true);
                assert_eq!(b.read32(at), Ok(0x11220000));
                b.write32(VDP2 + 0x10, 0).unwrap();
                assert_eq!(
                    store(&mut b, p, at + 4, 4, 0x55667788),
                    Err(BusError::Pending)
                );
                advance(&mut b, 1273, chunk, true);
                assert_eq!(b.read32(at), Ok(0x11220000));
                assert_eq!(b.read32(at + 4), Ok(0));
                assert!(!b.transactions.journals[p as usize].borrow().write_accepted);
                advance(&mut b, 1, chunk, true);
                assert_eq!(b.read32(at), Ok(0x11223344));
                advance(&mut b, 1, chunk, true);
                assert_eq!(store(&mut b, p, at + 4, 4, 0x55667788), Ok(()));
                b.finish_cpu_transaction(p);
                assert_eq!(b.replay_cpu(p).read32(at + 4), Err(BusError::Pending));
                advance(&mut b, 15, chunk, true);
                assert_eq!(b.read32(at + 4), Ok(0x55667788));
                assert_eq!(b.replay_cpu(p).read32(at + 4), Err(BusError::Pending));
                advance(&mut b, 8, chunk, true);
                assert_eq!(b.replay_cpu(p).read32(at + 4), Err(BusError::Pending));
                advance(&mut b, 8, chunk, true);
                assert_eq!(b.replay_cpu(p).read32(at + 4), Ok(0x55667788));
            }
        }
    }
}

#[test]
fn video_longword_second_publication_before_equal_after_display_acquisition() {
    for p in [Processor::Master, Processor::Slave] {
        for (start, expected) in [
            (14, [0x2222, 0x3333]),
            (16, [0x2222, 0x1111]),
            (17, [0x2222, 0x1111]),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                for (off, v) in [
                    (0, 0x8000),
                    (0x10, 0x4EEE),
                    (0x12, 0xEEEE),
                    (0x20, 0x101),
                    (0x28, 2),
                    (0x78, 1),
                    (0xF8, 1),
                ] {
                    b.write16(VDP2 + off, v).unwrap();
                }
                b.write32(VRAM + 4, 0x11111111).unwrap();
                advance(&mut b, start, chunk, true);
                assert_eq!(
                    store(&mut b, p, 0x25E00004, 4, 0x22223333),
                    Err(BusError::Pending)
                );
                advance(&mut b, 8, chunk, true);
                assert_eq!(b.read32(VRAM + 4), Ok(0x22221111));
                advance(&mut b, 40 - start - 8, chunk, true);
                let trace = b.vdp2.display_fetch.borrow();
                let packet = trace
                    .trace
                    .iter()
                    .find(|r| r.path == 1 && r.layer == 0 && r.address == 4)
                    .unwrap();
                assert_eq!(packet.words, expected, "{p:?} start{start} chunk{chunk}");
                assert_eq!(b.read32(VRAM + 4), Ok(0x22223333));
            }
        }
    }
}

#[test]
fn video_write_both_phases_own_vdp1_memory_while_independent_drawing_progresses() {
    for p in [Processor::Master, Processor::Slave] {
        for framebuffer in [false, true] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                for (off, v) in [
                    (0, 9),
                    (20, 511),
                    (22, 255),
                    (32, 6),
                    (36, 0xC0),
                    (38, 0x801F),
                    (48, 31),
                    (64, 0x8000),
                ] {
                    b.write16(vdp1::VRAM + off, v).unwrap();
                }
                b.write16(vdp1::REGISTERS + 2, 2).unwrap();
                b.write16(vdp1::REGISTERS + 4, 1).unwrap();
                advance(&mut b, 68, chunk, true);
                let at = (if framebuffer {
                    vdp1::FRAMEBUFFER
                } else {
                    vdp1::VRAM
                }) | 0x20000100;
                assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                advance(&mut b, 8, chunk, true);
                assert_eq!(b.read32(at), Ok(0x11220000));
                advance(&mut b, 7, chunk, true);
                assert_eq!(
                    b.read16(vdp1::FRAMEBUFFER + 8),
                    Ok(if framebuffer { 0 } else { 0x801F })
                );
                advance(&mut b, 1, chunk, true);
                assert_eq!(b.read32(at), Ok(0x11223344));
                advance(&mut b, 1, chunk, true);
                assert_eq!(b.read16(vdp1::FRAMEBUFFER + 8), Ok(0x801F));
            }
        }
    }
}

#[test]
fn video_resets_cancel_unaccepted_work_or_drain_only_the_accepted_longword() {
    use crate::onchip::{ResetRequest, WDT};
    for &at in ENDPOINTS {
        for p in [Processor::Master, Processor::Slave] {
            for dmac in [false, true] {
                for manual in [false, true] {
                    for assertion in [2u32, 8, 10, 16] {
                        for chunk in [1, 3, 17, 10000] {
                            let mut b = board();
                            let peer = 1 - p as usize;
                            b.onchip[peer].write8(0xFFFFFE00, 0x67).unwrap();
                            if dmac {
                                b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                                b.write32(HIGH_WRAM + 0x104, 0x55667788).unwrap();
                                dma(&mut b, p, 0, at, 2);
                                // Acceptance at3; reset stages are relative to write start3.
                                advance(&mut b, 3, chunk, true);
                            } else {
                                assert_eq!(
                                    store(&mut b, p, at, 4, 0x11223344),
                                    Err(BusError::Pending)
                                );
                            }
                            let chip = &mut b.onchip[p as usize];
                            chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                                .unwrap();
                            chip.write16(WDT, 0xA578).unwrap();
                            chip.write16(
                                WDT,
                                0x5A00 | u16::from(0u8.wrapping_sub((assertion / 2) as u8)),
                            )
                            .unwrap();
                            if !dmac {
                                advance(&mut b, 1, chunk, true);
                                assert_eq!(store(&mut b, p, at, 4, 0x11223344), Ok(()));
                                b.finish_cpu_transaction(p);
                                assert_eq!(
                                    store(&mut b, p, at + 4, 4, 0xDEADBEEF),
                                    Err(BusError::Pending)
                                );
                            }
                            if assertion == 10 {
                                advance(&mut b, if dmac { 8 } else { 7 }, chunk, true);
                                assert_eq!(b.read16(at), Ok(0x1122));
                                // A completed word must not replay in the reset drain.
                                b.write16(at, 0x7788).unwrap();
                            }
                            advance(&mut b, 600, chunk, true);
                            let expected = if assertion == 10 {
                                if manual { 0x77883344 } else { 0x77880000 }
                            } else if manual || assertion == 16 {
                                0x11223344
                            } else if assertion >= 8 {
                                0x11220000
                            } else {
                                0
                            };
                            assert_eq!(
                                b.read32(at),
                                Ok(expected),
                                "{p:?} dma{dmac} manual{manual} t{assertion}"
                            );
                            assert_eq!(b.read32(at + 4), Ok(0));
                            assert_eq!(b.onchip[peer].read8(0xFFFFFE00), Ok(0x67));
                            assert_eq!(
                                b.take_cpu_reset(p),
                                Some(if manual {
                                    ResetRequest::Manual
                                } else {
                                    ResetRequest::PowerOn
                                })
                            );
                            assert!(b.transactions.writes[p as usize].is_none());
                            advance(&mut b, 100, chunk, true);
                            assert_eq!(b.read32(at), Ok(expected));
                        }
                    }
                }
            }
            for dmac in [false, true] {
                for stage in [0, 1, 8, 9, 11, 12] {
                    let mut b = board();
                    if dmac {
                        b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                        dma(&mut b, p, 0, at, 2);
                    } else {
                        assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                    }
                    advance(&mut b, stage, 3, true);
                    b.system_reset();
                    advance(&mut b, 100, 17, true);
                    assert_eq!(b.read32(at), Ok(0));
                    assert!(b.transactions.writes.iter().all(Option::is_none));
                }
            }
        }
    }
}

#[test]
fn video_external_ubc_observes_one_cpu_longword_after_its_second_word() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            let mut b = board();
            let peer = 1 - p as usize;
            let at = 0x25E01000;
            b.onchip[peer]
                .write32(0xFFFFFF40, 0x05E01000 + offset)
                .unwrap();
            b.onchip[peer].write16(0xFFFFFF48, 0xA8).unwrap();
            b.onchip[peer].write16(0xFFFFFF78, 0x3000).unwrap();
            b.onchip[peer].read16(0xFFFFFF78).unwrap();
            assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
            advance(&mut b, 8, 3, true);
            assert_eq!(b.onchip[peer].interrupt(), None);
            advance(&mut b, 8, 3, true);
            assert_eq!(
                b.onchip[peer].interrupt(),
                if offset == 0 { Some((15, 12)) } else { None }
            );
        }
    }
}

#[test]
fn video_display_trace_and_pixels_ignore_advancement_chunks_and_debugger_previews() {
    let mut expected = None;
    for chunk in [1, 3, 17, 10000] {
        for preview in [false, true] {
            let mut b = board();
            for (off, v) in [
                (0, 0x8000),
                (0x10, 0x4EEE),
                (0x12, 0xEEEE),
                (0x20, 0x101),
                (0x28, 2),
                (0x78, 1),
                (0xF8, 1),
            ] {
                b.write16(VDP2 + off, v).unwrap();
            }
            for i in 0..160 {
                b.write16(VRAM + i * 2, 0x1111).unwrap();
            }
            for (index, v) in [(1, 0x801F), (2, 0x83E0), (3, 0xFC00)] {
                b.write16(crate::vdp2::CRAM + index * 2, v).unwrap();
            }
            advance(&mut b, 16, chunk, preview);
            assert_eq!(
                store(&mut b, Processor::Master, 0x25E00004, 4, 0x22223333),
                Err(BusError::Pending)
            );
            advance(&mut b, 1400, chunk, preview);
            let actual = (
                b.vdp2.display_fetch.borrow().trace.clone(),
                b.vdp2.scanning.clone(),
                b.read32(VRAM + 4),
            );
            if let Some(expected) = &expected {
                assert_eq!(&actual, expected);
            } else {
                expected = Some(actual);
            }
        }
    }
}

#[test]
fn video_dmac_p1_byte_lanes_and_word_controls_leave_neighboring_strobes_unchanged() {
    for &endpoint in ENDPOINTS {
        for p in [Processor::Master, Processor::Slave] {
            for (lane, size, expected) in [
                (0, 0, 0x11BBCCDD),
                (1, 0, 0xAA11CCDD),
                (2, 0, 0xAABB11DD),
                (3, 0, 0xAABBCC11),
                (0, 1, 0x1122CCDD),
                (2, 1, 0xAABB1122),
            ] {
                let mut b = board();
                let at = endpoint | 0x20000000;
                b.write32(at, 0xAABBCCDD).unwrap();
                b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                dma(&mut b, p, 0, at + lane, size);
                advance(&mut b, 10, 3, true);
                assert_eq!(b.read32(at), Ok(0xAABBCCDD));
                advance(&mut b, 1, 1, true);
                assert_eq!(b.read32(at), Ok(expected));
            }
        }
    }
}

#[test]
fn video_reset_cancels_dmac_write_queued_behind_its_accepted_buffer() {
    use crate::onchip::WDT;
    for p in [Processor::Master, Processor::Slave] {
        for &at in ENDPOINTS {
            for manual in [false, true] {
                let mut b = board();
                b.onchip[p as usize]
                    .write32(0xFFFFFF00, 0x11223344)
                    .unwrap();
                for (off, v) in [(0, 0xFFFFFF00), (4, at), (8, 2), (12, 0x4A01), (0x30, 1)] {
                    b.onchip[p as usize].write32(0xFFFFFF80 + off, v).unwrap();
                }
                advance(&mut b, 3, 3, true); // read1, accept1, next internal read1
                assert!(b.transactions.writes[p as usize].is_some());
                b.grant_bus_transaction();
                assert!(
                    b.transactions.journals[p as usize + 2]
                        .borrow()
                        .pending_write
                        .is_some()
                );
                let chip = &mut b.onchip[p as usize];
                chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                    .unwrap();
                chip.write16(WDT, 0xA578).unwrap();
                chip.write16(WDT, 0x5AFF).unwrap();
                advance(&mut b, 600, 17, true);
                assert_eq!(b.read32(at), Ok(if manual { 0x11223344 } else { 0 }));
                assert_eq!(b.read32(at + 4), Ok(0));
            }
        }
    }
}

#[test]
fn video_local_ubc_observes_requested_size_once_at_cpu_or_dmac_acceptance() {
    for p in [Processor::Master, Processor::Slave] {
        for dmac in [false, true] {
            for word_match in [false, true] {
                let mut b = board();
                let at = 0x25E01000;
                let chip = &mut b.onchip[p as usize];
                chip.write32(0xFFFFFF40, at).unwrap();
                chip.write16(
                    0xFFFFFF48,
                    (if dmac { 0xA8 } else { 0x68 }) | if word_match { 2 } else { 3 },
                )
                .unwrap();
                chip.write16(0xFFFFFF78, 0x1000).unwrap();
                chip.read16(0xFFFFFF78).unwrap();
                if dmac {
                    b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                    dma(&mut b, p, 0, at, 2);
                    advance(&mut b, 3, 3, true);
                } else {
                    assert_eq!(store(&mut b, p, at, 4, 0x11223344), Err(BusError::Pending));
                    advance(&mut b, 1, 1, true);
                    assert_eq!(store(&mut b, p, at, 4, 0x11223344), Ok(()));
                }
                // DMA's ordinary completion IRQ is lower priority than UBC.
                let expected = if word_match {
                    if dmac { Some((6, 0x44)) } else { None }
                } else {
                    Some((15, 12))
                };
                assert_eq!(b.onchip[p as usize].interrupt(), expected);
                b.onchip[p as usize].acknowledge_break();
                advance(&mut b, 100, 17, true);
                assert_eq!(
                    b.onchip[p as usize].interrupt(),
                    if dmac { Some((6, 0x44)) } else { None }
                );
            }
        }
    }
}
