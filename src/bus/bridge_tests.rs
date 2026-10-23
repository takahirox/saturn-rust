//! Independent ST-210 item32 / ST-097 fig3.7 CPU memory-bridge vectors.
use super::*;

// Include every existing memory mirror, not adjacent register windows.
const ENDPOINTS: &[u32] = &[
    0x05A01000, 0x05C01000, 0x05C81000, 0x05CC1000, 0x05E01000, 0x05E81000, 0x05F00100, 0x05F00900,
];
fn advance(b: &mut Bus, mut clocks: u32, chunk: u32) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_clocks(n).unwrap();
        clocks -= n;
        // Debugger inspection cannot advance or satisfy a pending transaction.
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
    }
}
fn operand(b: &mut Bus, p: Processor, address: u32, width: usize) -> Result<u32, BusError> {
    let m = b.replay_cpu(p);
    match width {
        1 => m.read8(address).map(u32::from),
        2 => m.read16(address).map(u32::from),
        _ => m.read32(address),
    }
}
fn board() -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.write16(SCSP + 0x400, 0x20F).unwrap(); // 512KiB sound memory
    b
}

#[test]
fn bridge_all_endpoints_lanes_processors_and_aliases_sample_two_words() {
    // Reset CS2: T1 + T2 + six Tw = 8 clocks per physical word.
    // No production deadline/width/grant helper supplies this oracle.
    for &endpoint in ENDPOINTS {
        for alias in [0, 0x20000000] {
            for p in [Processor::Master, Processor::Slave] {
                for chunk in [1, 3, 17, 10000] {
                    for (lane, width, expected) in [
                        (0, 1, 0x11),
                        (1, 1, 0x22),
                        (2, 1, 0x77),
                        (3, 1, 0x88),
                        (0, 2, 0x1122),
                        (2, 2, 0x7788),
                        (0, 4, 0x11227788),
                    ] {
                        let mut b = board();
                        let at = endpoint | alias;
                        b.write32(at, 0xAABBCCDD).unwrap();
                        assert_eq!(operand(&mut b, p, at + lane, width), Err(BusError::Pending));
                        b.grant_bus_transaction();
                        advance(&mut b, 7, chunk);
                        assert!(
                            b.transactions.journals[p as usize]
                                .borrow()
                                .beats
                                .is_empty()
                        );
                        b.write32(at, 0x11223344).unwrap(); // before first completion
                        advance(&mut b, 1, chunk);
                        assert_eq!(
                            b.transactions.journals[p as usize].borrow().beats[0].1,
                            Ok(0x1122)
                        );
                        b.write32(at, 0x55667788).unwrap(); // at boundary, after sampling
                        assert_eq!(operand(&mut b, p, at + lane, width), Err(BusError::Pending));
                        b.grant_bus_transaction();
                        advance(&mut b, 7, chunk);
                        assert_eq!(operand(&mut b, p, at + lane, width), Err(BusError::Pending));
                        advance(&mut b, 1, chunk);
                        b.write32(at, 0x99AABBCC).unwrap(); // after second completion
                        assert_eq!(operand(&mut b, p, at + lane, width), Ok(expected));
                        assert_eq!(operand(&mut b, p, at + lane, width), Ok(expected));
                        let events: Vec<_> = b.transactions.journals[p as usize]
                            .borrow()
                            .operations
                            .clone();
                        assert_eq!(events.len(), 1); // replay did not repeat acquisition
                        assert!(b.transactions.active.is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn bridge_narrow_operand_occupies_both_phases_before_competing_cpu_and_scu() {
    for p in [Processor::Master, Processor::Slave] {
        let other = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        for &at in ENDPOINTS {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                assert_eq!(operand(&mut b, p, at + 3, 1), Err(BusError::Pending));
                b.grant_bus_transaction();
                assert_eq!(operand(&mut b, other, HIGH_WRAM, 4), Err(BusError::Pending));
                b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                // Queue SCU DMA while the CPU bridge owns its read unit.
                for (off, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, HIGH_WRAM + 0x200),
                    (8, 4),
                    (12, 0x102),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(SCU + off, value).unwrap();
                }
                advance(&mut b, 8, chunk);
                assert_eq!(operand(&mut b, p, at + 3, 1), Err(BusError::Pending));
                b.grant_bus_transaction();
                assert!(
                    b.transactions
                        .active
                        .as_ref()
                        .is_some_and(|a| a.owner == Owner::Cpu(p)
                            && a.transfer.address == (at & !3) + 2
                            && a.transfer.width == 2)
                );
                advance(&mut b, 8, chunk);
                assert_eq!(operand(&mut b, p, at + 3, 1), Ok(0));
                b.finish_cpu_transaction(p);
                for _ in 0..100 {
                    advance(&mut b, 1, 1);
                    if operand(&mut b, other, HIGH_WRAM, 4).is_ok() {
                        break;
                    }
                }
                assert_eq!(operand(&mut b, other, HIGH_WRAM, 4), Ok(0));
                assert!(b.transactions.scu_units.iter().all(Option::is_none));
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x11223344));
                assert_ne!(b.read32(SCU + 0xA4).unwrap() & (1 << 11), 0);
            }
        }
    }
}

#[test]
fn bridge_cache_refills_wrap_in_longwords_and_hits_retain_private_data() {
    for p in [Processor::Master, Processor::Slave] {
        for &at in ENDPOINTS {
            for (lane, width, expected) in [
                (0, 1, 0x11),
                (1, 1, 0x22),
                (2, 1, 0x33),
                (3, 1, 0x44),
                (0, 2, 0x1122),
                (2, 2, 0x3344),
                (0, 4, 0x11223344),
            ] {
                let mut b = board();
                b.for_cpu(p).write8(CCR, 1).unwrap();
                for (offset, value) in [
                    (0, 0x11223344),
                    (4, 0x55667788),
                    (8, 0x99AABBCC),
                    (12, 0xDDEEFF00),
                ] {
                    b.write32(at + offset, value).unwrap();
                }
                let mut sequence = Vec::new();
                for expected_at in [
                    at + 4,
                    at + 6,
                    at + 8,
                    at + 10,
                    at + 12,
                    at + 14,
                    at,
                    at + 2,
                ] {
                    assert_eq!(operand(&mut b, p, at + lane, width), Err(BusError::Pending));
                    b.grant_bus_transaction();
                    let t = b.transactions.active.as_ref().unwrap().transfer;
                    sequence.push((t.address, t.width));
                    assert_eq!(t.address, expected_at);
                    advance(&mut b, 8, 3);
                }
                assert_eq!(
                    sequence,
                    [
                        (at + 4, 2),
                        (at + 6, 2),
                        (at + 8, 2),
                        (at + 10, 2),
                        (at + 12, 2),
                        (at + 14, 2),
                        (at, 2),
                        (at + 2, 2)
                    ]
                );
                assert_eq!(operand(&mut b, p, at + lane, width), Ok(expected));
                b.finish_cpu_transaction(p);
                b.write32(at, 0xAABBCCDD).unwrap();
                assert_eq!(operand(&mut b, p, at + lane, width), Ok(expected));
                assert!(
                    b.transactions.journals[p as usize]
                        .borrow()
                        .beats
                        .is_empty()
                );
                b.finish_cpu_transaction(p);
                assert_eq!(
                    operand(&mut b, p, at | 0x20000000, 4),
                    Err(BusError::Pending)
                );
            }
        }
    }
}

#[test]
fn bridge_reset_cancels_queued_and_partial_results_on_both_processors() {
    use crate::onchip::{ResetRequest, WDT};
    for p in [Processor::Master, Processor::Slave] {
        for &at in ENDPOINTS {
            for stage in [0, 1, 8, 9] {
                for manual in [false, true] {
                    let mut b = board();
                    b.write32(at, 0x11223344).unwrap();
                    if stage == 0 {
                        // Another master owns the bus through watchdog assertion;
                        // this CPU's request really remains queued at reset.
                        let peer = if p == Processor::Master {
                            Processor::Slave
                        } else {
                            Processor::Master
                        };
                        assert_eq!(operand(&mut b, peer, HIGH_WRAM, 4), Err(BusError::Pending));
                        b.grant_bus_transaction();
                    }
                    assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
                    if stage > 0 {
                        b.grant_bus_transaction();
                        advance(&mut b, stage.min(8), 3);
                    }
                    if stage > 8 {
                        assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
                        b.grant_bus_transaction();
                        advance(&mut b, 1, 1);
                    }
                    let chip = &mut b.onchip[p as usize];
                    chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                        .unwrap();
                    chip.write16(WDT, 0xA578).unwrap();
                    chip.write16(WDT, 0x5AFF).unwrap();
                    advance(&mut b, 600, 17);
                    assert_eq!(
                        b.take_cpu_reset(p),
                        Some(if manual {
                            ResetRequest::Manual
                        } else {
                            ResetRequest::PowerOn
                        })
                    );
                    assert!(
                        b.transactions.journals[p as usize]
                            .borrow()
                            .beats
                            .is_empty()
                    );
                    assert!(b.transactions.active.is_none());
                    b.write32(at, 0x55667788).unwrap();
                    assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
                    // Reset preserves the documented BSC idle/turnaround state;
                    // bound progress without assuming a fresh power-on bus history.
                    let mut result = Err(BusError::Pending);
                    for _ in 0..64 {
                        advance(&mut b, 1, 1);
                        result = operand(&mut b, p, at, 4);
                        if result.is_ok() {
                            break;
                        }
                    }
                    assert_eq!(
                        result,
                        Ok(0x55667788),
                        "{p:?} {at:X} stage={stage} manual={manual}"
                    );
                }
            }
        }
    }
}

#[test]
fn bridge_vdp2_display_grants_delay_each_physical_phase_without_early_operand() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            // Six Tw age before the grant lookup; blanking at1280 then
            // T1/T2 gives1282. Revoke grants between words in the third case.
            for (grants, next_grants, high, low) in [
                (0xFFFFFFFF, 0xFFFFFFFF, 8, 16),
                (0, 0, 1282, 1290),
                (0xFFFFFFFF, 0, 8, 1282),
            ] {
                let mut b = board();
                b.write16(VDP2, 0x8000).unwrap();
                b.write16(VDP2 + 0x20, 1).unwrap();
                b.write32(VDP2 + 0x10, grants).unwrap();
                b.write32(VRAM, 0x11223344).unwrap();
                assert_eq!(
                    operand(&mut b, p, VRAM | 0x20000003, 1),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
                advance(&mut b, high - 1, chunk);
                assert!(
                    b.transactions.journals[p as usize]
                        .borrow()
                        .beats
                        .is_empty()
                );
                advance(&mut b, 1, chunk);
                b.write32(VRAM, 0x55667788).unwrap();
                b.write32(VDP2 + 0x10, next_grants).unwrap();
                assert_eq!(
                    operand(&mut b, p, VRAM | 0x20000003, 1),
                    Err(BusError::Pending)
                );
                advance(&mut b, low - high - 1, chunk);
                assert_eq!(
                    operand(&mut b, p, VRAM | 0x20000003, 1),
                    Err(BusError::Pending)
                );
                advance(&mut b, 1, chunk);
                assert_eq!(operand(&mut b, p, VRAM | 0x20000003, 1), Ok(0x88));
            }
        }
    }
}

#[test]
fn bridge_longword_retains_first_word_until_second_completion() {
    for p in [Processor::Master, Processor::Slave] {
        for &at in ENDPOINTS {
            let mut b = board();
            b.write32(at, 0x11223344).unwrap();
            assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
            advance(&mut b, 8, 3);
            b.write32(at, 0x55667788).unwrap();
            assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
            advance(&mut b, 8, 3);
            b.write32(at, 0x99AABBCC).unwrap();
            assert_eq!(operand(&mut b, p, at, 4), Ok(0x11227788));
        }
    }
}

#[test]
fn bridge_cpu_operand_and_external_longword_observations_exclude_downstream_words() {
    for p in [Processor::Master, Processor::Slave] {
        let peer = 1 - p as usize;
        for offset in [0, 2] {
            let mut b = board();
            let at = 0x25E01000;
            for (index, bar, bbr) in [
                (p as usize, at + 3, 0x65),
                (peer, VRAM + 0x1000 + offset, 0xA4),
            ] {
                b.onchip[index].write32(0xFFFFFF40, bar).unwrap();
                b.onchip[index].write16(0xFFFFFF48, bbr).unwrap();
                b.onchip[index].write16(0xFFFFFF78, 0x3000).unwrap();
                b.onchip[index].read16(0xFFFFFF78).unwrap();
            }
            assert_eq!(operand(&mut b, p, at + 3, 1), Err(BusError::Pending));
            advance(&mut b, 8, 3);
            assert_eq!(b.onchip[peer].interrupt(), None);
            assert_eq!(b.onchip[p as usize].interrupt(), None);
            assert_eq!(operand(&mut b, p, at + 3, 1), Err(BusError::Pending));
            advance(&mut b, 8, 3);
            assert_eq!(
                b.onchip[peer].interrupt(),
                if offset == 0 { Some((15, 12)) } else { None }
            );
            assert_eq!(operand(&mut b, p, at + 3, 1), Ok(0));
            assert_eq!(b.onchip[p as usize].interrupt(), Some((15, 12)));
        }
    }
}

#[test]
fn bridge_alignment_and_unpopulated_sound_geometry_remain_checked() {
    for p in [Processor::Master, Processor::Slave] {
        for &at in ENDPOINTS {
            let mut b = board();
            assert_eq!(
                operand(&mut b, p, at + 1, 2),
                Err(BusError::Unaligned {
                    address: at + 1,
                    width: 2
                })
            );
            b.finish_cpu_transaction(p);
            assert_eq!(
                operand(&mut b, p, at + 2, 4),
                Err(BusError::Unaligned {
                    address: at + 2,
                    width: 4
                })
            );
            assert!(
                b.transactions.journals[p as usize]
                    .borrow()
                    .waiting
                    .is_none()
            );
        }
        let mut b = board();
        // ST-077 MEM4MB layout: the upper half is unpopulated, not a mirror.
        b.write32(SOUND_RAM, 0x11223344).unwrap();
        let at = 0x25A80000;
        assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
        advance(&mut b, 8, 3);
        assert_eq!(operand(&mut b, p, at, 4), Err(BusError::Pending));
        advance(&mut b, 8, 3);
        assert_eq!(operand(&mut b, p, at, 4), Ok(0xFFFFFFFF));
    }
}
