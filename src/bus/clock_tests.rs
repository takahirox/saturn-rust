//! Clock lifecycle boundaries. Expectations follow SH7604 table 14.3,
//! §§9.2.7/12.3.3/14.4 and the Saturn System Program clock-switch contract.
use super::*;
use crate::{Boot, Machine, cpu::CpuStatus, onchip};

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("../../tests/fixtures/clock.hex").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            bytes[at..at + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    bytes
}

#[test]
fn clock_guest_stops_transactions_then_stacks_nmi_and_returns_in_both_tv_standards() {
    let rom = rom();
    for pal in [false, true] {
        for hle in [false, true] {
            let mut m = Machine::new();
            m.set_pal(pal);
            m.reset(if hle {
                Boot::HleDisc(include_bytes!("../../tests/fixtures/clock.iso"))
            } else {
                Boot::ExternalBios(&rom)
            })
            .unwrap();
            for event in 1..=2 {
                while m.bus.onchip[0].nmi_ready() {
                    assert!(m.clocks < 9_000_000);
                    m.step_hardware_boundary().unwrap();
                }
                let stopped = m.cpu_state().clone();
                assert_eq!(stopped.status, CpuStatus::Sleeping);
                assert_eq!(m.bus.onchip[0].read8(onchip::SCI + 1), Ok(255));
                assert_eq!(m.bus.onchip[0].read32(onchip::DMAC + 0x30), Ok(0));
                let audio_before = m.audio_generated();
                let mut fixed_progress = false;
                while !m.nmi_pending {
                    m.step_hardware_boundary().unwrap();
                    assert_eq!(m.cpu_state().instructions, stopped.instructions);
                    assert_eq!(m.cpu_state().pc, stopped.pc);
                    fixed_progress |= m.audio_generated() > audio_before;
                    let journal = m.bus.transactions.journals[0].borrow();
                    assert!(journal.waiting.is_none());
                    assert!(journal.operations.is_empty());
                    assert!(m.clocks < 9_000_000);
                }
                assert!(fixed_progress); // fixed clock progresses
                assert_eq!(m.slave_state().status, CpuStatus::Reset);
                assert_eq!(m.bus.onchip[0].read32(onchip::DMAC + 0x30), Ok(2));
                assert!(!m.bus.onchip[0].nmi_ready());
                let wake = m.bus.main_clocks + u64::from(m.bus.onchip[0].clocks_to_event());
                while !m.bus.onchip[0].nmi_ready() {
                    m.step_hardware_boundary().unwrap();
                    assert_eq!(m.cpu_state().instructions, stopped.instructions);
                    assert!(m.bus.main_clocks <= wake + 8);
                }
                assert!(m.bus.main_clocks >= wake);
                while m.cpu_state().exceptions < event {
                    m.step_hardware_boundary().unwrap();
                }
                assert_eq!(
                    m.cpu_state().last_exception.as_ref().map(|e| e.vector),
                    Some(11)
                );
                assert_eq!(m.cpu_state().r[15], stopped.r[15] - 8);
                assert_eq!(m.bus.read32(stopped.r[15] - 8), Ok(stopped.pc));
                assert_eq!(m.bus.read32(stopped.r[15] - 4), Ok(stopped.sr));
                while m.cpu_state().pc != stopped.pc {
                    m.step_hardware_boundary().unwrap();
                    assert!(m.clocks < 9_000_000);
                }
                assert_eq!(m.cpu_state().r[15], stopped.r[15]);
                assert_eq!(m.cpu_state().sr, stopped.sr);
                // Pass recovery before searching for the following standby.
                m.step().unwrap();
            }
        }
    }
}

