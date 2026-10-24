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

// Keep the peer on resident NOPs so every clock near MA completion is observable.
fn cache_boundary_pair(code: &[u16]) -> Machine {
    let mut m = cached_pair(code, &[9; 64]);
    for offset in [0x110, 0x120] {
        m.bus
            .for_cpu(Processor::Slave)
            .fetch16(HIGH_WRAM + offset)
            .unwrap();
    }
    m.bus.cpu_delay(Processor::Slave, 0);
    m
}

#[test]
fn mac_first_operand_samples_after_ex_and_is_retained_during_second_ma_replay() {
    // P §7.2.3, figs7.34/64: DMUL EX=34, MAC EX=37. Its first
    // MA starts at 38, so the two-clock SAR0 read samples at 40, not 39.
    // The second MA then spans 40..42, overlapping the older mm tail.
    let mut m = cache_boundary_pair(&[0xE103, 0xE207, 0x321D, 0x0FFF, 0x001B, 9]);
    m.cpu = Cpu::start(HIGH_WRAM, onchip::DMAC);
    for (offset, value) in [(0, 3), (4, 7)] {
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC + offset, value)
            .unwrap();
    }
    advance_to(&mut m, 39);
    assert_eq!(m.clocks, 39);
    assert_eq!(m.cpu_state().instructions, 3);
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    m.bus
        .for_cpu(Processor::Master)
        .write32(onchip::DMAC, 5)
        .unwrap();
    advance_to(&mut m, 40);
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    // The completed first read must retain 5 while the second is pending.
    m.bus
        .for_cpu(Processor::Master)
        .write32(onchip::DMAC, 9)
        .unwrap();
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    advance_to(&mut m, 42);
    assert!(m.issue_cpu(Processor::Master).unwrap());
    assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 56));
    assert_eq!(m.cpu_state().r[15], onchip::DMAC + 8);
    m.bus
        .for_cpu(Processor::Master)
        .write32(onchip::DMAC + 4, 11)
        .unwrap();
    advance_to(&mut m, 64);
    assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 56));
    assert_eq!(m.cpu_state().r[15], onchip::DMAC + 8);
    assert_eq!(m.cpu_state().exceptions, 0);
    assert!(m.slave_state().instructions >= 8);
}

#[test]
fn mac_second_operands_sample_at_the_multiplier_boundary_and_replay_once() {
    // P figs7.64/65: DMUL's final mm ends at t42. The second MAC
    // operand's MA must extend through t42, even for a zero-clock cache hit.
    for (address, op, width, boundary) in [
        (0xC0000400, 0x0FFF, 4, 42),
        (0xC0000400, 0x4FFF, 2, 42),
        (HIGH_WRAM + 0x2400, 0x0FFF, 4, 42),
        (HIGH_WRAM + 0x2400, 0x4FFF, 2, 42),
        // Both peripheral-read clocks fit inside the extended second MA.
        (onchip::DMAC, 0x0FFF, 4, 42),
    ] {
        let mut m = cache_boundary_pair(&[0xE103, 0xE207, 0x321D, op, 0x001B, 9]);
        m.cpu = Cpu::start(HIGH_WRAM, address);
        let write = |m: &mut Machine, offset, value| {
            let mut memory = m.bus.for_cpu(Processor::Master);
            if width == 4 {
                memory.write32(address + offset, value).unwrap();
            } else {
                memory.write16(address + offset, value as u16).unwrap();
            }
        };
        write(&mut m, 0, 3);
        write(&mut m, width, 7);
        // Warm both data operands; reads from the array are synchronous too.
        let memory = m.bus.for_cpu(Processor::Master);
        if width == 4 {
            assert_eq!(memory.read32(address), Ok(3));
            assert_eq!(memory.read32(address + width), Ok(7));
        } else {
            assert_eq!(memory.read16(address), Ok(3));
            assert_eq!(memory.read16(address + width), Ok(7));
        }
        m.bus.cpu_delay(Processor::Master, 0);
        advance_to(&mut m, 41);
        assert_eq!(m.clocks, 41);
        assert_eq!(
            m.cpu_state().instructions,
            3,
            "{op:04x} at {address:08x}: early MAC"
        );
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
        assert_eq!(m.cpu_state().r[15], address);
        assert!(!m.issue_cpu(Processor::Master).unwrap());
        // The first read has completed; replay must retain 3. Only the
        // second value may change, giving 21 + 3*11 = 54, not 42 or 76.
        write(&mut m, 0, 5);
        write(&mut m, width, 11);
        assert!(!m.issue_cpu(Processor::Master).unwrap());
        advance_to(&mut m, 42);
        assert!(m.issue_cpu(Processor::Master).unwrap());
        assert_eq!(m.bus.main_clocks, boundary, "{op:04x} at {address:08x}");
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 54));
        assert_eq!(m.cpu_state().r[15], address + 2 * width);
        write(&mut m, width, 13);
        advance_to(&mut m, 64);
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 54));
        assert_eq!(m.cpu_state().r[15], address + 2 * width);
        assert_eq!(m.cpu_state().exceptions, 0);
        assert!(m.slave_state().instructions >= 8);
    }
}

