//! Independently authored IF/ID oracle; docs/sh2-fetch-pipeline.md.
use super::*;
use crate::{Machine, cpu::Cpu};

#[test]
fn p1_fetch_of_second_successor_owns_bus_before_first_retirement_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        let mut m = Machine::new();
        m.bus.set_slave_enabled(p == Processor::Slave);
        for at in (0..32).step_by(2) {
            m.bus.write16(HIGH_WRAM + at, 9).unwrap();
        }
        m.bus.for_cpu(p).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
        let c = Cpu::start(HIGH_WRAM | 0x20000000, HIGH_WRAM + 0x1000);
        if p == Processor::Master {
            m.cpu = c;
        } else {
            m.slave = c;
        }
        // Keep the other CPU stopped without issuing any bus access.
        m.cpu_ready[1 - p as usize] = u64::MAX;
        while m.bus.main_clocks < 4 {
            tick(&mut m, p);
        }
        let c = if p == Processor::Master {
            &m.cpu
        } else {
            &m.slave
        };
        assert_eq!(
            c.state().instructions,
            0,
            "EX must share the waited PC+4 IF slot"
        );
        let active = m.bus.transactions.active.as_ref().unwrap();
        assert!(active.owner == Owner::Cpu(p));
        assert_eq!((active.transfer.address, active.remaining), (0x26000004, 1));
        assert_eq!(m.bus.read16(HIGH_WRAM + 4), Ok(9));
    }
}

// One-clock observation uses exactly the production issue/grant/completion
// protocol. Inspection itself neither fetches instructions nor ages devices.
fn tick(m: &mut Machine, p: Processor) {
    while m.issue_cpu(p).unwrap() && m.cpu_ready[p as usize] <= m.bus.main_clocks {}
    m.bus.grant_bus_transaction();
    let (reference, command) = m.bus.advance_cpu_clocks(1).unwrap();
    assert!(command.is_none());
    m.clocks += u64::from(reference);
    if m.bus.take_cpu_transaction_completion(p) {
        m.cpu_ready[p as usize] = m.bus.main_clocks;
    }
}

fn board(p: Processor, offset: u32, resident: Option<bool>, waits: u32) -> Machine {
    let mut m = Machine::new();
    m.bus.set_slave_enabled(p == Processor::Slave);
    for at in (0..128).step_by(2) {
        m.bus.write16(HIGH_WRAM + at, 9).unwrap();
    }
    m.bus
        .for_cpu(p)
        .write32(0xFFFFFFE8, 0xA55A0000 | waits)
        .unwrap();
    if let Some(warm) = resident {
        m.bus.for_cpu(p).write8(CCR, 0x11).unwrap();
        if warm {
            for at in (0..128).step_by(16) {
                m.bus.for_cpu(p).fetch16(HIGH_WRAM + at).unwrap();
            }
        }
    }
    m.bus.cpu_delay(p, 0);
    let alias = if resident.is_some() { 0 } else { 0x20000000 };
    let c = Cpu::start(HIGH_WRAM + alias + offset, HIGH_WRAM + 0x1000);
    if p == Processor::Master {
        m.cpu = c;
    } else {
        m.slave = c;
    }
    m.cpu_ready[1 - p as usize] = u64::MAX;
    m
}

// Capture grants and retirements at production boundaries. Byte sampling is
// separately checked with same-edge mutations below; the trace is not an oracle.
type StageTrace = (Vec<(u64, u32, usize)>, Vec<(u64, u64)>);
fn trace(m: &mut Machine, p: Processor, end: u64, chunk: u32, inspect: bool) -> StageTrace {
    let mut grants = Vec::new();
    let mut retired = Vec::new();
    let mut last_grant = None;
    while m.bus.main_clocks < end {
        while m.issue_cpu(p).unwrap() {
            retired.push((
                m.bus.main_clocks,
                if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                }
                .instructions,
            ));
            if m.cpu_ready[p as usize] > m.bus.main_clocks {
                break;
            }
        }
        m.bus.grant_bus_transaction();
        if let Some(a) = &m.bus.transactions.active {
            let key = (a.owner, a.transfer.address, a.transfer.width);
            if last_grant != Some(key) {
                grants.push((m.bus.main_clocks, a.transfer.address, a.transfer.width));
                last_grant = Some(key);
            }
        } else {
            last_grant = None;
        }
        if inspect {
            let _ = m.bus.debug_read32(HIGH_WRAM);
            let _ = m.cpu_state();
            let _ = m.slave_state();
        }
        let n = chunk
            .min(m.bus.bus_main_deadline())
            .min((end - m.bus.main_clocks) as u32)
            .min(
                m.cpu_ready[p as usize]
                    .saturating_sub(m.bus.main_clocks)
                    .max(1)
                    .min(u64::from(u32::MAX)) as u32,
            )
            .max(1);
        let (reference, command) = m.bus.advance_cpu_clocks(n).unwrap();
        assert!(command.is_none());
        m.clocks += u64::from(reference);
        if m.bus.take_cpu_transaction_completion(p) {
            m.cpu_ready[p as usize] = m.bus.main_clocks;
        }
    }
    (grants, retired)
}

#[test]
fn literal_resident_cold_refill_p1_waited_and_alignment_traces_are_chunk_invariant() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut m = board(p, 0, Some(true), 0);
                let (grants, retired) = trace(&mut m, p, 6, chunk, inspect);
                assert!(grants.is_empty());
                assert_eq!(retired, [(2, 1), (3, 2), (4, 3), (5, 4)]);
                let mut m = board(p, 0, Some(false), 0);
                let (grants, retired) = trace(&mut m, p, 24, chunk, inspect);
                assert_eq!(
                    grants,
                    [
                        (0, HIGH_WRAM + 4, 4),
                        (2, HIGH_WRAM + 8, 4),
                        (4, HIGH_WRAM + 12, 4),
                        (6, HIGH_WRAM, 4),
                        (15, HIGH_WRAM + 20, 4),
                        (17, HIGH_WRAM + 24, 4),
                        (19, HIGH_WRAM + 28, 4),
                        (21, HIGH_WRAM + 16, 4)
                    ]
                );
                assert_eq!(
                    retired,
                    [
                        (9, 1),
                        (10, 2),
                        (11, 3),
                        (12, 4),
                        (13, 5),
                        (14, 6),
                        (23, 7),
                        (23, 8)
                    ]
                );
                for (offset, waits, first, grants_expected) in [
                    (
                        0,
                        0,
                        5,
                        vec![
                            (0, 0x26000000, 4),
                            (3, 0x26000004, 4),
                            (6, 0x26000008, 4),
                            (9, 0x2600000C, 4),
                        ],
                    ),
                    (
                        2,
                        0,
                        4,
                        vec![
                            (0, 0x26000000, 4),
                            (2, 0x26000004, 4),
                            (5, 0x26000008, 4),
                            (8, 0x2600000C, 4),
                        ],
                    ),
                    (
                        0,
                        0xC0,
                        17,
                        vec![
                            (0, 0x26000000, 4),
                            (9, 0x26000004, 4),
                            (18, 0x26000008, 4),
                            (27, 0x2600000C, 4),
                        ],
                    ),
                ] {
                    let mut m = board(p, offset, None, waits);
                    let (grants, retired) = trace(
                        &mut m,
                        p,
                        grants_expected.last().unwrap().0 + 1,
                        chunk,
                        inspect,
                    );
                    assert_eq!(grants, grants_expected);
                    assert_eq!(retired[0], (first, 1));
                }
            }
        }
    }
}

