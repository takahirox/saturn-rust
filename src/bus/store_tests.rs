//! Executing CPU/bus boundaries, independently derived in sh2-store-pipeline.md.
use super::*;
use crate::{
    Machine,
    cpu::{Cpu, CpuState},
    scheduling_tests::cached_pair,
};
use crate::{onchip, vdp2::CRAM};

fn state(m: &Machine, p: Processor) -> &CpuState {
    if p == Processor::Master {
        m.cpu_state()
    } else {
        m.slave_state()
    }
}
fn board(p: Processor, offset: u32, address: u32, successor: u16) -> Machine {
    let mut words = vec![9; offset as usize / 2];
    words.extend([0x2FF6, successor, 0xE65A, 0x001B, 9]); // MOV.L R15,@-R15
    let mut m = if p == Processor::Master {
        cached_pair(&words, &[0xAFFE, 9])
    } else {
        cached_pair(&[0xAFFE, 9], &words)
    };
    m.bus.onchip[p as usize]
        .write32(0xFFFFFFE8, 0xA55A0030)
        .unwrap();
    m.bus.onchip[p as usize]
        .write32(0xFFFFFFEC, 0xA55A0000)
        .unwrap();
    let c = Cpu::start(HIGH_WRAM + p as u32 * 0x100 + offset, address + 4);
    if p == Processor::Master {
        m.cpu = c;
    } else {
        m.slave = c;
    }
    m
}
fn advance(m: &mut Machine, end: u64) {
    while m.clocks < end {
        let mut committed = false;
        for p in [Processor::Master, Processor::Slave] {
            committed |= m.issue_cpu(p).unwrap();
        }
        m.bus.grant_bus_transaction();
        if committed && m.cpu_ready.iter().any(|&ready| ready <= m.clocks) {
            continue;
        }
        m.bus.advance_clocks(1).unwrap();
        m.clocks = m.bus.main_clocks;
        for p in [Processor::Master, Processor::Slave] {
            if m.bus.take_cpu_transaction_completion(p) {
                m.cpu_ready[p as usize] = m.clocks;
            }
        }
    }
}
fn advance_bus(m: &mut Machine, clocks: u32, chunk: u32, inspect: bool) {
    let mut left = clocks;
    while left > 0 {
        let n = left.min(chunk);
        m.bus.advance_clocks(n).unwrap();
        m.clocks = m.bus.main_clocks;
        left -= n;
        if inspect {
            let _ = m.bus.debug_read32(HIGH_WRAM + 0x2400);
            let _ = m.bus.debug_read16(VDP2 + 4);
            let _ = m.cpu_state();
        }
    }
}

#[test]
fn executing_stores_request_at_ma_and_accept_before_external_publication_on_both_cpus() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            for (address, warm, publication) in [
                (0xC0000400, false, 34),        // cache data array
                (HIGH_WRAM + 0x2400, true, 35), // write-through hit
                (0x26002400, false, 35),        // cache-through
                (onchip::DMAC, false, 34),      // on-chip MMIO
                (CRAM | 0x20000000, false, 49), // B-bus two words
            ] {
                let payload = address + 4;
                let mut m = board(p, offset, address, 0xEF55);
                m.bus.for_cpu(p).write32(address, 0xAABBCCDD).unwrap();
                if warm {
                    assert_eq!(m.bus.for_cpu(p).read32(address), Ok(0xAABBCCDD));
                }
                m.bus.cpu_delay(p, 0);
                assert!(!m.issue_cpu(p).unwrap());
                // EX has prepared address/data but no write request was admitted.
                assert!(m.bus.transactions.writes[p as usize].is_none());
                assert!(
                    m.bus.transactions.journals[p as usize]
                        .borrow()
                        .pending_write
                        .is_none()
                );
                assert_eq!(state(&m, p).r[15], payload);
                advance(&mut m, 33);
                assert!(!m.issue_cpu(p).unwrap());
                m.bus.grant_bus_transaction();
                if publication > 34 {
                    let b = m.bus.transactions.writes[p as usize].as_ref().unwrap();
                    assert_eq!(b.beats.front().unwrap().address, address);
                    assert_eq!(
                        b.beats.front().unwrap().bridge_write.unwrap_or(payload),
                        payload
                    );
                }
                assert_eq!(state(&m, p).instructions, 0);
                advance(&mut m, 34);
                assert!(m.issue_cpu(p).unwrap());
                assert_eq!(state(&m, p).r[15], address);
                assert_eq!(state(&m, p).sr, 0xF0);
                assert_eq!(state(&m, p).instructions, 1);
                if publication > 34 {
                    assert_eq!(m.bus.read32(address), Ok(0xAABBCCDD));
                }
                // Both independent and address-dependent successors can share MA:
                // stores have no WB. A real IF split prevents third-EX overlap.
                assert!(m.issue_cpu(p).unwrap());
                assert_eq!(state(&m, p).r[15], 0x55);
                assert_eq!(m.issue_cpu(p).unwrap(), offset == 0);
                if offset != 0 {
                    advance(&mut m, 35);
                    assert!(m.issue_cpu(p).unwrap());
                }
                assert_eq!(state(&m, p).r[6], 0x5A);
                advance(&mut m, publication.max(35));
                assert_eq!(
                    m.bus.for_cpu(p).read32(address),
                    Ok(payload),
                    "{p:?} {address:08x}"
                );
                assert_eq!(state(&m, p).exceptions, 0);
            }
        }
    }
}

