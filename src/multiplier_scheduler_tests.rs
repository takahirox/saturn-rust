//! Clocked multiplier boundaries, independently authored from P figs7.53/63.
use super::*;
use crate::cpu::CpuStatus;
use scheduling_tests::cached_pair;

fn advance_to(m: &mut Machine, clock: u64) {
    while m.clocks < clock {
        m.step_hardware_boundary().unwrap();
    }
}
fn retired(m: &mut Machine, n: u64) {
    while m.cpu_state().instructions < n {
        m.step_hardware_boundary().unwrap();
    }
}

#[test]
fn multiplier_wait_replays_before_a_dependent_store_without_blocking_peer_or_dma() {
    // MOV #3,R1; MOV #7,R2; DMULS.L R1,R2; STS.L MACL,@-R15.
    // Long producer EX=t34, MA ends t38 (one IF split), mm ends t42.
    // Its consumer starts at t37 and extends its MA through that final mm.
    let mut m = cached_pair(&[0xE103, 0xE207, 0x321D, 0x4F12, 0x001B, 9], &[9; 64]);
    m.cpu = Cpu::start(HIGH_WRAM, (HIGH_WRAM + 0x2004) | 0x20000000);
    retired(&mut m, 3);
    assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
    advance_to(&mut m, 37);
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    // An independent slave-local DMA reads and writes while the master waits.
    let chip = &mut m.bus.for_cpu(Processor::Slave);
    chip.write8(0xFFFFFE00, 0x5A).unwrap();
    for (off, value) in [
        (0, 0xFFFFFE00),
        (4, 0xFFFFFE01),
        (8, 1),
        (12, 0x205),
        (0x30, 1),
    ] {
        chip.write32(onchip::DMAC + off, value).unwrap();
    }
    advance_to(&mut m, 40);
    assert_eq!(m.bus.for_cpu(Processor::Slave).read8(0xFFFFFE01), Ok(0x5A));
    assert_eq!(m.cpu_state().r[15], (HIGH_WRAM + 0x2004) | 0x20000000);
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(0));
    assert!(m.slave_state().instructions >= 8);
    retired(&mut m, 4);
    assert_eq!(m.cpu_state().r[15], (HIGH_WRAM + 0x2000) | 0x20000000);
    advance_to(&mut m, 64);
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(21));
    // Replay and later boundaries must not perform a second predecrement/write.
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x1FFC), Ok(0));
    assert_eq!(
        m.bus.for_cpu(Processor::Slave).read32(onchip::DMAC + 8),
        Ok(0)
    );
}

#[test]
fn separated_multiplier_stores_complete_at_the_ma_boundary_even_without_extra_stalls() {
    // P figs7.63/7.71: EX=t34, final MA=t38, final mm=t42.
    // Two NOPs still need one extra stall; three need none. Both stores
    // complete at t42. A fourth NOP moves the ordinary MA boundary to t43.
    for (spacing, boundary, clocks) in [(2, 42, 3), (3, 42, 1), (4, 43, 2)] {
        for (op, value) in [(0x4F02, 0), (0x4F12, 21)] {
            let mut code = vec![0xE103, 0xE207, 0x321D];
            code.extend(std::iter::repeat_n(9, spacing));
            code.extend([op, 0x001B, 9]);
            let mut m = cached_pair(&code, &[9; 64]);
            // On-chip SAR0 exposes the actual completion clock without an
            // external write buffer or bus wait hiding an early transfer.
            m.cpu = Cpu::start(HIGH_WRAM, onchip::DMAC + 4);
            m.bus
                .for_cpu(Processor::Master)
                .write32(onchip::DMAC, 0x12345678)
                .unwrap();
            advance_to(&mut m, boundary - 1);
            assert_eq!(
                m.bus.for_cpu(Processor::Master).read32(onchip::DMAC),
                Ok(0x12345678),
                "{op:04x}, spacing {spacing}: early store"
            );
            assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
            advance_to(&mut m, boundary);
            assert_eq!(
                m.bus.for_cpu(Processor::Master).read32(onchip::DMAC),
                Ok(value),
                "{op:04x}, spacing {spacing}: missing store"
            );
            retired(&mut m, 4 + spacing as u64);
            assert_eq!(m.cpu.last_clocks(), clocks);
            assert_eq!(m.cpu_state().r[15], onchip::DMAC);
            advance_to(&mut m, 60);
            assert_eq!(m.cpu_state().r[15], onchip::DMAC);
            assert_eq!(
                m.bus.for_cpu(Processor::Master).read32(onchip::DMAC),
                Ok(value)
            );
            assert!(m.slave_state().instructions >= 8);
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}

#[test]
fn zero_stall_multiplier_loads_sample_at_the_ma_boundary_and_replay_once() {
    // P figs7.63/7.73: three independent NOPs permit an unextended MA
    // ending at t42, even though the instruction has no extra mm stall.
    for op in [0x4F06, 0x4F16] {
        let code = [0xE103, 0xE207, 0x321D, 9, 9, 9, op, 0x001B, 9];
        let mut m = cached_pair(&code, &[9; 64]);
        m.cpu = Cpu::start(HIGH_WRAM, onchip::DMAC);
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC, 0x12345678)
            .unwrap();
        advance_to(&mut m, 41);
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
        assert_eq!(m.cpu_state().r[15], onchip::DMAC);
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC, 0x87654321)
            .unwrap();
        advance_to(&mut m, 42);
        retired(&mut m, 7);
        assert_eq!(m.cpu.last_clocks(), 1);
        let result = if op == 0x4F06 {
            (0x87654321, 21)
        } else {
            (0, 0x87654321)
        };
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
        assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
        advance_to(&mut m, 60);
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
        assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
        assert_eq!(m.cpu_state().exceptions, 0);
    }
}

