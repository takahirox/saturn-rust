//! Original firmware/media: no host writes to volatile memory or CPU state.
use saturn_rust::{Boot, BootMode, Machine, bus::Memory, media::Media};

fn decode(source: &str) -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in source.lines().filter(|s| !s.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
fn firmware() -> Vec<u8> {
    decode(include_str!("fixtures/external-startup.hex"))
}
const IMAGE: &[u8] = include_bytes!("fixtures/external-startup.iso");
const R: u32 = 0x06010000;
#[derive(Clone, Copy)]
struct Disc {
    image: &'static [u8],
    extent: usize,
    size: usize,
    destination: u32,
    stack: u32,
    aip: u32,
    root: u32,
    directory_size: u32,
    file_offset: usize,
    sectors: &'static [u32],
}
const DISCS: [Disc; 3] = [
    Disc {
        image: IMAGE,
        extent: 21,
        size: 4096,
        destination: 0x06004000,
        stack: 0x060FE000,
        aip: 0x06002E20,
        root: 20,
        directory_size: 2048,
        file_offset: 68,
        sectors: &[0, 1, 16, 17, 20, 21, 22],
    },
    Disc {
        image: include_bytes!("fixtures/external-startup-moved.iso"),
        extent: 35,
        size: 4093,
        destination: 0x06030000,
        stack: 0x060FC000,
        aip: 0x06002E40,
        root: 27,
        directory_size: 4096,
        file_offset: 92,
        sectors: &[0, 1, 16, 17, 18, 27, 28, 35, 36],
    },
    Disc {
        image: include_bytes!("fixtures/external-startup-long.iso"),
        extent: 48,
        size: 6145,
        destination: 0x06050000,
        stack: 0x060FA000,
        aip: 0x06002E80,
        root: 31,
        directory_size: 4096,
        file_offset: 116,
        sectors: &[0, 1, 16, 17, 31, 32, 48, 49, 50, 51],
    },
];
fn finish(machine: &mut Machine, chunk: u32) {
    for _ in 0..(2000000 / chunk) {
        machine.run_steps(chunk).unwrap();
        if machine.bus().read32(R + 0xFC).unwrap() != 0 {
            return;
        }
    }
    panic!(
        "firmware deadline exceeded at {:08X}, clock {}, phase {:?}, reads {:?}",
        machine.cpu_state().pc,
        machine.clocks(),
        machine.bus().read32(R + 0x32C),
        machine.bus().read32(R + 0x330)
    );
}
fn successful(machine: &Machine, disc: Disc) {
    let records: Vec<_> = (0..4)
        .map(|i| machine.bus().read32(R + i * 4).unwrap())
        .collect();
    assert_eq!(machine.bus().read32(R + 0xFC), Ok(0x600D));
    assert_eq!(records, [disc.destination + 4, disc.stack, 0x06000000, 48]);
    // CD I/F tables4.1/4.3 and command formats: request CR1..4, response CR1..4.
    // These literal tuples include PAUSE, BUSY, DATATRNS, TOC size and DataEnd
    // word counts; no emulator output is used to generate an expected file.
    let mut expected = vec![
        [0x0401, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
        [0, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
        [0x0200, 0, 0, 0, 0x4180, 204, 0, 0],
        [0x0600, 0, 0, 0, 0x0100, 204, 0x0100, 150],
        [0x3000, 0, 0, 0, 0x0180, 0x4101, 0x0100, 150],
    ];
    for &lba in disc.sectors {
        let fad = lba + 150;
        expected.extend([
            [0x1080, fad, 0x80, 1, 0x0400, 0x4101, 0x0100, fad],
            [0x5100, 0, 0, 0, 0x0180, 0, 0, 1],
            [0x6300, 0, 0, 1, 0x4180, 0x4101, 0x0100, fad + 1],
            [0x0600, 0, 0, 0, 0x0100, 1024, 0x0100, fad + 1],
        ]);
    }
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
        disc.destination + 4,
        disc.stack,
        0x06000000,
        48,
        1,
        disc.sectors.len() as u32,
        1,
        0x5353,
        0x8000,
        1024,
        0x41000096,
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
    assert_eq!(
        machine.cpu_state().exceptions,
        disc.sectors.len() as u64 + 2
    );
    assert_eq!(machine.slave_state().exceptions, 0);
    for (i, byte) in disc.image[disc.extent * 2048..disc.extent * 2048 + disc.size]
        .iter()
        .enumerate()
    {
        assert_eq!(
            machine.bus().read8(disc.destination + i as u32),
            Ok(*byte),
            "file byte {i}"
        );
    }
    // Partial final sectors must not overwrite even one byte beyond the file.
    assert_eq!(
        machine
            .bus()
            .read32(disc.destination + disc.size.next_multiple_of(4) as u32),
        Ok(0)
    );
    assert_eq!(
        machine.bus().read8(disc.destination + disc.size as u32),
        Ok(0)
    );
    for i in 0..4096 {
        assert_eq!(
            machine.bus().read8(0x06002000 + i),
            Ok(disc.image[i as usize])
        );
    }
    assert_eq!(machine.bus().read32(R + 0x2F0), Ok(disc.aip + 4));
    assert_eq!(machine.bus().read32(R + 0x2F4), Ok(0x06001000));
    assert_eq!(machine.slave_state().r[15], 0x06001000);
    assert_eq!(machine.bus().read32(R + 0x308), Ok(disc.stack));
    assert_eq!(machine.bus().read32(R + 0x310), Ok(disc.destination));
    assert_eq!(machine.bus().read32(R + 0x318), Ok(disc.root));
    assert_eq!(machine.bus().read32(R + 0x31C), Ok(disc.directory_size));
    assert_eq!(machine.bus().read32(R + 0x320), Ok(disc.extent as u32));
    assert_eq!(machine.bus().read32(R + 0x324), Ok(disc.size as u32));
    assert_eq!(machine.bus().read32(R + 0x328), Ok(disc.size as u32));
    assert_eq!(machine.debug_read16(0x25890008).unwrap() & 0x82, 0x80);
    assert_eq!(machine.bus().read32(0x25FE0000), Ok(0x05818000));
    assert_eq!(machine.bus().read32(0x25FE0004), Ok(0x06017000));
    // HLE pointer/data are absent; these RAM words were never installed.
    assert_eq!(machine.bus().read32(0x06000320), Ok(0));
    assert_eq!(machine.bus().read32(0x06000348), Ok(0));
}

#[test]
fn firmware_loads_and_executes_actual_disc_bytes_then_cold_restarts_without_ejecting_media() {
    let rom = firmware();
    for disc in DISCS {
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.mount_media(Media::iso(disc.image).unwrap());
        finish(&mut m, 10000);
        successful(&m, disc);
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        assert!(
            m.bus().cd.media().is_some(),
            "a cold hardware reset must not eject the physical medium"
        );
        assert_eq!(m.bus().read32(R + 0xFC), Ok(0));
        finish(&mut m, 10000);
        successful(&m, disc);
    }
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
            image[21 * 2048 + 0xF07] = 12;
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
            Ok(if control == 0 {
                0xE001
            } else if control == 3 {
                0xE003
            } else {
                0xBAD
            }),
            "control {control}"
        );
        assert_eq!(
            m.bus().read32(R),
            Ok(if matches!(control, 1 | 2) {
                0x06004004
            } else {
                0
            }),
            "real-byte control {control}"
        );
        assert_eq!(m.mode(), Some(BootMode::ExternalBios));
        assert_eq!(m.bus().read32(0x06000320), Ok(0));
    }
}

#[test]
fn startup_records_are_independent_of_step_chunks_and_passive_cd_inspection() {
    let rom = firmware();
    for disc in DISCS {
        let mut reference = None;
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut m = Machine::new();
                m.reset(Boot::ExternalBios(&rom)).unwrap();
                m.mount_media(Media::iso(disc.image).unwrap());
                let mut steps = 1000000;
                while steps != 0 {
                    let n = steps.min(chunk);
                    m.run_steps(n).unwrap();
                    if inspect {
                        let _ = m.debug_read32(0x25818000).unwrap();
                        let _ = m.debug_read16(0x25890024).unwrap();
                    }
                    steps -= n;
                }
                successful(&m, disc);
                let snapshot = (
                    m.clocks(),
                    m.cpu_state().clone(),
                    m.slave_state().clone(),
                    (0..0x1400 / 4)
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
    successful(&m, DISCS[0]);
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
    successful(&m, DISCS[0]);
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
    // A different genuine 512 KiB original ROM cannot inherit the last boot's
    // executable or its success marker; its own fixed-layout checksum rejects.
    let other = decode(include_str!("fixtures/external-startup-fixed.hex"));
    m.reset(Boot::ExternalBios(&other)).unwrap();
    finish(&mut m, 10000);
    assert_eq!(m.bus().read32(R + 0xFC), Ok(0xE002));
    assert_eq!(m.bus().read32(R), Ok(0));
    assert_eq!(m.bus().read32(0x06000320), Ok(0));
}

#[test]
fn inherited_fixed_layout_negative_control() {
    let original = include_bytes!("fixtures/external-startup-fixed.iso");
    let rom = decode(include_str!("fixtures/external-startup-fixed.hex"));
    let mut baseline = Machine::new();
    baseline.reset(Boot::ExternalBios(&rom)).unwrap();
    baseline.mount_media(Media::iso(original).unwrap());
    finish(&mut baseline, 10000);
    assert_eq!(baseline.bus().read32(R + 0xFC), Ok(0x600D));
    let mut image = original.to_vec();
    let payload = image[21 * 2048..21 * 2048 + 4093].to_vec();
    image[35 * 2048..35 * 2048 + payload.len()].copy_from_slice(&payload);
    image[21 * 2048..23 * 2048].fill(0);
    let record = 20 * 2048 + 68;
    image[record + 2..record + 6].copy_from_slice(&35u32.to_le_bytes());
    image[record + 6..record + 10].copy_from_slice(&35u32.to_be_bytes());
    image[record + 10..record + 14].copy_from_slice(&4093u32.to_le_bytes());
    image[record + 14..record + 18].copy_from_slice(&4093u32.to_be_bytes());
    let parsed = saturn_rust::disc::parse(&image).unwrap();
    assert_eq!(
        parsed.executable, payload,
        "valid relocated first-read file"
    );
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(Media::iso(&image).unwrap());
    finish(&mut m, 10000);
    assert_eq!(m.bus().read32(R + 0xFC), Ok(0xE002));
    assert_eq!(m.bus().read32(R), Ok(0));
    assert_eq!(m.bus().read32(0x06004000), Ok(0));
    assert_eq!(m.bus().read32(R + 0x400 + 5 * 32 + 4), Ok(171));
}

fn dual32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    bytes[at + 4..at + 8].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn firmware_rejects_malformed_metadata_and_unsupported_layouts_before_aip() {
    // Literal field offsets authored independently of the fixture builder and
    // firmware generator. No host parser supplies a firmware decision.
    for control in 0..47 {
        let disc = DISCS[0];
        let mut image = disc.image.to_vec();
        let pvd = 16 * 2048;
        let root = 20 * 2048;
        let file = root + disc.file_offset;
        match control {
            0 => image[0] = b'X',
            1 => image[0x40] = b' ',
            2 => image[0x41] = b'U', // duplicate
            3 => image[0x42] = b'J', // symbol after space
            4 => image[0x40..0x45].copy_from_slice(b"JTUEJ"),
            5 => image[0x40] = b'Z',
            6 => image[0xE0..0xE4].copy_from_slice(&0xFFCu32.to_be_bytes()),
            7 => image[0xE0..0xE4].copy_from_slice(&0x8004u32.to_be_bytes()),
            8 => image[0xE0..0xE4].copy_from_slice(&0x1001u32.to_be_bytes()),
            9 => image[0xE8..0xEC].copy_from_slice(&0x06010000u32.to_be_bytes()),
            10 => image[0xEC..0xF0].copy_from_slice(&0x060FE002u32.to_be_bytes()),
            11 => image[0xF0..0xF4].copy_from_slice(&0x06002000u32.to_be_bytes()),
            12 => image[0xF0..0xF4].copy_from_slice(&0x06010000u32.to_be_bytes()),
            13 => image[0xF0..0xF4].copy_from_slice(&0xFFFFFFFCu32.to_be_bytes()),
            14 => image[pvd + 1] = b'X',
            15 => image[pvd + 6] = 2,
            16 => dual32(&mut image, pvd + 80, 65), // exceeds actual TOC medium
            17 => image[pvd + 84] = 1,              // endian disagreement
            18 => image[pvd + 128] = 1,
            19 => image[pvd + 156] = 33,
            20 => image[pvd + 156 + 33] = 1,
            21 => dual32(&mut image, pvd + 158, 16),
            22 => dual32(&mut image, pvd + 166, 0),
            23 => dual32(&mut image, pvd + 166, 18432),
            24 => image[root + 33] = 1,
            25 => image[root + 34 + 33] = 0,
            26 => image[file] = 33,
            27 => image[file + 32] = 255,
            28 => image[file + 25] = 2, // do not search past a directory at fid 2
            29 => image[file..root + 2048].fill(0), // missing first child
            30 => dual32(&mut image, file + 10, 0),
            31 => dual32(&mut image, file + 10, 65537),
            32 => dual32(&mut image, file + 2, 64),
            33 => dual32(&mut image, file + 2, 20),
            34 => image[file + 1] = 1, // extended attributes outside this increment
            35 => image[file + 26] = 1, // file unit/interleave outside this increment
            36 => image[file + 27] = 1,
            37 => image[file + 25] = 0x80, // multi-extent / excessive fragmentation
            38 => image[file + 25] = 4,    // associated file
            39 => image[0xE4] = 1,
            40 => image[0xD0] = 1,
            41 => image[0xF8] = 1,
            42 => image[file] = 45, // odd record length
            43 => image[file + 28] = 2,
            44 => image[root + 34 + 6] = 1, // parent endian copies
            45 => {
                dual32(&mut image, pvd + 80, 18);
            } // root outside volume
            46 => image.truncate(24 * 2048),
            _ => unreachable!(),
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&firmware())).unwrap();
        m.mount_media(Media::iso(&image).unwrap());
        finish(&mut m, 10000);
        assert_eq!(
            m.bus().read32(R + 0xFC),
            Ok(0xE002),
            "metadata control {control}"
        );
        assert_eq!(m.bus().read32(R), Ok(0), "no program entry {control}");
        assert_eq!(m.bus().read32(R + 0x2F0), Ok(0), "no AIP entry {control}");
        assert_eq!(
            m.bus().read32(R + 0x328),
            Ok(0),
            "no partial executable {control}"
        );
        assert!(
            m.cpu_state().pc < 0x80000,
            "firmware owns failure {control}"
        );
        assert_eq!(m.bus().read32(0x06000320), Ok(0));
    }
}

#[test]
fn file_record_extent_and_contents_actually_determine_loaded_instructions_and_results() {
    for disc in DISCS {
        // Move actual file bytes again, independently of the fixture writer.
        // Native host parser is just a validity cross-check; ExternalBios never calls it.
        let mut image = disc.image.to_vec();
        let old = disc.extent * 2048;
        let new_extent = 40;
        let payload = image[old..old + disc.size].to_vec();
        image[old..old + disc.size.next_multiple_of(2048)].fill(0);
        image[new_extent * 2048..new_extent * 2048 + disc.size].copy_from_slice(&payload);
        dual32(
            &mut image,
            disc.root as usize * 2048 + disc.file_offset + 2,
            new_extent as u32,
        );
        assert_eq!(
            saturn_rust::disc::parse(&image).unwrap().executable,
            payload
        );
        // Mutate a genuine executed MOV immediate, keeping it legal: change the
        // guest sum loop count from 4 to 3. The result must become 31, not 48.
        let at = payload
            .windows(4)
            .position(|p| p == [0xE0, 0, 0xE3, 4])
            .unwrap();
        image[new_extent * 2048 + at + 3] = 3;
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&firmware())).unwrap();
        m.mount_media(Media::iso(&image).unwrap());
        finish(&mut m, 10000);
        assert_eq!(m.bus().read32(R + 0xFC), Ok(0xBAD));
        assert_eq!(m.bus().read32(R + 12), Ok(31));
        for (i, byte) in image[new_extent * 2048..new_extent * 2048 + disc.size]
            .iter()
            .enumerate()
        {
            assert_eq!(m.bus().read8(disc.destination + i as u32), Ok(*byte));
        }
        assert_eq!(m.bus().read32(R + 0x320), Ok(40));
    }
}