#[test]
fn dependent_scheduled_lds_overlaps_its_full_operand_latency_with_mm() {
    // P §7.2.3, figs7.63/73/89: DMUL EX=34, mm ends=42.
    // LDS starts MA at 38/39/40/41/42 for 0/1/2/3/4 NOPs.
    // Its two-clock SAR0 read ends at max(MA start + 2, 42).
    // Thus contention hides both clocks for 0–2 NOPs; with 3–4 NOPs
    // the operand, rather than the multiplier, determines completion.
    for (spacing, boundary) in [(0, 42), (1, 42), (2, 42), (3, 43), (4, 44)] {
        for op in [0x4F06, 0x4F16] {
            let mut code = vec![0xE103, 0xE207, 0x321D];
            code.extend(std::iter::repeat_n(9, spacing));
            code.extend([op, 0x001B, 9]);
            let mut m = cache_boundary_pair(&code);
            m.cpu = Cpu::start(HIGH_WRAM, onchip::DMAC);
            m.bus
                .for_cpu(Processor::Master)
                .write32(onchip::DMAC, 0x12345678)
                .unwrap();
            advance_to(&mut m, boundary - 1);
            assert_eq!(m.clocks, boundary - 1);
            assert_eq!(m.cpu_state().instructions, 3 + spacing as u64);
            assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
            assert_eq!(m.cpu_state().r[15], onchip::DMAC);
            assert!(!m.issue_cpu(Processor::Master).unwrap());
            m.bus
                .for_cpu(Processor::Master)
                .write32(onchip::DMAC, 0x87654321)
                .unwrap();
            assert!(!m.issue_cpu(Processor::Master).unwrap());
            advance_to(&mut m, boundary);
            assert!(
                m.issue_cpu(Processor::Master).unwrap(),
                "{op:04x}, spacing {spacing}: operand and mm waits must overlap"
            );
            assert_eq!(m.bus.main_clocks, boundary);
            let result = if op == 0x4F06 {
                (0x87654321, 21)
            } else {
                (0, 0x87654321)
            };
            assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
            assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
            m.bus
                .for_cpu(Processor::Master)
                .write32(onchip::DMAC, 0x11223344)
                .unwrap();
            advance_to(&mut m, 64);
            assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
            assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
            assert_eq!(m.cpu_state().exceptions, 0);
            assert!(m.slave_state().instructions >= 8);
        }
    }
}