// Keep the peer on resident NOPs so every clock near t42 is observable.
fn cache_boundary_pair(code: &[u16]) -> Machine {
    let mut m = cached_pair(code, &[9; 64]);
    m.bus
        .for_cpu(Processor::Slave)
        .fetch16(HIGH_WRAM + 0x110)
        .unwrap();
    m.bus.cpu_delay(Processor::Slave, 0);
    m
}

#[test]
fn cache_array_multiplier_stores_wait_for_ma_completion_and_replay_once() {
    // P figs7.63/7.71: the final mm ends at t42. Unlike on-chip
    // peripheral writes, cache-array writes consume no scheduled bus clock.
    for (spacing, boundary, clocks) in [(0, 42, 5), (1, 42, 3), (2, 42, 3), (3, 42, 1), (4, 43, 2)]
    {
        for (op, value) in [(0x4F02, 0), (0x4F12, 21)] {
            let address = 0xC0000400;
            let mut code = vec![0xE103, 0xE207, 0x321D];
            code.extend(std::iter::repeat_n(9, spacing));
            code.extend([op, 0x001B, 9]);
            let mut m = cache_boundary_pair(&code);
            m.cpu = Cpu::start(HIGH_WRAM, address + 4);
            m.bus
                .for_cpu(Processor::Master)
                .write32(address, 0x12345678)
                .unwrap();
            advance_to(&mut m, boundary - 1);
            assert_eq!(m.clocks, boundary - 1);
            assert_eq!(
                m.bus.for_cpu(Processor::Master).read32(address),
                Ok(0x12345678),
                "{op:04x}, spacing {spacing}: early cache store"
            );
            assert_eq!(m.cpu_state().r[15], address + 4);
            // Retrying at the same clock must retain the original deadline.
            assert!(
                !m.issue_cpu(Processor::Master).unwrap(),
                "spacing {spacing}, op {op:04x}, at {}, retired {}",
                m.clocks,
                m.cpu_state().instructions
            );
            advance_to(&mut m, boundary);
            assert_eq!(m.clocks, boundary);
            assert!(m.issue_cpu(Processor::Master).unwrap());
            assert_eq!(m.bus.for_cpu(Processor::Master).read32(address), Ok(value));
            retired(&mut m, 4 + spacing as u64);
            assert_eq!(m.cpu.last_clocks(), clocks);
            assert_eq!(m.cpu_state().r[15], address);
            m.bus
                .for_cpu(Processor::Master)
                .write32(address, 0x87654321)
                .unwrap();
            advance_to(&mut m, 64);
            assert_eq!(
                m.bus.for_cpu(Processor::Master).read32(address),
                Ok(0x87654321)
            );
            assert_eq!(m.cpu_state().r[15], address);
            assert_eq!(m.bus.for_cpu(Processor::Master).read32(address - 4), Ok(0));
            assert!(m.slave_state().instructions >= 8);
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}

#[test]
fn cache_backed_multiplier_loads_sample_at_ma_completion_and_replay_once() {
    // P figs7.63/7.73: cache arrays and resident data lines both complete
    // synchronously. Sample the value changed just before the final MA.
    for address in [0xC0000400, HIGH_WRAM + 0x2400] {
        for (spacing, boundary, clocks) in
            [(0, 42, 5), (1, 42, 3), (2, 42, 3), (3, 42, 1), (4, 43, 2)]
        {
            for op in [0x4F06, 0x4F16] {
                let mut code = vec![0xE103, 0xE207, 0x321D];
                code.extend(std::iter::repeat_n(9, spacing));
                code.extend([op, 0x001B, 9]);
                let mut m = cache_boundary_pair(&code);
                m.cpu = Cpu::start(HIGH_WRAM, address);
                let mut memory = m.bus.for_cpu(Processor::Master);
                memory.write32(address, 0x12345678).unwrap();
                assert_eq!(memory.read32(address), Ok(0x12345678)); // warm the data line
                m.bus.cpu_delay(Processor::Master, 0); // discard setup bus costs
                advance_to(&mut m, boundary - 1);
                assert_eq!(m.clocks, boundary - 1);
                assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
                assert_eq!(m.cpu_state().r[15], address);
                assert!(
                    !m.issue_cpu(Processor::Master).unwrap(),
                    "spacing {spacing}, op {op:04x}, at {}, retired {}",
                    m.clocks,
                    m.cpu_state().instructions
                );
                m.bus
                    .for_cpu(Processor::Master)
                    .write32(address, 0x87654321)
                    .unwrap();
                advance_to(&mut m, boundary);
                retired(&mut m, 4 + spacing as u64);
                assert_eq!(m.cpu.last_clocks(), clocks);
                let result = if op == 0x4F06 {
                    (0x87654321, 21)
                } else {
                    (0, 0x87654321)
                };
                assert_eq!(
                    (m.cpu_state().mach, m.cpu_state().macl),
                    result,
                    "{op:04x}, spacing {spacing}, address {address:08x}: early cache sample"
                );
                assert_eq!(m.cpu_state().r[15], address + 4);
                m.bus
                    .for_cpu(Processor::Master)
                    .write32(address, 0xDEADBEEF)
                    .unwrap();
                advance_to(&mut m, 64);
                assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
                assert_eq!(m.cpu_state().r[15], address + 4);
                assert!(m.slave_state().instructions >= 8);
                assert_eq!(m.cpu_state().exceptions, 0);
            }
        }
    }
}

#[test]
fn multiplier_ages_during_external_and_onchip_waits_and_is_cpu_local() {
    for address in [(HIGH_WRAM + 0x2000) | 0x20000000, 0xFFFFFE01] {
        // DMUL; MOV operand; STS. The external read exhausts all four mm
        // clocks. The one-clock internal read removes part of the hazard.
        let mut m = cached_pair(&[0xE103, 0xE207, 0x321D, 0x60F0, 0x061A, 9], &[9; 64]);
        m.cpu = Cpu::start(HIGH_WRAM, address);
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        retired(&mut m, 4);
        retired(&mut m, 5);
        assert_eq!(m.cpu_state().r[6], 21);
        assert_eq!(
            m.cpu.last_clocks(),
            if address == 0xFFFFFE01 { 2 } else { 1 }
        );
        assert!(m.slave_state().instructions > m.cpu_state().instructions);
        assert_eq!((m.slave_state().mach, m.slave_state().macl), (0, 0));
    }
}

#[test]
fn reset_cancels_multiplier_wait_and_never_publishes_its_suspended_transfer() {
    for p in [Processor::Master, Processor::Slave] {
        for reset in [onchip::ResetRequest::PowerOn, onchip::ResetRequest::Manual] {
            let code = [0xE103, 0xE207, 0x321D, 0x4F12, 9, 9];
            let mut m = cached_pair(&code, &code);
            m.cpu = Cpu::start(HIGH_WRAM, (HIGH_WRAM + 0x2004) | 0x20000000);
            m.slave = Cpu::start(HIGH_WRAM + 0x100, (HIGH_WRAM + 0x3004) | 0x20000000);
            advance_to(&mut m, 37);
            assert!(!m.issue_cpu(p).unwrap());
            let mut rom = vec![0; bus::BIOS_SIZE];
            for v in [0, 8] {
                rom[v..v + 4].copy_from_slice(&HIGH_WRAM.to_be_bytes());
                rom[v + 4..v + 8].copy_from_slice(&(HIGH_WRAM + 0x4000).to_be_bytes());
            }
            m.bus.load_bios(&rom).unwrap();
            m.schedule_cpu_reset(p, reset);
            let state = if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            };
            assert_eq!(state, &CpuState::default());
            advance_to(&mut m, 50);
            assert_eq!(
                m.bus.read32(if p == Processor::Master {
                    HIGH_WRAM + 0x2000
                } else {
                    HIGH_WRAM + 0x3000
                }),
                Ok(0)
            );
        }
    }
}

#[test]
fn watchdog_assertion_during_a_multiplier_interlock_cancels_only_the_affected_cpu() {
    for p in [Processor::Master, Processor::Slave] {
        for manual in [false, true] {
            let code = [0xE103, 0xE207, 0x321D, 0x4F12, 0x001B, 9];
            let peer = [0xAFFE, 0x7001];
            let mut m = if p == Processor::Master {
                cached_pair(&code, &peer)
            } else {
                cached_pair(&peer, &code)
            };
            let target = HIGH_WRAM + 0x2000;
            if p == Processor::Master {
                m.cpu = Cpu::start(HIGH_WRAM, (target + 4) | 0x20000000);
            } else {
                m.slave = Cpu::start(HIGH_WRAM + 0x100, (target + 4) | 0x20000000);
            }
            let mut rom = vec![0; bus::BIOS_SIZE];
            for v in [0, 8] {
                rom[v..v + 4].copy_from_slice(&(HIGH_WRAM + 0x300).to_be_bytes());
                rom[v + 4..v + 8].copy_from_slice(&(HIGH_WRAM + 0x4000).to_be_bytes());
            }
            m.bus.write16(HIGH_WRAM + 0x300, 0x001B).unwrap();
            m.bus.load_bios(&rom).unwrap();
            advance_to(&mut m, 37);
            assert!(!m.issue_cpu(p).unwrap());
            let mut chip = m.bus.for_cpu(p);
            chip.write16(onchip::WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                .unwrap();
            chip.write16(onchip::WDT, 0xA578).unwrap();
            chip.write16(onchip::WDT, 0x5AFF).unwrap();
            advance_to(&mut m, 40);
            m.step_hardware_boundary().unwrap();
            let state = if p == Processor::Master {
                m.cpu_state()
            } else {
                m.slave_state()
            };
            assert_eq!(state, &CpuState::default());
            advance_to(&mut m, 610);
            let (state, peer) = if p == Processor::Master {
                (m.cpu_state(), m.slave_state())
            } else {
                (m.slave_state(), m.cpu_state())
            };
            assert_eq!((state.mach, state.macl, state.exceptions), (0, 0, 0));
            assert_eq!(state.status, CpuStatus::Sleeping);
            assert!(peer.r[0] > 100);
            assert_eq!(m.bus.read32(target), Ok(0));
        }
    }
}

#[test]
fn slave_stop_start_and_cold_boot_cancel_a_suspended_multiplier_transfer() {
    let peer = [0xAFFE, 0x7001];
    let mut m = cached_pair(&peer, &peer);
    let code = [0xE103, 0xE207, 0x321D, 0x4F12, 0x001B, 9];
    for (i, op) in code.iter().enumerate() {
        m.bus
            .write16(HIGH_WRAM + 0x200 + i as u32 * 2, *op)
            .unwrap();
    }
    m.bus
        .for_cpu(Processor::Slave)
        .fetch16(HIGH_WRAM + 0x200)
        .unwrap();
    let mut rom = vec![0; bus::BIOS_SIZE];
    rom[0..4].copy_from_slice(&0x200u32.to_be_bytes());
    rom[4..8].copy_from_slice(&(HIGH_WRAM + 0x4000).to_be_bytes());
    // MOV #42,R0; LDS R0,MACL; STS MACL,R6; SLEEP.
    rom[0x200..0x208].copy_from_slice(&[0xE0, 42, 0x40, 0x1A, 0x06, 0x1A, 0, 0x1B]);
    m.bus.load_bios(&rom).unwrap();
    m.bus.write8(devices::SMPC + 0x63, 1).unwrap();
    m.bus.write8(devices::SMPC + 0x1F, 3).unwrap();
    advance_to(&mut m, 833); // SSHOFF completes at 32+807=839.
    m.slave = Cpu::start(HIGH_WRAM + 0x200, (HIGH_WRAM + 0x2004) | 0x20000000);
    m.cpu_ready[1] = 833;
    advance_to(&mut m, 838);
    assert!(!m.issue_cpu(Processor::Slave).unwrap());
    assert_eq!(m.slave_state().instructions, 3);
    advance_to(&mut m, 839);
    assert!(!m.bus.slave_enabled);
    assert_eq!(m.slave_state(), &CpuState::default());
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(0));
    m.bus.write8(devices::SMPC + 0x63, 1).unwrap();
    m.bus.write8(devices::SMPC + 0x1F, 2).unwrap();
    advance_to(&mut m, 1800);
    assert_eq!((m.slave_state().macl, m.slave_state().r[6]), (42, 42));
    assert_eq!(m.slave_state().status, CpuStatus::Sleeping);
    assert_eq!(m.slave_state().exceptions, 0);
    assert!(m.cpu_state().r[0] > 100);
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(0));

    // A new cold selection during the same kind of pending access must also
    // discard both deadline and journal; the new original ROM supplies results.
    let mut m = cached_pair(&code, &peer);
    m.cpu = Cpu::start(HIGH_WRAM, (HIGH_WRAM + 0x2004) | 0x20000000);
    advance_to(&mut m, 37);
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.run_steps(50).unwrap();
    assert_eq!((m.cpu_state().macl, m.cpu_state().r[6]), (42, 42));
    assert_eq!(m.cpu_state().exceptions, 0);
    assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(0));
}