#[test]
fn fetched_instruction_bytes_are_retained_before_retirement_without_snooping_later_writes() {
    for p in [Processor::Master, Processor::Slave] {
        let mut m = board(p, 0, None, 0);
        m.bus.write32(HIGH_WRAM, 0xE001E102).unwrap();
        m.bus.write32(HIGH_WRAM + 4, 0xE203E304).unwrap();
        // PC+4 samples at 5. Mutating PC+4 after completion must not replace
        // its queued bytes; PC+8 has not been sampled and must see the change.
        for _ in 0..5 {
            tick(&mut m, p);
        }
        m.bus.write32(HIGH_WRAM + 4, 0xE22AE32B).unwrap();
        m.bus.write32(HIGH_WRAM + 8, 0xE42CE52D).unwrap();
        trace(&mut m, p, 13, 1, true);
        let c = if p == Processor::Master {
            m.cpu_state()
        } else {
            m.slave_state()
        };
        assert_eq!(&c.r[..6], &[1, 2, 3, 4, 44, 45]);
        assert_eq!(c.exceptions, 0);
    }
}

#[test]
fn mov_ma_precedes_the_contending_if_and_dependency_is_counted_once() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            for successor in [9, 0x7001] {
                let mut m = board(p, offset, None, 0);
                m.bus.write16(HIGH_WRAM + offset, 0x60F2).unwrap();
                m.bus.write16(HIGH_WRAM + offset + 2, successor).unwrap();
                let c = Cpu::start(0x26000000 + offset, 0x26004000);
                if p == Processor::Master {
                    m.cpu = c;
                } else {
                    m.slave = c;
                }
                m.bus.write32(0x26004000, 0x12345678).unwrap();
                let (grants, _) = trace(&mut m, p, 12, 1, false);
                let data = grants
                    .iter()
                    .position(|&(_, a, _)| a == 0x26004000)
                    .unwrap();
                assert_eq!(grants[data], (5, 0x26004000, 4));
                assert_eq!(
                    grants[data + 1],
                    (7 + u64::from(offset == 0 && successor != 9), 0x26000008, 4)
                );
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!(c.r[0], 0x12345678 + u32::from(successor != 9));
                assert_eq!(c.r[15], 0x26004000);
            }
        }
    }
}

// P figs7.6/7.7/7.82: the target stream, not the branch/slot alignment,
// determines whether IF beside the delay-slot MOV's MA needs a longword.
#[test]
fn delay_slot_mov_uses_target_alignment_for_contention_and_dependency_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            for target_offset in [0, 2] {
                for (mov, add, expected_r0) in [
                    (0x60F2, 0x7601, 42), // load, independent ADD
                    (0x60F2, 0x7001, 43), // load, dependent ADD
                    (0x2FF2, 0x7601, 0),  // store, independent ADD
                ] {
                    for chunk in [1, 3, 17, 10000] {
                        for inspect in [false, true] {
                            let mut m = board(p, offset, Some(true), 0);
                            let target = HIGH_WRAM + 0x100 + target_offset;
                            let branch = 0xA000 | ((0x100 + target_offset - offset - 4) / 2) as u16;
                            m.bus
                                .for_cpu(p)
                                .write16(HIGH_WRAM + offset, branch)
                                .unwrap();
                            m.bus
                                .for_cpu(p)
                                .write16(HIGH_WRAM + offset + 2, mov)
                                .unwrap();
                            m.bus.write16(target, add).unwrap();
                            m.bus.write16(target + 2, 0x001B).unwrap();
                            m.bus.write16(target + 4, 0x7763).unwrap();
                            m.bus.for_cpu(p).fetch16(target).unwrap();
                            let stack = 0xC0000400;
                            m.bus.for_cpu(p).write32(stack, 42).unwrap();
                            m.bus.cpu_delay(p, 0);
                            let cpu = Cpu::start(HIGH_WRAM + offset, stack);
                            if p == Processor::Master {
                                m.cpu = cpu;
                            } else {
                                m.slave = cpu;
                            }
                            // Branch EX=2, target IF=3, slot EX=4, MA=5..6.
                            // Independent target EX shares MA only with retained
                            // if (target=4n+2); dependency and real IF split once.
                            let sleep = if target_offset == 2 && add != 0x7001 {
                                6
                            } else {
                                7
                            };
                            let (grants, retired) = trace(&mut m, p, sleep + 1, chunk, inspect);
                            assert!(grants.is_empty());
                            assert_eq!(
                                retired.last(),
                                Some(&(sleep, 4)),
                                "{p:?} branch={offset} target={target_offset} mov={mov:X} add={add:X}"
                            );
                            let c = if p == Processor::Master {
                                m.cpu_state()
                            } else {
                                m.slave_state()
                            };
                            assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                            assert_eq!(
                                (c.r[0], c.r[6], c.r[7]),
                                (expected_r0, u32::from(add == 0x7601), 0)
                            );
                            assert_eq!((c.r[15], c.pc, c.exceptions), (stack, target + 4, 0));
                            assert_eq!(
                                m.bus.for_cpu(p).read32(stack),
                                Ok(if mov == 0x2FF2 { stack } else { 42 })
                            );
                        }
                    }
                }
            }
        }
    }
}

