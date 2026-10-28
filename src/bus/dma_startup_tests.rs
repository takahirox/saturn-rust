//! ST-210 §3 item12: authored publication order, not copied-byte equivalence.
use super::*;
use crate::cd::{DATA, REGISTERS as CD};

fn command(b: &mut Bus, words: [u16; 4]) -> [u16; 4] {
    b.cd.write16(CD + 8, 0xfffe).unwrap();
    for (i, word) in words.into_iter().enumerate() {
        b.cd.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
    }
    b.cd.advance(1075);
    std::array::from_fn(|i| b.cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}
fn board() -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.cd.mount(crate::media::Media::iso(&vec![0; 2048 * 8]).unwrap());
    command(&mut b, [0x6400, 0, 0, 1]);
    for value in [0x1122, 0x3344, 0x5566, 0x7788, 0x99aa, 0xbbcc] {
        b.cd.write16(DATA, value).unwrap();
    }
    command(&mut b, [0x0600, 0, 0, 0]);
    command(&mut b, [0x6100, 0, 0, 1]);
    assert_eq!(b.debug_read32(DATA), Ok(0x11223344));
    b
}
fn channel(b: &mut Bus, level: u32, indirect: bool, hardware: bool, destination: u32) {
    for (n, v) in [8, destination, DATA | 0x80000000].into_iter().enumerate() {
        b.write32(HIGH_WRAM + 0x80 + n as u32 * 4, v).unwrap();
    }
    for (offset, value) in [
        (0, DATA),
        (
            4,
            if indirect {
                HIGH_WRAM + 0x80
            } else {
                destination
            },
        ),
        (8, 8),
        (12, 1),
        (
            20,
            if indirect { 1 << 24 } else { 0 } | if hardware { 5 } else { 7 },
        ),
        (16, if hardware { 0x100 } else { 0x101 }),
    ] {
        b.write32(SCU + level * 32 + offset, value).unwrap();
    }
    if hardware {
        b.scu.signal(6);
    }
}
fn pending(b: &mut Bus, p: Processor, destination: u32) {
    // An already granted peer RAM read genuinely occupies the bus. The new
    // write is accepted internally, but has received no external grant.
    let peer = if p == Processor::Master {
        Processor::Slave
    } else {
        Processor::Master
    };
    assert_eq!(b.replay_cpu(peer).read32(HIGH_WRAM), Err(BusError::Pending));
    b.grant_bus_transaction();
    assert!(matches!(
        b.transactions.active.as_ref().unwrap().owner,
        Owner::Cpu(_)
    ));
    assert_eq!(
        b.replay_cpu(p)
            .write32(destination | 0x20000000, 0xaabbccdd),
        Err(BusError::Pending)
    );
    b.advance_clocks(1).unwrap();
    assert_eq!(
        b.replay_cpu(p)
            .write32(destination | 0x20000000, 0xaabbccdd),
        Ok(())
    );
    b.finish_cpu_transaction(p);
    assert!(b.transactions.writes[p as usize].is_some());
    assert_eq!(b.read32(destination), Ok(0));
}
fn advance(b: &mut Bus, mut clocks: u32, chunk: u32, inspect: bool) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_clocks(n).unwrap();
        clocks -= n;
        if inspect {
            let before = b.debug_read32(DATA);
            for _ in 0..3 {
                assert_eq!(b.debug_read32(DATA), before);
            }
            b.debug_read32(SCU + 0xa4).unwrap();
        }
    }
}
#[test]
fn queued_cpu_ab_writes_publish_before_first_dma_source_read_all_routes_and_levels() {
    // Independent oracle: CPU high/low, DMA word0 high/low, DMA word1
    // high/low. No production route, priority or width selector constructs it.
    for p in [Processor::Master, Processor::Slave] {
        for level in 0..3 {
            for indirect in [false, true] {
                for hardware in [false, true] {
                    for destination in [0x05a01000, 0x05c01000, 0x05e01000] {
                        let mut b = board();
                        pending(&mut b, p, destination);
                        channel(&mut b, level, indirect, hardware, destination);
                        let mut publications = Vec::new();
                        let mut previous = [0u16; 4];
                        for _ in 0..500 {
                            advance(&mut b, 1, 1, true);
                            let current = std::array::from_fn(|i| {
                                b.read16(destination + i as u32 * 2).unwrap()
                            });
                            for i in 0..4 {
                                if current[i] != previous[i] {
                                    publications.push((i, current[i]));
                                }
                            }
                            previous = current;
                            if publications.len() < 2 {
                                assert_eq!(
                                    b.debug_read32(DATA),
                                    Ok(0x11223344),
                                    "DMA consumed FIFO ahead of queued {p:?} write, level={level}, indirect={indirect}"
                                );
                            }
                            if publications.len() < 6 {
                                assert_eq!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - level)), 0);
                            } else {
                                break;
                            }
                        }
                        assert_eq!(
                            publications,
                            [
                                (0, 0xaabb),
                                (1, 0xccdd),
                                (0, 0x1122),
                                (1, 0x3344),
                                (2, 0x5566),
                                (3, 0x7788)
                            ]
                        );
                        advance(&mut b, 20, 3, true);
                        assert_ne!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - level)), 0);
                        assert_eq!(b.debug_read32(DATA), Ok(0x99aabbcc));
                        assert_eq!(command(&mut b, [0x0600, 0, 0, 0])[1], 4);
                    }
                }
            }
        }
    }
}

