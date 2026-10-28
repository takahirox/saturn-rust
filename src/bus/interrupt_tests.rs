//! Independent literal phase oracles from H Rev4 figs5.7/5.10.
use super::*;
use crate::{
    Machine,
    cpu::{Cpu, CpuStatus},
    onchip,
};

#[test]
fn interrupt_entry_does_not_issue_a_stack_write_at_acceptance() {
    let mut b = Bus::default();
    let mut c = Cpu::start(0x200, HIGH_WRAM + 0x1000);
    assert!(matches!(
        c.accept_nmi(&mut b.replay_cpu(Processor::Master)),
        Err(crate::cpu::CpuError::ExceptionEntry {
            source: BusError::Pending,
            ..
        })
    ));
    assert!(
        b.transactions.journals[0].borrow().waiting.is_none()
            && b.transactions.journals[0].borrow().pending_write.is_none(),
        "Figure 5.7 requires D,E,E before the SR stack access"
    );
}

fn board(p: Processor, kind: u8, waits: u32) -> Machine {
    let mut m = Machine::new();
    let mut rom = vec![0; BIOS_SIZE];
    let vector = match kind {
        0 => 11,
        1 => 0x60,
        2 => 0x46,
        _ => 0x42,
    };
    rom[vector * 4..vector * 4 + 4].copy_from_slice(&0x26002000u32.to_be_bytes());
    m.bus.load_bios(&rom).unwrap();
    m.bus.set_slave_enabled(p == Processor::Slave);
    for (off, word) in [0xE000, 0x400E, 9, 0x001B].into_iter().enumerate() {
        m.bus.write16(HIGH_WRAM + off as u32 * 2, word).unwrap();
    }
    for off in (0..64).step_by(2) {
        m.bus.write16(HIGH_WRAM + 0x2000 + off, 9).unwrap();
    }
    let mut c = Cpu::start(HIGH_WRAM, 0x26001000);
    // Architectural setup only; the timed subject starts asleep at t=0.
    for _ in 0..4 {
        c.step(&mut m.bus).unwrap();
    }
    assert_eq!(c.state().status, CpuStatus::Sleeping);
    if p == Processor::Master {
        m.cpu = c;
    } else {
        m.slave = c;
    }
    m.cpu_ready[1 - p as usize] = u64::MAX;
    m.bus
        .for_cpu(p)
        .write32(0xFFFFFFE8, 0xA55A0000 | waits)
        .unwrap();
    match kind {
        0 => {
            m.nmi_pending = true;
            m.bus.signal_nmi(p);
        }
        1 => {
            let chip = &mut m.bus.onchip[p as usize];
            chip.write16(onchip::IPRB, 0x6000).unwrap();
            chip.write16(onchip::VCRB, 0x6000).unwrap(); // TXI
            chip.write8(onchip::SCI + 2, 0xA0).unwrap(); // TIE, TE; TDRE initially set
            assert_eq!(chip.interrupt(), Some((6, 0x60)));
        }
        _ => {
            if p == Processor::Master {
                m.bus.scu.write32(SCU + 0xA0, 0xBFFB).unwrap();
                m.bus.scu.signal(2);
                m.bus.onchip[0]
                    .write16(onchip::ICR, u16::from(kind == 3))
                    .unwrap();
            } else {
                m.bus.vdp2.hblank = true;
                // Slave IRL2's autovector is 65.
                let mut rom = vec![0; BIOS_SIZE];
                rom[65 * 4..65 * 4 + 4].copy_from_slice(&0x26002000u32.to_be_bytes());
                m.bus.load_bios(&rom).unwrap();
            }
        }
    }
    m.bus.cpu_delay(p, 0);
    m
}

#[derive(Debug, PartialEq, Eq)]
struct Trace {
    grants: Vec<(u64, u32, usize, bool)>,
    vectors: Vec<(u64, u8)>,
    clear: Vec<u64>,
    completed: Vec<u64>,
}