#[test]
fn executing_video_store_retains_each_half_and_trace_across_chunks_and_inspection() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let address = CRAM | 0x20000000;
                let mut m = board(p, 0, address, 0xEF55);
                m.bus.write32(address, 0xAABBCCDD).unwrap();
                assert!(!m.issue_cpu(p).unwrap());
                advance(&mut m, 34);
                assert!(m.issue_cpu(p).unwrap());
                assert!(m.issue_cpu(p).unwrap());
                // No interpreter replay is needed to publish the accepted data.
                advance_bus(&mut m, 6, chunk, inspect); //40
                assert_eq!(m.bus.read32(address), Ok(0xAABBCCDD));
                advance_bus(&mut m, 1, chunk, inspect); //41
                assert_eq!(m.bus.read32(address), Ok(0x25F0CCDD));
                m.bus.write16(address, 0x7788).unwrap();
                advance_bus(&mut m, 7, chunk, inspect); //48
                assert_eq!(m.bus.read32(address), Ok(0x7788CCDD));
                advance_bus(&mut m, 1, chunk, inspect); //49
                assert_eq!(m.bus.read32(address), Ok(0x77880004));
                assert!(m.bus.transactions.writes[p as usize].is_none());
            }
        }
    }
}

#[test]
fn store_buffer_backpressure_retains_payload_then_external_read_is_a_publication_barrier() {
    for p in [Processor::Master, Processor::Slave] {
        let a = 0x26002400;
        let mut m = board(p, 0, a, 0x2FF6);
        // Replace the third instruction with an external read through decremented SP.
        let base = HIGH_WRAM + p as u32 * 0x100;
        m.bus.write16(base + 4, 0x60F2).unwrap();
        m.bus.for_cpu(p).write8(CCR, 0x11).unwrap();
        m.bus.for_cpu(p).fetch16(base).unwrap();
        m.bus.cpu_delay(p, 0);
        m.bus.onchip[p as usize]
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        // Host cache warming reserves physical read timing. Drain it before
        // the independent E=64 kernel: H §7.9's read-to-write gap is satisfied.
        advance_bus(&mut m, 32, 17, false);
        m.cpu_ready = [m.clocks; 2];
        let origin = m.clocks - 32;
        advance(&mut m, origin + 34);
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).instructions, 1);
        assert!(!m.issue_cpu(p).unwrap());
        advance(&mut m, origin + 40);
        assert_eq!(state(&m, p).instructions, 1);
        assert_eq!(state(&m, p).r[15], a);
        assert_eq!(m.bus.read32(a), Ok(0));
        assert_eq!(m.bus.read32(a - 4), Ok(0));
        advance(&mut m, origin + 41);
        assert_eq!(
            m.bus.read32(a),
            Ok(a + 4),
            "{p:?} bus{} active{:?}",
            m.bus.main_clocks,
            m.bus
                .transactions
                .active
                .as_ref()
                .map(|v| (v.transfer.address, v.remaining))
        );
        assert_eq!(state(&m, p).instructions, 1);
        advance(&mut m, origin + 42);
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).r[15], a - 4);
        assert_eq!(state(&m, p).instructions, 2);
        assert!(!m.issue_cpu(p).unwrap());
        advance(&mut m, origin + 48);
        assert_eq!(state(&m, p).instructions, 2);
        assert_eq!(m.bus.read32(a - 4), Ok(0));
        advance(&mut m, origin + 49);
        assert_eq!(m.bus.read32(a - 4), Ok(a));
        // WCR1 applies to the read too: write49, barrier read49..57.
        advance(&mut m, origin + 56);
        assert_eq!(state(&m, p).instructions, 2);
        advance(&mut m, origin + 57);
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).r[0], a);
        assert_eq!(state(&m, p).r[15], a - 4);
        let peer = if p == Processor::Master {
            m.slave_state()
        } else {
            m.cpu_state()
        };
        assert!(peer.instructions >= 10);
    }
}

