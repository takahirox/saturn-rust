use saturn_rust::{
    Boot, Machine, MachineError,
    backup::{BACKUP_RAM, BACKUP_SIZE, BACKUP_WINDOW},
    bus::{BIOS_SIZE, Bus, BusError, Memory},
    cpu::CpuStatus,
};

fn original_rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/backup-pad.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let mut address = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            let word = u16::from_str_radix(word, 16).unwrap();
            bytes[address..address + 2].copy_from_slice(&word.to_be_bytes());
            address += 2;
        }
    }
    bytes
}

#[test]
fn backup_byte_lanes_endpoints_and_cache_through_access_do_not_alias_work_ram() {
    let mut bus = Bus::default();
    for index in 0..BACKUP_SIZE {
        bus.write8(
            BACKUP_RAM + 2 * index as u32 + 1,
            (index ^ (index >> 8)) as u8,
        )
        .unwrap();
    }
    for index in 0..BACKUP_SIZE {
        let address = BACKUP_RAM + 2 * index as u32 + 1;
        let value = (index ^ (index >> 8)) as u8;
        assert_eq!(bus.read8(address), Ok(value));
        assert_eq!(bus.read8(address + 0x2000_0000), Ok(value));
    }
    bus.write8(BACKUP_RAM + BACKUP_WINDOW - 1 + 0x2000_0000, 0xFE)
        .unwrap();
    assert_eq!(bus.read8(BACKUP_RAM + BACKUP_WINDOW - 1), Ok(0xFE));
    assert_eq!(bus.read32(0x0020_0000), Ok(0));
    assert_eq!(bus.read32(0x0600_0000), Ok(0));
    assert!(matches!(
        bus.read8(BACKUP_RAM - 1),
        Err(BusError::UnsupportedDevice { .. }) // mirrored SMPC EXLE, write-only
    ));
    assert!(matches!(
        bus.read8(BACKUP_RAM + BACKUP_WINDOW),
        Err(BusError::Unmapped { .. })
    ));
    // Even lanes are unconnected; wider bus cycles strobe only odd bytes.
    let before = bus.read8(BACKUP_RAM + 1).unwrap();
    bus.write8(BACKUP_RAM, 42).unwrap();
    assert_eq!(bus.read8(BACKUP_RAM), Ok(255));
    assert_eq!(bus.read16(BACKUP_RAM), Ok(0xFF00 | u16::from(before)));
    bus.write16(BACKUP_RAM, 0x1267).unwrap();
    assert_eq!(bus.read16(BACKUP_RAM), Ok(0xFF67));
    bus.write32(BACKUP_RAM, 0x12345678).unwrap();
    assert_eq!(bus.read32(BACKUP_RAM), Ok(0xFF34FF78));
    assert_eq!(bus.read8(BACKUP_RAM + 1), Ok(0x34));
    assert_eq!(bus.read8(BACKUP_RAM + 3), Ok(0x78));
}

#[test]
fn packed_import_is_atomic_and_survives_every_boot_mode_and_rejected_boot() {
    let mut machine = Machine::new();
    assert_eq!(machine.backup_ram(), &[0; BACKUP_SIZE]);
    let bytes: Vec<_> = (0..BACKUP_SIZE).map(|i| (i ^ (i >> 8)) as u8).collect();
    machine.import_backup_ram(&bytes).unwrap();
    for size in [0, 1, BACKUP_SIZE - 1, BACKUP_SIZE + 1, BACKUP_SIZE * 2] {
        assert_eq!(
            machine.import_backup_ram(&vec![0xEE; size]),
            Err(MachineError::Bus(BusError::BackupLength { actual: size }))
        );
        assert_eq!(machine.backup_ram(), bytes);
    }
    for boot in [
        Boot::HlePrototype,
        Boot::ExternalBios(&original_rom()),
        Boot::HleDisc(include_bytes!("../web/star-collector.iso")),
        Boot::ExternalBios(&[]),
        Boot::HleDisc(&[]),
    ] {
        let _ = machine.reset(boot);
        assert_eq!(machine.backup_ram(), bytes);
        assert_eq!(machine.bus().read8(BACKUP_RAM + 1), Ok(bytes[0]));
        assert_eq!(
            machine.bus().read8(BACKUP_RAM + BACKUP_WINDOW - 1),
            Ok(bytes[BACKUP_SIZE - 1])
        );
    }
    assert!(Machine::new().backup_ram().iter().all(|&byte| byte == 0));
}

#[test]
fn guest_reads_and_writes_battery_ram_and_reads_second_pad_after_reset() {
    let rom = original_rom();
    let mut machine = Machine::new();
    for (initial, buttons, expected_pad) in [(0, 1, 0x3E), (1, 8, 0x37), (2, 0, 0x3F)] {
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        assert_eq!(machine.backup_ram()[0], initial);
        machine.set_port2_buttons(buttons);
        machine.set_buttons(0x1FFF); // Port 1 must not leak into the second pad.
        machine.run_steps(83).unwrap();
        assert_eq!(machine.cpu_state().status, CpuStatus::Running);
        assert_eq!(machine.cpu_state().instructions, 83);
        assert_eq!(machine.backup_ram()[0], initial + 1);
        assert_eq!(machine.bus().read32(0x0600_1000), Ok(0x20));
        assert_eq!(machine.bus().read32(0x0600_1004), Ok(expected_pad));
    }
    let exported = machine.backup_ram().to_vec();
    let mut restored = Machine::new();
    restored.import_backup_ram(&exported).unwrap();
    restored.reset(Boot::ExternalBios(&rom)).unwrap();
    restored.run_steps(83).unwrap();
    assert_eq!(restored.backup_ram()[0], 4);
}

#[test]
fn diagnostic_guest_also_executes_from_a_locally_built_hle_disc() {
    // Use the independently authored ISO container, replacing its first-read
    // program with the position-independent test guest. No emulator shortcuts.
    let mut image = include_bytes!("../web/star-collector.iso").to_vec();
    let program = &original_rom()[0x100..0x150];
    image[21 * 2048..21 * 2048 + program.len()].copy_from_slice(program);
    image[0xF4..0xF8].copy_from_slice(&(program.len() as u32).to_be_bytes());
    let record = 20 * 2048 + 68;
    image[record + 10..record + 14].copy_from_slice(&(program.len() as u32).to_le_bytes());
    image[record + 14..record + 18].copy_from_slice(&(program.len() as u32).to_be_bytes());
    let mut machine = Machine::new();
    machine.reset(Boot::HleDisc(&image)).unwrap();
    machine.set_port2_buttons(1);
    machine.run_steps(86).unwrap(); // Three AIP instructions + the same guest.
    assert_eq!(machine.backup_ram()[0], 1);
    assert_eq!(machine.bus().read32(0x0600_1004), Ok(0x3E));
    machine.set_port2_buttons(0);
    machine.run_steps(12).unwrap();
    assert_eq!(machine.bus().read32(0x0600_1004), Ok(0x3F));
    assert_eq!(machine.backup_ram()[0], 1); // Polling does not repeatedly increment.
}