#[test]
fn production_scheduler_accepts_interrupt_detected_during_idle_batch_on_time() {
    for p in [Processor::Master, Processor::Slave] {
        for request_clock in 1..=7 {
            for inspect in [false, true] {
                let mut m = board(p, 1, 0);
                let chip = &mut m.bus.onchip[p as usize];
                chip.write8(onchip::SCI + 2, 0).unwrap();
                chip.write16(onchip::IPRB, 0x0600).unwrap();
                chip.write16(onchip::VCRC, 0x0060).unwrap();
                chip.write8(onchip::FRT + 4, 0).unwrap();
                chip.write8(onchip::FRT + 5, 0).unwrap(); // OCRA=0
                chip.write8(onchip::FRT, 8).unwrap(); // OCIA enabled
                chip.advance_clocks(8 - request_clock);
                assert_eq!(chip.interrupt(), None);

                let mut accepted = None;
                let mut sr_push_completed = None;
                while m.bus.main_clocks < 24 {
                    // Use the production issue/advance path. The trace helper
                    // polls issue_cpu at extra boundaries and hid this bug.
                    m.step_hardware_boundary().unwrap();
                    if inspect {
                        let _ = m.debug_read32(HIGH_WRAM);
                        let _ = m.cpu_state();
                        let _ = m.slave_state();
                    }
                    if let Some(boundary) = m.cpu_boundary[p as usize]
                        && boundary.irq.is_some()
                    {
                        accepted.get_or_insert(boundary.started);
                    }
                    let journal = m.bus.transactions.journals[p as usize].borrow();
                    if journal.beats.iter().any(|(transfer, result)| {
                        transfer.address == 0x26000FFC
                            && transfer.write == Some(0)
                            && result.is_ok()
                    }) {
                        sr_push_completed.get_or_insert(m.bus.main_clocks);
                    }
                }
                // H Table 5.8: recognition takes five clocks. Figure 5.7:
                // D,E,E precede the first stack access; WRAM takes two states.
                assert_eq!(accepted, Some(u64::from(request_clock + 5)), "{p:?}");
                assert_eq!(sr_push_completed, Some(u64::from(request_clock + 10)));
                assert_eq!(m.bus.read32(0x26000FFC), Ok(0));
                assert_eq!(m.bus.read32(0x26000FF8), Ok(HIGH_WRAM + 8));
            }
        }
    }
}
fn trace(m: &mut Machine, p: Processor, end: u64, chunk: u32, inspect: bool) -> Trace {
    let mut t = Trace {
        grants: vec![],
        vectors: vec![],
        clear: vec![],
        completed: vec![],
    };
    let mut last = None;
    while m.bus.main_clocks < end {
        let pending = m.nmi_pending;
        let done = m.issue_cpu(p).unwrap();
        if pending && !m.nmi_pending {
            t.clear.push(m.bus.main_clocks);
        }
        if done
            && (if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            })
            .exceptions
                != 0
        {
            t.completed.push(m.bus.main_clocks);
        }
        m.bus.grant_bus_transaction();
        if let Some(a) = &m.bus.transactions.active {
            let key = (a.owner, a.transfer.address, a.transfer.width);
            if last != Some(key) {
                if let Owner::Vector(_, level) = a.owner {
                    t.vectors.push((m.bus.main_clocks, level));
                } else {
                    t.grants.push((
                        m.bus.main_clocks,
                        a.transfer.address,
                        a.transfer.width,
                        a.transfer.write.is_some(),
                    ));
                }
                last = Some(key);
            }
        } else {
            last = None;
        }
        if inspect {
            let _ = m.debug_read32(HIGH_WRAM);
            let _ = m.cpu_state();
            let _ = m.slave_state();
        }
        let n = chunk
            .min((end - m.bus.main_clocks) as u32)
            .min(m.bus.bus_main_deadline())
            .min(m.bus.interrupt_deadline())
            .min(
                m.cpu_ready[p as usize]
                    .saturating_sub(m.bus.main_clocks)
                    .max(1)
                    .min(u64::from(u32::MAX)) as u32,
            )
            .min(if m.nmi_pending {
                m.nmi_detected.map_or(1, |r| {
                    (r + 2).saturating_sub(m.bus.main_clocks).max(1) as u32
                })
            } else {
                u32::MAX
            })
            .max(1);
        let (r, command) = m.bus.advance_cpu_clocks(n).unwrap();
        assert!(command.is_none());
        m.clocks += u64::from(r);
        if m.bus.take_cpu_transaction_completion(p) {
            m.cpu_ready[p as usize] = m.bus.main_clocks;
        }
    }
    t
}