#[test]
fn dependent_external_lds_overlaps_short_and_long_bus_latencies_with_mm() {
    // P §7.2.3/fig7.73: complete at max(operand end, mm end).
    // H §7.3: a 32-bit ordinary read is T1/T2 plus WCR1's 0/1/2/6 Tw.
    // The MA begins at 38 + spacing; mm ends at 42. Literal boundaries
    // below cover both a hidden operand wait and an operand outlasting mm.
    for (wait_control, boundaries) in [
        (0x00, [42, 42, 42, 43, 44]),
        (0x40, [42, 42, 43, 44, 45]),
        (0x80, [42, 43, 44, 45, 46]),
        (0xC0, [46, 47, 48, 49, 50]),
    ] {
        for (spacing, boundary) in boundaries.into_iter().enumerate() {
            for op in [0x4F06, 0x4F16] {
                let address = (HIGH_WRAM + 0x2400) | 0x20000000;
                let mut code = vec![0xE103, 0xE207, 0x321D];
                code.extend(std::iter::repeat_n(9, spacing));
                code.extend([op, 0x001B, 9]);
                let mut m = cache_boundary_pair(&code);
                m.cpu = Cpu::start(HIGH_WRAM, address);
                m.bus
                    .for_cpu(Processor::Master)
                    .write32(0xFFFFFFE8, 0xA55A0000 | wait_control)
                    .unwrap();
                m.bus.write32(address, 0x12345678).unwrap();
                advance_to(&mut m, boundary - 1);
                assert_eq!(m.clocks, boundary - 1);
                assert_eq!(m.cpu_state().instructions, 3 + spacing as u64);
                assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 21));
                assert_eq!(m.cpu_state().r[15], address);
                assert!(!m.issue_cpu(Processor::Master).unwrap());
                m.bus.write32(address, 0x87654321).unwrap();
                assert!(!m.issue_cpu(Processor::Master).unwrap());
                advance_to(&mut m, boundary);
                assert!(
                    m.issue_cpu(Processor::Master).unwrap(),
                    "{op:04x}, WCR1={wait_control:02x}, spacing {spacing}: late operand"
                );
                let result = if op == 0x4F06 {
                    (0x87654321, 21)
                } else {
                    (0, 0x87654321)
                };
                assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
                assert_eq!(m.cpu_state().r[15], address + 4);
                m.bus.write32(address, 0x11223344).unwrap();
                advance_to(&mut m, 64);
                assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
                assert_eq!(m.cpu_state().r[15], address + 4);
                assert_eq!(m.cpu_state().exceptions, 0);
                assert!(m.slave_state().instructions >= 8);
            }
        }
    }
}

#[test]
fn mac_operand_completion_preserves_successor_ex_overlap() {
    // P figs7.23/34: MAC's two baseline EX intervals plus the IF split
    // give three issue clocks. At 4n, second-MA completion is one clock
    // later than the successor EX; at 4n+2 it coincides with that EX.
    for (op, width, address, extra) in [
        (0x0FFF, 4, 0xC0000400, 0),
        (0x4FFF, 2, 0xC0000400, 0),
        // Each two-clock peripheral MA stretches its slot by one clock.
        // First MA starts after EX; its IF split precedes the second MA.
        (0x0FFF, 4, onchip::DMAC, 2),
        (0x4FFF, 2, onchip::VCRB, 2),
    ] {
        for unaligned in [false, true] {
            let mut code = vec![];
            if unaligned {
                code.push(9);
            }
            let check_tail = width == 4 && address == onchip::DMAC;
            code.extend([
                op,
                9,
                0xE65A,
                if check_tail { 0x4F12 } else { 9 },
                0x001B,
                9,
            ]);
            let mut m = cache_boundary_pair(&code);
            m.cpu = Cpu::start(HIGH_WRAM, address);
            let mut memory = m.bus.for_cpu(Processor::Master);
            if width == 4 {
                memory.write32(address, 3).unwrap();
                memory.write32(address + width, 7).unwrap();
            } else {
                memory.write16(address, 3).unwrap();
                memory.write16(address + width, 7).unwrap();
            }
            advance_to(&mut m, 35 + extra);
            assert_eq!(m.cpu_state().instructions, u64::from(unaligned));
            assert_eq!(m.cpu_state().macl, 0);
            assert!(!m.issue_cpu(Processor::Master).unwrap());
            advance_to(&mut m, 36 + extra);
            assert!(m.issue_cpu(Processor::Master).unwrap()); // MAC, at MA end
            assert_eq!(m.cpu_state().macl, 21);
            assert_eq!(m.cpu_state().r[15], address + 2 * width);
            assert!(m.issue_cpu(Processor::Master).unwrap()); // successor NOP
            if unaligned {
                assert!(!m.issue_cpu(Processor::Master).unwrap());
                advance_to(&mut m, 37 + extra);
            }
            assert!(m.issue_cpu(Processor::Master).unwrap()); // third MOV
            assert_eq!(m.bus.main_clocks, 36 + extra + u64::from(unaligned));
            assert_eq!(m.cpu_state().r[6], 0x5A);
            if check_tail {
                // The long tail begins at actual second-MA completion=38,
                // so the dependent STS.L must not write DAR0 before 42.
                advance_to(&mut m, 41);
                assert_eq!(
                    m.bus.for_cpu(Processor::Master).read32(onchip::DMAC + 4),
                    Ok(7)
                );
                assert_eq!(m.cpu_state().r[15], address + 8);
                advance_to(&mut m, 42);
                retired(&mut m, 4 + u64::from(unaligned));
                assert_eq!(m.clocks, 42);
                assert_eq!(
                    m.bus.for_cpu(Processor::Master).read32(onchip::DMAC + 4),
                    Ok(21)
                );
                assert_eq!(m.cpu_state().r[15], address + 4);
            }
        }
    }
}