#[test]
fn store_address_array_ram_and_purge_effects_wait_for_synchronous_ma_end() {
    for p in [Processor::Master, Processor::Slave] {
        for address in [0x60000400, 0xC0000400, 0x46002400] {
            let mut m = board(p, 0, address, 9);
            m.bus.for_cpu(p).write8(CCR, 0x19).unwrap();
            m.bus
                .for_cpu(p)
                .fetch16(HIGH_WRAM + p as u32 * 0x100)
                .unwrap();
            m.bus.cpu_delay(p, 0);
            if address == 0x46002400 {
                m.bus.write32(0x06002400, 7).unwrap();
                m.bus.for_cpu(p).read32(0x06002400).unwrap();
                m.bus.cpu_delay(p, 0);
                assert!(m.bus.cache[p as usize].borrow().read_hit(0x06002400));
            }
            let before = m.bus.for_cpu(p).read32(address).ok();
            m.bus.cpu_delay(p, 0);
            advance(&mut m, 33);
            assert!(!m.issue_cpu(p).unwrap());
            if address == 0x46002400 {
                assert!(m.bus.cache[p as usize].borrow().read_hit(0x06002400));
            } else {
                assert_eq!(m.bus.for_cpu(p).read32(address).ok(), before);
            }
            advance(&mut m, 34);
            assert!(m.issue_cpu(p).unwrap());
            match address {
                0x60000400 => assert_eq!(m.bus.for_cpu(p).read32(address), Ok(0x400)),
                0xC0000400 => assert_eq!(m.bus.for_cpu(p).read32(address), Ok(address + 4)),
                _ => assert!(!m.bus.cache[p as usize].borrow().read_hit(0x06002400)),
            }
            assert_eq!(state(&m, p).r[15], address);
        }
    }
}

#[test]
fn reset_before_store_admission_cancels_and_manual_reset_after_acceptance_drains() {
    for p in [Processor::Master, Processor::Slave] {
        for reset in [onchip::ResetRequest::Manual, onchip::ResetRequest::PowerOn] {
            for edge in [32, 34, 40, 41] {
                let a = 0x26002400;
                let mut m = board(p, 0, a, 9);
                m.bus.onchip[p as usize]
                    .write32(0xFFFFFFE8, 0xA55A00C0)
                    .unwrap();
                assert!(!m.issue_cpu(p).unwrap());
                advance(&mut m, edge);
                if edge >= 33 {
                    let _ = m.issue_cpu(p).unwrap();
                    m.bus.grant_bus_transaction();
                }
                let mut rom = vec![0; BIOS_SIZE];
                for at in [0, 8] {
                    rom[at..at + 4].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
                    rom[at + 4..at + 8].copy_from_slice(&(HIGH_WRAM + 0x4000).to_be_bytes());
                }
                m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
                m.bus.load_bios(&rom).unwrap();
                // Watchdog asserts two clocks after arming. With the CPU held
                // at its boundary, edge32 cannot admit a store; edge34 has
                // already accepted one. Exercise the hardware reset drain.
                let chip = &mut m.bus.onchip[p as usize];
                chip.write16(
                    onchip::WDT + 2,
                    if reset == onchip::ResetRequest::Manual {
                        0x5A7F
                    } else {
                        0x5A5F
                    },
                )
                .unwrap();
                chip.write16(onchip::WDT, 0xA578).unwrap();
                chip.write16(onchip::WDT, 0x5AFF).unwrap();
                advance_bus(&mut m, 2, 1, false);
                assert!(m.bus.cpu_reset_active(p));
                m.issue_cpu(p).unwrap();
                advance_bus(&mut m, 100, 17, true);
                let published = edge >= 40 || reset == onchip::ResetRequest::Manual && edge >= 34;
                assert_eq!(
                    m.bus.read32(a),
                    Ok(if published { a + 4 } else { 0 }),
                    "{p:?} {reset:?} {edge}"
                );
                assert_eq!(state(&m, p).instructions, 0);
            }
        }
    }
}