#[test]
fn literal_interrupt_phases_nmi_internal_and_irl_on_both_processors() {
    for p in [Processor::Master, Processor::Slave] {
        for kind in 0..=3 {
            if p == Processor::Slave && (kind == 0 || kind == 3) {
                continue;
            }
            for chunk in [1, 3, 17, 10000] {
                for inspect in [false, true] {
                    let mut m = board(p, kind, 0);
                    let a = if kind == 0 { 2 } else { 5 };
                    let vector: u8 = if kind == 0 {
                        11
                    } else if kind == 1 {
                        0x60
                    } else if p == Processor::Slave {
                        65
                    } else if kind == 3 {
                        0x42
                    } else {
                        0x46
                    };
                    let extra = if kind == 3 { 2 } else { 0 };
                    let t = trace(&mut m, p, a + 16 + extra, chunk, inspect);
                    assert_eq!(
                        t.grants[..4],
                        [
                            (a + 3, 0x26000FFC, 4, true),
                            (a + 5, 0x26000FF8, 4, true),
                            (a + 8 + extra, u32::from(vector) * 4, 2, false),
                            (a + 10 + extra, u32::from(vector) * 4 + 2, 2, false)
                        ],
                        "p={p:?} kind={kind} {t:?}"
                    );
                    // CS0 is physically 16-bit: vector read m3=4, versus WRAM m1=m2=2.
                    assert_eq!(t.grants[4], (a + 13 + extra, 0x26002000, 4, false), "{t:?}");
                    assert_eq!(
                        t.vectors,
                        if kind == 3 { vec![(a + 8, 13)] } else { vec![] }
                    );
                    assert_eq!(t.clear, if kind == 0 { vec![3] } else { vec![] });
                    assert_eq!(m.bus.read32(0x26000FF8), Ok(HIGH_WRAM + 8));
                    assert_eq!(m.bus.read32(0x26000FFC), Ok(0));
                }
            }
        }
    }
}

#[test]
fn nmi_edges_on_either_side_of_clear_are_coalesced_or_retained_during_waited_entry() {
    for edge in [2, 4, 6, 12] {
        let mut m = board(Processor::Master, 0, 0xC0);
        trace(&mut m, Processor::Master, edge, 1, false);
        m.nmi_pending = true;
        m.bus.signal_nmi(Processor::Master);
        trace(&mut m, Processor::Master, 35, 1, false);
        assert!(m.issue_cpu(Processor::Master).unwrap());
        assert_eq!(m.cpu_state().exceptions, 1);
        assert_eq!(m.nmi_pending, edge > 3, "edge={edge}");
        assert_eq!(m.bus.onchip[0].read32(onchip::DMAC + 0x30).unwrap() & 2, 2);
    }
}

