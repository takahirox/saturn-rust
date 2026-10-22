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
    m.step_hardware_boundary().unwrap(); // PC data phase completes at t8
    assert_eq!(m.clocks, 8);
    assert_eq!(m.cpu_state(), &CpuState::default());
    m.bus.load_bios(&rom(0x300, 0x060FE000)).unwrap();
    m.step_hardware_boundary().unwrap(); // SP data phase completes at t16
    assert_eq!(m.clocks, 16);
    assert_eq!(m.cpu_state(), &CpuState::default());
    assert!(!m.issue_cpu(Processor::Master).unwrap());
    assert_eq!((m.cpu_state().pc, m.cpu_state().r[15]), (0x200, 0x060FE000));
    assert_eq!(m.cpu_state().instructions, 0);
    assert!(!m.reset_in_progress());
    m.step().unwrap();
    assert_eq!(m.cpu_state().instructions, 1);
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