// P fig7.85 / Rev.5 p.491: successor ID stalls during initial EX.
// Aligned P1 IF=0..2, ID=2..3, EX=3..4, synchronous MA=4..5,
// then contending PC+4 IF=5..7. Six Tw changes these to 0..8,
// 8..9, 9..10, 10..11 and 11..19 respectively.
#[test]
fn stc_long_holds_younger_if_through_ex_and_retains_completion_bytes_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (waits, ex_end, younger_if, sample) in [(0, 4, 5, 7), (0xC0, 10, 11, 19)] {
            for opcode in [0x4F03, 0x4F13, 0x4F23] {
                for chunk in [1, 3, 17, 10000] {
                    for inspect in [false, true] {
                        let mut m = board(p, 0, None, waits);
                        for (at, op) in [(0, opcode), (2, 9), (4, 0xE82A), (6, 0x001B)] {
                            m.bus.write16(HIGH_WRAM + at, op).unwrap();
                        }
                        let stack = 0xC0000404;
                        let cpu = Cpu::start(0x26000000, stack);
                        if p == Processor::Master {
                            m.cpu = cpu;
                        } else {
                            m.slave = cpu;
                        }
                        let (grants, retired) = trace(&mut m, p, ex_end, chunk, inspect);
                        assert_eq!(grants, [(0, 0x26000000, 4)], "{p:?} {opcode:X}");
                        assert!(retired.is_empty());
                        let (grants, _) = trace(&mut m, p, sample - 1, chunk, inspect);
                        assert_eq!(grants, [(younger_if, 0x26000004, 4)]);
                        assert!(
                            m.bus
                                .transactions
                                .active
                                .as_ref()
                                .is_some_and(|a| a.owner == Owner::Cpu(p)
                                    && a.transfer.address == 0x26000004
                                    && a.remaining == 1)
                        );
                        m.bus.write16(HIGH_WRAM + 4, 0xE82B).unwrap();
                        trace(&mut m, p, sample, chunk, inspect);
                        m.bus.write16(HIGH_WRAM + 4, 0xE82C).unwrap();
                        trace(&mut m, p, 80, chunk, inspect);
                        let c = if p == Processor::Master {
                            m.cpu_state()
                        } else {
                            m.slave_state()
                        };
                        let stored = if opcode == 0x4F03 { c.sr } else { 0 };
                        assert_eq!(
                            (c.r[8], c.r[15], c.instructions, c.exceptions),
                            (43, stack - 4, 4, 0)
                        );
                        assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                        assert_eq!(m.bus.for_cpu(p).read32(stack - 4), Ok(stored));
                        assert_eq!(m.bus.for_cpu(p).read32(stack - 8), Ok(0));
                    }
                }
            }
        }
    }
}

// The stalled EX is followed by MA/IF arbitration, including buffered-store
// read fencing. Retained if adds no real fetch beside MA. With six Tw, the
// buffer still admits in one state, alongside the external drain
// (2+Tw states); the existing write-to-read turnaround then precedes IF.
#[test]
fn stc_long_ma_if_contention_preserves_retained_and_buffered_routes_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (resident, offset, stack, waits, expected_grants, sleep) in [
            (Some(true), 0, 0xC0000404, 0, vec![], 6),
            (Some(true), 2, 0xC0000404, 0, vec![], 5),
            (
                None,
                0,
                0xC0000404,
                0,
                vec![(0, 0x26000000, 4), (5, 0x26000004, 4)],
                8,
            ),
            (
                None,
                2,
                0xC0000404,
                0,
                vec![(0, 0x26000000, 4), (2, 0x26000004, 4), (6, 0x26000008, 4)],
                8,
            ),
            (
                None,
                2,
                0xC0000404,
                0xC0,
                vec![(0, 0x26000000, 4), (8, 0x26000004, 4), (18, 0x26000008, 4)],
                26,
            ),
            (
                None,
                0,
                0x26004004,
                0,
                vec![(0, 0x26000000, 4), (4, 0x26004000, 4), (7, 0x26000004, 4)],
                10,
            ),
            (
                None,
                0,
                0x26004004,
                0xC0,
                vec![(0, 0x26000000, 4), (10, 0x26004000, 4), (19, 0x26000004, 4)],
                28,
            ),
            (
                None,
                2,
                0x26004004,
                0,
                vec![
                    (0, 0x26000000, 4),
                    (2, 0x26000004, 4),
                    (5, 0x26004000, 4),
                    (8, 0x26000008, 4),
                ],
                10,
            ),
            (
                None,
                2,
                0x26004004,
                0xC0,
                vec![
                    (0, 0x26000000, 4),
                    (8, 0x26000004, 4),
                    (17, 0x26004000, 4),
                    (26, 0x26000008, 4),
                ],
                34,
            ),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut m = board(p, offset, resident, waits);
                for (at, op) in [(0, 0x4F03), (2, 9), (4, 0x001B), (6, 0x7763)] {
                    m.bus
                        .for_cpu(p)
                        .write16(HIGH_WRAM + offset + at, op)
                        .unwrap();
                }
                m.bus.cpu_delay(p, 0);
                let pc = HIGH_WRAM + offset + if resident.is_some() { 0 } else { 0x20000000 };
                let cpu = Cpu::start(pc, stack);
                if p == Processor::Master {
                    m.cpu = cpu;
                } else {
                    m.slave = cpu;
                }
                let (grants, retired) = trace(&mut m, p, sleep + 1, chunk, true);
                assert_eq!(
                    grants, expected_grants,
                    "{p:?} resident={resident:?} offset={offset} stack={stack:X} waits={waits:X}"
                );
                assert_eq!(retired.last(), Some(&(sleep, 3)));
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!((c.r[15], c.r[7], c.exceptions), (stack - 4, 0, 0));
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                assert_eq!(m.bus.for_cpu(p).read32(stack - 4), Ok(0xF0));
                assert_eq!(m.bus.for_cpu(p).read32(stack - 8), Ok(0));
            }
        }
    }
}

