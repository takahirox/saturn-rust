//! Original native + browser-shared guest, independently expected mailbox results.
//! ST-202 §§1.1/1.2/3–4; ST-169 table 1.1, §2.3; SH7604 §§4/5/11.
use saturn_rust::{
    Boot, CLOCKS_PER_STEP, Machine, MachineError,
    bus::{BIOS_SIZE, Bus, BusError, Memory, Processor},
    cpu::{CpuError, CpuState, CpuStatus},
    devices::SMPC,
    onchip::{FRT, IPRB, VCRC},
};

fn rom() -> Vec<u8> {
    parse_rom(include_str!("fixtures/dual-sh2.hex"))
}

fn parse_rom(source: &str) -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in source.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let mut address = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for word in parts {
            let word = u16::from_str_radix(word, 16).unwrap();
            bytes[address..address + 2].copy_from_slice(&word.to_be_bytes());
            address += 2;
        }
    }
    bytes
}

#[test]
fn guest_compare_irq_waits_for_the_edge_leaving_ocra_and_wakes_sleep() {
    // SH7604 §§11.4.3/11.4.6, figs. 11.7/11.11. Each phi/128 count
    // edge takes 128 clocks, including BSC waits during guest setup/SLEEP.
    for compare in [0, 3] {
        let mut image = parse_rom(include_str!("fixtures/frt-compare.hex"));
        image[0x114..0x116].copy_from_slice(&(0xE000u16 | compare).to_be_bytes());
        let mut machine = Machine::new();
        machine.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..100 {
            if machine.cpu_state().pc == 0x13C {
                break;
            }
            machine.step().unwrap();
        }
        assert_eq!(machine.cpu_state().pc, 0x13C);
        let deadline = machine.clocks() + 128 * (u64::from(compare) + 1);
        machine.step().unwrap(); // Start timer, including this ROM fetch's Tw.
        machine.step().unwrap(); // Execute SLEEP, also fetched from ROM.
        while machine.clocks() + u64::from(CLOCKS_PER_STEP) < deadline {
            machine.step().unwrap();
        }
        assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(machine.cpu_state().exceptions, 0);
        assert_eq!(machine.bus().read8(FRT + 1).unwrap(), 1);
        assert_eq!(machine.bus().read8(FRT + 2).unwrap(), 0);
        assert_eq!(machine.bus().read8(FRT + 3).unwrap(), compare as u8);
        machine.step().unwrap(); // Edge leaving OCRA: clear FRC and set OCFA.
        assert_eq!(machine.cpu_state().exceptions, 0);
        assert_eq!(machine.bus().read8(FRT + 1).unwrap(), 9);
        assert_eq!(machine.bus().read8(FRT + 2).unwrap(), 0);
        assert_eq!(machine.bus().read8(FRT + 3).unwrap(), 0);
        machine.step().unwrap(); // Next CPU boundary accepts the level request.
        assert_eq!(machine.cpu_state().pc, 0x200);
        assert_eq!(machine.cpu_state().exceptions, 1);
        let exception = machine.cpu_state().last_exception.as_ref().unwrap();
        assert_eq!((exception.vector, exception.saved_pc), (32, 0x140));
        assert_eq!(machine.bus().read32(0x060FFFF8).unwrap(), 0x140);
        assert_eq!(machine.bus().read32(0x060FFFFC).unwrap(), 0);
        machine.run_steps(100).unwrap();
        assert_eq!(machine.bus().read32(0x06002000).unwrap(), 9);
        assert_eq!(machine.bus().read8(FRT + 1).unwrap(), 1);
        assert_eq!(machine.cpu_state().exceptions, 1);
        assert_eq!(machine.cpu_state().r[15], 0x06100000);
        assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    }
}

#[test]
fn dual_guest_runs_both_cpus_capture_irqs_slave_restart_and_master_nmi() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    assert_eq!(machine.slave_state(), &CpuState::default());
    // Observe first startup independently of terminal state: two live register
    // banks, separate stack and on-chip configuration, shared guest mailbox.
    for _ in 0..2000 {
        machine.step().unwrap();
        if machine.bus().read32(0x06002004).unwrap() == 1 {
            break;
        }
    }
    assert_eq!(machine.bus().read32(0x06002004).unwrap(), 1);
    assert!(machine.slave_state().instructions > 0);
    assert_eq!(machine.slave_state().r[14], 17);
    assert_eq!(machine.cpu_state().r[14], 42);
    assert_eq!(machine.slave_state().r[15], 0x060FE000);
    assert_eq!(machine.cpu_state().r[15], 0x06100000);
    assert_eq!(machine.bus().read16(IPRB).unwrap(), 0x0500);
    machine.run_steps(3000).unwrap();
    let expected = [1, 2, 35, 42, 2, 0x1234, 2, 0x5678, 2, 42, 0x600D];
    for (i, word) in expected.into_iter().enumerate() {
        assert_eq!(
            machine.bus().read32(0x06002000 + i as u32 * 4).unwrap(),
            word,
            "mailbox {i}"
        );
    }
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 3); // Two FRT IRQs and one NMI.
    assert_eq!(
        machine.cpu_state().last_exception.as_ref().unwrap().vector,
        11
    );
    assert_eq!(machine.cpu_state().r[15], 0x06100000);
    assert_eq!(machine.cpu_state().sr, 0xF0);
    assert_eq!(machine.bus().read32(0x0600202C).unwrap(), 0x34E);
    assert_eq!(machine.bus().read32(0x06002030).unwrap(), 0xF0);
    assert_eq!(machine.bus().read32(0x06002034).unwrap(), 1);
    assert_eq!(machine.slave_state(), &CpuState::default()); // SSHOFF reset domain.
    let sleeping = machine.cpu_state().clone();
    machine.run_steps(200).unwrap();
    assert_eq!(machine.cpu_state(), &sleeping);

    // Cold reset drops both processors, timer/command state and shared RAM.
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    assert_eq!(machine.clocks(), 0);
    assert_eq!(machine.slave_state(), &CpuState::default());
    assert_eq!(machine.bus().read8(FRT).unwrap(), 1);
    assert_eq!(machine.bus().read8(SMPC + 0x63).unwrap(), 0);
    assert_eq!(machine.bus().read32(0x06002000).unwrap(), 0);
    machine.run_steps(4000).unwrap();
    assert_eq!(machine.cpu_state(), &sleeping);
}