#[test]
fn finite_continued_writes_defer_start_but_late_writes_wait_for_running_dma() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board();
        let destination = 0x05e01000;
        pending(&mut b, p, destination);
        channel(&mut b, 0, true, true, destination);
        // Backpressure retains the next operand while the preceding one drains.
        // Refill a finite chain before the next external arbitration boundary.
        for n in 0..6 {
            let value = 0xa0000000 | n;
            assert_eq!(
                b.replay_cpu(p).write32(destination | 0x20000000, value),
                Err(BusError::Pending)
            );
            let mut accepted = false;
            for _ in 0..100 {
                advance(&mut b, 1, 1, true);
                assert_eq!(b.debug_read32(DATA), Ok(0x11223344));
                if b.replay_cpu(p).write32(destination | 0x20000000, value) == Ok(()) {
                    b.finish_cpu_transaction(p);
                    accepted = true;
                    break;
                }
            }
            assert!(accepted);
        }
        // The source must advance in bounded time once the finite stream ends.
        for _ in 0..100 {
            advance(&mut b, 1, 1, true);
            if b.debug_read32(DATA) != Ok(0x11223344) {
                break;
            }
        }
        assert_eq!(b.debug_read32(DATA), Ok(0x33445566)); // first physical A word
        assert_eq!(
            b.replay_cpu(p)
                .write32(destination | 0x20000000, 0xdeadbeef),
            Err(BusError::Pending)
        );
        advance(&mut b, 1, 1, true);
        assert_eq!(
            b.replay_cpu(p)
                .write32(destination | 0x20000000, 0xdeadbeef),
            Ok(())
        );
        b.finish_cpu_transaction(p);
        for _ in 0..100 {
            advance(&mut b, 1, 1, true);
            if b.read32(SCU + 0xa4).unwrap() & 0x800 != 0 {
                break;
            }
            assert_ne!(b.read16(destination), Ok(0xdead));
        }
        assert_eq!(b.read32(destination), Ok(0x11223344));
        assert_eq!(b.read32(destination + 4), Ok(0x55667788));
        assert_eq!(b.debug_read32(DATA), Ok(0x99aabbcc));
        advance(&mut b, 100, 17, true);
        assert_eq!(b.read32(destination), Ok(0xdeadbeef));
    }
}

#[test]
fn pending_a_bus_write_qualifies_but_cpu_ram_and_unrelated_dma_routes_keep_priority() {
    for p in [Processor::Master, Processor::Slave] {
        for (cpu_destination, source, destination, must_wait) in [
            (CD + 12, DATA, 0x05e01000, true),
            (HIGH_WRAM + 0x100, DATA, 0x05e01000, false),
            (0x05e01000, HIGH_WRAM + 0x200, 0x05e02000, false),
            (0x05e01000, DATA, HIGH_WRAM + 0x200, false),
        ] {
            let mut b = board();
            let peer = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            assert_eq!(b.replay_cpu(peer).read32(HIGH_WRAM), Err(BusError::Pending));
            b.grant_bus_transaction();
            assert_eq!(
                b.replay_cpu(p)
                    .write16(cpu_destination | 0x20000000, 0x1234),
                Err(BusError::Pending)
            );
            advance(&mut b, 1, 1, true);
            assert_eq!(
                b.replay_cpu(p)
                    .write16(cpu_destination | 0x20000000, 0x1234),
                Ok(())
            );
            b.finish_cpu_transaction(p);
            for (off, value) in [
                (0, source),
                (4, destination),
                (8, 4),
                (
                    12,
                    if destination == HIGH_WRAM + 0x200 {
                        2
                    } else {
                        if source == DATA { 1 } else { 0x101 }
                    },
                ),
                (20, 7),
                (16, 0x101),
            ] {
                b.write32(SCU + off, value).unwrap();
            }
            let mut observed = false;
            for _ in 0..200 {
                advance(&mut b, 1, 1, true);
                let cpu_done = b.transactions.writes[p as usize].is_none();
                let dma_done = b.read32(SCU + 0xa4).unwrap() & 0x800 != 0;
                if must_wait && cpu_done {
                    assert!(!dma_done);
                    observed = true;
                    break;
                }
                if !must_wait && dma_done {
                    assert!(!cpu_done);
                    observed = true;
                    break;
                }
            }
            assert!(observed);
        }
    }
}