// P figs7.6/7.7/7.63/7.82: redirected IF pairing belongs to the target.
// Resident JMP EX=2, P1 target IF=3..5, slot EX=5. Target 4n+2
// needs real IF after first MA=6..7; target 4n retains that halfword,
// and the ADD's EX at 8 requests target+4. Six Tw adds six to both.
#[test]
fn delay_slot_multiplier_uses_target_alignment_for_grants_and_samples_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            for (target_offset, waits, younger_if, sample) in [
                (0, 0, 8, 10),
                (2, 0, 7, 9),
                (0, 0xC0, 14, 22),
                (2, 0xC0, 13, 21),
            ] {
                for opcode in [0x0007, 0x3005, 0x300D] {
                    for chunk in [1, 3, 17, 10000] {
                        for inspect in [false, true] {
                            let mut m = board(p, offset, Some(true), waits);
                            let target = 0x26000100 + target_offset;
                            m.bus
                                .for_cpu(p)
                                .write16(HIGH_WRAM + offset, 0x4F2B)
                                .unwrap();
                            m.bus
                                .for_cpu(p)
                                .write16(HIGH_WRAM + offset + 2, opcode)
                                .unwrap();
                            m.bus.write16(target, 0x7601).unwrap();
                            m.bus.write16(target + 2, 0xE82A).unwrap();
                            m.bus.write16(target + 4, 0x001B).unwrap();
                            m.bus.write16(target + 6, 0x7763).unwrap();
                            m.bus.cpu_delay(p, 0);
                            let cpu = Cpu::start(HIGH_WRAM + offset, target);
                            if p == Processor::Master {
                                m.cpu = cpu;
                            } else {
                                m.slave = cpu;
                            }
                            let (grants, _) = trace(&mut m, p, sample - 1, chunk, inspect);
                            assert_eq!(
                                grants,
                                [(3, target & !3, 4), (younger_if, (target + 4) & !3, 4)],
                                "{p:?} branch={offset} target={target_offset} op={opcode:X}"
                            );
                            assert!(
                                m.bus
                                    .transactions
                                    .active
                                    .as_ref()
                                    .is_some_and(|a| a.owner == Owner::Cpu(p) && a.remaining == 1)
                            );
                            // This instruction is in the first target longword at
                            // 4n, and in the later contending longword at 4n+2.
                            m.bus.write16(target + 2, 0xE82B).unwrap();
                            trace(&mut m, p, sample, chunk, inspect);
                            m.bus.write16(target + 2, 0xE82C).unwrap();
                            trace(&mut m, p, 100, chunk, inspect);
                            let c = if p == Processor::Master {
                                m.cpu_state()
                            } else {
                                m.slave_state()
                            };
                            assert_eq!(
                                (c.r[6], c.r[7], c.r[8]),
                                (1, 0, if target_offset == 0 { 42 } else { 43 })
                            );
                            assert_eq!(
                                (c.r[15], c.macl, c.mach, c.instructions, c.exceptions),
                                (target, 0, 0, 5, 0)
                            );
                            assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                        }
                    }
                }
            }
        }
    }
}

// P fig7.84 / Rev.5 printed p.491: successor ID stalls during EX and MA.
// For aligned P1, IF=0..2, ID=2..3, EX=3..4, MA=4..6,
// final EX/third IF=6..8. Six Tw stretches each real transfer to eight.
#[test]
fn ldc_long_stalls_younger_if_until_operand_completion_and_retains_samples_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (waits, ma, operand_sample, younger_if, instruction_sample) in
            [(0, 4, 6, 6, 8), (0xC0, 10, 18, 18, 26)]
        {
            for opcode in [0x4F07, 0x4F17, 0x4F27] {
                for chunk in [1, 3, 17, 10000] {
                    for inspect in [false, true] {
                        let mut m = board(p, 0, None, waits);
                        let stack = 0x26004000;
                        for (at, op) in [(0, opcode), (2, 9), (4, 0xE82A), (6, 0x001B), (8, 0x7763)]
                        {
                            m.bus.write16(HIGH_WRAM + at, op).unwrap();
                        }
                        m.bus.write32(stack, 1).unwrap();
                        let cpu = Cpu::start(0x26000000, stack);
                        if p == Processor::Master {
                            m.cpu = cpu;
                        } else {
                            m.slave = cpu;
                        }
                        let (grants, retired) =
                            trace(&mut m, p, operand_sample - 1, chunk, inspect);
                        assert_eq!(
                            grants,
                            [(0, 0x26000000, 4), (ma, stack, 4)],
                            "{p:?} {opcode:X}"
                        );
                        assert!(retired.is_empty());
                        assert!(
                            m.bus
                                .transactions
                                .active
                                .as_ref()
                                .is_some_and(|a| a.owner == Owner::Cpu(p)
                                    && a.transfer.address == stack
                                    && a.remaining == 1)
                        );
                        m.bus.write32(stack, 2).unwrap();
                        trace(&mut m, p, operand_sample, chunk, inspect);
                        m.bus.write32(stack, 3).unwrap();
                        let (grants, retired) =
                            trace(&mut m, p, instruction_sample - 1, chunk, inspect);
                        assert_eq!(grants, [(younger_if, 0x26000004, 4)]);
                        assert!(retired.is_empty());
                        assert!(
                            m.bus
                                .transactions
                                .active
                                .as_ref()
                                .is_some_and(|a| a.owner == Owner::Cpu(p)
                                    && a.transfer.address == 0x26000004
                                    && a.remaining == 1)
                        );
                        m.bus.write16(HIGH_WRAM + 4, 0xE82B).unwrap();
                        trace(&mut m, p, instruction_sample, chunk, inspect);
                        m.bus.write16(HIGH_WRAM + 4, 0xE82C).unwrap();
                        let (grants, retired) = trace(&mut m, p, 100, chunk, inspect);
                        assert_eq!(grants, [(instruction_sample + 1, 0x26000008, 4)]);
                        assert_eq!(retired.iter().filter(|&&(_, n)| n == 1).count(), 1);
                        let c = if p == Processor::Master {
                            m.cpu_state()
                        } else {
                            m.slave_state()
                        };
                        let control = match opcode {
                            0x4F07 => c.sr,
                            0x4F17 => c.gbr,
                            _ => c.vbr,
                        };
                        assert_eq!(control, 2);
                        assert_eq!((c.r[8], c.r[7], c.r[15]), (43, 0, stack + 4));
                        assert_eq!((c.instructions, c.exceptions), (4, 0));
                        assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                    }
                }
            }
        }
    }
}