#[test]
fn cpu_local_frt_views_share_only_external_memory_and_capture_wires() {
    let mut bus = Bus::default();
    for (processor, high, low, vector) in [
        (Processor::Master, 0x12, 0x34, 0x4142),
        (Processor::Slave, 0x56, 0x78, 0x4344),
    ] {
        let mut view = bus.for_cpu(processor);
        view.write8(FRT + 2, high).unwrap();
        view.write8(FRT + 3, low).unwrap();
        view.write16(VCRC, vector).unwrap();
        view.write32(0x06001000, u32::from(vector)).unwrap();
    }
    assert_eq!(bus.read32(0x06001000).unwrap(), 0x4344);
    assert_eq!(bus.read16(VCRC).unwrap(), 0x4142);
    assert_eq!(bus.for_cpu(Processor::Slave).read16(VCRC).unwrap(), 0x4344);
    bus.write16(0x21800000, 0).unwrap();
    assert_eq!(bus.read8(FRT + 1).unwrap(), 0x80);
    assert_eq!(bus.read8(FRT + 8).unwrap(), 0x12);
    assert_eq!(bus.read8(FRT + 9).unwrap(), 0x34);
    assert_eq!(bus.for_cpu(Processor::Slave).read8(FRT + 1).unwrap(), 0);
    for address in [0x01000000, 0x01800000, 0x21000000, 0x21800000] {
        assert_eq!(bus.read16(address), Ok(0xFFFF)); // no read responder or strobe
        assert!(bus.write8(address, 0).is_err());
        assert_eq!(bus.write32(address, 0), Ok(())); // word strobes in the four-byte select
        assert!(bus.write16(address + 1, 0).is_err());
    }
    // A held-reset slave ignores a strobe; it must not begin with a stale IRQ.
    bus.write16(0x21000000, 0xFFFF).unwrap();
    assert_eq!(bus.for_cpu(Processor::Slave).read8(FRT + 1).unwrap(), 0);
}

#[test]
fn slave_fault_identifies_processor_and_stops_the_entire_machine_until_reset() {
    let mut image = rom();
    // Corrupt the slave's stack literal, then its first capture IRQ must report
    // a slave exception-entry bus error while preserving the master's CPU.
    let needle = 0x060FE000u32.to_be_bytes();
    let offset = image.windows(4).position(|w| w == needle).unwrap();
    image[offset..offset + 4].copy_from_slice(&0x08000010u32.to_be_bytes());
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&image)).unwrap();
    let error = machine.run_steps(4000).unwrap_err();
    assert!(matches!(
        error,
        MachineError::SlaveCpu(CpuError::ExceptionEntry { vector: 0x42, .. })
    ));
    assert!(error.to_string().contains("Slave SH-2"));
    let master = machine.cpu_state().clone();
    let slave = machine.slave_state().clone();
    let clocks = machine.clocks();
    assert_eq!(machine.step(), Err(error));
    assert_eq!(machine.cpu_state(), &master);
    assert_eq!(machine.slave_state(), &slave);
    assert_eq!(machine.clocks(), clocks);
    assert!(matches!(slave.status, CpuStatus::Faulted(_)));
    machine.reset(Boot::HlePrototype).unwrap();
    machine.step().unwrap();
    // The first uncached CS3 fetch inserts six reset-state BSC Tw clocks.
    assert_eq!(machine.clocks(), u64::from(CLOCKS_PER_STEP) + 6);
    assert_eq!(machine.slave_state(), &CpuState::default());
    assert_eq!(machine.fault(), None);
    assert!(matches!(
        machine.bus().read32(0),
        Err(BusError::Unmapped { .. })
    ));
}

#[test]
fn rejected_cold_boot_clears_both_cpus_pending_devices_and_volatile_ram_but_keeps_backup() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    for _ in 0..2000 {
        machine.step().unwrap();
        if machine.bus().read32(0x06002004).unwrap() == 1 {
            break;
        }
    }
    assert!(machine.slave_state().instructions > 0);
    let backup = vec![0xA5; 32768];
    machine.import_backup_ram(&backup).unwrap();
    assert!(machine.reset(Boot::ExternalBios(&[])).is_err());
    assert_eq!(machine.cpu_state(), &CpuState::default());
    assert_eq!(machine.slave_state(), &CpuState::default());
    assert_eq!(machine.clocks(), 0);
    assert_eq!(machine.bus().read8(SMPC + 0x63).unwrap(), 0);
    assert_eq!(machine.bus().read16(IPRB).unwrap(), 0);
    assert_eq!(machine.bus().read32(0x06002004).unwrap(), 0);
    assert_eq!(machine.backup_ram(), &backup);
    assert_eq!(machine.step(), Err(MachineError::Cpu(CpuError::NotRunning)));
}
