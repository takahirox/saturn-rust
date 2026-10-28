//! Independent STN-28 §2.3 / SH7604 table5.4 board vectors.
use super::*;

#[test]
fn slave_raster_boundaries_modes_fields_plls_and_standards() {
    for pal in [false, true] {
        for fast in [false, true] {
            for mode in [0, 2, 0x10, 0x20, 0x40, 0xC0, 4, 5] {
                let mut b = Bus::default();
                b.set_pal(pal);
                b.change_system_clock(fast);
                b.write16(VDP2, mode).unwrap(); // DISP=0 throughout
                // Explicit inherited raster conventions, in main clocks.
                let (line, h) = match (mode & 4 != 0, mode & 1 != 0, fast) {
                    (true, false, _) => (852u32, 640u32),
                    (true, true, _) => (848, 704),
                    (false, _, true) => (1820, 1408),
                    _ => (1708, 1280),
                };
                let active = if mode & 4 != 0 {
                    480
                } else {
                    match mode & 0x30 {
                        0 => 224,
                        0x10 => 240,
                        _ => 256,
                    }
                };
                for field in 0..2 {
                    let lines = if mode & 4 != 0 {
                        if mode & 1 == 0 { 525 } else { 562 }
                    } else {
                        (if pal { 313 } else { 263 }) - u32::from(mode & 0xC0 != 0 && field == 1)
                    };
                    let end = line * lines;
                    let mut samples = vec![
                        0,
                        1,
                        h - 1,
                        h,
                        h + 1,
                        line - 1,
                        line,
                        line + 1,
                        active * line - 1,
                        active * line,
                        active * line + 1,
                        active * line + h - 1,
                        active * line + h,
                        active * line + h + 1,
                        end - 1,
                        end,
                    ];
                    samples.sort_unstable();
                    samples.dedup();
                    let mut previous = 0;
                    for t in samples {
                        b.advance_main_clocks(t - previous, 0).unwrap();
                        previous = t;
                        let expected = if t >= active * line && t < end {
                            Some((6, 0x43))
                        } else if t % line >= h {
                            Some((2, 0x41))
                        } else {
                            None
                        };
                        assert_eq!(
                            b.interrupt(Processor::Slave),
                            expected,
                            "PAL={pal} fast={fast} mode={mode:X} field={field} t={t}"
                        );
                        let phase = t % line;
                        assert_eq!(
                            b.raster.clocks_to_event(),
                            if phase < h { h - phase } else { line - phase }
                        );
                        // TVSTAT's forced VBLANK must never be used as an IRQ.
                        assert_ne!(b.read16(VDP2 + 4).unwrap() & 8, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn slave_raster_reference_deadlines_are_chunk_invariant() {
    for (pal, fast, h, line, freq) in [
        (false, false, 1280u64, 1708u64, 26_874_100u64),
        (false, true, 1408, 1820, 28_636_400),
        (true, false, 1280, 1708, 26_687_500),
        (true, true, 1408, 1820, 28_437_500),
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut b = Bus::default();
            b.set_pal(pal);
            b.change_system_clock(fast);
            let mut now = 0;
            for edge in [h, line, 224 * line, (if pal { 313 } else { 263 }) * line] {
                let deadline = (edge * 26_874_100).div_ceil(freq);
                for t in [deadline - 1, deadline, deadline + 1] {
                    while now < t {
                        let n = (t - now).min(chunk) as u32;
                        b.advance_clocks(n).unwrap();
                        now += u64::from(n);
                    }
                    let main = t * freq / 26_874_100;
                    let expected =
                        if main >= 224 * line && main < (if pal { 313 } else { 263 }) * line {
                            Some((6, 0x43))
                        } else if main % line >= h {
                            Some((2, 0x41))
                        } else {
                            None
                        };
                    assert_eq!(
                        b.interrupt(Processor::Slave),
                        expected,
                        "{pal}/{fast}/{chunk}/{t}"
                    );
                }
            }
        }
    }
}

#[test]
fn slave_levels_ignore_scu_acknowledgement_and_arbitrate_internal_frt() {
    let mut b = Bus::default();
    b.advance_clocks(1280).unwrap();
    assert_eq!(b.interrupt(Processor::Slave), Some((2, 0x41)));
    for mask in [0u32, 0xFFFF_FFFF] {
        b.write32(SCU + 0xA0, mask).unwrap();
        let pending = b.read32(SCU + 0xA4).unwrap();
        for _ in 0..3 {
            b.acknowledge_interrupt(Processor::Slave, (2, 0x41));
        }
        assert_eq!(b.read32(SCU + 0xA4), Ok(pending));
        assert_eq!(b.interrupt(Processor::Slave), Some((2, 0x41)));
    }
    b.write32(SCU + 0xA0, 0).unwrap();
    assert_eq!(b.interrupt(Processor::Master), Some((13, 0x46)));
    b.acknowledge_interrupt(Processor::Master, (13, 0x46));

    assert_eq!(b.interrupt(Processor::Slave), Some((2, 0x41)));
    b.advance_clocks(224 * 1708 + 1280 - 1280).unwrap();
    assert!(b.vdp2.hblank && b.vdp2.vblank);
    assert_eq!(b.interrupt(Processor::Slave), Some((6, 0x43)));
    b.onchip[1].write16(0xFFFFFE66, 0x5500).unwrap();
    b.onchip[1].write8(0xFFFFFE10, 0x81).unwrap();
    b.onchip[1].capture_strobe();
    for (priority, expected) in [(7, (7, 0x55)), (6, (6, 0x43)), (5, (6, 0x43))] {
        b.onchip[1].write16(0xFFFFFE60, priority << 8).unwrap();
        assert_eq!(b.interrupt(Processor::Slave), Some(expected));
    }
    // No fabricated slave D7-D0 vector in unsupported VECMD=1 mode.
    b.onchip[1].write16(0xFFFFFEE0, 1).unwrap();
    assert_eq!(b.interrupt(Processor::Slave), Some((5, 0x55)));
}

#[test]
fn slave_restart_observes_live_levels_and_board_resets_clear_them() {
    let mut b = Bus::default();
    b.write32(HIGH_WRAM, 0x12345678).unwrap();
    b.write8(BACKUP_RAM + 1, 0xA5).unwrap();
    b.advance_clocks(224 * 1708).unwrap();
    for _ in 0..2 {
        b.set_slave_enabled(false);
        b.advance_clocks(3).unwrap();
        b.set_slave_enabled(true);
        assert_eq!(b.interrupt(Processor::Slave), Some((6, 0x43)));
        assert_eq!(b.read32(HIGH_WRAM), Ok(0x12345678));
    }
    b.change_system_clock(true);
    assert_eq!(b.interrupt(Processor::Slave), None);
    assert_eq!(b.read32(HIGH_WRAM), Ok(0x12345678));
    b.advance_main_clocks(1408, 0).unwrap();
    b.set_slave_enabled(true);
    assert_eq!(b.interrupt(Processor::Slave), Some((2, 0x41)));
    b.system_reset();
    assert_eq!(b.interrupt(Processor::Slave), None);
    assert_eq!(b.read32(HIGH_WRAM), Ok(0));
    assert_eq!(b.read8(BACKUP_RAM + 1), Ok(0xA5));
    b.advance_clocks(1280).unwrap();
    b.reset_volatile();
    assert_eq!(b.interrupt(Processor::Slave), None);
    assert_eq!(b.read8(BACKUP_RAM + 1), Ok(0xA5));
}

fn cpu(b: &mut Bus, mask: u8, tail: &[u16]) -> crate::cpu::Cpu {
    let mut rom = vec![0; BIOS_SIZE];
    for vector in [0x41, 0x43] {
        rom[vector * 4..vector * 4 + 4].copy_from_slice(&(HIGH_WRAM + 0x400).to_be_bytes());
    }
    b.load_bios(&rom).unwrap();
    for (i, word) in [0xE000 | u16::from(mask), 0x400E, 9]
        .iter()
        .chain(tail)
        .enumerate()
    {
        b.write16(HIGH_WRAM + i as u32 * 2, *word).unwrap();
    }
    b.write16(HIGH_WRAM + 0x400, 0x002B).unwrap();
    b.write16(HIGH_WRAM + 0x402, 9).unwrap();
    let mut c = crate::cpu::Cpu::start(HIGH_WRAM | 0x20000000, (HIGH_WRAM + 0x1000) | 0x20000000);
    for _ in 0..3 {
        c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
    }
    c
}
fn accept(b: &mut Bus, c: &mut crate::cpu::Cpu) -> bool {
    if let Some((level, vector)) = b.interrupt(Processor::Slave) {
        c.accept_interrupt(&mut b.for_cpu(Processor::Slave), level, vector)
            .unwrap()
    } else {
        false
    }
}
#[test]
fn slave_sr_thresholds_sleep_stack_rte_reentry_and_no_stale_masked_level() {
    for (clock, level, vector) in [(1280, 2, 0x41), (224 * 1708, 6, 0x43)] {
        for mask in [0, 1, 2, 5, 6, 7, 15] {
            let mut b = Bus::default();
            let mut c = cpu(&mut b, mask << 4, &[0x001B, 9]);
            c.step(&mut b.for_cpu(Processor::Slave)).unwrap(); // genuine SLEEP
            b.advance_clocks(clock).unwrap();
            assert_eq!(accept(&mut b, &mut c), mask < level);
            if mask < level {
                assert_eq!(c.state().status, crate::cpu::CpuStatus::Running);
                assert_eq!(c.state().last_exception.as_ref().unwrap().vector, vector);
                assert_eq!(b.read32(c.state().r[15]), Ok((HIGH_WRAM | 0x20000000) + 8));
                assert_eq!(b.read32(c.state().r[15] + 4), Ok(u32::from(mask) << 4));
                assert_eq!(c.state().instructions, 4);
                c.step(&mut b.for_cpu(Processor::Slave)).unwrap(); // RTE
                assert!(!accept(&mut b, &mut c)); // pending delay slot
                c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
                assert!(accept(&mut b, &mut c)); // live level was not acknowledged
                assert_eq!(c.state().exceptions, 2);
            }
        }
    }
    for (start, end) in [(1280, 1708), (224 * 1708, 263 * 1708)] {
        for during in [false, true] {
            let mut b = Bus::default();
            let mut c = cpu(&mut b, 0xF0, &[0xE000, 0x400E, 9, 9]);
            b.advance_clocks(start).unwrap();
            assert!(!accept(&mut b, &mut c));
            if !during {
                b.advance_clocks(end - start).unwrap();
            }
            c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
            c.step(&mut b.for_cpu(Processor::Slave)).unwrap(); // LDC SR
            assert!(!accept(&mut b, &mut c)); // successor must run first
            c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
            assert_eq!(accept(&mut b, &mut c), during);
        }
    }
}
#[test]
fn slave_raster_waits_for_delayed_branch_and_control_transfer_successor() {
    for prefix in [0xA001, 0x400E] {
        // BRA +1 or LDC R0,SR (R0=0)
        let mut b = Bus::default();
        let mut c = cpu(&mut b, 0, &[prefix, 0x7101, 9, 9]);
        c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
        b.advance_clocks(1280).unwrap();
        assert!(!accept(&mut b, &mut c));
        c.step(&mut b.for_cpu(Processor::Slave)).unwrap();
        assert_eq!(c.state().r[1], 1);
        assert!(accept(&mut b, &mut c));
    }
}

fn sleeping_machine(clock: u32) -> crate::Machine {
    let mut m = crate::Machine::new();
    m.slave = cpu(&mut m.bus, 0x10, &[0x001B, 9, 0xAFFE, 9]);
    m.slave.step(&mut m.bus.for_cpu(Processor::Slave)).unwrap();
    m.bus.set_slave_enabled(true);
    m.bus.advance_clocks(clock).unwrap();
    m.clocks = u64::from(clock);
    m.cpu_ready = [u64::MAX, m.bus.main_clocks];
    m
}
#[test]
fn slave_raster_exception_replay_retains_one_sample_after_level_ends() {
    let mut m = sleeping_machine(1707); // final HBlank clock
    let pending = m.bus.read32(SCU + 0xA4).unwrap();
    assert!(!m.issue_cpu(Processor::Slave).unwrap());
    assert_eq!(m.slave.state().exceptions, 0);
    assert_eq!(m.cpu_boundary[1].unwrap().irq, Some((2, 0x41)));
    let mut changed_completed_store = false;
    for _ in 0..100 {
        m.step_hardware_boundary().unwrap();
        if !changed_completed_store && m.bus.read32(0x26000FFC) == Ok(0x10) {
            // Unit-test probe: mutate the already completed SR store while PC
            // stacking/vector reads are pending. Replay must not store it again.
            assert_eq!(m.slave.state().exceptions, 0);
            m.bus.write32(0x26000FFC, 0x11).unwrap();
            changed_completed_store = true;
        }
        if m.slave.state().exceptions == 1 {
            break;
        }
    }
    assert!(changed_completed_store);
    assert_eq!(m.bus.interrupt(Processor::Slave), None);
    assert_eq!(m.slave.state().exceptions, 1);
    assert_eq!(m.slave.state().instructions, 4);
    let sp = m.slave.state().r[15];
    assert_eq!(sp, 0x26000FF8);
    assert_eq!(m.bus.read32(sp), Ok(0x26000008));
    assert_eq!(m.bus.read32(sp + 4), Ok(0x11));
    assert_eq!(m.bus.read32(SCU + 0xA4), Ok(pending));
    for _ in 0..100 {
        if m.slave.state().instructions >= 6 {
            break;
        }
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.slave.state().instructions, 6);
    assert_eq!(m.slave.state().exceptions, 1);
    assert_eq!(m.slave.state().r[15], sp + 8);
    assert_eq!(m.slave.state().pc, 0x26000008);
}

#[test]
fn slave_raster_pending_exception_is_canceled_by_smpc_and_cold_reset() {
    for command in [3, 0x0D, 0x0E, 0x0F, 0] {
        let mut m = sleeping_machine(224 * 1708);
        m.bus.write32(HIGH_WRAM + 0x2000, 0x12345678).unwrap();
        m.bus.write8(BACKUP_RAM + 1, 0xA5).unwrap();
        if command != 0 {
            m.bus.write8(SMPC + 0x63, 1).unwrap();
            m.bus.write8(SMPC + 0x1F, command).unwrap();
            // Unit setup: arrange SMPC completion one clock after CPU issue,
            // so reset precedes the pending exception's first data phase.
            let remaining = m.bus.smpc.clocks_to_event();
            assert_eq!(m.bus.smpc.advance_clocks(remaining - 1), None);
        }
        assert!(!m.issue_cpu(Processor::Slave).unwrap());
        assert!(m.cpu_boundary[1].is_some());
        if command == 0 {
            m.reset(crate::Boot::HlePrototype).unwrap();
        } else {
            m.step_hardware_boundary().unwrap();
        }
        assert!(m.cpu_boundary[1].is_none());
        assert_eq!(m.slave.state(), &crate::cpu::CpuState::default());
        m.bus.advance_clocks(100).unwrap();
        assert!(!m.bus.take_cpu_transaction_completion(Processor::Slave));
        assert_eq!(m.bus.read32(0x26000FFC), Ok(0));
        assert_eq!(
            m.bus.read32(HIGH_WRAM + 0x2000),
            Ok(if matches!(command, 3 | 0x0E | 0x0F) {
                0x12345678
            } else {
                0
            })
        );
        assert_eq!(m.bus.read8(BACKUP_RAM + 1), Ok(0xA5));
        if command == 3 {
            assert_eq!(m.bus.interrupt(Processor::Slave), Some((6, 0x43)));
        } else {
            assert_eq!(m.bus.interrupt(Processor::Slave), None);
        }
    }
}

#[test]
fn slave_raster_smpc_restarts_unmask_current_live_vblank() {
    let mut rom = vec![0; BIOS_SIZE];
    rom[..4].copy_from_slice(&0x200u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x26001000u32.to_be_bytes());
    // CKCHG also delivers master NMI; retain the slave-raster coverage.
    for vector in [11, 0x41, 0x43] {
        rom[vector * 4..vector * 4 + 4].copy_from_slice(&0x300u32.to_be_bytes());
    }
    // Reset code explicitly unmasks only after its reset-vector acquisition.
    for (i, word) in [0xE010u16, 0x400E, 9, 0x001B, 0xAFFE, 9].iter().enumerate() {
        rom[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    rom[0x300..0x304].copy_from_slice(&[0, 0x2B, 0, 9]);
    let mut m = crate::Machine::new();
    m.reset(crate::Boot::ExternalBios(&rom)).unwrap();
    m.bus.write8(BACKUP_RAM + 1, 0xA5).unwrap();
    for restart in [0, 3, 0x0E, 0x0F, 0x0D] {
        if restart != 0 {
            m.bus.write8(SMPC + 0x63, 1).unwrap();
            m.bus.write8(SMPC + 0x1F, restart).unwrap();
            if restart != 3 {
                let remaining = m.bus.smpc.clocks_to_event();
                m.bus.smpc.advance_clocks(remaining - 1);
            }
            for _ in 0..1000 {
                if !m.bus.slave_enabled {
                    break;
                }
                m.step_hardware_boundary().unwrap();
            }
            assert!(!m.bus.slave_enabled);
            assert_eq!(m.slave.state(), &crate::cpu::CpuState::default());
            assert!(m.cpu_boundary[1].is_none());
        }
        // Wait using actual board time; no host IRQ injection.
        for _ in 0..1_000_000 {
            if m.bus.interrupt(Processor::Slave) == Some((6, 0x43)) {
                break;
            }
            m.step_hardware_boundary().unwrap();
        }
        assert_eq!(m.bus.interrupt(Processor::Slave), Some((6, 0x43)));
        m.bus.write32(HIGH_WRAM + 0x2000, 0x12345678).unwrap();
        m.bus.write8(SMPC + 0x63, 1).unwrap();
        m.bus.write8(SMPC + 0x1F, 2).unwrap();
        for _ in 0..1000 {
            m.step_hardware_boundary().unwrap();
            if m.slave.state().exceptions != 0 {
                break;
            }
        }
        assert_eq!(m.slave.state().exceptions, 1, "restart {restart:X}");
        assert_eq!(
            m.slave.state().last_exception.as_ref().unwrap().vector,
            0x43
        );
        assert_eq!(m.slave.state().sr & 0xF0, 0x60);
        assert_eq!(m.bus.read32(HIGH_WRAM + 0x2000), Ok(0x12345678));
        assert_eq!(m.bus.read8(BACKUP_RAM + 1), Ok(0xA5));
    }
}
