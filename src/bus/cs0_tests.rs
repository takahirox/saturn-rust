//! VA0.5 sheet 1; SH7604 table3.9, figs7.3/7.44–46, §§7.11/9.3.
//! Independently authored word phases. No production timing/width helper is
//! used to calculate the expected addresses, data, or completion clocks.
use super::*;

fn board(p: Processor, burst: bool, waits: u32) -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    let chip = &mut b.onchip[p as usize];
    chip.write32(0xFFFFFFE0, if burst { 0xA55A0BF0 } else { 0xA55A03F0 })
        .unwrap();
    chip.write32(0xFFFFFFE8, 0xA55A0000 | waits).unwrap();
    let mut rom = vec![0; BIOS_SIZE];
    rom[0x100..0x110].copy_from_slice(&[
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF,
        0x00,
    ]);
    b.load_bios(&rom).unwrap();
    for (a, v) in [
        (0x00200100, 0x11223344),
        (0x00200104, 0x55667788),
        (0x00200108, 0x99AABBCC),
        (0x0020010C, 0xDDEEFF00),
    ] {
        b.write32(a, v).unwrap();
    }
    b
}
fn elapse(b: &mut Bus, mut n: u32, chunk: u32) {
    while n != 0 {
        let delta = n.min(chunk);
        b.advance_clocks(delta).unwrap();
        n -= delta;
    }
}
fn request(b: &mut Bus, p: Processor, at: u32, size: usize) -> Result<u32, BusError> {
    let m = b.replay_cpu(p);
    match size {
        1 => m.read8(at).map(u32::from),
        2 => m.read16(at).map(u32::from),
        4 => m.read32(at),
        _ => m.fetch16(at).map(u32::from),
    }
}
fn phase(b: &mut Bus, address: u32, width: usize, duration: u32, chunk: u32) {
    b.grant_bus_transaction();
    let a = b.transactions.active.as_ref().unwrap();
    assert_eq!(
        (a.transfer.address, a.transfer.width, a.remaining),
        (address, width, duration)
    );
    // Debugger reads cannot age, sample or publish the outstanding phase.
    for _ in 0..3 {
        b.read32(address & !3).unwrap();
    }
    assert_eq!(b.transactions.active.as_ref().unwrap().remaining, duration);
    elapse(b, duration, chunk);
}