#[test]
fn external_acquisition_uses_accepted_level_and_acknowledges_once_at_completion() {
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let p = Processor::Master;
            let mut m = board(p, 3, 0xC0); // accepted HBlank IRL13, long stack waits
            trace(&mut m, p, 12, chunk, inspect);
            m.bus.scu.write32(SCU + 0xA0, 0xBFF8).unwrap();
            m.bus.scu.signal(0); // newly winning IRL15 must not supply IRL13's vector
            trace(&mut m, p, 26, chunk, inspect);
            // A=5, pushes 8..16 and 16..24, E24..25, IVECF25..27.
            let a = m.bus.transactions.active.as_ref().unwrap();
            assert!(matches!(a.owner, Owner::Vector(Processor::Master, 13)));
            assert_eq!(a.remaining, 1);
            assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 5, 5);
            trace(&mut m, p, 27, chunk, inspect);
            assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 5, 1);
            // A second same-source edge after acquisition must survive all entry
            // replays and the waited vector/handler fetch.
            m.bus.scu.signal(2);
            m.bus.write32(SCU + 0xA0, 0xBFFF).unwrap(); // retain pending bits, mask nesting
            trace(&mut m, p, 44, chunk, inspect);
            assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 0x42);
            assert_eq!(m.cpu_state().sr & 0xF0, 0xD0);
            assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 5, 5);
            let mut memory = m.bus.replay_cpu(p);
            // Entry journal is already retired: do not call a new acquisition.
            assert_eq!(memory.sh2_pipeline_wait(0), Ok(()));
        }
    }
}

#[test]
fn accepted_level_selects_a_live_same_level_source_and_keeps_other_levels_pending() {
    let p = Processor::Master;
    let mut m = board(p, 3, 0);
    m.bus.scu.acknowledge(0x42);
    m.bus.scu.write32(SCU + 0xA0, 0xBE7F).unwrap();
    m.bus.scu.signal(8); // PAD level8
    // Install the independent level8 vectors in the physical vector table.
    let mut rom = vec![0; BIOS_SIZE];
    for v in [0x47, 0x48] {
        rom[v * 4..v * 4 + 4].copy_from_slice(&0x26002000u32.to_be_bytes());
    }
    m.bus.load_bios(&rom).unwrap();
    trace(&mut m, p, 10, 1, false);
    m.bus.scu.signal(7); // equal-level, lower-vector convention at acquisition
    trace(&mut m, p, 16, 1, false);
    assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 0x180, 0x100);
    m.bus.scu.write32(SCU + 0xA0, 0xBFFF).unwrap();
    trace(&mut m, p, 23, 1, false);
    assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 0x47);
}

#[test]
fn vector_cycle_ignores_programmable_waits_and_replay_never_reacquires() {
    for waits in [0, 0xAAFF] {
        let p = Processor::Master;
        let mut b = Bus::default();
        b.onchip[0].write16(onchip::ICR, 1).unwrap();
        b.onchip[0].write32(0xFFFFFFE8, 0xA55A0000 | waits).unwrap();
        b.scu.write32(SCU + 0xA0, 0xBFFB).unwrap();
        b.scu.signal(2);
        let request = b.interrupt_request(p).unwrap();
        b.prepare_interrupt(p, Some(request));
        assert_eq!(
            b.replay_cpu(p).sh2_interrupt_vector(13, 0x42),
            Err(BusError::Pending)
        );
        b.grant_bus_transaction();
        assert!(matches!(
            b.transactions.active.as_ref().unwrap().owner,
            Owner::Vector(_, 13)
        ));
        assert_eq!(b.transactions.active.as_ref().unwrap().remaining, 2);
        b.advance_clocks(1).unwrap();
        assert_eq!(b.scu.read32(SCU + 0xA4).unwrap() & 4, 4);
        b.advance_clocks(1).unwrap();
        assert_eq!(b.scu.read32(SCU + 0xA4).unwrap() & 4, 0);
        b.scu.signal(2);
        for _ in 0..5 {
            assert_eq!(b.replay_cpu(p).sh2_interrupt_vector(13, 0x42), Ok(0x42));
        }
        assert_eq!(b.scu.read32(SCU + 0xA4).unwrap() & 4, 4);
        assert_eq!(b.transactions.journals[0].borrow().operations.len(), 1);
    }
}