#[test]
fn mac_memory_transfers_overlap_the_successor_in_elapsed_clocks() {
    // P table7.2, figs7.89/91: EX=t32, MA ends t34, successor
    // NOP's EX=t33. Retaining effects until MA must not lose that EX slot.
    // At 4n+2, IF/MA contention instead gives two EX intervals.
    for (op, unaligned, address) in [0x4F02, 0x4F12, 0x4F06, 0x4F16]
        .into_iter()
        .flat_map(|op| [(op, false), (op, true)])
        .flat_map(|(op, unaligned)| {
            [0xC0000400, onchip::DMAC].map(|address| (op, unaligned, address))
        })
    {
        let store = op & 4 == 0;
        let mut code = vec![];
        if unaligned {
            code.push(9);
        }
        code.extend([op, 9, 0xE65A, 9]);
        let mut m = cache_boundary_pair(&code);
        m.cpu = Cpu::start(HIGH_WRAM, address + if store { 4 } else { 0 });
        m.bus
            .for_cpu(Processor::Master)
            .write32(address, 0x12345678)
            .unwrap();
        let offset = u64::from(unaligned);
        let operand_wait = u64::from(!store && address == onchip::DMAC);
        advance_to(&mut m, 33 + offset);
        assert_eq!(m.cpu_state().instructions, offset);
        assert_eq!(
            m.bus.for_cpu(Processor::Master).read32(address),
            Ok(0x12345678)
        );
        if unaligned {
            advance_to(&mut m, 35 + operand_wait);
            for _ in m.cpu_state().instructions..3 {
                assert!(m.issue_cpu(Processor::Master).unwrap());
            }
            assert!(
                !m.issue_cpu(Processor::Master).unwrap(),
                "{op:04x}: IF/MA contention must delay the next EX"
            );
        }
        // P §7.2.3 stretches the entire shared MA/EX slot for a two-clock
        // peripheral read. It adds one clock at 4n; at 4n+2 the IF/MA split
        // also retains that operand wait (§7.4.1).
        let boundary = 34 + 2 * offset + operand_wait;
        advance_to(&mut m, boundary);
        for _ in m.cpu_state().instructions..3 + offset {
            assert!(
                m.issue_cpu(Processor::Master).unwrap(),
                "{op:04x} at {address:08x}: successor EX overlap lost at {} (ready {})",
                m.bus.main_clocks,
                m.cpu_ready[0]
            );
        }
        assert_eq!(
            m.bus.main_clocks, boundary,
            "{op:04x}: successor EX overlap lost"
        );
        assert_eq!(m.cpu_state().r[6], 0x5A);
        assert_eq!(m.cpu_state().r[15], address + if store { 0 } else { 4 });
        assert_eq!(
            m.bus.for_cpu(Processor::Master).read32(address),
            Ok(if store { 0 } else { 0x12345678 })
        );
        assert_eq!(
            (m.cpu_state().mach, m.cpu_state().macl),
            match op {
                0x4F06 => (0x12345678, 0),
                0x4F16 => (0, 0x12345678),
                _ => (0, 0),
            }
        );
    }
}

