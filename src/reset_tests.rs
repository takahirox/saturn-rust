//! SH7604 §§4.2.2, 7.3/7.11; ST-169 §§2.3.2, 2.3.8.
//! Literal original ROM/clock vectors, through production Machine scheduling.
use super::*;

fn rom(pc: u32, sp: u32) -> Vec<u8> {
    let mut bytes = vec![0; bus::BIOS_SIZE];
    bytes[..4].copy_from_slice(&pc.to_be_bytes());
    bytes[4..8].copy_from_slice(&sp.to_be_bytes());
    for at in [0x200, 0x300] {
        // BRA -2; ADD #1,R0. Both CPUs can make observable progress.
        bytes[at..at + 4].copy_from_slice(&[0xAF, 0xFE, 0x70, 0x01]);
    }
    bytes
}

fn command(m: &mut Machine, command: u8) {
    m.bus.write8(devices::SMPC + 0x63, 1).unwrap();
    m.bus.write8(devices::SMPC + 0x1F, command).unwrap();
}

#[test]
fn cold_vectors_sample_separately_and_publish_together_before_any_instruction() {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060FF000)))
        .unwrap();
    assert_eq!(m.cpu_state(), &CpuState::default());
    assert_eq!(m.clocks, 0);
    // H fig.7.3: each vector is two separately sampled big-endian words.
    // Reset W0=3/A0LW=3 gives T1+six Tw+T2=8 per word.
    for (clock, pc, sp) in [
        (8, 0x11110200, 0x060FF000),
        (16, 0x22220300, 0x060FF000),
        (24, 0x33330400, 0x060ED000),
        (32, 0x44440500, 0x060DC000),
    ] {
        // Reference-device events may yield inside a waited vector word.
        // Preserve the literal word deadlines and inspect before each yield.
        while m.clocks < clock {
            assert_eq!(m.cpu_state(), &CpuState::default());
            m.step_hardware_boundary().unwrap();
        }
        assert_eq!(m.clocks, clock);
        assert_eq!(m.cpu_state(), &CpuState::default());
        m.bus.load_bios(&rom(pc, sp)).unwrap();
    }
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    assert_eq!((m.cpu_state().pc, m.cpu_state().r[15]), (0x200, 0x060FD000));
    assert_eq!(m.cpu_state().instructions, 0);
    assert!(!m.reset_in_progress());
    // The first instruction issues only after both acquired registers publish.
    m.step().unwrap();
    assert_eq!(m.cpu_state().instructions, 1);
}

#[test]
fn every_cold_and_sysres_vector_word_boundary_cancels_cleanly_on_new_boot_selection() {
    for sysres in [false, true] {
        for words in 0..=4 {
            let mut m = Machine::new();
            m.reset(Boot::ExternalBios(&rom(0x200, 0x060FF000)))
                .unwrap();
            if sysres {
                m.step().unwrap();
                command(&mut m, 0x0D);
                while m.cpu_reset_vectors[0].is_none() {
                    m.step_hardware_boundary().unwrap();
                }
            }
            let started = m.clocks;
            for word in 1..=words {
                while m.clocks < started + word * 8 {
                    m.step_hardware_boundary().unwrap();
                    assert_eq!(m.cpu_state(), &CpuState::default());
                }
                assert_eq!(m.clocks, started + word * 8);
                assert_eq!(m.cpu_state(), &CpuState::default());
            }
            m.reset(Boot::ExternalBios(&rom(0x300, 0x060FD000)))
                .unwrap();
            assert_eq!(m.cpu_state(), &CpuState::default());
            for clock in [8, 16, 24, 32] {
                while m.clocks < clock {
                    assert_eq!(m.cpu_state(), &CpuState::default());
                    m.step_hardware_boundary().unwrap();
                }
                assert_eq!(m.clocks, clock);
                assert_eq!(m.cpu_state(), &CpuState::default());
            }
            assert!(!m.issue_cpu(Processor::Master).unwrap());
            assert_eq!(
                (
                    m.cpu_state().pc,
                    m.cpu_state().r[15],
                    m.cpu_state().instructions
                ),
                (0x300, 0x060FD000, 0)
            );
            m.step().unwrap();
            assert_eq!(m.cpu_state().instructions, 1);
        }
    }
}

