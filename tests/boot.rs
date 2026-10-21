use saturn_rust::{bus::*, cpu::*, *};

fn bios() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    bytes[0..4].copy_from_slice(&0x100_u32.to_be_bytes());
    bytes[4..8].copy_from_slice(&0x0610_0000_u32.to_be_bytes());
    bytes[0x100..0x102].copy_from_slice(&0xD001_u16.to_be_bytes());
    bytes[0x108..0x10C].copy_from_slice(&0x05FE00CC_u32.to_be_bytes());
    // MOV.B R0,@R0 to a real unsupported SCU port, distinct from an absent
    // device's normal undriven data and a ROM's missing write-enable pin.
    bytes[0x102..0x104].copy_from_slice(&0x2000_u16.to_be_bytes());
    bytes
}

#[test]
fn external_bios_uses_power_on_vectors_and_reset_registers() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&bios())).unwrap();
    let s = machine.cpu_state();
    assert_eq!(s.pc, 0x100);
    assert_eq!(s.r[15], 0x0610_0000);
    assert_eq!((s.sr, s.vbr), (0xF0, 0));
    assert_eq!((s.pr, s.gbr, s.mach, s.macl), (0, 0, 0, 0));
    assert_eq!(s.status, CpuStatus::Running);
    machine.step().unwrap();
    assert_eq!(machine.cpu_state().r[0], 0x05FE00CC);
    assert_eq!(machine.bus().read32(LOW_WRAM), Ok(0));
    assert_eq!(machine.bus().read32(HIGH_WRAM), Ok(0));
}

#[test]
fn missing_short_and_long_bios_fail_with_no_hle_or_stale_rom() {
    let mut machine = Machine::new();
    for size in [0, BIOS_SIZE - 1, BIOS_SIZE + 1] {
        machine.reset(Boot::ExternalBios(&bios())).unwrap();
        machine.step().unwrap();
        let result = machine.reset(Boot::ExternalBios(&vec![0; size]));
        assert_eq!(
            result,
            Err(MachineError::Bus(BusError::BiosLength { actual: size }))
        );
        assert_eq!(machine.mode(), Some(BootMode::ExternalBios));
        assert_eq!(machine.cpu_state(), &CpuState::default());
        assert!(machine.bus().read8(0).is_err());
        assert_eq!(machine.step(), Err(MachineError::Cpu(CpuError::NotRunning)));
        assert!(result.unwrap_err().to_string().contains("512 KiB"));
    }
}

#[test]
fn hle_boot_runs_original_program_without_rom() {
    let mut machine = Machine::new();
    machine.reset(Boot::HlePrototype).unwrap();
    assert_eq!(machine.cpu_state().pc, FIXTURE_PC);
    assert_eq!(machine.cpu_state().r[15], 0x0610_0000);
    assert!(machine.bus().read8(0).is_err());
    // Independently calculated trace, including the single BRA delay slot.
    let trace = [
        (2, 5, 0),
        (4, 4, 0),
        (6, 4, 4),
        (8, 4, 4),
        (12, 4, 7),
        (14, 4, 7),
    ];
    for (offset, r0, r1) in trace {
        machine.step().unwrap();
        let s = machine.cpu_state();
        assert_eq!((s.pc, s.r[0], s.r[1]), (FIXTURE_PC + offset, r0, r1));
    }
    assert_eq!(machine.cpu_state().instructions, 6);
    machine.run_steps(100).unwrap();
    assert_eq!(machine.cpu_state().pc, FIXTURE_PC + 14);
    assert_eq!(machine.cpu_state().r[1], 7);
}

#[test]
fn mode_switches_clear_rom_ram_registers_delay_counters_and_faults() {
    let mut machine = Machine::new();
    machine.reset(Boot::HlePrototype).unwrap();
    let initial = machine.cpu_state().clone();
    machine.run_steps(4).unwrap();
    assert!(machine.cpu_state().delay_target.is_some());
    machine.reset(Boot::ExternalBios(&bios())).unwrap();
    assert_eq!(machine.cpu_state().delay_target, None);
    assert_eq!(machine.cpu_state().instructions, 0);
    assert_eq!(machine.bus().read32(FIXTURE_PC), Ok(0));
    machine.step().unwrap();
    assert!(machine.step().is_err());
    machine.reset(Boot::HlePrototype).unwrap();
    assert_eq!(machine.mode(), Some(BootMode::HlePrototype));
    assert_eq!(machine.cpu_state(), &initial);
    assert!(machine.bus().read8(0).is_err());
    machine.run_steps(6).unwrap();
    machine.reset(Boot::HlePrototype).unwrap();
    assert_eq!(machine.cpu_state(), &initial);
}

#[test]
fn run_is_bounded_and_stops_at_first_fault() {
    let mut machine = Machine::new();
    machine.reset(Boot::HlePrototype).unwrap();
    let initial = machine.cpu_state().clone();
    for count in [0, MAX_RUN_STEPS + 1, u32::MAX] {
        assert_eq!(
            machine.run_steps(count),
            Err(MachineError::StepLimit(count))
        );
        assert_eq!(machine.cpu_state(), &initial);
    }
    machine.run_steps(MAX_RUN_STEPS).unwrap();
    assert_eq!(machine.cpu_state().instructions, MAX_RUN_STEPS as u64);
    machine.reset(Boot::ExternalBios(&bios())).unwrap();
    assert!(machine.run_steps(20).is_err());
    assert_eq!(machine.cpu_state().instructions, 1);
    assert_eq!(machine.cpu_state().pc, 0x102);
}
