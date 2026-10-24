//! Literal boundaries derived before implementation in docs/sh2-load-pipeline.md.
use super::*;
use scheduling_tests::cached_pair;

fn advance_to(m: &mut Machine, clock: u64) {
    while m.clocks < clock {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.clocks, clock);
}
fn cpu(m: &Machine, p: Processor) -> &CpuState {
    if p == Processor::Master {
        m.cpu_state()
    } else {
        m.slave_state()
    }
}
fn board(p: Processor, offset: u32, address: u32, successor: u16) -> Machine {
    let mut words = vec![9; (offset / 2) as usize];
    words.extend([0x60F6, successor, 0xE65A, 0x001B, 9]); // MOV.L @R15+,R0
    let mut m = if p == Processor::Master {
        cached_pair(&words, &[9; 64])
    } else {
        cached_pair(&[9; 64], &words)
    };
    let c = Cpu::start(HIGH_WRAM + p as u32 * 0x100 + offset, address);
    if p == Processor::Master {
        m.cpu = c;
    } else {
        m.slave = c;
    }
    m
}

#[test]
fn mov_load_ma_sampling_and_combined_dependency_fetch_splits_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            for (address, completion, warm) in [
                (0xC0000400, 34, false),
                (HIGH_WRAM + 0x2400, 34, true),
                (onchip::DMAC, 35, false),
                (0x26002400, 35, false),
            ] {
                // NOP, source dependency, destination-only dependency.
                for (successor, dependent) in [(9, false), (0x7001, true), (0xE02A, true)] {
                    let mut m = board(p, offset, address, successor);
                    m.bus.for_cpu(p).write32(address, 0x12345678).unwrap();
                    if warm {
                        assert_eq!(m.bus.for_cpu(p).read32(address), Ok(0x12345678));
                    }
                    m.bus.cpu_delay(p, 0);
                    advance_to(&mut m, completion - 1);
                    assert_eq!(cpu(&m, p).instructions, 0, "early MOV at {address:08x}");
                    assert_eq!(cpu(&m, p).r[15], address);
                    assert!(!m.issue_cpu(p).unwrap());
                    m.bus.for_cpu(p).write32(address, 0x87654321).unwrap();
                    m.bus.cpu_delay(p, 0); // mutation is outside the guest instruction
                    assert!(!m.issue_cpu(p).unwrap());
                    advance_to(&mut m, completion);
                    assert!(m.issue_cpu(p).unwrap(), "late MOV at {address:08x}");
                    assert_eq!(cpu(&m, p).r[0], 0x87654321);
                    assert_eq!(cpu(&m, p).r[15], address + 4);
                    // Completed scheduled samples survive a same-edge mutation;
                    // synchronous reads were consumed by the replay just above.
                    m.bus.for_cpu(p).write32(address, 0x11223344).unwrap();
                    m.bus.cpu_delay(p, 0);
                    assert!(
                        m.issue_cpu(p).unwrap(),
                        "successor {p:?} off{offset} address{address:08x} op{successor:04x} t{} ready{:?}",
                        m.clocks,
                        m.cpu_ready
                    );
                    assert_eq!(
                        cpu(&m, p).r[0],
                        match successor {
                            0x7001 => 0x87654322,
                            0xE02A => 42,
                            _ => 0x87654321,
                        }
                    );
                    let split = offset != 0 || dependent;
                    assert_eq!(m.issue_cpu(p).unwrap(), !split, "incorrect combined split");
                    if split {
                        advance_to(&mut m, completion + 1);
                        assert!(m.issue_cpu(p).unwrap());
                    }
                    assert_eq!(cpu(&m, p).r[6], 0x5A);
                    assert_eq!(cpu(&m, p).r[15], address + 4);
                    assert_eq!(cpu(&m, p).exceptions, 0);
                }
            }
        }
    }
}