#[test]
fn startup_and_active_cancellation_follow_scu_and_cpu_reset_domains_and_chunking() {
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            for indirect in [false, true] {
                for phase in 0..4 {
                    let mut b = board();
                    let destination = 0x05e01000;
                    pending(&mut b, Processor::Master, destination);
                    channel(&mut b, 0, indirect, true, destination);
                    // pending startup, partial source, first completed B word,
                    // or full transfer. Stop by observable effects, not helpers.
                    for _ in 0..300 {
                        if phase == 0
                            || (phase == 1 && b.debug_read32(DATA) == Ok(0x33445566))
                            || (phase == 2 && b.read16(destination) == Ok(0x1122))
                            || (phase == 3 && b.read32(SCU + 0xa4).unwrap() & 0x800 != 0)
                        {
                            break;
                        }
                        advance(&mut b, 1, 1, inspect);
                    }
                    if phase == 2 {
                        assert_eq!(b.read32(destination), Ok(0x1122ccdd));
                    }
                    if phase == 3 {
                        assert_eq!(b.read32(destination), Ok(0x11223344));
                    }
                    let fifo = b.debug_read32(DATA).unwrap();
                    if phase == 0 {
                        assert_eq!(fifo, 0x11223344);
                    }
                    // Remove unrelated CPU work, then use the real PLL-change
                    // domain: SCU/video/sound reset; work RAM and CD are retained.
                    b.cancel_cpu_transaction(Processor::Master);
                    b.write32(HIGH_WRAM + 0x500, 0x12345678).unwrap();
                    b.change_system_clock(true);
                    advance(&mut b, 1000, chunk, inspect);
                    assert_eq!(b.debug_read32(DATA), Ok(fifo));
                    assert_eq!(b.read32(destination), Ok(0));
                    assert_eq!(b.read32(HIGH_WRAM + 0x500), Ok(0x12345678));
                    assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0x800, 0);
                    b.system_reset();
                    b.write32(destination, 0xcafebabe).unwrap();
                    advance(&mut b, 1000, chunk, inspect);
                    assert_eq!(b.read32(destination), Ok(0xcafebabe));
                    assert_eq!(b.debug_read32(DATA), Ok(u32::MAX));
                    assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0x800, 0);
                }
            }
            for p in [Processor::Master, Processor::Slave] {
                let mut b = board();
                let destination = 0x05e01000;
                pending(&mut b, p, destination);
                channel(&mut b, 0, false, false, destination);
                // CPU-local reset removes its write, not the SCU request.
                b.cancel_cpu_transaction(p);
                advance(&mut b, 1000, chunk, inspect);
                assert_eq!(b.read32(destination), Ok(0x11223344));
                assert_eq!(b.debug_read32(DATA), Ok(0x99aabbcc));
                assert_ne!(b.read32(SCU + 0xa4).unwrap() & 0x800, 0);
            }
        }
    }
}