#[test]
fn sshon_schedules_vectors_and_master_progresses_during_slave_waits() {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060FF000)))
        .unwrap();
    m.step().unwrap();
    command(&mut m, 2);
    while !m.bus.slave_enabled {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.slave_state(), &CpuState::default());
    assert_eq!(m.cpu_reset_vectors[1], Some(onchip::ResetRequest::PowerOn));
    // Keep the master's kernel in its local cache. Its instruction boundaries
    // must continue while the slave uses independent BSC wait settings.
    m.bus
        .for_cpu(Processor::Master)
        .write8(cache::CCR, 1)
        .unwrap();
    m.bus.for_cpu(Processor::Master).fetch16(0x200).unwrap();
    m.bus
        .for_cpu(Processor::Slave)
        .write32(0xFFFFFFE8, 0xA55A00FF)
        .unwrap();
    let before = m.cpu_state().instructions;
    let started = m.clocks;
    while m.cpu_reset_vectors[1].is_some() {
        assert_eq!(m.slave_state().instructions, 0);
        m.step_hardware_boundary().unwrap();
    }
    assert!(m.clocks > started + 4);
    assert!(m.cpu_state().instructions > before);
    assert_eq!(
        (m.slave_state().pc, m.slave_state().r[15]),
        (0x200, 0x060FF000)
    );
    assert_eq!(m.slave_state().instructions, 0);
}

#[test]
fn sshoff_cancels_vectors_waiting_behind_another_processors_video_cycle() {
    // ST-058 §3.3: a CPU VRAM cycle with no active-display grants waits
    // until HBlank. SSHOFF must cancel the slave's queued IF-independent
    // reset reads while the master owns that longer external cycle.
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060FF000)))
        .unwrap();
    m.bus.write16(HIGH_WRAM, 0x60F1).unwrap(); // MOV.W @R15,R0
    m.bus.write16(HIGH_WRAM + 2, 0x001B).unwrap();
    m.cpu_reset_vectors[0] = None;
    m.cpu = Cpu::start(HIGH_WRAM, devices::VRAM | 0x20000000);
    m.bus.write16(devices::VDP2, 0x8000).unwrap();
    m.bus.write16(devices::VDP2 + 0x20, 1).unwrap();
    m.bus.write32(devices::VDP2 + 0x10, 0).unwrap();
    while m.clocks < 16 {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.cpu_state().instructions, 0);
    m.bus.set_slave_enabled(true);
    m.schedule_cpu_reset(Processor::Slave, onchip::ResetRequest::PowerOn);
    command(&mut m, 3);
    while m.bus.slave_enabled {
        m.step_hardware_boundary().unwrap();
        assert_eq!(m.slave_state(), &CpuState::default());
    }
    assert_eq!(m.cpu_reset_vectors[1], None);
    for _ in 0..200 {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.slave_state(), &CpuState::default());
    // A later SSHON samples new vectors, with no completion from the canceled
    // request. The master has released its video cycle by then.
    m.bus.load_bios(&rom(0x300, 0x060FE000)).unwrap();
    command(&mut m, 2);
    while !m.bus.slave_enabled || m.cpu_reset_vectors[1].is_some() {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.slave_state().pc, 0x300);
    assert_eq!(m.slave_state().r[15], 0x060FE000);
    assert_eq!(m.slave_state().instructions, 0);
}

#[test]
fn sysres_starts_a_new_vector_journal_and_cold_selection_cancels_partial_vectors() {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060FF000)))
        .unwrap();
    m.step().unwrap();
    m.bus.load_bios(&rom(0x300, 0x060FE000)).unwrap();
    command(&mut m, 0x0D);
    while m.cpu_reset_vectors[0].is_none() {
        m.step_hardware_boundary().unwrap();
    }
    assert_eq!(m.cpu_state(), &CpuState::default());
    assert_eq!(m.slave_state(), &CpuState::default());
    m.step_hardware_boundary().unwrap(); // old SYSRES PC completed, SP pending
    assert_eq!(m.cpu_state().instructions, 0);
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060FD000)))
        .unwrap();
    m.step().unwrap();
    assert_eq!(m.cpu_state().r[15], 0x060FD000);
    assert_eq!(m.cpu_state().pc, 0x202);
    // A mode change or rejected file must also cancel a not-yet-complete read.
    m.reset(Boot::ExternalBios(&rom(0x300, 0x060FE000)))
        .unwrap();
    m.step_hardware_boundary().unwrap();
    m.reset(Boot::HlePrototype).unwrap();
    m.run_steps(6).unwrap();
    assert_eq!((m.cpu_state().pc, m.cpu_state().r[1]), (FIXTURE_PC + 14, 7));
    assert!(!m.reset_in_progress());
    m.reset(Boot::ExternalBios(&rom(0x300, 0x060FE000)))
        .unwrap();
    m.step_hardware_boundary().unwrap();
    assert!(m.reset(Boot::ExternalBios(&[])).is_err());
    assert_eq!(m.cpu_state(), &CpuState::default());
    assert!(!m.reset_in_progress());
}