#[test]
fn cs0_literal_byte_word_longword_and_fetch_vectors_on_both_cpus_and_aliases() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for base in [0x00000100, 0x20000100, 0x00200100, 0x20200100] {
                for (offset, size, value, beats) in [
                    (0, 1, 0x11, 1),
                    (1, 1, 0x22, 1),
                    (2, 1, 0x33, 1),
                    (3, 1, 0x44, 1),
                    (0, 2, 0x1122, 1),
                    (2, 2, 0x3344, 1),
                    (0, 4, 0x11223344, 2),
                    (0, 0, 0x1122, 2),
                    (2, 0, 0x3344, 2),
                ] {
                    let mut b = board(p, false, 3);
                    assert_eq!(
                        request(&mut b, p, base + offset, size),
                        Err(BusError::Pending)
                    );
                    let at = if size == 0 { base } else { base + offset };
                    phase(&mut b, at, if size == 1 { 1 } else { 2 }, 8, chunk);
                    if beats == 2 {
                        assert_eq!(
                            request(&mut b, p, base + offset, size),
                            Err(BusError::Pending)
                        );
                        phase(&mut b, base + 2, 2, 8, chunk);
                    }
                    assert_eq!(request(&mut b, p, base + offset, size), Ok(value));
                    b.finish_cpu_transaction(p);
                    if size == 0 && offset == 0 {
                        // Successor is retained from the same IF longword.
                        assert_eq!(request(&mut b, p, base + 2, 0), Ok(0x3344));
                        assert!(
                            b.transactions.journals[p as usize]
                                .borrow()
                                .waiting
                                .is_none()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cs0_longword_sampling_and_complete_cache_fill_restart_each_bstrom_pair() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            // Literal cumulative deadlines: ordinary/reset 8,16; BSTROM
            // one Tw 3,5; two Tw 4,7; zero Tw 2,4 (fig7.46 exception).
            for (burst, waits, first, second) in [
                (false, 3, 8, 8),
                (true, 1, 3, 2),
                (true, 2, 4, 3),
                (true, 0, 2, 2),
            ] {
                let mut b = board(p, burst, waits);
                assert_eq!(request(&mut b, p, 0x20200100, 4), Err(BusError::Pending));
                phase(&mut b, 0x20200100, 2, first, chunk);
                b.write32(0x00200100, 0xAABBCCDD).unwrap();
                assert_eq!(request(&mut b, p, 0x20200100, 4), Err(BusError::Pending));
                phase(&mut b, 0x20200102, 2, second, chunk);
                b.write32(0x00200100, 0xDEADBEEF).unwrap();
                assert_eq!(request(&mut b, p, 0x20200100, 4), Ok(0x1122CCDD));
                b.finish_cpu_transaction(p);
                // Adjacent independent word reads each have their own T1.
                for at in [0x20200104, 0x20200106] {
                    assert_eq!(request(&mut b, p, at, 2), Err(BusError::Pending));
                    phase(&mut b, at, 2, first, chunk);
                    assert!(request(&mut b, p, at, 2).is_ok());
                    b.finish_cpu_transaction(p);
                }
                let mut b = board(p, burst, waits);
                b.for_cpu(p).write8(CCR, 1).unwrap();
                // Critical-longword first: next longword, then wrap through
                // the remainder. Eight separate 16-bit sampling phases.
                let mut clock = 0;
                for (at, duration) in [
                    (0x00200108, first),
                    (0x0020010A, second),
                    (0x0020010C, first),
                    (0x0020010E, second),
                    (0x00200100, first),
                    (0x00200102, second),
                    (0x00200104, first),
                    (0x00200106, second),
                ] {
                    assert_eq!(request(&mut b, p, 0x00200104, 4), Err(BusError::Pending));
                    phase(&mut b, at, 2, duration, chunk);
                    clock += duration;
                }
                assert_eq!(
                    clock,
                    if !burst {
                        64
                    } else {
                        match waits {
                            0 => 16,
                            1 => 20,
                            _ => 28,
                        }
                    }
                );
                assert_eq!(request(&mut b, p, 0x00200104, 4), Ok(0x55667788));
                b.finish_cpu_transaction(p);
                b.write32(0x00200100, 0).unwrap();
                for (at, value) in [
                    (0x00200100, 0x11223344),
                    (0x00200108, 0x99AABBCC),
                    (0x0020010C, 0xDDEEFF00),
                ] {
                    assert_eq!(request(&mut b, p, at, 4), Ok(value));
                    b.finish_cpu_transaction(p);
                }
            }
        }
    }
}

#[test]
fn cs0_cancellation_and_peer_ownership_cannot_arm_a_later_burst() {
    for p in [Processor::Master, Processor::Slave] {
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        let mut b = board(p, true, 2);
        b.onchip[peer as usize]
            .write32(0xFFFFFFE0, 0xA55A0BF0)
            .unwrap();
        b.onchip[peer as usize]
            .write32(0xFFFFFFE8, 0xA55A0002)
            .unwrap();
        assert_eq!(request(&mut b, p, 0x20200100, 4), Err(BusError::Pending));
        phase(&mut b, 0x20200100, 2, 4, 1);
        b.cancel_cpu_transaction(p);
        assert_eq!(request(&mut b, peer, 0x20200102, 2), Err(BusError::Pending));
        phase(&mut b, 0x20200102, 2, 4, 1);
        assert_eq!(request(&mut b, peer, 0x20200102, 2), Ok(0x3344));
        b.finish_cpu_transaction(peer);
        assert_eq!(request(&mut b, p, 0x20200102, 2), Err(BusError::Pending));
        phase(&mut b, 0x20200102, 2, 4, 1);
        assert_eq!(request(&mut b, p, 0x20200102, 2), Ok(0x3344));
    }
}

#[test]
fn cs0_both_dmacs_retain_word_samples_restart_pairs_and_signal_at_write_acceptance() {
    for source in [0x00000100, 0x00200100] {
        for p in [Processor::Master, Processor::Slave] {
            for channel in [0, 0x10] {
                for chunk in [1, 3, 17, 10000] {
                    for (burst, waits, head, tail) in
                        [(false, 3, 8, 8), (true, 1, 3, 2), (true, 0, 2, 2)]
                    {
                        for (size, count) in [(2, 1), (3, 4)] {
                            let mut b = board(p, burst, waits);
                            let d = 0xFFFFFF80 + channel;
                            let chip = &mut b.onchip[p as usize];
                            chip.write16(0xFFFFFEE2, 0x600).unwrap();
                            for (a, v) in [
                                (d, source),
                                (d + 4, 0x00200200),
                                (d + 8, count),
                                (d + 12, 0x5205 | size << 10),
                                (0xFFFFFFA0 + channel / 2, 0x44),
                                (0xFFFFFFB0, 1),
                            ] {
                                chip.write32(a, v).unwrap();
                            }
                            // Reads are grouped across one or four longwords,
                            // but every even word starts a fresh BSTROM pair.
                            for i in 0..if size == 3 { 8 } else { 2 } {
                                let a = source + i * 2;
                                phase(&mut b, a, 2, if i % 2 == 0 { head } else { tail }, chunk);
                                if i == 0 {
                                    if source == 0x00200100 {
                                        b.write32(source, 0xAABBCCDD).unwrap();
                                    } else {
                                        let mut image = b.bios.clone().unwrap();
                                        image[0x100..0x104]
                                            .copy_from_slice(&0xAABBCCDDu32.to_be_bytes());
                                        b.load_bios(&image).unwrap();
                                    }
                                }
                                assert_eq!(b.onchip[p as usize].read32(d + 8), Ok(count));
                                assert_eq!(b.onchip[p as usize].interrupt(), None);
                            }
                            // Each buffered write is accepted in one internal
                            // clock; count/TE advance at the final acceptance,
                            // before the final two external word writes drain.
                            for i in 0..count {
                                elapse(&mut b, 1, chunk);
                                if i + 1 == count {
                                    assert_eq!(b.onchip[p as usize].read32(d + 8), Ok(0));
                                    assert_eq!(b.onchip[p as usize].interrupt(), Some((6, 0x44)));
                                    assert_eq!(b.read32(0x00200200 + i * 4), Ok(0));
                                } else {
                                    assert_eq!(b.onchip[p as usize].read32(d + 8), Ok(count));
                                    assert_eq!(b.onchip[p as usize].interrupt(), None);
                                }
                                // Read-to-write turnaround is one state with
                                // IW0=0, ordinary writes never shorten for BSTROM.
                                elapse(&mut b, head - 1 + if i == 0 { 1 } else { 0 }, chunk);
                                let expected =
                                    [0x1122CCDD, 0x55667788, 0x99AABBCC, 0xDDEEFF00][i as usize];
                                assert_eq!(
                                    b.read32(0x00200200 + i * 4),
                                    Ok(expected & 0xFFFF0000),
                                    "{p:?} ch{channel} chunk{chunk} burst{burst} waits{waits} size{size} i{i}"
                                );
                                elapse(&mut b, head, chunk);
                                assert_eq!(b.read32(0x00200200 + i * 4), Ok(expected));
                            }
                            assert_eq!(b.onchip[p as usize].read32(d), Ok(source + count * 4));
                            assert_eq!(
                                b.onchip[p as usize].read32(d + 4),
                                Ok(0x00200200 + count * 4)
                            );
                            assert_eq!(b.take_dma_errors(), [false; 2]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cs0_buffered_writes_hold_peer_and_tas_excludes_legal_low_ram_routes() {
    for p in [Processor::Master, Processor::Slave] {
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        for chunk in [1, 3, 17, 10000] {
            let mut b = board(p, true, 3);
            assert_eq!(
                b.replay_cpu(p).write32(0x20200200, 0x12345678),
                Err(BusError::Pending)
            );
            elapse(&mut b, 1, chunk);
            b.grant_bus_transaction();
            assert_eq!(request(&mut b, peer, 0x20200200, 4), Err(BusError::Pending));
            assert_eq!(b.replay_cpu(p).write32(0x20200200, 0x12345678), Ok(()));
            b.finish_cpu_transaction(p);
            assert_eq!(b.read32(0x00200200), Ok(0));
            elapse(&mut b, 7, chunk);
            assert_eq!(b.read32(0x00200200), Ok(0x12340000));
            assert!(!b.take_cpu_transaction_completion(peer));
            elapse(&mut b, 8, chunk);
            assert_eq!(b.read32(0x00200200), Ok(0x12345678));
            phase(&mut b, 0x20200200, 2, 8, chunk);
            assert_eq!(request(&mut b, peer, 0x20200200, 4), Err(BusError::Pending));
            phase(&mut b, 0x20200202, 2, 8, chunk);
            assert_eq!(request(&mut b, peer, 0x20200200, 4), Ok(0x12345678));
            b.finish_cpu_transaction(peer);

            assert_eq!(
                b.replay_cpu(p).tas_read8(0x20200200),
                Err(BusError::Pending)
            );
            phase(&mut b, 0x20200200, 1, 8, chunk);
            assert_eq!(b.replay_cpu(p).tas_read8(0x20200200), Ok(0x12));
            assert_eq!(request(&mut b, peer, 0x20200200, 2), Err(BusError::Pending));
            elapse(&mut b, 20, chunk);
            assert!(!b.take_cpu_transaction_completion(peer));
            // Replay the TAS read before its write, as the interpreter does.
            let mut memory = b.replay_cpu(p);
            assert_eq!(memory.tas_read8(0x20200200), Ok(0x12));
            assert_eq!(memory.write8(0x20200200, 0x92), Err(BusError::Pending));
            phase(&mut b, 0x20200200, 1, 8, chunk); // empty clocks satisfy IW0
            let mut memory = b.replay_cpu(p);
            memory.tas_read8(0x20200200).unwrap();
            assert_eq!(memory.write8(0x20200200, 0x92), Ok(()));
            b.finish_cpu_transaction(p);
            phase(&mut b, 0x20200200, 2, 8, chunk);
            assert_eq!(request(&mut b, peer, 0x20200200, 2), Ok(0x9234));
        }
    }
}

#[test]
fn cs0_refresh_waits_for_both_word_phases_before_peer_grant() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut b = board(p, false, 3);
            b.onchip[0].write32(0xFFFFFFE0, 0xA55A03F4).unwrap();
            b.onchip[0].write32(0xFFFFFFEC, 0xA55A0048).unwrap();
            b.onchip[0].write32(0xFFFFFFF8, 0xA55A0001).unwrap();
            b.onchip[0].write32(0xFFFFFFF0, 0xA55A0008).unwrap();
            assert_eq!(request(&mut b, p, 0x20200100, 4), Err(BusError::Pending));
            phase(&mut b, 0x20200100, 2, 8, chunk);
            b.onchip[0].read32(0xFFFFFFF0).unwrap();
            b.onchip[0].write32(0xFFFFFFF0, 0xA55A0000).unwrap();
            assert_eq!(request(&mut b, p, 0x20200100, 4), Err(BusError::Pending));
            phase(&mut b, 0x20200102, 2, 8, chunk);
            assert_eq!(request(&mut b, p, 0x20200100, 4), Ok(0x11223344));
            b.finish_cpu_transaction(p);
            b.grant_bus_transaction();
            let a = b.transactions.active.as_ref().unwrap();
            assert!(a.owner == Owner::Refresh);
            assert_eq!(a.remaining, 6);
            let peer = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            assert_eq!(request(&mut b, peer, 0x20200100, 2), Err(BusError::Pending));
            elapse(&mut b, 6, chunk);
            phase(&mut b, 0x20200100, 2, 8, chunk);
            assert_eq!(request(&mut b, peer, 0x20200100, 2), Ok(0x1122));
        }
    }
}

#[test]
fn cs0_watchdog_word_boundaries_drain_manual_cancel_power_and_preserve_the_strap() {
    use crate::onchip::{ResetRequest, WDT};
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for manual in [false, true] {
                for assertion in [2, 8, 10, 16] {
                    let mut b = board(p, true, 3);
                    b.onchip[p as usize]
                        .write32(0xFFFFFFE4, 0xA55A0054)
                        .unwrap();
                    assert_eq!(
                        b.replay_cpu(p).write32(0x20200200, 0x12345678),
                        Err(BusError::Pending)
                    );
                    let chip = &mut b.onchip[p as usize];
                    chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                        .unwrap();
                    chip.write16(WDT, 0xA578).unwrap();
                    chip.write16(WDT, 0x5A00 | (256 - assertion / 2)).unwrap();
                    elapse(&mut b, 600, chunk);
                    assert_eq!(
                        b.read32(0x00200200),
                        Ok(if manual || assertion == 16 {
                            0x12345678
                        } else if assertion >= 8 {
                            0x12340000
                        } else {
                            0
                        })
                    );
                    assert_eq!(
                        b.take_cpu_reset(p),
                        Some(if manual {
                            ResetRequest::Manual
                        } else {
                            ResetRequest::PowerOn
                        })
                    );
                    assert_eq!(b.onchip[p as usize].external_bus_width(0x00200000), 2);
                    assert_eq!(
                        b.onchip[p as usize].external_bus_width(0x06000000),
                        if manual { 1 } else { 4 }
                    );
                    b.write32(0x00200200, 0xAABBCCDD).unwrap();
                    elapse(&mut b, 100, chunk);
                    assert_eq!(b.read32(0x00200200), Ok(0xAABBCCDD));
                    assert!(b.transactions.active.is_none());
                }
            }
        }
    }
}

#[test]
fn cs0_reset_at_each_refill_and_dma_read_word_stops_at_the_documented_unit() {
    use crate::onchip::{ResetRequest, WDT};
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for dma in [false, true] {
                for manual in [false, true] {
                    for assertion in [
                        2u32, 8, 10, 16, 18, 24, 26, 32, 34, 40, 42, 48, 50, 56, 58, 64,
                    ] {
                        let mut b = board(p, false, 3);
                        if dma {
                            let chip = &mut b.onchip[p as usize];
                            for (a, v) in [
                                (0xFFFFFF80, 0x00200100),
                                (0xFFFFFF84, 0x00200200),
                                (0xFFFFFF88, 4),
                                (0xFFFFFF8C, 0x5E05),
                                (0xFFFFFFB0, 1),
                            ] {
                                chip.write32(a, v).unwrap();
                            }
                        } else {
                            b.for_cpu(p).write8(CCR, 1).unwrap();
                            assert_eq!(request(&mut b, p, 0x00200100, 4), Err(BusError::Pending));
                        }
                        let chip = &mut b.onchip[p as usize];
                        chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                            .unwrap();
                        chip.write16(WDT, 0xA578).unwrap();
                        chip.write16(WDT, 0x5A00 | (256 - assertion / 2) as u16)
                            .unwrap();
                        let mut trace = Vec::new();
                        let mut clock = 0;
                        while clock < 650 {
                            if !dma && clock < assertion {
                                assert_eq!(
                                    request(&mut b, p, 0x00200100, 4),
                                    Err(BusError::Pending)
                                );
                            }
                            b.grant_bus_transaction();
                            let mut delta =
                                chunk.min(650 - clock).min(b.bus_transaction_deadline());
                            if clock < assertion {
                                delta = delta.min(assertion - clock);
                            }
                            if let Some(a) = &b.transactions.active
                                && a.remaining <= delta
                            {
                                assert!(a.transfer.write.is_none());
                                trace.push((
                                    clock + a.remaining,
                                    a.transfer.address,
                                    a.transfer.width,
                                ));
                            }
                            b.advance_clocks(delta).unwrap();
                            clock += delta;
                        }
                        let addresses = if dma {
                            [
                                0x00200100, 0x00200102, 0x00200104, 0x00200106, 0x00200108,
                                0x0020010A, 0x0020010C, 0x0020010E,
                            ]
                        } else {
                            [
                                0x00200104, 0x00200106, 0x00200108, 0x0020010A, 0x0020010C,
                                0x0020010E, 0x00200100, 0x00200102,
                            ]
                        };
                        let words = if manual {
                            assertion.div_ceil(16) * 2
                        } else {
                            assertion / 8
                        };
                        let expected: Vec<_> = addresses[..words as usize]
                            .iter()
                            .enumerate()
                            .map(|(i, a)| ((i as u32 + 1) * 8, *a, 2))
                            .collect();
                        assert_eq!(
                            trace, expected,
                            "{p:?} dma{dma} manual{manual} t{assertion} chunk{chunk}"
                        );
                        assert_eq!(
                            b.take_cpu_reset(p),
                            Some(if manual {
                                ResetRequest::Manual
                            } else {
                                ResetRequest::PowerOn
                            })
                        );
                        assert_eq!(b.read32(0x00200200), Ok(0));
                        if !dma {
                            b.for_cpu(p).write8(CCR, 1).unwrap();
                            assert_eq!(request(&mut b, p, 0x00200100, 4), Err(BusError::Pending));
                            b.cancel_cpu_transaction(p);
                        }
                        b.write32(0x00200200, 0xAABBCCDD).unwrap();
                        elapse(&mut b, 100, chunk);
                        assert_eq!(b.read32(0x00200200), Ok(0xAABBCCDD));
                        assert!(b.transactions.active.is_none());
                        if p == Processor::Slave {
                            b.set_slave_enabled(false);
                            b.set_slave_enabled(true);
                            assert_eq!(b.onchip[1].external_bus_width(0x00200000), 2);
                        }
                        b.system_reset();
                        assert_eq!(b.onchip[p as usize].external_bus_width(0x00200000), 2);
                    }
                }
            }
        }
    }
}

#[test]
fn cs0_refill_samples_changed_future_words_without_publishing_a_partial_line() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut b = board(p, true, 1);
            b.for_cpu(p).write8(CCR, 1).unwrap();
            for (i, (address, clocks)) in [
                (0x00200108, 3),
                (0x0020010A, 2),
                (0x0020010C, 3),
                (0x0020010E, 2),
                (0x00200100, 3),
                (0x00200102, 2),
                (0x00200104, 3),
                (0x00200106, 2),
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(request(&mut b, p, 0x00200104, 4), Err(BusError::Pending));
                phase(&mut b, address, 2, clocks, chunk);
                if i == 0 {
                    b.write32(0x00200108, 0x01234567).unwrap();
                }
                if i == 2 {
                    b.write32(0x00200104, 0xAABBCCDD).unwrap();
                }
                if i == 6 {
                    b.write32(0x00200104, 0xDEADBEEF).unwrap();
                }
            }
            assert_eq!(request(&mut b, p, 0x00200104, 4), Ok(0xAABBBEEF));
            b.finish_cpu_transaction(p);
            assert_eq!(request(&mut b, p, 0x00200108, 4), Ok(0x99AA4567));
        }
    }
}