#[test]
fn mov_external_latency_stretches_ma_before_successor_ex() {
    // H §7.3: external T1/T2 plus WCR1 Tw. EX32, MA33, sample35+Tw.
    for p in [Processor::Master, Processor::Slave] {
        for (wait, completion) in [(0, 35), (0x40, 36), (0x80, 37), (0xC0, 41)] {
            for offset in [0, 2] {
                let address = 0x26002400;
                let mut m = board(p, offset, address, 9);
                m.bus
                    .for_cpu(p)
                    .write32(0xFFFFFFE8, 0xA55A0000 | wait)
                    .unwrap();
                m.bus.write32(address, 3).unwrap();
                advance_to(&mut m, completion - 1);
                assert_eq!(cpu(&m, p).instructions, 0);
                m.bus.write32(address, 7).unwrap();
                advance_to(&mut m, completion);
                assert!(m.issue_cpu(p).unwrap());
                assert_eq!(cpu(&m, p).r[0], 7);
                assert!(m.issue_cpu(p).unwrap());
                assert_eq!(m.issue_cpu(p).unwrap(), offset == 0);
                m.bus.write32(address, 11).unwrap();
                advance_to(&mut m, 50);
                assert_eq!(cpu(&m, p).r[0], 7);
            }
        }
    }
}

#[test]
fn reset_cancels_a_pending_mov_and_clears_its_overlap_on_each_cpu() {
    for p in [Processor::Master, Processor::Slave] {
        for reset in [onchip::ResetRequest::PowerOn, onchip::ResetRequest::Manual] {
            for edge in [33, 34] {
                let mut m = board(p, 0, 0x26002400, 9);
                m.bus.write32(0x26002400, 0x12345678).unwrap();
                advance_to(&mut m, edge);
                assert!(!m.issue_cpu(p).unwrap());
                assert_eq!(cpu(&m, p).instructions, 0);
                let mut rom = vec![0; bus::BIOS_SIZE];
                for at in [0, 8] {
                    rom[at..at + 4].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
                    rom[at + 4..at + 8].copy_from_slice(&(HIGH_WRAM + 0x4000).to_be_bytes());
                }
                m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
                m.bus.load_bios(&rom).unwrap();
                m.schedule_cpu_reset(p, reset);
                assert_eq!(cpu(&m, p), &CpuState::default());
                advance_to(&mut m, 60);
                assert_eq!(cpu(&m, p).r[0], 0);
                assert_eq!(cpu(&m, p).r[15], HIGH_WRAM + 0x4000);
                assert_eq!(cpu(&m, p).exceptions, 0);
            }
        }
    }
}

#[test]
fn mov_operand_mutations_before_at_and_after_sample_edges_follow_documented_ordering() {
    for p in [Processor::Master, Processor::Slave] {
        for (address, completion, synchronous) in [
            (0xC0000400, 34, true),
            (HIGH_WRAM + 0x2400, 34, true),
            (onchip::DMAC, 35, false),
            (0x26002400, 35, false),
        ] {
            for edge in [completion - 1, completion, completion + 1] {
                let mut m = board(p, 0, address, 9);
                m.bus.for_cpu(p).write32(address, 3).unwrap();
                if address == HIGH_WRAM + 0x2400 {
                    assert_eq!(m.bus.for_cpu(p).read32(address), Ok(3));
                }
                m.bus.cpu_delay(p, 0);
                advance_to(&mut m, edge);
                m.bus.for_cpu(p).write32(address, 7).unwrap();
                m.bus.cpu_delay(p, 0);
                advance_to(&mut m, 40);
                let changed = edge < completion || synchronous && edge == completion;
                assert_eq!(
                    cpu(&m, p).r[0],
                    if changed { 7 } else { 3 },
                    "{p:?} {address:08x} mutation{edge}"
                );
                assert_eq!(cpu(&m, p).r[15], address + 4);
            }
        }
    }
}