#[test]
fn reset_at_each_interrupt_phase_cancels_only_unfinished_work_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for edge in [5, 6, 8, 9, 10, 12, 13, 14, 16, 17] {
            let mut m = board(p, 1, 0);
            trace(&mut m, p, edge, 1, false);
            let sr = m.bus.read32(0x26000FFC).unwrap();
            let pc = m.bus.read32(0x26000FF8).unwrap();
            m.schedule_cpu_reset(p, onchip::ResetRequest::PowerOn);
            assert!(m.cpu_boundary[p as usize].is_none());
            assert!(
                m.bus.transactions.journals[p as usize]
                    .borrow()
                    .operations
                    .is_empty()
            );
            m.bus.advance_clocks(100).unwrap();
            assert_eq!(m.bus.read32(0x26000FFC), Ok(sr));
            assert_eq!(m.bus.read32(0x26000FF8), Ok(pc));
            assert!(!m.bus.take_cpu_transaction_completion(p));
        }
    }
}

#[test]
fn completed_stack_effects_are_not_repeated_and_other_hardware_keeps_advancing() {
    for p in [Processor::Master, Processor::Slave] {
        let mut m = board(p, 1, 0xC0);
        m.bus.set_slave_enabled(true);
        trace(&mut m, p, 17, 1, false); // SR push completed at16, PC write waits until24
        m.bus.write32(0x26000FFC, 0x12345678).unwrap();
        let before = m.bus.onchip[1 - p as usize].read8(onchip::FRT + 3).unwrap();
        trace(&mut m, p, 45, 1, true);
        assert_eq!(m.bus.read32(0x26000FFC), Ok(0x12345678));
        assert_eq!(
            (if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            })
            .exceptions,
            1
        );
        let _ = m.bus.onchip[1 - p as usize].read8(onchip::FRT + 2).unwrap();
        assert_ne!(
            m.bus.onchip[1 - p as usize].read8(onchip::FRT + 3).unwrap(),
            before
        );
    }
}

#[test]
fn autovector_ack_uses_the_accepted_source_instead_of_the_current_winner() {
    let p = Processor::Master;
    let mut m = board(p, 2, 0xC0);
    trace(&mut m, p, 12, 1, false);
    m.bus.scu.write32(SCU + 0xA0, 0xBFFA).unwrap();
    m.bus.scu.signal(0);
    trace(&mut m, p, 26, 1, false);
    assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 5, 1);
    m.bus.scu.signal(2);
    m.bus.scu.write32(SCU + 0xA0, 0xBFFF).unwrap();
    trace(&mut m, p, 42, 1, false);
    assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 0x46);
    assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 5, 5);
}

fn masked_scu_request_survives_before_execution_break(external: bool) {
    // H §§5.4/6.3.2: recognition does not imply acceptance. IRL13 is
    // masked by SR.I=14, while the native before-execution UBC is level15.
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let p = Processor::Master;
            let mut m = board(p, if external { 3 } else { 2 }, 0xC0);
            let mut rom = vec![0; BIOS_SIZE];
            rom[12 * 4..12 * 4 + 4].copy_from_slice(&0x26002000u32.to_be_bytes());
            m.bus.load_bios(&rom).unwrap();
            for (off, word) in [0xE0E0, 0x400E, 9, 9].into_iter().enumerate() {
                m.bus.write16(HIGH_WRAM + off as u32 * 2, word).unwrap();
            }
            let mut c = Cpu::start(HIGH_WRAM, 0x26001000);
            for _ in 0..3 {
                c.step(&mut m.bus).unwrap();
            }
            let pc = HIGH_WRAM + 6;
            assert_eq!(c.state().pc, pc);
            assert_eq!(c.state().sr & 0xF0, 0xE0);
            m.cpu = c;
            m.cpu_ready[0] = 5; // allow the masked IRL13 request to be recognized
            let chip = &mut m.bus.onchip[0];
            chip.write32(0xFFFFFF40, pc).unwrap();
            chip.write16(0xFFFFFF48, 0x54).unwrap(); // CPU instruction read
            chip.write16(0xFFFFFF78, 0x1000).unwrap(); // native, before execution
            chip.read16(0xFFFFFF78).unwrap(); // configuration barrier
            assert!(!chip.break_pending());
            trace(&mut m, p, 5, chunk, inspect);
            assert_eq!(m.bus.recognized_interrupt(p).unwrap().level, 13);
            let t = trace(&mut m, p, 70, chunk, inspect);
            assert!(t.vectors.is_empty(), "UBC must not issue IVECF: {t:?}");
            assert_eq!(m.cpu_state().exceptions, 1);
            assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 12);
            assert_eq!(m.bus.scu.read32(SCU + 0xA4).unwrap() & 4, 4);
            assert_eq!(m.bus.read32(0x26000FFC), Ok(0x3E0));
            assert_eq!(m.bus.read32(0x26000FF8), Ok(pc));
            assert!(!m.bus.onchip[0].break_pending());
            assert!(t.grants.iter().any(|&(_, a, _, w)| a == 48 && !w));
            assert!(t.grants.iter().any(|&(_, a, _, w)| a == 0x26002000 && !w));
        }
    }
}