// The same two stalled slots apply with resident IF, retained if and a
// synchronous cache-array operand. Literal edges include external MA stretching
// and, for P1 entry at 4n+2, the real IF beside the following NOP.
#[test]
fn ldc_long_stall_edges_cover_resident_retained_and_synchronous_routes_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (resident, offset, stack, waits, expected_grants, sleep) in [
            (Some(true), 0, 0xC0000400, 0, vec![], 6),
            (Some(true), 2, 0xC0000400, 0, vec![], 6),
            (Some(true), 0, 0x26004000, 0, vec![(3, 0x26004000, 4)], 7),
            (
                Some(true),
                2,
                0x26004000,
                0xC0,
                vec![(3, 0x26004000, 4)],
                13,
            ),
            (
                None,
                0,
                0xC0000400,
                0,
                vec![(0, 0x26000000, 4), (5, 0x26000004, 4)],
                8,
            ),
            (
                None,
                2,
                0xC0000400,
                0,
                vec![(0, 0x26000000, 4), (2, 0x26000004, 4), (7, 0x26000008, 4)],
                9,
            ),
            (
                None,
                2,
                0x26004000,
                0,
                vec![
                    (0, 0x26000000, 4),
                    (2, 0x26000004, 4),
                    (5, 0x26004000, 4),
                    (8, 0x26000008, 4),
                ],
                10,
            ),
            (
                None,
                2,
                0x26004000,
                0xC0,
                vec![
                    (0, 0x26000000, 4),
                    (8, 0x26000004, 4),
                    (17, 0x26004000, 4),
                    (26, 0x26000008, 4),
                ],
                34,
            ),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut m = board(p, offset, resident, waits);
                for (at, op) in [(0, 0x4F07), (2, 9), (4, 0x001B), (6, 0x7763)] {
                    m.bus
                        .for_cpu(p)
                        .write16(HIGH_WRAM + offset + at, op)
                        .unwrap();
                }
                m.bus.for_cpu(p).write32(stack, 1).unwrap();
                m.bus.cpu_delay(p, 0);
                let pc = HIGH_WRAM + offset + if resident.is_some() { 0 } else { 0x20000000 };
                let cpu = Cpu::start(pc, stack);
                if p == Processor::Master {
                    m.cpu = cpu;
                } else {
                    m.slave = cpu;
                }
                let (grants, retired) = trace(&mut m, p, sleep + 1, chunk, true);
                assert_eq!(
                    grants, expected_grants,
                    "{p:?} resident={resident:?} offset={offset} stack={stack:X}"
                );
                assert_eq!(retired.last(), Some(&(sleep, 3)));
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!((c.sr, c.r[15], c.r[7], c.exceptions), (1, stack + 4, 0, 0));
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
            }
        }
    }
}

#[test]
fn fixed_word_cs0_fetch_samples_each_half_at_its_own_grant_and_retains_the_result() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut m = board(p, 0, None, 0);
            let mut rom = vec![0; BIOS_SIZE];
            for at in (0x200..0x240).step_by(2) {
                rom[at..at + 2].copy_from_slice(&9u16.to_be_bytes());
            }
            m.bus.load_bios(&rom).unwrap();
            let c = Cpu::start(0x20000200, HIGH_WRAM + 0x1000);
            if p == Processor::Master {
                m.cpu = c;
            } else {
                m.slave = c;
            }
            let (grants, retired) = trace(&mut m, p, 10, chunk, true);
            assert_eq!(
                grants,
                [
                    (0, 0x20000200, 2),
                    (2, 0x20000202, 2),
                    (5, 0x20000204, 2),
                    (7, 0x20000206, 2)
                ]
            );
            assert_eq!(retired, [(9, 1), (9, 2)]);
        }
    }
}

// P figs7.82/92: an unconditional branch stalls its slot's ID, with no
// second-successor IF in EX. RTE adds two synchronous MA slots before target IF.
#[test]
fn unconditional_branch_and_rte_hold_the_slot_without_sequential_if_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (offset, waits, branch_if, rte_if, initial) in [
            (0, 0, 4, 6, vec![(0, 0x26000000, 4)]),
            (2, 0, 5, 7, vec![(0, 0x26000000, 4), (2, 0x26000004, 4)]),
            (0, 0xC0, 10, 12, vec![(0, 0x26000000, 4)]),
            (
                2,
                0xC0,
                17,
                19,
                vec![(0, 0x26000000, 4), (8, 0x26000004, 4)],
            ),
        ] {
            for branch in [0xA000, 0xB000, 0x002B] {
                for chunk in [1, 3, 17, 10000] {
                    for inspect in [false, true] {
                        let mut m = board(p, offset, None, waits);
                        let opcode = if branch == 0x002B {
                            branch
                        } else {
                            branch | ((0x100 - offset - 4) / 2) as u16
                        };
                        for (at, op) in
                            [(offset, opcode), (offset + 2, 0x7601), (offset + 4, 0x7763)]
                        {
                            m.bus.write16(HIGH_WRAM + at, op).unwrap();
                        }
                        let stack = 0xC0000400;
                        m.bus.for_cpu(p).write32(stack, 0x26000100).unwrap();
                        m.bus.for_cpu(p).write32(stack + 4, 1).unwrap();
                        m.bus.write32(HIGH_WRAM + 0x100, 0xE82A001B).unwrap();
                        let c = Cpu::start(0x26000000 + offset, stack);
                        if p == Processor::Master {
                            m.cpu = c;
                        } else {
                            m.slave = c;
                        }
                        let target_if = if branch == 0x002B { rte_if } else { branch_if };
                        let mut expected = initial.clone();
                        expected.push((target_if, 0x26000100, 4));
                        let (grants, _) = trace(&mut m, p, target_if + 1, chunk, inspect);
                        assert_eq!(
                            grants, expected,
                            "{p:?} op={opcode:X} offset={offset} waits={waits:X}"
                        );
                        trace(&mut m, p, 100, chunk, inspect);
                        let c = if p == Processor::Master {
                            m.cpu_state()
                        } else {
                            m.slave_state()
                        };
                        assert_eq!(&c.r[6..9], &[1, 0, 42]);
                        assert_eq!(c.instructions, 4);
                        assert_eq!(c.exceptions, 0);
                        assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                        if branch == 0x002B {
                            assert_eq!((c.r[15], c.sr & 1), (stack + 8, 1));
                        } else if branch == 0xB000 {
                            assert_eq!(c.pr, 0x26000004 + offset);
                        }
                    }
                }
            }
        }
    }
}