#[test]
fn omitted_read_or_partition_deletion_and_actual_decode_error_fail_without_aip() {
    for control in 0..5 {
        let mut rom = firmware();
        if control == 0 {
            patch(&mut rom, 0x1080, 0x1180);
        }
        if control == 1 {
            patch(&mut rom, 0x6300, 0x6100);
        }
        if control == 2 {
            rom = decode(include_str!("fixtures/external-startup-abort.hex"));
        }
        if control == 4 {
            rom = decode(include_str!("fixtures/external-startup-read-reset.hex"));
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        if control == 3 {
            let medium = Media::iso(IMAGE).unwrap();
            let mut raw = Vec::new();
            for lba in 0..64 {
                let mut sector = medium.raw_sector(lba + 150).unwrap().to_vec();
                // An actual uncorrectable sector at the second executable
                // sector, after the first has already been copied to RAM.
                if lba == 22 {
                    sector[16..2064].fill(0xB7);
                }
                raw.extend(sector);
            }
            m.mount_media(
                Media::cue(
                    "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
                    &[("d", &raw)],
                )
                .unwrap(),
            );
        } else {
            m.mount_media(Media::iso(IMAGE).unwrap());
        }
        finish(&mut m, 10000);
        assert_eq!(
            m.bus().read32(R + 0xFC),
            Ok(0xE003),
            "I/O control {control}"
        );
        assert_eq!(m.bus().read32(R), Ok(0));
        assert_eq!(m.bus().read32(R + 0x2F0), Ok(0));
        if control == 3 {
            assert_eq!(m.bus().read32(R + 0x328), Ok(2048));
        }
        if matches!(control, 2 | 4) {
            assert_eq!(m.bus().read32(R + 0x400 + 5 * 32), Ok(0x1080));
            assert_eq!(m.bus().read32(R + 0x400 + 5 * 32 + 16), Ok(0x0400)); // actual seek is pending
            assert_eq!(
                m.bus().read32(R + 0x400 + 6 * 32),
                Ok(if control == 2 { 0x7500 } else { 0x0401 })
            );
            assert_eq!(m.bus().read32(R + 0x328), Ok(0));
            // Re-select the active ROM on the retained medium; old read state
            // must be canceled before fresh transfers can produce success.
            m.reset(Boot::ExternalBios(&firmware())).unwrap();
            finish(&mut m, 10000);
            successful(&m, DISCS[0]);
        }
    }
}

#[test]
fn reset_and_ejection_during_metadata_or_partial_file_never_reuse_stale_data() {
    for phase in [2, 3, 4] {
        let rom = firmware();
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.mount_media(Media::iso(IMAGE).unwrap());
        let mut reached = false;
        for _ in 0..1000000 {
            m.step().unwrap();
            if m.bus().read32(R + 0x32C) == Ok(phase)
                && (phase != 4 || m.bus().read32(R + 0x328) == Ok(2048))
                && m.debug_read16(0x25890008).unwrap() & 1 == 0
            {
                reached = true;
                break;
            }
        }
        assert!(reached, "pending transfer phase {phase}");
        assert_eq!(m.bus().read32(R + 0x2F0), Ok(0));
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        assert_eq!(m.bus().read32(R + 0x328), Ok(0));
        assert_eq!(m.bus().read32(0x06004000), Ok(0));
        finish(&mut m, 10000);
        successful(&m, DISCS[0]);
        // Ejecting and restarting must not enter the previous RAM program.
        m.clear_media();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        finish(&mut m, 10000);
        assert_eq!(m.bus().read32(R + 0xFC), Ok(0xE001));
        assert_eq!(m.bus().read32(R + 0x2F0), Ok(0));
    }
}

#[test]
fn first_read_uses_identifier_two_and_ip_defaults_or_explicit_slave_stack() {
    for control in 0..3 {
        let mut image = IMAGE.to_vec();
        image[20 * 2048 + 68 + 33..20 * 2048 + 68 + 43].copy_from_slice(b"BOOT.BIN;1");
        if control == 0 {
            image[0xE8..0xEC].fill(0);
            let file = &mut image[21 * 2048..23 * 2048];
            let at = file
                .windows(4)
                .position(|p| p == 0x060FE000u32.to_be_bytes())
                .unwrap();
            file[at..at + 4].copy_from_slice(&0x06002000u32.to_be_bytes());
        }
        if control == 1 {
            image[0xEC..0xF0].copy_from_slice(&0x060EE000u32.to_be_bytes());
        }
        if control == 2 {
            // Select LICENSE.TXT as fid 2 without changing the actual bytes. It
            // is not executable; the original AIP reaches the ROM illegal trap.
            dual32(&mut image, 20 * 2048 + 68 + 2, 23);
            dual32(
                &mut image,
                20 * 2048 + 68 + 10,
                include_bytes!("fixtures/LICENSE").len() as u32,
            );
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&firmware())).unwrap();
        m.mount_media(Media::iso(&image).unwrap());
        finish(&mut m, 10000);
        assert_eq!(
            m.bus().read32(R + 0xFC),
            Ok(if control == 2 { 0xE004 } else { 0x600D })
        );
        if control == 0 {
            assert_eq!(m.bus().read32(R + 4), Ok(0x06002000));
        }
        if control == 1 {
            assert_eq!(m.slave_state().r[15], 0x060EE000);
        }
        if control == 2 {
            assert_eq!(m.bus().read32(R), Ok(0));
            assert_eq!(m.bus().read32(R + 0x320), Ok(23));
        }
    }
}