#[test]
fn masked_scu_external_vector_request_cannot_supply_a_native_ubc_vector() {
    masked_scu_request_survives_before_execution_break(true);
}

#[test]
fn masked_scu_autovector_request_is_not_acknowledged_by_a_native_ubc() {
    masked_scu_request_survives_before_execution_break(false);
}

#[test]
fn same_level_scu_changes_do_not_restart_irl_recognition_and_external_wins_internal_ties() {
    let p = Processor::Master;
    let mut m = board(p, 1, 0);
    // Internal TXI6 versus SCU DMA1-end6: external wins (H table5.4).
    m.bus.scu.write32(SCU + 0xA0, 0xB9FF).unwrap();
    m.bus.scu.signal(10);
    m.bus.sample_interrupts();
    m.bus.advance_clocks(4).unwrap();
    assert!(m.bus.recognized_interrupt(p).is_none());
    m.bus.scu.signal(9);
    m.bus.sample_interrupts(); // still IRL6
    m.bus.advance_clocks(1).unwrap();
    let r = m.bus.recognized_interrupt(p).unwrap();
    assert_eq!(
        (r.level, r.vector, r.source),
        (6, 67, InterruptSource::Scu(0x49))
    );
}

#[test]
fn peer_cpu_dma_and_refresh_delay_entry_without_reordering_stack_or_vector_phases() {
    for p in [Processor::Master, Processor::Slave] {
        for contender in 0..3 {
            let mut m = board(p, 1, 0);
            m.bus.set_slave_enabled(true);
            trace(&mut m, p, 8, 1, false); // recognition5 + D,E,E; first SR request next
            let peer = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            match contender {
                0 => {
                    m.bus.onchip[peer as usize]
                        .write32(0xFFFFFFE8, 0xA55A0000)
                        .unwrap();
                    assert_eq!(
                        m.bus.replay_cpu(peer).read32(0x26003000),
                        Err(BusError::Pending)
                    );
                    m.bus.transactions.previous = Some(p);
                }
                1 => {
                    for (off, val) in [
                        (0, 0x26003000),
                        (4, 0x26004000),
                        (8, 1),
                        (12, 0x5A05),
                        (0x30, 1),
                    ] {
                        m.bus.onchip[peer as usize]
                            .write32(onchip::DMAC + off, val)
                            .unwrap();
                    }
                }
                _ => {
                    let chip = &mut m.bus.onchip[0];
                    for (a, v) in [
                        (0xFFFFFFE0, 0xA55A0004),
                        (0xFFFFFFEC, 0xA55A0048),
                        (0xFFFFFFF8, 0xA55A0001),
                        (0xFFFFFFF4, 0xA55A0000),
                        (0xFFFFFFF0, 0xA55A0008),
                    ] {
                        chip.write32(a, v).unwrap();
                    }
                    chip.advance_clocks(8); // latch one six-state refresh
                    chip.read32(0xFFFFFFF0).unwrap();
                    chip.write32(0xFFFFFFF0, 0xA55A0000).unwrap();
                }
            }
            let t = trace(&mut m, p, 50, 1, true);
            let writes: Vec<_> = t
                .grants
                .iter()
                .filter(|(_, a, _, w)| *w && [0x26000FFC, 0x26000FF8].contains(a))
                .copied()
                .collect();
            assert_eq!(writes.len(), 2, "{t:?}");
            assert!(writes[0].0 > 8, "contender={contender} {t:?}");
            assert_eq!(writes[0].1, 0x26000FFC);
            assert_eq!(writes[1].1, 0x26000FF8);
            if contender == 0 {
                assert_eq!((writes[0].0, writes[1].0), (10, 12));
            }
            if contender == 2 {
                assert_eq!((writes[0].0, writes[1].0), (14, 16));
            }
            assert_eq!(m.bus.read32(0x26000FF8), Ok(HIGH_WRAM + 8));
        }
    }
}