fn attempt(b: &mut Bus, c: &mut Cpu, p: Processor) -> bool {
    let mut next = c.clone();
    match next.step(&mut b.replay_cpu(p)) {
        Ok(()) => {
            *c = next;
            b.finish_cpu_transaction(p);
            true
        }
        Err(crate::cpu::CpuError::Bus {
            source: BusError::Pending,
            ..
        }) => false,
        other => panic!("unexpected store replay: {other:?}"),
    }
}

#[test]
fn real_peer_dmac_and_scu_requests_arbitrate_with_executing_store_ma() {
    for p in [Processor::Master, Processor::Slave] {
        for competitor in 0..3 {
            let other = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            let address = 0x26002400;
            let m = board(p, 0, address, 9);
            let mut c = if p == Processor::Master {
                m.cpu
            } else {
                m.slave
            };
            let mut b = m.bus;
            b.write32(HIGH_WRAM + 0x3000, 0x11223344).unwrap();
            b.write16(HIGH_WRAM + 0x500, 0x60F6).unwrap();
            b.for_cpu(other).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
            b.for_cpu(other).fetch16(HIGH_WRAM + 0x500).unwrap();
            b.cpu_delay(other, 0);
            let mut peer = Cpu::start(HIGH_WRAM + 0x500, 0x26003000);
            if competitor == 0 {
                assert!(!attempt(&mut b, &mut peer, other));
            }
            assert!(!attempt(&mut b, &mut c, p));
            b.advance_clocks(1).unwrap();
            assert!(!attempt(&mut b, &mut c, p));
            match competitor {
                0 => assert!(!attempt(&mut b, &mut peer, other)),
                1 => {
                    for (off, v) in [
                        (0, HIGH_WRAM + 0x3000),
                        (4, HIGH_WRAM + 0x4000),
                        (8, 1),
                        (12, 0xA05),
                        (0x30, 1),
                    ] {
                        b.for_cpu(other).write32(onchip::DMAC + off, v).unwrap();
                    }
                }
                _ => {
                    for (off, v) in [
                        (0, HIGH_WRAM + 0x3000),
                        (4, HIGH_WRAM + 0x4000),
                        (8, 4),
                        (12, 0x102),
                        (20, 7),
                        (16, 0x101),
                    ] {
                        b.write32(SCU + off, v).unwrap();
                    }
                }
            }
            b.grant_bus_transaction();
            let a = b.transactions.active.as_ref().unwrap();
            assert!(match competitor {
                0 => matches!(
                    a.owner,
                    Owner::Cpu(Processor::Master) | Owner::WriteBuffer(Processor::Master, false)
                ),
                1 => a.owner == Owner::Dmac(other),
                _ => a.owner == Owner::Scu,
            });
            b.advance_clocks(1).unwrap();
            assert!(attempt(&mut b, &mut c, p)); // internal acceptance is independent
            assert_eq!(c.state().r[15], address);
            assert_eq!(b.read32(address), Ok(0));
            let expected = match competitor {
                0 if p == Processor::Master => 35,
                0 => 37,
                1 => 40,
                _ => 42,
            };
            while b.main_clocks < expected {
                assert_eq!(
                    b.read32(address),
                    Ok(0),
                    "early publication competitor{competitor}"
                );
                b.advance_clocks(1).unwrap();
            }
            assert_eq!(
                b.read32(address),
                Ok(address + 4),
                "competitor{competitor} {p:?}"
            );
            b.advance_clocks((80 - b.main_clocks) as u32).unwrap();
            if competitor == 0 {
                assert!(attempt(&mut b, &mut peer, other));
                assert_eq!(peer.state().r[0], 0x11223344);
            } else {
                assert_eq!(b.read32(HIGH_WRAM + 0x4000), Ok(0x11223344));
                if competitor == 1 {
                    assert_eq!(b.onchip[other as usize].read32(onchip::DMAC + 8), Ok(0));
                    assert_ne!(
                        b.onchip[other as usize].read32(onchip::DMAC + 12).unwrap() & 2,
                        0
                    );
                } else {
                    assert_ne!(b.scu.read32(SCU + 0xA4).unwrap() & (1 << 11), 0);
                }
            }
            assert_eq!(c.state().instructions, 1);
            assert!(
                b.transactions.journals[p as usize]
                    .borrow()
                    .operations
                    .is_empty()
            );
        }
    }
}