#[test]
fn active_indirect_chain_is_one_startup_and_hardware_retrigger_is_only_one_deep() {
    for chunk in [1, 3, 17, 10000] {
        let mut b = board();
        let destination = 0x05e01000;
        channel(&mut b, 0, true, true, destination);
        // Two descriptors in the same activation, with distinct destinations.
        b.write32(HIGH_WRAM + 0x88, DATA).unwrap();
        for (n, v) in [4, destination + 8, DATA | 0x80000000]
            .into_iter()
            .enumerate()
        {
            b.write32(HIGH_WRAM + 0x8c + n as u32 * 4, v).unwrap();
        }
        for _ in 0..100 {
            advance(&mut b, 1, 1, true);
            if b.debug_read32(DATA) == Ok(0x33445566) {
                break;
            }
        }
        assert_eq!(b.debug_read32(DATA), Ok(0x33445566));
        assert_eq!(
            b.replay_cpu(Processor::Slave)
                .write32(destination | 0x20000000, 0xdeadbeef),
            Err(BusError::Pending)
        );
        for _ in 0..200 {
            advance(&mut b, 1, 1, true);
            if b.read32(SCU + 0xa4).unwrap() & 0x800 != 0 {
                break;
            }
            assert_ne!(b.read16(destination), Ok(0xdead));
        }
        assert_eq!(b.read32(destination), Ok(0x11223344));
        assert_eq!(b.read32(destination + 4), Ok(0x55667788));
        assert_eq!(b.read32(destination + 8), Ok(0x99aabbcc));
        advance(&mut b, 100, chunk, true);
        assert_eq!(b.read32(destination), Ok(0xdeadbeef));
        assert_eq!(command(&mut b, [0x0600, 0, 0, 0])[1], 6);

        let mut b = board();
        pending(&mut b, Processor::Master, destination);
        channel(&mut b, 0, false, true, destination);
        for _ in 0..5 {
            b.scu.signal(6);
        }
        advance(&mut b, 1000, chunk, true);
        // Original request plus one queued request, never five more.
        assert_eq!(command(&mut b, [0x0600, 0, 0, 0])[1], 8);
        assert_ne!(b.read32(SCU + 0xa4).unwrap() & 0x800, 0);
    }
}

#[test]
fn running_a_to_b_can_be_preempted_by_level_two_without_restarting_its_source_unit() {
    for chunk in [1, 3, 17, 10000] {
        let mut b = board();
        let low = 0x05e01000;
        channel(&mut b, 0, false, false, low);
        advance(&mut b, 3, chunk, true); // setup plus granted first A word
        // Valid two-level concurrency: level2 may preempt level0. Never start
        // level2 during level1 (ST-210 item35), or demand three-level ordering.
        let high = 0x05c02000;
        channel(&mut b, 2, false, false, high);
        for _ in 0..200 {
            advance(&mut b, 1, 1, true);
            if b.read32(SCU + 0xa4).unwrap() & 0x200 != 0 {
                break;
            }
        }
        assert_eq!(b.read32(high), Ok(0x33445566));
        assert_eq!(b.read32(high + 4), Ok(0x778899aa));
        assert_eq!(b.read32(low), Ok(0));
        assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0xa00, 0x200);
        advance(&mut b, 200, chunk, true);
        assert_eq!(b.read32(low), Ok(0x1122bbcc)); // retained high halfword
        assert_eq!(b.read32(low + 4), Ok(0));
        assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0xa00, 0xa00);
        assert_eq!(command(&mut b, [0x0600, 0, 0, 0])[1], 8);
    }
}

#[test]
fn startup_publications_and_fifo_position_repeat_across_advance_chunks_and_inspection() {
    for p in [Processor::Master, Processor::Slave] {
        for indirect in [false, true] {
            let mut baseline = None;
            for chunk in [1, 3, 17, 10000] {
                for inspect in [false, true] {
                    let mut b = board();
                    let destination = 0x05e01000;
                    pending(&mut b, p, destination);
                    channel(&mut b, 1, indirect, true, destination);
                    let mut samples = Vec::new();
                    let mut elapsed = 0;
                    for deadline in [7, 15, 23, 31, 39, 47, 55, 63, 71, 79, 87, 95, 111, 151, 251] {
                        advance(&mut b, deadline - elapsed, chunk, inspect);
                        elapsed = deadline;
                        samples.push((
                            b.read32(destination).unwrap(),
                            b.read32(destination + 4).unwrap(),
                            b.debug_read32(DATA).unwrap(),
                            b.read32(SCU + 0xa4).unwrap(),
                        ));
                    }
                    assert_eq!(samples.last().unwrap().0, 0x11223344);
                    assert_eq!(samples.last().unwrap().1, 0x55667788);
                    assert_eq!(samples.last().unwrap().2, 0x99aabbcc);
                    if let Some(old) = &baseline {
                        assert_eq!(&samples, old);
                    } else {
                        baseline = Some(samples);
                    }
                }
            }
        }
    }
}