#[test]
fn multi_clock_mac_loads_preserve_shared_ma_ex_slot_and_replay_once() {
    // P §7.2.3 and fig7.89, independent of production timing helpers:
    // LDS.L EX starts at 32; its two-clock SAR0 read stretches MA to 35.
    // NOP shares that MA slot, so the third instruction's EX starts at 35.
    for op in [0x4F06, 0x4F16] {
        let mut m = cache_boundary_pair(&[op, 9, 0xE65A, 0x001B, 9]);
        m.cpu = Cpu::start(HIGH_WRAM, onchip::DMAC);
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC, 0x12345678)
            .unwrap();
        advance_to(&mut m, 34);
        assert_eq!(m.clocks, 34);
        assert_eq!(m.cpu_state().instructions, 0);
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), (0, 0));
        assert_eq!(m.cpu_state().r[15], onchip::DMAC);
        assert!(!m.issue_cpu(Processor::Master).unwrap());
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC, 0x87654321)
            .unwrap();
        assert!(!m.issue_cpu(Processor::Master).unwrap());
        advance_to(&mut m, 35);
        assert!(m.issue_cpu(Processor::Master).unwrap()); // LDS.L at MA end
        let result = if op == 0x4F06 {
            (0x87654321, 0)
        } else {
            (0, 0x87654321)
        };
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
        assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
        assert!(m.issue_cpu(Processor::Master).unwrap()); // NOP shares MA
        assert!(
            m.issue_cpu(Processor::Master).unwrap(),
            "{op:04x}: third EX must start at 35 after the shared MA/NOP slot"
        );
        assert_eq!(m.clocks, 35);
        assert_eq!(m.cpu_state().r[6], 0x5A);
        m.bus
            .for_cpu(Processor::Master)
            .write32(onchip::DMAC, 0x11223344)
            .unwrap();
        advance_to(&mut m, 64);
        assert_eq!((m.cpu_state().mach, m.cpu_state().macl), result);
        assert_eq!(m.cpu_state().r[15], onchip::DMAC + 4);
        assert_eq!(m.cpu_state().exceptions, 0);
        assert!(m.slave_state().instructions >= 5);
    }
}

#[test]
fn a_waited_successor_fetch_exhausts_memory_transfer_overlap() {
    // Start the same cold MOV fetch at MA completion, after either an aligned
    // STS.L/LDS.L or an independent NOP control. Include stretched load MA.
    // A real fetch wait consumes that overlap; both deadlines must agree.
    for (transfer, address, boundary) in [
        (0x4F12, 0xC0000404, 34),
        (0x4F06, onchip::DMAC, 35),
        (0x4F16, onchip::DMAC, 35),
    ] {
        let mut deadlines = vec![];
        for first in [transfer, 9] {
            let mut m = cache_boundary_pair(&[first, 0xE65A, 9]);
            m.cpu = Cpu::start(HIGH_WRAM, address);
            if m.issue_cpu(Processor::Master).unwrap() {
                m.bus.advance_clocks((boundary - 32) as u32).unwrap();
                m.clocks = boundary;
            } else {
                advance_to(&mut m, boundary);
                assert!(m.issue_cpu(Processor::Master).unwrap());
            }
            assert_eq!(m.bus.main_clocks, boundary);
            assert_eq!(m.cpu_state().instructions, 1);
            let mut memory = m.bus.for_cpu(Processor::Master);
            memory.write8(cache::CCR, 0x11).unwrap();
            memory.sh2_discard_prefetch();
            m.bus.cpu_delay(Processor::Master, 0);
            retired(&mut m, 2);
            assert_eq!(m.cpu_state().r[6], 0x5A);
            assert!(m.bus.main_clocks > boundary);
            deadlines.push((m.bus.main_clocks, m.cpu_ready[0]));
        }
        assert_eq!(deadlines[0], deadlines[1], "{transfer:04x}");
    }
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
        // clocks. EX/MA lead-in and the internal read now exhaust it too.
        let mut m = cached_pair(&[0xE103, 0xE207, 0x321D, 0x60F0, 0x061A, 9], &[9; 64]);
        m.cpu = Cpu::start(HIGH_WRAM, address);
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        retired(&mut m, 4);
        retired(&mut m, 5);
        assert_eq!(m.cpu_state().r[6], 21);
        assert_eq!(m.cpu.last_clocks(), 1);
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