#[test]
fn a_dependent_lds_samples_memory_after_the_wait_and_updates_its_address_once() {
    let mut m = cached_pair(&[0xE103, 0xE207, 0x321D, 0x4F16, 0x001B, 9], &[9; 64]);
    let address = HIGH_WRAM + 0x2000;
    m.cpu = Cpu::start(HIGH_WRAM, address | 0x20000000);
    m.bus.write32(address, 0xDEADBEEF).unwrap();
    retired(&mut m, 3);
    advance_to(&mut m, 37);
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    advance_to(&mut m, 40);
    assert_eq!(
        (m.cpu_state().macl, m.cpu_state().r[15]),
        (21, address | 0x20000000)
    );
    m.bus.write32(address, 0x12345678).unwrap();
    retired(&mut m, 4);
    assert_eq!(
        (m.cpu_state().macl, m.cpu_state().r[15]),
        (0x12345678, (address + 4) | 0x20000000)
    );
    advance_to(&mut m, 64);
    assert_eq!(
        (m.cpu_state().macl, m.cpu_state().r[15]),
        (0x12345678, (address + 4) | 0x20000000)
    );
    assert_eq!(m.cpu_state().exceptions, 0);
}

#[test]
fn a_cold_consumer_fetch_exhausts_the_older_multiplier_tail() {
    let mut m = cached_pair(&[0xE103, 0xE207, 0x321D, 0x061A, 0x001B, 9], &[9; 64]);
    retired(&mut m, 3);
    let mut memory = m.bus.for_cpu(Processor::Master);
    memory.sh2_discard_prefetch();
    memory.write8(cache::CCR, 0).unwrap();
    memory.write32(0xFFFFFFE8, 0xA55A00C0).unwrap(); // fetch waits exceed mm
    retired(&mut m, 4);
    assert_eq!(m.cpu_state().r[6], 21);
    assert_eq!(m.cpu.last_clocks(), 2); // STS at 4n+2: IF split, no mm stall
    assert!(m.slave_state().instructions >= 8); // peer drains its resident line
}