#[test]
fn actual_system_reset_cancels_pending_and_partially_completed_startup_work() {
    for chunk in [1, 3, 17, 10000] {
        for phase in 0..4 {
            let mut b = board();
            let destination = 0x05e01000;
            pending(&mut b, Processor::Slave, destination);
            channel(&mut b, 0, true, true, destination);
            for _ in 0..300 {
                if phase == 0
                    || (phase == 1 && b.debug_read32(DATA) == Ok(0x33445566))
                    || (phase == 2 && b.read16(destination) == Ok(0x1122))
                    || (phase == 3 && b.read32(destination + 4) == Ok(0x55660000))
                {
                    break;
                }
                advance(&mut b, 1, 1, true);
            }
            match phase {
                0 => assert_eq!(b.debug_read32(DATA), Ok(0x11223344)),
                1 => assert_eq!(b.debug_read32(DATA), Ok(0x33445566)),
                2 => assert_eq!(b.read32(destination), Ok(0x1122ccdd)),
                _ => assert_eq!(b.read32(destination + 4), Ok(0x55660000)),
            }
            b.system_reset();
            b.write32(destination, 0xcafebabe).unwrap();
            advance(&mut b, 1000, chunk, true);
            assert_eq!(b.debug_read32(DATA), Ok(u32::MAX));
            assert_eq!(b.read32(destination), Ok(0xcafebabe));
            assert_eq!(b.read32(destination + 4), Ok(0));
            assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0x800, 0);
        }
    }
}

#[test]
fn executing_mov_qualifies_a_to_b_startup_only_after_store_ma_request() {
    use crate::cpu::{Cpu, CpuError};
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board();
        let destination = 0x05e01000;
        b.write16(HIGH_WRAM, 0x2FF6).unwrap(); // MOV.L R15,@-R15
        b.for_cpu(p).write8(CCR, 0x11).unwrap();
        b.for_cpu(p).fetch16(HIGH_WRAM).unwrap();
        b.cpu_delay(p, 0);
        b.advance_clocks(32).unwrap();
        let mut c = Cpu::start(HIGH_WRAM, (destination | 0x20000000) + 4);
        assert!(matches!(
            c.clone().step(&mut b.replay_cpu(p)),
            Err(CpuError::Bus {
                source: BusError::Pending,
                ..
            })
        ));
        assert!(!b.cpu_ab_write_pending()); // address/data prepared, EX incomplete
        b.advance_clocks(1).unwrap();
        assert!(matches!(
            c.clone().step(&mut b.replay_cpu(p)),
            Err(CpuError::Bus {
                source: BusError::Pending,
                ..
            })
        ));
        assert!(b.cpu_ab_write_pending()); // MA qualified, not yet accepted
        channel(&mut b, 0, false, false, destination);
        b.grant_bus_transaction();
        // SCU can initialize its channel in a width-zero reservation; the
        // startup interlock must prevent its first physical source request.
        let active = b.transactions.active.as_ref().unwrap();
        assert!(
            matches!(active.owner,Owner::WriteBuffer(cpu,false) if cpu==p)
                || active.owner == Owner::Scu && active.transfer.width == 0
        );
        assert_eq!(b.debug_read32(DATA), Ok(0x11223344));
        let mut publications = Vec::new();
        let mut previous = [0u16; 4];
        for _ in 0..500 {
            b.advance_clocks(1).unwrap();
            if c.state().instructions == 0 {
                let mut next = c.clone();
                match next.step(&mut b.replay_cpu(p)) {
                    Ok(()) => {
                        c = next;
                        b.finish_cpu_transaction(p);
                    }
                    Err(CpuError::Bus {
                        source: BusError::Pending,
                        ..
                    }) => (),
                    other => panic!("store replay: {other:?}"),
                }
            }
            let current = std::array::from_fn(|i| b.read16(destination + i as u32 * 2).unwrap());
            for i in 0..4 {
                if current[i] != previous[i] {
                    publications.push((i, current[i]));
                }
            }
            previous = current;
            if publications.len() < 2 {
                assert_eq!(b.debug_read32(DATA), Ok(0x11223344));
            }
            if publications.len() < 6 {
                assert_eq!(b.read32(SCU + 0xA4).unwrap() & (1 << 11), 0);
            } else {
                break;
            }
        }
        assert_eq!(c.state().instructions, 1);
        assert_eq!(c.state().r[15], destination | 0x20000000);
        assert_eq!(
            publications,
            [
                (0, 0x25e0),
                (1, 0x1004),
                (0, 0x1122),
                (1, 0x3344),
                (2, 0x5566),
                (3, 0x7788)
            ]
        );
        b.advance_clocks(20).unwrap();
        assert_ne!(b.read32(SCU + 0xA4).unwrap() & (1 << 11), 0);
        assert_eq!(b.debug_read32(DATA), Ok(0x99aabbcc));
    }
}