#[test]
fn standby_cancels_every_unfinished_internal_dma_phase_and_guest_can_restart() {
    for phase in 0..=2 {
        for chunk in [1, 3, 17, 10000] {
            let mut b = Bus::default();
            b.onchip[0].write8(onchip::SCI, 0x37).unwrap();
            for (off, value) in [
                (0, onchip::SCI),
                (4, onchip::SCI + 1),
                (8, 2),
                (12, 0x205),
                (0x30, 1),
            ] {
                b.onchip[0].write32(onchip::DMAC + off, value).unwrap();
            }
            if phase != 0 {
                b.advance_clocks(phase).unwrap();
            }
            b.onchip[0].write8(onchip::SBYCR, 0xE0).unwrap();
            b.for_cpu(Processor::Master).enter_sleep();
            assert_eq!(
                b.onchip[0].read32(onchip::DMAC + 8),
                Ok(if phase == 2 { 1 } else { 2 })
            );
            assert!(b.transactions.journals[2].borrow().waiting.is_none());
            assert!(b.transactions.internal[0].is_none());
            b.advance_clocks(10000).unwrap();
            assert_eq!(b.onchip[0].read8(onchip::SCI + 1), Ok(255));
            b.signal_nmi(Processor::Master);
            let mut remaining = 511;
            while remaining != 0 {
                let n = remaining.min(chunk);
                b.advance_clocks(n).unwrap();
                remaining -= n;
            }
            assert!(!b.onchip[0].nmi_ready());
            b.advance_clocks(1).unwrap();
            assert!(b.onchip[0].nmi_ready());
            assert_eq!(b.onchip[0].read32(onchip::DMAC + 0x30), Ok(2));
            b.onchip[0].write32(onchip::DMAC + 12, 0x205).unwrap();
            b.onchip[0].write32(onchip::DMAC + 0x30, 3).unwrap();
            b.advance_clocks(17).unwrap();
            assert_eq!(b.onchip[0].read8(onchip::SCI + 1), Ok(255));
            // Read-before-clear NMIF and ordinary re-enable are guest actions.
            b.onchip[0].read32(onchip::DMAC + 0x30).unwrap();
            b.onchip[0].write32(onchip::DMAC + 0x30, 1).unwrap();
            b.onchip[0].write8(onchip::SCI, 0x52).unwrap();
            b.advance_clocks(17).unwrap();
            assert_eq!(b.onchip[0].read8(onchip::SCI + 1), Ok(0x52));
        }
    }
}

#[test]
fn clock_completion_preserves_pending_master_reads_and_writes() {
    for fast in [false, true] {
        for pal in [false, true] {
            for write in [false, true] {
                for remaining in [1, 2, 3] {
                    let mut m = Machine::new();
                    m.set_pal(pal);
                    let mut rom = vec![0; BIOS_SIZE];
                    rom[44..48].copy_from_slice(&0x1000u32.to_be_bytes());
                    rom[0x1000..0x1004].copy_from_slice(&[0, 0x2B, 0, 9]);
                    m.bus.load_bios(&rom).unwrap();
                    for (i, word) in [if write { 0x2FF2 } else { 0x60F2 }, 0x001B, 0xAFFD, 9]
                        .iter()
                        .enumerate()
                    {
                        m.bus
                            .for_cpu(Processor::Master)
                            .write16(0xC0000000 + i as u32 * 2, *word)
                            .unwrap();
                    }
                    m.bus.write32(0x26002000, 0x13579BDF).unwrap();
                    m.cpu = crate::cpu::Cpu::start(0xC0000000, 0x26002000);
                    m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
                    m.bus.write8(SMPC + 0x63, 1).unwrap();
                    m.bus
                        .write8(SMPC + 0x1F, if fast { 14 } else { 15 })
                        .unwrap();
                    // Place command completion inside an independently pending
                    // operand phase, without altering the CPU's bus journal.
                    let deadline = m.bus.smpc.clocks_to_event();
                    m.bus.smpc.advance_clocks(deadline - remaining);
                    assert!(!m.issue_cpu(Processor::Master).unwrap());
                    assert!(m.cpu_boundary[0].is_some());
                    for _ in 0..200 {
                        m.step_hardware_boundary().unwrap();
                        if m.cpu_state().exceptions == 1
                            && m.cpu_state().status == CpuStatus::Sleeping
                        {
                            break;
                        }
                    }
                    assert_eq!(m.cpu_state().exceptions, 1);
                    assert_eq!(m.cpu_state().r[15], 0x26002000);
                    assert_eq!(
                        m.bus.read32(0x26002000),
                        Ok(if write { 0x26002000 } else { 0x13579BDF })
                    );
                    if !write {
                        assert_eq!(m.cpu_state().r[0], 0x13579BDF);
                    }
                    assert!(m.cpu_boundary[0].is_none());
                }
            }
        }
    }
}

