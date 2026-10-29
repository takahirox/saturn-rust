//! Original firmware/media: no host writes to volatile memory or CPU state.
use saturn_rust::{Boot, BootMode, Machine, bus::Memory, media::Media};

fn firmware() -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in include_str!("fixtures/external-startup.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
const IMAGE: &[u8] = include_bytes!("fixtures/external-startup.iso");
const R: u32 = 0x06010000;
fn finish(machine: &mut Machine, chunk: u32) {
    for _ in 0..(2000000 / chunk) {
        machine.run_steps(chunk).unwrap();
        if machine.bus().read32(R + 0xFC).unwrap() != 0 {
            return;
        }
    }
    panic!(
        "firmware deadline exceeded at {:08X}, clock {}",
        machine.cpu_state().pc,
        machine.clocks()
    );
}
fn successful(machine: &Machine) {
    let records: Vec<_> = (0..4)
        .map(|i| machine.bus().read32(R + i * 4).unwrap())
        .collect();
    assert_eq!(machine.bus().read32(R + 0xFC), Ok(0x600D));
    assert_eq!(records, [0x06004004, 0x060FE000, 0x06000000, 48]);
    // CD I/F tables4.1/4.3 and command formats: request CR1..4, response CR1..4.
    // These literal tuples include PAUSE, BUSY, DATATRNS, TOC size and DataEnd
    // word counts; no emulator output is used to generate an expected file.
    let expected = [
        [0x0401, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
        [0, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
        [0x0200, 0, 0, 0, 0x4180, 204, 0, 0],
        [0x0600, 0, 0, 0, 0x0100, 204, 0x0100, 150],
        [0x3000, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
        [0x1080, 171, 0x0080, 2, 0x0400, 0x4101, 0x0100, 171],
        [0x5100, 0, 0, 0, 0x0180, 0, 0, 2],
        [0x6300, 0, 0, 2, 0x4180, 0x4101, 0x0100, 173],
        [0x0600, 0, 0, 0, 0x0100, 2048, 0x0100, 173],
    ];
    for (i, words) in expected.iter().enumerate() {
        let got: Vec<_> = (0..8)
            .map(|j| {
                machine
                    .bus()
                    .read32(R + 0x400 + (i * 8 + j) as u32 * 4)
                    .unwrap()
            })
            .collect();
        assert_eq!(got, *words, "command {i}");
    }
    let expected_records = [
        0x06004004, 0x060FE000, 0x06000000, 48, 1, 1, 1, 0x5353, 0x8000, 2048, 0x41000096,
        0xB0000009,
    ];
    for (i, value) in expected_records.iter().enumerate() {
        assert_eq!(
            machine.bus().read32(R + i as u32 * 4),
            Ok(*value),
            "guest record {i}"
        );
    }
    for (offset, command) in [(0x248, 2), (0x25C, 7), (0x264, 9), (0x260, 8)] {
        assert_eq!(machine.bus().read32(R + offset), Ok(command));
    }
    assert_eq!(machine.bus().read32(R + 0x218), Ok(0x35));
    assert_eq!(machine.bus().read8(0x20100063), Ok(0));
    assert_eq!(machine.mode(), Some(BootMode::ExternalBios));
    assert_eq!(machine.cpu_state().exceptions, 3);
    assert_eq!(machine.slave_state().exceptions, 0);
    for (i, byte) in IMAGE[21 * 2048..23 * 2048].iter().enumerate() {
        assert_eq!(machine.bus().read8(0x06004000 + i as u32), Ok(*byte));
    }
    // HLE pointer/data are absent; these RAM words were never installed.
    assert_eq!(machine.bus().read32(0x06000320), Ok(0));
    assert_eq!(machine.bus().read32(0x06000348), Ok(0));
}

#[test]
fn firmware_loads_and_executes_actual_disc_bytes_then_cold_restarts_without_ejecting_media() {
    let rom = firmware();
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(Media::iso(IMAGE).unwrap());
    finish(&mut m, 10000);
    successful(&m);
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    assert!(
        m.bus().cd.media().is_some(),
        "a cold hardware reset must not eject the physical medium"
    );
    assert_eq!(m.bus().read32(R + 0xFC), Ok(0));
    finish(&mut m, 10000);
    successful(&m);
}

fn patch(rom: &mut [u8], before: u32, after: u32) {
    let mut count = 0;
    for at in (0x200..rom.len() - 4).step_by(2) {
        if rom[at..at + 4] == before.to_be_bytes() {
            rom[at..at + 4].copy_from_slice(&after.to_be_bytes());
            count += 1;
        }
    }
    assert_eq!(count, 1, "unique operand {before:X}");
}

#[test]
fn firmware_reports_absent_media_changed_sectors_and_omitted_selector_without_handoff() {
    for control in 0..4 {
        let mut rom = firmware();
        let mut image = IMAGE.to_vec();
        if control == 1 {
            image[21 * 2048 + 0xF03] = 8;
        }
        if control == 2 {
            image[21 * 2048] ^= 1;
        }
        if control == 3 {
            patch(&mut rom, 0x3000, 0x3100);
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        if control != 0 {
            m.mount_media(Media::iso(&image).unwrap());
        }
        finish(&mut m, 10000);
        assert_eq!(
            m.bus().read32(R + 0xFC),
            Ok(if matches!(control, 1 | 2) {
                0xE002
            } else {
                0xE001
            }),
            "control {control}"
        );
        assert_eq!(m.bus().read32(R), Ok(0), "disc guest must not execute");
        assert_eq!(m.mode(), Some(BootMode::ExternalBios));
        assert_eq!(m.bus().read32(0x06000320), Ok(0));
    }
}

#[test]
fn startup_records_are_independent_of_step_chunks_and_passive_cd_inspection() {
    let rom = firmware();
    let mut reference = None;
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let mut m = Machine::new();
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.mount_media(Media::iso(IMAGE).unwrap());
            let mut steps = 200000;
            while steps != 0 {
                let n = steps.min(chunk);
                m.run_steps(n).unwrap();
                if inspect {
                    let _ = m.debug_read32(0x25818000).unwrap();
                    let _ = m.debug_read16(0x25890024).unwrap();
                }
                steps -= n;
            }
            successful(&m);
            let snapshot = (
                m.clocks(),
                m.cpu_state().clone(),
                m.slave_state().clone(),
                (0..0x520 / 4)
                    .map(|i| m.bus().read32(R + i * 4).unwrap())
                    .collect::<Vec<_>>(),
            );
            if let Some(reference) = &reference {
                assert_eq!(&snapshot, reference);
            }
            reference = Some(snapshot);
        }
    }
}

#[test]
fn guest_sysres_restarts_firmware_media_and_devices_and_retains_backup() {
    let mut rom = firmware();
    rom[0x7F00..0x7F04].copy_from_slice(&1u32.to_be_bytes());
    let mut m = Machine::new();
    m.import_backup_ram(&vec![0xA5; 32768]).unwrap();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(Media::iso(IMAGE).unwrap());
    finish(&mut m, 10000);
    successful(&m);
    assert_eq!(m.backup_ram()[0], 0x5A);
    assert!(m.backup_ram()[1..].iter().all(|v| *v == 0xA5));
    assert!(m.clocks() > 2_687_410);
    assert_eq!(m.bus().read32(0x7F00), Ok(1));
}

#[test]
fn reset_during_pending_command_cancels_volatile_cd_dma_and_cpu_state() {
    let rom = firmware();
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(Media::iso(IMAGE).unwrap());
    for _ in 0..10000 {
        m.step().unwrap();
        if m.debug_read16(0x25890008).unwrap() & 1 == 0 {
            break;
        }
    }
    assert_eq!(m.debug_read16(0x25890008).unwrap() & 1, 0);
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    assert_eq!(m.clocks(), 0);
    assert_eq!(m.slave_state().instructions, 0);
    assert_eq!(m.debug_read16(0x25890008).unwrap() & 1, 1);
    assert_eq!(m.debug_read16(0x2589000C), Ok(0));
    assert_eq!(m.debug_read32(0x25818000), Ok(u32::MAX));
    assert_eq!(m.bus().read32(R + 0x200), Ok(0));
    assert_eq!(m.bus().read32(0x06004000), Ok(0));
    finish(&mut m, 10000);
    successful(&m);
    m.clear_media();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    finish(&mut m, 10000);
    assert_eq!(m.bus().read32(R + 0xFC), Ok(0xE001));
}

#[test]
fn invalid_rom_preserves_selected_medium_and_backup_but_cannot_fall_back_to_hle() {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&firmware())).unwrap();
    m.mount_media(Media::iso(IMAGE).unwrap());
    m.import_backup_ram(&vec![0xA5; 32768]).unwrap();
    finish(&mut m, 10000);
    assert!(m.reset(Boot::ExternalBios(&[0; 8])).is_err());
    assert_eq!(m.mode(), Some(BootMode::ExternalBios));
    assert!(m.bus().cd.media().is_some());
    assert!(m.bus().read32(0).is_err());
    assert_eq!(m.bus().read32(R + 0xFC), Ok(0));
    assert!(m.step().is_err());
    assert!(m.backup_ram().iter().all(|v| *v == 0xA5));
}