#[test]
fn nmi_entry_microsequence_is_identical_on_both_cpu_bus_adapters() {
    // CPU pin-level test only: Saturn's SMPC route remains master-only.
    for p in [Processor::Master, Processor::Slave] {
        let mut m = board(p, 1, 0);
        let mut rom = vec![0; BIOS_SIZE];
        rom[44..48].copy_from_slice(&0x26002000u32.to_be_bytes());
        m.bus.load_bios(&rom).unwrap();
        let original = if p == Processor::Master {
            m.cpu.clone()
        } else {
            m.slave.clone()
        };
        let mut grants = vec![];
        let mut last = None;
        let mut clear = None;
        for now in 0..16 {
            let mut c = original.clone();
            let result = c.accept_nmi(&mut m.bus.replay_cpu(p));
            if m.bus.take_nmi_clear(p) {
                assert!(clear.is_none());
                clear = Some(now);
            }
            if result == Ok(true) {
                assert_eq!(now, 15);
                assert_eq!(c.state().sr & 0xF0, 0xF0);
                assert_eq!(c.state().pc, 0x26002000);
                assert_eq!(
                    grants,
                    [
                        (3, 0x26000FFC),
                        (5, 0x26000FF8),
                        (8, 44),
                        (10, 46),
                        (13, 0x26002000)
                    ]
                );
                break;
            }
            m.bus.grant_bus_transaction();
            if let Some(a) = &m.bus.transactions.active {
                let key = (a.owner, a.transfer.address);
                if last != Some(key) {
                    grants.push((now, a.transfer.address));
                    last = Some(key);
                }
            } else {
                last = None;
            }
            m.bus.advance_clocks(1).unwrap();
        }
        assert_eq!(clear, Some(1));
        assert_eq!(m.bus.read32(0x26000FF8), Ok(HIGH_WRAM + 8));
    }
}

#[test]
fn peripheral_vector_twelve_is_not_mistaken_for_the_user_break_source() {
    let p = Processor::Master;
    let mut m = board(p, 1, 0);
    m.bus.onchip[0].write16(onchip::IPRB, 0xF000).unwrap();
    m.bus.onchip[0].write16(onchip::VCRB, 0x0C00).unwrap();
    assert!(!m.bus.for_cpu(p).break_pending());
    assert_eq!(
        m.bus.interrupt_request(p).unwrap().source,
        InterruptSource::Internal
    );
    m.bus.scu.write32(SCU + 0xA0, 0xBFFE).unwrap();
    m.bus.scu.signal(0);
    assert_eq!(
        m.bus.interrupt_request(p).unwrap().source,
        InterruptSource::Scu(0x40)
    );
}