#[test]
fn delay_slot_store_finishes_ma_before_target_and_nmi_entry() {
    for p in [Processor::Master, Processor::Slave] {
        let a = 0x26002400;
        let code = [0xA002, 0x2FF6, 0xEF55, 9, 0x001B, 9];
        let mut m = if p == Processor::Master {
            cached_pair(&code, &[9; 64])
        } else {
            cached_pair(&[9; 64], &code)
        };
        let base = HIGH_WRAM + p as u32 * 0x100;
        let c = Cpu::start(base, a + 4);
        if p == Processor::Master {
            m.cpu = c;
        } else {
            m.slave = c;
        }
        m.bus.onchip[p as usize]
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).delay_target, Some(base + 8));
        advance(&mut m, 35);
        assert_eq!(state(&m, p).instructions, 1);
        assert_eq!(state(&m, p).pc, base + 2);
        assert_eq!(state(&m, p).r[15], a + 4);
        if p == Processor::Master {
            let mut rom = vec![0; BIOS_SIZE];
            rom[44..48].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
            m.bus.load_bios(&rom).unwrap();
            m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
            m.nmi_pending = true;
            m.bus.signal_nmi(p);
        }
        advance(&mut m, 36);
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).pc, base + 8);
        assert_eq!(state(&m, p).delay_target, None);
        assert_eq!(state(&m, p).r[15], a);
        assert_eq!(state(&m, p).instructions, 2);
        assert_eq!(state(&m, p).exceptions, 0);
        assert_eq!(m.bus.read32(a), Ok(0));
        advance(&mut m, 43);
        assert_eq!(m.bus.read32(a), Ok(a + 4));
        if p == Processor::Master {
            for _ in 0..200 {
                if m.cpu_state().exceptions != 0 {
                    break;
                }
                m.step_hardware_boundary().unwrap();
            }
            assert_eq!(
                m.cpu_state().last_exception,
                Some(crate::cpu::Exception {
                    vector: 11,
                    saved_pc: base + 8
                })
            );
            assert_eq!(m.cpu_state().instructions, 2);
            assert_eq!(m.bus.read32(a), Ok(a + 4));
        }
    }
}

#[test]
fn invalid_store_finishes_ma_and_auto_update_then_dispatches_address_error_once() {
    for p in [Processor::Master, Processor::Slave] {
        let a = 0x26002401;
        let mut m = board(p, 0, a, 9);
        let mut rom = vec![0; BIOS_SIZE];
        rom[36..40].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
        m.bus.load_bios(&rom).unwrap();
        m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
        advance(&mut m, 33);
        assert!(!m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).instructions, 0);
        assert_eq!(state(&m, p).r[15], a + 4);
        advance(&mut m, 34);
        assert!(m.issue_cpu(p).unwrap());
        assert_eq!(state(&m, p).instructions, 1);
        assert_eq!(state(&m, p).r[15], a);
        assert!(state(&m, p).address_error_pending);
        assert_eq!(m.bus.read32(a & !3), Ok(0));
        assert!(m.bus.transactions.writes[p as usize].is_none());
        // H §4.8.3: odd-SP exception stacking still issues byte cycles under
        // the established deterministic undefined-data policy. It is separate
        // from the suppressed ordinary store and does not recursively trap.
        for _ in 0..300 {
            if state(&m, p).exceptions != 0 {
                break;
            }
            m.step_hardware_boundary().unwrap();
        }
        assert_eq!(state(&m, p).instructions, 1);
        assert_eq!(
            state(&m, p).last_exception,
            Some(crate::cpu::Exception {
                vector: 9,
                saved_pc: HIGH_WRAM + p as u32 * 0x100 + 2,
            })
        );
        assert_eq!(state(&m, p).r[15], a - 8);
        assert!(!state(&m, p).address_error_pending);
        assert_eq!(m.bus.read32(a & !3), Ok(0xF0000000));
    }
}