// Renesas Rev.5 fig7.93: TRAPA fetches both successors but executes neither.
// A real PC+4 IF in its EX slot must finish through the arbiter before entry.
#[test]
fn trapa_samples_and_discards_second_successor_before_exception_accesses_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (waits, successor_if, sample) in [(0, 3, 5), (0xC0, 9, 17)] {
            for chunk in [1, 3, 17, 10000] {
                for inspect in [false, true] {
                    let mut m = board(p, 0, None, waits);
                    m.bus.write32(HIGH_WRAM, 0xC3207663).unwrap();
                    m.bus.write32(HIGH_WRAM + 4, 0x77637763).unwrap();
                    m.bus.write32(HIGH_WRAM + 0x100, 0xE82A001B).unwrap();
                    let mut rom = vec![0; BIOS_SIZE];
                    rom[0x80..0x84].copy_from_slice(&0x26000100u32.to_be_bytes());
                    m.bus.load_bios(&rom).unwrap();
                    let (grants, retired) = trace(&mut m, p, sample - 1, chunk, inspect);
                    assert_eq!(grants, [(0, 0x26000000, 4), (successor_if, 0x26000004, 4)]);
                    assert!(retired.is_empty());
                    assert_eq!(
                        m.bus
                            .transactions
                            .active
                            .as_ref()
                            .map(|a| (a.transfer.address, a.remaining)),
                        Some((0x26000004, 1))
                    );
                    // Change bytes before sampling and after completion. Neither
                    // sampled nor replacement instructions may retire after entry.
                    // Sample an illegal opcode, which must not raise an exception.
                    m.bus.write16(HIGH_WRAM + 4, 0xFFFF).unwrap();
                    trace(&mut m, p, sample, chunk, inspect);
                    assert!(m.bus.transactions.active.is_none());
                    assert!(
                        m.bus.transactions.journals[p as usize]
                            .borrow()
                            .beats
                            .iter()
                            .any(|(transfer, value)| transfer.address == 0x26000004
                                && *value == Ok(0xFFFF7763))
                    );
                    m.bus.write16(HIGH_WRAM + 4, 0x7863).unwrap();
                    let (grants, _) = trace(&mut m, p, 200, chunk, inspect);
                    assert_eq!(grants[0], (sample, HIGH_WRAM + 0xFFC, 4));
                    assert!(
                        !grants
                            .iter()
                            .any(|&(_, a, _)| a == 0x26000004 || a == 0x26000008)
                    );
                    let c = if p == Processor::Master {
                        m.cpu_state()
                    } else {
                        m.slave_state()
                    };
                    assert_eq!(&c.r[6..9], &[0, 0, 42]);
                    assert_eq!((c.instructions, c.exceptions), (3, 1));
                    assert_eq!(c.last_exception.as_ref().unwrap().vector, 0x20);
                    assert_eq!(c.last_exception.as_ref().unwrap().saved_pc, 0x26000002);
                    assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                    assert_eq!(m.bus.debug_read32(HIGH_WRAM + 0xFF8).unwrap(), 0x26000002);
                    assert_eq!(m.bus.debug_read32(HIGH_WRAM + 0xFFC).unwrap(), 0xF0);
                }
            }
        }
    }
}

#[test]
fn trapa_discards_an_unavailable_overrun_without_fetching_or_raising_an_address_error() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut m = board(p, 0, None, 0);
            // High RAM mirrors throughout CS3; use the end of that full route.
            let pc = 0x27FFFFFC;
            m.bus.write32(pc, 0xC3207663).unwrap();
            m.bus.write32(HIGH_WRAM + 0x100, 0xE82A001B).unwrap();
            let mut rom = vec![0; BIOS_SIZE];
            rom[0x80..0x84].copy_from_slice(&0x26000100u32.to_be_bytes());
            m.bus.load_bios(&rom).unwrap();
            let c = Cpu::start(pc, HIGH_WRAM + 0x1000);
            if p == Processor::Master {
                m.cpu = c;
            } else {
                m.slave = c;
            }
            // PC+4 falls outside the allowed instruction routes. The same
            // lookahead guard excludes MMIO; exception entry must discard its
            // deferred word instead of demanding an architectural fetch.
            assert!(!m.bus.for_cpu(p).sh2_speculative_fetch(pc + 4));
            let (grants, _) = trace(&mut m, p, 100, chunk, true);
            assert_eq!(&grants[..2], &[(0, pc, 4), (3, HIGH_WRAM + 0xFFC, 4)]);
            assert!(!grants.iter().any(|&(_, a, _)| a == pc + 4));
            let c = if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            };
            assert_eq!((c.instructions, c.exceptions), (3, 1));
            assert_eq!(c.last_exception.as_ref().unwrap().vector, 0x20);
            assert_eq!(c.last_exception.as_ref().unwrap().saved_pc, pc + 2);
            assert_eq!(&c.r[6..9], &[0, 0, 42]);
            assert!(!c.address_error_pending);
            assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
        }
    }
}

// P fig7.92: resident IF=0, ID=1, EX=2; the two MA slots end
// at 4/5 for a synchronous stack. External MA takes two states plus Tw,
// stretching each slot before target IF. Expected edges are literal.
#[test]
fn rte_target_if_follows_both_ma_slots_and_retains_completion_bytes_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (stack, waits, expected_grants, sample) in [
            (0xC0000400, 0, vec![(5, 0x26000100, 4)], 7),
            (
                0x26004000,
                0,
                vec![(3, 0x26004000, 4), (5, 0x26004004, 4), (7, 0x26000100, 4)],
                9,
            ),
            (
                0x26004000,
                0xC0,
                vec![(3, 0x26004000, 4), (11, 0x26004004, 4), (19, 0x26000100, 4)],
                27,
            ),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut m = board(p, 0, Some(true), waits);
                for (at, op) in [(0, 0x002B), (2, 0x7601), (4, 0x7763)] {
                    m.bus.for_cpu(p).write16(HIGH_WRAM + at, op).unwrap();
                }
                m.bus.for_cpu(p).write32(stack, 0x26000100).unwrap();
                m.bus.for_cpu(p).write32(stack + 4, 1).unwrap();
                m.bus.write32(HIGH_WRAM + 0x100, 0xE82A001B).unwrap();
                m.bus.cpu_delay(p, 0);
                let cpu = Cpu::start(HIGH_WRAM, stack);
                if p == Processor::Master {
                    m.cpu = cpu;
                } else {
                    m.slave = cpu;
                }
                let (grants, _) = trace(&mut m, p, sample - 1, chunk, true);
                assert_eq!(grants, expected_grants, "{p:?} stack={stack:X}");
                assert_eq!(
                    if p == Processor::Master {
                        m.cpu_state()
                    } else {
                        m.slave_state()
                    }
                    .instructions,
                    0
                );
                assert_eq!(
                    m.bus
                        .transactions
                        .active
                        .as_ref()
                        .map(|a| (a.transfer.address, a.remaining)),
                    Some((0x26000100, 1))
                );
                // Change the target while its IF still owns the bus, then
                // again at completion. Only the value sampled at completion
                // may execute; the second write must not snoop retained bytes.
                m.bus.write16(HIGH_WRAM + 0x100, 0xE82B).unwrap();
                trace(&mut m, p, sample, chunk, true);
                assert!(m.bus.transactions.active.is_none());
                m.bus.write16(HIGH_WRAM + 0x100, 0xE82C).unwrap();
                trace(&mut m, p, 100, chunk, true);
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!(
                    &c.r[6..9],
                    &[1, 0, 43],
                    "{p:?} stack={stack:X} waits={waits:X} chunk={chunk}"
                );
                assert_eq!((c.r[15], c.sr & 1), (stack + 8, 1));
                assert_eq!(c.exceptions, 0);
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
            }
        }
    }
}