#[test]
fn cold_boot_selection_discards_pending_vbe_sample_and_accepted_erase_job() {
    for clocks in [224 * 1708 + 100, 224 * 1708 + 1300] {
        let mut m = Machine::new();
        let image = rom(0x200, 0x060FF000);
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for (o, v) in [(0, 8), (6, 0x1234), (8, 0), (10, 0x80FF), (2, 3)] {
            m.bus.write16(vdp1::REGISTERS + o, v).unwrap();
        }
        m.bus.vdp1.vblank_out();
        m.bus.advance_clocks(clocks).unwrap();
        assert_eq!(
            m.bus.vdp1.display_pixel(0, 0),
            if clocks % 1708 < 1280 { 0 } else { 0x1234 }
        );
        m.reset(Boot::ExternalBios(&image)).unwrap();
        m.bus.vdp1.draw_buffer_mut().fill(0xAA);
        m.run_steps(2000).unwrap();
        assert!(m.bus.vdp1.draw_buffer().iter().all(|&v| v == 0xAA));
        assert_eq!(m.bus.vdp1.display_pixel(0, 0), 0);
        assert_eq!(m.bus.read16(vdp1::REGISTERS + 0x16).unwrap() & 8, 0);
        assert_eq!(m.bus.read32(scu::SCU + 0xA4).unwrap() & 0x2000, 0);
    }
}

#[test]
fn sndoff_completion_cancels_a_rearmed_sound_input_before_division_retires() {
    fn until(m: &mut Machine, predicate: impl Fn(&Machine) -> bool) {
        for _ in 0..5000 {
            if predicate(m) {
                return;
            }
            m.step().unwrap();
        }
        panic!(
            "SMPC phase did not complete: clocks={} enabled={} sound={:?}",
            m.clocks(),
            m.bus.sound.enabled,
            m.sound_state()
        );
    }
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(0x200, 0x060ff000)))
        .unwrap();
    for line in include_str!("../tests/fixtures/m68k-irq.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        if !(0x2000..0x3000).contains(&at) {
            continue;
        }
        for (i, word) in words.enumerate() {
            m.bus
                .write16(
                    sound::SOUND_RAM + (at - 0x2000 + i * 2) as u32,
                    u16::from_str_radix(word, 16).unwrap(),
                )
                .unwrap();
        }
    }
    for offset in [0x41e, 0x424, 0x426, 0x428] {
        m.bus.write16(sound::SCSP + offset, 32).unwrap();
    }
    command(&mut m, 6);
    until(&mut m, |m| m.bus.sound.enabled && m.sound_state().stopped);
    command(&mut m, 7);
    let off_start = m.clocks();
    // SMPC's 807-reference-clock command delay is independent of CPU work.
    // Start the interrupt late enough that SNDOFF lands inside DIVU.
    until(&mut m, |m| m.clocks() >= off_start + 200);
    m.bus.write16(sound::SCSP + 0x420, 32).unwrap();
    until(&mut m, |m| m.sound_state().pc == 0x320);
    assert!(m.bus.sound.enabled);
    assert_eq!(m.sound_state().exceptions, 1);
    m.bus.write16(sound::SCSP + 0x422, 32).unwrap();
    let low = m.clocks();
    until(&mut m, |m| m.clocks() >= low + 80);
    m.bus.write16(sound::SCSP + 0x420, 32).unwrap();
    until(&mut m, |m| !m.bus.sound.enabled);
    assert_eq!(
        m.sound_state().d[0],
        7,
        "SNDOFF must precede DIVU retirement"
    );
    m.bus.write16(sound::SCSP + 0x422, 32).unwrap();
    m.bus.write32(sound::SOUND_RAM + 4, 0x400).unwrap();
    m.bus.write16(sound::SOUND_RAM + 0x400, 0x4e72).unwrap();
    m.bus.write16(sound::SOUND_RAM + 0x402, 0x2700).unwrap();
    command(&mut m, 6);
    until(&mut m, |m| m.bus.sound.enabled && m.sound_state().stopped);
    m.run_steps(1000).unwrap();
    assert_eq!(m.sound_state().exceptions, 0);
    assert_eq!(m.sound_state().pc, 0x404);
    assert_eq!(m.sound_state().d[0], 7);
}