#[test]
fn preceding_load_forwards_store_data_base_and_r0_index_after_one_combined_split() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0u32, 2] {
            for (store, loaded, destination, payload, dependent) in [
                (0x2FF2, 0x100, 0xC0000400, 0xC0000400, false),
                (0x2F02, 0x100, 0xC0000400, 0x100, true),
                (0x20F2, 0xC0000600, 0xC0000600, 0xC0000400, true),
                (0x0FF6, 0x100, 0xC0000500, 0xC0000400, true),
            ] {
                let mut code = vec![9; offset as usize / 2];
                code.extend([0x60F2, store, 0x001B, 9]);
                let mut m = if p == Processor::Master {
                    cached_pair(&code, &[0xAFFE, 9])
                } else {
                    cached_pair(&[0xAFFE, 9], &code)
                };
                let c = Cpu::start(HIGH_WRAM + p as u32 * 0x100 + offset, 0xC0000400);
                if p == Processor::Master {
                    m.cpu = c;
                } else {
                    m.slave = c;
                }
                m.bus.for_cpu(p).write32(0xC0000400, loaded).unwrap();
                m.bus.cpu_delay(p, 0);
                let completion = if offset != 0 || dependent { 36 } else { 35 };
                advance(&mut m, completion - 1);
                let _ = m.issue_cpu(p).unwrap();
                assert_eq!(state(&m, p).instructions, 1);
                assert_eq!(state(&m, p).r[0], loaded);
                if destination != 0xC0000400 {
                    assert_eq!(m.bus.for_cpu(p).read32(destination), Ok(0));
                }
                advance(&mut m, completion);
                assert!(m.issue_cpu(p).unwrap());
                assert_eq!(state(&m, p).instructions, 2);
                assert_eq!(m.bus.for_cpu(p).read32(destination), Ok(payload));
                assert_eq!(state(&m, p).r[15], 0xC0000400);
            }
        }
    }
}

#[test]
fn cold_and_system_reset_cancel_store_before_admission_or_after_acceptance() {
    for p in [Processor::Master, Processor::Slave] {
        for system in [false, true] {
            for admitted in [false, true] {
                let a = 0x26002400;
                let mut m = board(p, 0, a, 9);
                m.mode = Some(crate::BootMode::HlePrototype);
                m.bus.onchip[p as usize]
                    .write32(0xFFFFFFE8, 0xA55A00C0)
                    .unwrap();
                if system {
                    m.bus.write8(crate::devices::SMPC + 0x63, 1).unwrap();
                    m.bus.write8(crate::devices::SMPC + 0x1F, 0x0D).unwrap();
                    let elapsed =
                        crate::smpc::Smpc::SYSTEM_RESET_CLOCKS - if admitted { 4 } else { 1 };
                    m.bus.advance_clocks(elapsed).unwrap();
                    m.clocks = m.bus.main_clocks;
                    m.cpu_ready = [m.clocks; 2];
                }
                let start = m.clocks;
                assert!(!m.issue_cpu(p).unwrap());
                if admitted {
                    advance(&mut m, start + 2);
                    assert!(m.issue_cpu(p).unwrap());
                    assert_eq!(state(&m, p).instructions, 1);
                    assert_eq!(m.bus.read32(a), Ok(0));
                }
                if system {
                    // Drive the actual board command path, including cancellation
                    // of a retained instruction or accepted BSC entry.
                    while m.clocks < start + if admitted { 4 } else { 1 } {
                        m.step_hardware_boundary().unwrap();
                    }
                } else {
                    m.reset(crate::Boot::HlePrototype).unwrap();
                }
                assert_eq!(m.cpu_state().pc, crate::FIXTURE_PC);
                assert_eq!(m.cpu_state().instructions, 0);
                assert_eq!(m.slave_state(), &CpuState::default());
                advance_bus(&mut m, 100, 17, true);
                assert_eq!(m.bus.read32(a), Ok(0));
                m.step().unwrap();
                assert_eq!(m.cpu_state().r[0], 5);
            }
        }
    }
}