// P fig7.94 permits only the immediate successor's IF. At 4n it is
// retained from the initial longword; at 4n+2 it needs one additional IF.
#[test]
fn sleep_fetches_only_its_immediate_successor_with_waits_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (offset, waits, expected_grants, retirement) in [
            (0, 0, vec![(0, 0x26000000, 4)], 3),
            (2, 0, vec![(0, 0x26000000, 4), (2, 0x26000004, 4)], 4),
            (0, 0xC0, vec![(0, 0x26000000, 4)], 9),
            (2, 0xC0, vec![(0, 0x26000000, 4), (8, 0x26000004, 4)], 16),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut m = board(p, offset, None, waits);
                m.bus.write16(HIGH_WRAM + offset, 0x001B).unwrap();
                m.bus.write16(HIGH_WRAM + offset + 2, 0x7663).unwrap();
                m.bus.write16(HIGH_WRAM + offset + 4, 0x7763).unwrap();
                let (grants, retired) = trace(&mut m, p, 40, chunk, true);
                assert_eq!(
                    grants, expected_grants,
                    "{p:?} offset={offset} waits={waits:X}"
                );
                assert_eq!(retired[0], (retirement, 1));
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                assert_eq!((c.instructions, &c.r[6..9]), (1, &[0, 0, 0][..]));
            }
        }
    }
}

#[test]
fn sleep_irq_wake_discards_the_prefetched_successor_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for (waits, successor_grant) in [(0, 3), (0xC0, 9)] {
            for chunk in [1, 3, 17, 10000] {
                let mut m = board(p, 0, None, waits);
                // LDC R0,SR; NOP; SLEEP; ADD #99,R6; ADD #99,R7.
                // Unmask IRQs and finish LDC's inhibited successor before sleep.
                for (i, op) in [0x400E, 9, 0x001B, 0x7663, 0x7763].into_iter().enumerate() {
                    m.bus.write16(HIGH_WRAM + i as u32 * 2, op).unwrap();
                }
                // Wake through a real FRT input-capture IRQ. The handler
                // computes a value and sleeps without executing the old stream.
                let mut rom = vec![0; BIOS_SIZE];
                rom[0x45 * 4..0x46 * 4].copy_from_slice(&0x26000300u32.to_be_bytes());
                m.bus.load_bios(&rom).unwrap();
                m.bus.write32(HIGH_WRAM + 0x300, 0xE82A001B).unwrap();
                m.bus
                    .for_cpu(p)
                    .write16(crate::onchip::IPRB, 0x0B00)
                    .unwrap();
                m.bus
                    .for_cpu(p)
                    .write16(crate::onchip::VCRC, 0x4500)
                    .unwrap();
                m.bus.for_cpu(p).write8(crate::onchip::FRT, 0x80).unwrap();
                let (grants, _) = trace(&mut m, p, 80, chunk, true);
                assert_eq!(
                    grants,
                    [(0, 0x26000000, 4), (successor_grant, 0x26000004, 4)]
                );
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!((c.instructions, c.exceptions, c.sr), (3, 0, 0));
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
                m.bus.onchip[p as usize].capture_strobe();
                let (grants, _) = trace(&mut m, p, 280, chunk, true);
                assert!(grants.iter().any(|&(_, address, _)| address == 0x26000300));
                assert!(!grants.iter().any(|&(_, address, _)| address == 0x26000008));
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!(&c.r[6..9], &[0, 0, 42]);
                assert_eq!(c.exceptions, 1);
                assert_eq!(c.last_exception.as_ref().unwrap().saved_pc, 0x26000006);
                assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
            }
        }
    }
}

#[test]
fn taken_untaken_delayed_call_return_and_rte_discard_only_the_documented_instructions() {
    for p in [Processor::Master, Processor::Slave] {
        for (words, expected) in [
            (
                vec![0x0018, 0x8901, 0xE663, 0xE763, 0xE82A, 0x001B],
                [0, 0, 42],
            ),
            (
                vec![0x0008, 0x8901, 0xE663, 0xE763, 0xE82A, 0x001B],
                [99, 99, 42],
            ),
            (
                vec![0xA002, 0x7601, 0x7763, 0x7763, 0xE82A, 0x001B],
                [1, 0, 42],
            ),
            (
                vec![
                    0xB004, 0x7601, 0xE82A, 0x001B, 9, 9, 0x7701, 0x000B, 0x7602, 0x7763,
                ],
                [3, 1, 42],
            ),
            (
                vec![0x002B, 0x7601, 0x7763, 0x7763, 0xE82A, 0x001B],
                [1, 0, 42],
            ),
        ] {
            let mut m = board(p, 0, None, 0);
            for (i, op) in words.into_iter().enumerate() {
                m.bus.write16(HIGH_WRAM + i as u32 * 2, op).unwrap();
            }
            m.bus.write32(HIGH_WRAM + 0x1000, 0x26000008).unwrap();
            m.bus.write32(HIGH_WRAM + 0x1004, 0).unwrap();
            trace(&mut m, p, 200, 3, true);
            let c = if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            };
            assert_eq!(&c.r[6..9], &expected);
            assert_eq!(c.status, crate::cpu::CpuStatus::Sleeping);
            assert_eq!(c.exceptions, 0);
        }
    }
}