#[test]
fn zero_first_read_address_runs_original_aip_without_loading_a_file() {
    for children in 0..3 {
        let mut image = IMAGE.to_vec();
        image[0xF0..0xF4].fill(0);
        if children == 0 {
            image[20 * 2048 + 68..21 * 2048].fill(0);
        } // self + parent, no first-read file
        if children == 2 {
            image[20 * 2048..21 * 2048].fill(0);
        }
        // Independently encoded original AIP: MOV.L PC literals, MOV.L marker,
        // SLEEP/BRA. There is no first-read jump or preloaded executable.
        let aip: [u16; 12] = [
            0xD103, 0xD004, 0x2102, 0x001B, 0xAFFE, 0x0009, 0x0009, 0x0009, 0x2601, 0x00FC, 0,
            0x600D,
        ];
        for (i, word) in aip.iter().enumerate() {
            image[0xE20 + i * 2..0xE22 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&firmware())).unwrap();
        m.mount_media(Media::iso(&image).unwrap());
        finish(&mut m, 10000);
        assert_eq!(
            m.bus().read32(R + 0xFC),
            Ok(if children == 2 { 0xE002 } else { 0x600D })
        );
        assert_eq!(m.bus().read32(R + 0x328), Ok(0));
        assert_eq!(m.bus().read32(0x06004000), Ok(0));
        assert_eq!(m.bus().read32(R + 0x330), Ok(5));
        if children != 2 {
            assert!((0x06002E20..0x06002E40).contains(&m.cpu_state().pc));
        } else {
            assert!(m.cpu_state().pc < 0x80000);
        }
    }
}