#[test]
fn nmi_arriving_during_mov_wait_is_accepted_after_the_retained_load() {
    let mut m = board(Processor::Master, 0, 0x26002400, 9);
    m.bus.write32(0x26002400, 7).unwrap();
    let mut rom = vec![0; bus::BIOS_SIZE];
    rom[44..48].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
    m.bus.load_bios(&rom).unwrap();
    m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
    advance_to(&mut m, 33);
    m.nmi_pending = true;
    m.bus.signal_nmi(Processor::Master);
    advance_to(&mut m, 35);
    assert!(m.issue_cpu(Processor::Master).unwrap());
    assert_eq!(m.cpu_state().r[0], 7);
    assert_eq!(m.cpu_state().exceptions, 0);
    for _ in 0..200 {
        if m.cpu_state().exceptions != 0 {
            break;
        }
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.cpu_state().r[0], 7);
    assert_eq!(
        m.cpu_state().last_exception,
        Some(cpu::Exception {
            vector: 11,
            saved_pc: HIGH_WRAM + 2
        })
    );
    assert_eq!(m.bus.read32(0x26002400 - 4), Ok(HIGH_WRAM + 2));
}

#[test]
fn mac_dependency_exception_forwards_the_loaded_address_without_an_extra_ex_split() {
    // Load EX32/MA33..34; aligned load permits MAC EX33 (word offset).
    // That MAC reads at35/36. A word-offset load already split IF/MA;
    // its aligned MAC starts EX34 and reads at36/38 (first-MA IF split).
    for p in [Processor::Master, Processor::Slave] {
        for (offset, completion) in [(0, 36), (2, 38)] {
            let mut m = board(p, offset, 0xC0000400, 0x00FF);
            for (address, value) in [(0xC0000400, 0xC0000600), (0xC0000404, 7), (0xC0000600, 3)] {
                m.bus.for_cpu(p).write32(address, value).unwrap();
            }
            m.bus.cpu_delay(p, 0);
            advance_to(&mut m, completion - 1);
            assert_eq!(cpu(&m, p).instructions, 1);
            assert_eq!(cpu(&m, p).macl, 0);
            assert_eq!(cpu(&m, p).r[0], 0xC0000600);
            advance_to(&mut m, completion);
            assert!(m.issue_cpu(p).unwrap());
            assert_eq!(cpu(&m, p).instructions, 2);
            assert_eq!(cpu(&m, p).macl, 21);
            assert_eq!(
                (cpu(&m, p).r[0], cpu(&m, p).r[15]),
                (0xC0000604, 0xC0000408)
            );
        }
    }
}

#[test]
fn cold_boot_and_sysres_cancel_mov_including_a_same_edge_completed_sample() {
    for p in [Processor::Master, Processor::Slave] {
        for sysres in [false, true] {
            let mut m = board(p, 0, 0x26002400, 9);
            m.bus.write32(0x26002400, 7).unwrap();
            m.mode = Some(BootMode::HlePrototype);
            if sysres {
                m.bus.write8(devices::SMPC + 0x63, 1).unwrap();
                m.bus.write8(devices::SMPC + 0x1F, 0x0D).unwrap();
                // Leave three clocks: EX plus the two-clock operand. Hardware
                // completes the sample at the SYSRES edge before CPU replay.
                let elapsed = smpc::Smpc::SYSTEM_RESET_CLOCKS - 3;
                assert_eq!(m.bus.advance_clocks(elapsed).unwrap(), None);
                m.clocks += u64::from(elapsed);
                m.cpu_ready = [m.bus.main_clocks; 2];
            }
            assert!(!m.issue_cpu(p).unwrap());
            assert_eq!(cpu(&m, p).instructions, 0);
            if sysres {
                let end = m.clocks + 3;
                advance_to(&mut m, end);
            } else {
                m.reset(Boot::HlePrototype).unwrap();
            }
            assert_eq!(m.cpu_state().r[0], 0);
            assert_eq!(m.slave_state().r[0], 0);
            assert_eq!(m.cpu_state().pc, FIXTURE_PC);
            assert_eq!(m.cpu_state().instructions, 0);
            m.step().unwrap();
            assert_eq!(m.cpu_state().r[0], 5); // New boot's real first MOV.
            assert_eq!(m.slave_state(), &CpuState::default());
        }
    }
}