#[test]
fn future_if_uses_existing_peer_dma_scu_and_refresh_grants_without_repeating_samples() {
    for p in [Processor::Master, Processor::Slave] {
        for competitor in 0..4 {
            let mut m = board(p, 0, None, 0);
            let other = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            m.bus.set_slave_enabled(true);
            for _ in 0..3 {
                tick(&mut m, p);
            }
            assert!(!m.issue_cpu(p).unwrap()); // PC+4 IF queued, older EX retained
            m.bus
                .for_cpu(other)
                .write32(0xFFFFFFE8, 0xA55A0000)
                .unwrap();
            m.bus.write32(HIGH_WRAM + 0x3000, 0x12345678).unwrap();
            let expected = match competitor {
                0 => {
                    assert_eq!(
                        m.bus.replay_cpu(other).read32(0x26003000),
                        Err(BusError::Pending)
                    );
                    // The first IF's owner was p; round-robin offers the peer.
                    Owner::Cpu(other)
                }
                1 => {
                    for (off, value) in [
                        (0, HIGH_WRAM + 0x3000),
                        (4, HIGH_WRAM + 0x4000),
                        (8, 1),
                        (12, 0xA05),
                        (0x30, 1),
                    ] {
                        m.bus
                            .for_cpu(other)
                            .write32(crate::onchip::DMAC + off, value)
                            .unwrap();
                    }
                    Owner::Dmac(other)
                }
                2 => {
                    for (off, value) in [
                        (0, HIGH_WRAM + 0x3000),
                        (4, HIGH_WRAM + 0x4000),
                        (8, 4),
                        (12, 0x102),
                        (20, 7),
                        (16, 0x101),
                    ] {
                        m.bus.write32(SCU + off, value).unwrap();
                    }
                    Owner::Scu
                }
                _ => {
                    let chip = &mut m.bus.onchip[0];
                    chip.write32(0xFFFFFFE0, 0xA55A0004).unwrap();
                    chip.write32(0xFFFFFFEC, 0xA55A0048).unwrap();
                    chip.write32(0xFFFFFFF8, 0xA55A0001).unwrap();
                    chip.write32(0xFFFFFFF4, 0xA55A0000).unwrap();
                    chip.write32(0xFFFFFFF0, 0xA55A0008).unwrap();
                    // Latch one refresh while the already queued IF remains ungranted.
                    m.bus.onchip[0].advance_clocks(4);
                    chip_stop_refresh(&mut m.bus);
                    Owner::Refresh
                }
            };
            m.bus.grant_bus_transaction();
            assert!(m.bus.transactions.active.as_ref().unwrap().owner == expected);
            assert_eq!(
                if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                }
                .instructions,
                0
            );
            // The completed peer read does not need a second physical access.
            let mut peer_done = false;
            while m.bus.main_clocks < 80 {
                if competitor == 0
                    && !peer_done
                    && m.bus.replay_cpu(other).read32(0x26003000) == Ok(0x12345678)
                {
                    peer_done = true;
                    m.bus.finish_cpu_transaction(other);
                }
                tick(&mut m, p);
            }
            if competitor == 0 {
                assert!(peer_done);
            }
            if competitor == 1 || competitor == 2 {
                assert_eq!(m.bus.read32(HIGH_WRAM + 0x4000), Ok(0x12345678));
            }
            let c = if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            };
            assert!(c.instructions > 4);
            assert_eq!(c.exceptions, 0);
        }
    }
}
fn chip_stop_refresh(b: &mut Bus) {
    b.onchip[0].read32(0xFFFFFFF0).unwrap();
    b.onchip[0].write32(0xFFFFFFF0, 0xA55A0000).unwrap();
}

#[test]
fn nmi_during_pending_if_waits_for_older_execution_and_discards_younger_decoded_words() {
    let p = Processor::Master;
    for arrival in [1, 2, 3, 4, 5] {
        let mut m = board(p, 0, None, 0);
        m.bus.write32(HIGH_WRAM, 0x76017763).unwrap();
        m.bus.write32(HIGH_WRAM + 4, 0x77637763).unwrap();
        m.bus.write32(HIGH_WRAM + 0x300, 0x7801001B).unwrap();
        let mut rom = vec![0; BIOS_SIZE];
        rom[44..48].copy_from_slice(&0x26000300u32.to_be_bytes());
        m.bus.load_bios(&rom).unwrap();
        while m.bus.main_clocks < arrival {
            tick(&mut m, p);
        }
        m.nmi_pending = true;
        m.bus.signal_nmi(p);
        trace(&mut m, p, 100, 3, true);
        assert_eq!(&m.cpu_state().r[6..9], &[1, 0, 1]);
        assert_eq!(m.cpu_state().exceptions, 1);
        assert_eq!(
            m.cpu_state().last_exception.as_ref().unwrap().saved_pc,
            0x26000002
        );
        assert_eq!(m.cpu_state().status, crate::cpu::CpuStatus::Sleeping);
    }
}

#[test]
fn watchdog_resets_discard_pending_and_completed_if_and_restart_each_cpu_from_new_vectors() {
    for p in [Processor::Master, Processor::Slave] {
        for manual in [false, true] {
            for edge in [1, 2, 3, 4, 5] {
                let mut m = board(p, 0, None, 0);
                m.bus.write32(HIGH_WRAM, 0x00090009).unwrap();
                m.bus.write16(HIGH_WRAM + 4, 0x2FF2).unwrap(); // must never publish
                m.bus.write32(HIGH_WRAM + 0x300, 0xE62A001B).unwrap();
                let mut rom = vec![0; BIOS_SIZE];
                for at in [0, 8] {
                    rom[at..at + 4].copy_from_slice(&0x26000300u32.to_be_bytes());
                    rom[at + 4..at + 8].copy_from_slice(&(HIGH_WRAM + 0x2000).to_be_bytes());
                }
                m.bus.load_bios(&rom).unwrap();
                while m.bus.main_clocks < edge {
                    tick(&mut m, p);
                }
                let chip = &mut m.bus.onchip[p as usize];
                chip.write16(crate::onchip::WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                    .unwrap();
                chip.write16(crate::onchip::WDT, 0xA578).unwrap();
                chip.write16(crate::onchip::WDT, 0x5AFF).unwrap();
                for _ in 0..200 {
                    m.step_hardware_boundary().unwrap();
                }
                let c = if p == Processor::Master {
                    m.cpu_state()
                } else {
                    m.slave_state()
                };
                assert_eq!(c.r[6], 42, "{p:?} manual{manual} edge{edge}");
                assert_eq!(c.r[15], HIGH_WRAM + 0x2000);
                assert_eq!(c.exceptions, 0);
                assert_eq!(m.bus.read32(HIGH_WRAM + 0x1000), Ok(0));
            }
        }
    }
}

#[test]
fn pll_preserves_master_fetch_journal_and_cancels_the_slave_domain() {
    let mut m = board(Processor::Master, 0, None, 0);
    m.bus.write32(HIGH_WRAM, 0xE62A001B).unwrap();
    tick(&mut m, Processor::Master);
    assert_eq!(m.cpu_state().instructions, 0);
    m.bus.change_system_clock(true);
    // This is the existing PLL reset contract: no master reset or replacement
    // journal. A byte already sampled in the old clock domain remains retained.
    trace(&mut m, Processor::Master, 100, 1, true);
    assert_eq!(m.cpu_state().r[6], 42);
    assert_eq!(m.cpu_state().exceptions, 0);
    assert!(!m.bus.slave_enabled);
}