#[test]
fn cs0_instruction_refills_at_both_word_positions_finish_all_eight_phases() {
    for p in [Processor::Master, Processor::Slave] {
        for base in [0x00000100, 0x00200100] {
            for offset in [0, 2] {
                for burst in [false, true] {
                    for chunk in [1, 3, 17, 10000] {
                        let mut b = board(p, burst, 1);
                        b.for_cpu(p).write8(CCR, 1).unwrap();
                        for (i, address) in [
                            base + 4,
                            base + 6,
                            base + 8,
                            base + 10,
                            base + 12,
                            base + 14,
                            base,
                            base + 2,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            assert_eq!(
                                request(&mut b, p, base + offset, 0),
                                Err(BusError::Pending)
                            );
                            phase(
                                &mut b,
                                address,
                                2,
                                if burst && i % 2 == 1 { 2 } else { 3 },
                                chunk,
                            );
                        }
                        assert_eq!(
                            request(&mut b, p, base + offset, 0),
                            Ok(if offset == 0 { 0x1122 } else { 0x3344 })
                        );
                        b.finish_cpu_transaction(p);
                        if offset == 0 {
                            assert_eq!(request(&mut b, p, base + 2, 0), Ok(0x3344));
                            b.finish_cpu_transaction(p);
                        }
                        // All four longwords are installed, not only the
                        // executed pair; the next IF is an immediate hit.
                        assert_eq!(request(&mut b, p, base + 4, 0), Ok(0x5566));
                        assert!(b.transactions.active.is_none());
                        assert!(
                            b.transactions.journals[p as usize]
                                .borrow()
                                .waiting
                                .is_none()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cs0_rom_protection_mirrors_backup_odd_lanes_and_smpc_bytes_keep_their_contracts() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, true, 3);
        assert_eq!(
            b.replay_cpu(p).write32(0x20000100, 0xDEADBEEF),
            Err(BusError::Pending)
        );
        elapse(&mut b, 16, 3);
        assert_eq!(b.read32(0x00000100), Ok(0x11223344));
        assert_eq!(b.replay_cpu(p).write32(0x20000100, 0xDEADBEEF), Ok(()));
        b.finish_cpu_transaction(p);
        assert_eq!(request(&mut b, p, 0x20300100, 4), Err(BusError::Pending));
        phase(&mut b, 0x20300100, 2, 8, 3);
        assert_eq!(request(&mut b, p, 0x20300100, 4), Err(BusError::Pending));
        phase(&mut b, 0x20300102, 2, 7, 3);
        assert_eq!(request(&mut b, p, 0x20300100, 4), Ok(0x11223344));
        b.finish_cpu_transaction(p);
        b.write32(0x00180000, 0x11223344).unwrap();
        assert_eq!(request(&mut b, p, 0x20190000, 4), Err(BusError::Pending));
        phase(&mut b, 0x20190000, 2, 8, 3);
        assert_eq!(request(&mut b, p, 0x20190000, 4), Err(BusError::Pending));
        phase(&mut b, 0x20190002, 2, 7, 3);
        assert_eq!(request(&mut b, p, 0x20190000, 4), Ok(0xFF22FF44));
        b.finish_cpu_transaction(p);
        assert_eq!(request(&mut b, p, 0x20100063, 1), Err(BusError::Pending));
        phase(&mut b, 0x20100063, 1, 8, 3);
        assert_eq!(request(&mut b, p, 0x20100063, 1), Ok(0));
    }
}