#[test]
fn cold_boot_cancels_clock_standby_and_pending_wake() {
    let rom = rom();
    for wake in [false, true] {
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        while m.bus.onchip[0].nmi_ready() {
            m.step_hardware_boundary().unwrap();
        }
        if wake {
            while !m.nmi_pending {
                m.step_hardware_boundary().unwrap();
            }
        }
        m.reset(Boot::HlePrototype).unwrap();
        assert!(!m.nmi_pending);
        assert!(m.bus.onchip[0].nmi_ready());
        m.run_steps(10000).unwrap();
        assert_eq!(m.cpu_state().exceptions, 0);
        assert_eq!(m.bus.read32(0x0600E200), Ok(0));
    }
}

#[test]
fn stopped_master_preserves_ram_and_fixed_clock_rtc_across_pll_completion() {
    let mut b = Bus::default();
    for (i, value) in [0x20, 0x26, 1, 1, 0, 0, 0].into_iter().enumerate() {
        b.write8(SMPC + 1 + i as u32 * 2, value).unwrap();
    }
    b.write8(SMPC + 0x63, 1).unwrap();
    b.write8(SMPC + 0x1F, 0x16).unwrap();
    b.advance_clocks(1882).unwrap();
    b.write32(HIGH_WRAM, 0x13579BDF).unwrap();
    b.write32(LOW_WRAM, 0x2468ACE0).unwrap();
    b.write8(BACKUP_RAM + 1, 0xA5).unwrap();
    b.write16(crate::cd::REGISTERS + 12, 0x56).unwrap();
    b.onchip[0].write8(onchip::SBYCR, 0xE0).unwrap();
    b.for_cpu(Processor::Master).enter_sleep();
    b.advance_clocks(26_874_100).unwrap();
    assert!(!b.onchip[0].nmi_ready());
    b.write8(SMPC + 0x63, 1).unwrap();
    b.write8(SMPC + 0x1F, 14).unwrap();
    assert_eq!(
        b.advance_clocks(crate::smpc::Smpc::SYSTEM_RESET_CLOCKS)
            .unwrap(),
        Some(14)
    );
    assert_eq!(b.read32(HIGH_WRAM), Ok(0x13579BDF));
    assert_eq!(b.read32(LOW_WRAM), Ok(0x2468ACE0));
    assert_eq!(b.read8(BACKUP_RAM + 1), Ok(0xA5));
    assert_eq!(b.read16(crate::cd::REGISTERS + 12), Ok(0x56));
    for (off, value) in [(1, 1), (3, 0), (5, 0xF0), (0x63, 1), (0x1F, 0x10)] {
        b.write8(SMPC + off, value).unwrap();
    }
    b.advance_clocks(8600).unwrap();
    assert_eq!(b.read8(SMPC + 0x2F), Ok(1)); // OREG7: one RTC second
    assert!(!b.onchip[0].nmi_ready()); // Bus completion alone is not CPU exception entry
}

#[test]
fn standby_cancels_external_dma_phases_without_replaying_completed_effects() {
    for phase in 0..=10 {
        let mut b = Bus::default();
        b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
        b.write32(HIGH_WRAM + 0x104, 0x13579BDF).unwrap();
        for (off, value) in [
            (0, 0x26000100),
            (4, 0x26000200),
            (8, 2),
            (12, 0x5A01),
            (0x30, 1),
        ] {
            b.onchip[0].write32(onchip::DMAC + off, value).unwrap();
        }
        if phase != 0 {
            b.advance_clocks(phase).unwrap();
        }
        let retained = b.onchip[0].read32(onchip::DMAC + 8).unwrap();
        let completed = b.read32(HIGH_WRAM + 0x200).unwrap();
        b.onchip[0].write8(onchip::SBYCR, 0xE0).unwrap();
        b.for_cpu(Processor::Master).enter_sleep();
        b.advance_clocks(100).unwrap();
        assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(completed));
        assert_eq!(b.onchip[0].read32(onchip::DMAC + 8), Ok(retained));
        assert!(b.transactions.journals[2].borrow().operations.is_empty());
        assert!(b.transactions.writes[0].is_none());
        assert!(b.transactions.active.is_none());
    }
}
