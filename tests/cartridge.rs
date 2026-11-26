//! Original reset-vector guest exercises the installed slot through both CPUs
//! and scheduled SH-2 DMAC / SCU DMA, in both playback timing modes.
use saturn_rust::{
    Boot, Machine,
    backup::BACKUP_SIZE,
    bus::{BIOS_SIZE, Memory},
    cartridge::{Cartridge, POWER_MEMORY_SIZE},
};

const TRACE: u32 = 0x0602_8000;
const PEER: u32 = 0x0602_8100;
const DONE: u32 = 42;
const OPEN: u32 = u32::MAX;

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cartridge.hex").lines() {
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

fn run(machine: &mut Machine, address: u32) {
    for _ in 0..8 {
        machine.advance_frame().unwrap();
        if machine.bus().read32(address) == Ok(0x600D) {
            assert_eq!(machine.cpu_state().exceptions, 0);
            assert_eq!(machine.slave_state().exceptions, 0);
            return;
        }
    }
    panic!(
        "guest did not finish: {:?}, {:?}",
        machine.cpu_state(),
        machine.slave_state()
    );
}

fn words(machine: &Machine, address: u32, count: usize) -> Vec<u32> {
    (0..count)
        .map(|i| machine.bus().read32(address + i as u32 * 4).unwrap())
        .collect()
}

#[test]
fn reset_vectors_cover_a_bus_widths_banks_aliases_dma_and_slave_access() {
    let rom = rom();
    for fast in [false, true] {
        for (kind, id) in [
            (Cartridge::None, 0xFF),
            (Cartridge::Backup, 0x21),
            (Cartridge::Ram1MiB, 0x5A),
            (Cartridge::Ram4MiB, 0x5C),
        ] {
            let mut machine = Machine::new();
            machine.set_fast_mode(fast);
            machine.set_cartridge(kind).unwrap();
            machine.reset(Boot::ExternalBios(&rom)).unwrap();
            run(&mut machine, TRACE + DONE * 4);
            let power = kind == Cartridge::Backup;
            let expansion = matches!(kind, Cartridge::Ram1MiB | Cartridge::Ram4MiB);
            let small = kind == Cartridge::Ram1MiB;
            let size4 = kind == Cartridge::Ram4MiB;
            let select = |mapped, value| if mapped { value } else { OPEN };
            let expected = [
                id,
                0xFF00 | id,
                (if power { 0xFF00_FF00 } else { 0xFFFF_FF00 }) | id,
                0xFF00_FF00,
                select(power, 0xFF00_FF00),
                select(expansion, 0xAB02_5678),
                select(expansion, 0xAB02_5678),
                select(expansion, 0x1020_3040),
                select(expansion, if small { 0xAB02_5678 } else { 0 }),
                select(expansion, if small { 0x1020_3040 } else { 0 }),
                select(size4, 0),
                select(expansion, 0x5060_7080),
                select(expansion, if small { 0x5060_7080 } else { 0 }),
                select(size4, 0xDEAD_BEEF),
                OPEN,
                select(power, 0xFFB2_FFD4),
                select(power, 0xFFB2_FFD4),
                if power { 0xFF34 } else { 0xFFFF },
                0xFF,
                id,
                0xFFFF,
                select(expansion, 0x9ABC_DEF0),
                select(expansion, 0xAB02_5678),
                0,
                if power { 0xFF67 } else { 0xFFFF },
                if expansion { 0x0089 } else { 0xFFFF },
            ];
            assert_eq!(
                words(&machine, TRACE, expected.len()),
                expected,
                "{kind:?}, relaxed={fast}"
            );
            assert_eq!(
                words(&machine, PEER, 4),
                [
                    id,
                    select(power, 0xFFB2_FFD4),
                    select(expansion, 0xAB02_5678),
                    0x600D,
                ]
            );
            assert_eq!(&machine.backup_ram()[..2], &[0x22, 0x44]);
            if power {
                let external = machine.cartridge_backup_ram().unwrap().to_vec();
                assert_eq!(external.len(), POWER_MEMORY_SIZE);
                assert_eq!(&external[..3], &[0xB2, 0xD4, 0x67]);
                assert_eq!(external[POWER_MEMORY_SIZE - 1], 0x34);
                let revisions = [
                    machine.backup_revision(),
                    machine.cartridge_backup_revision(),
                ];
                machine.set_cartridge(Cartridge::Ram1MiB).unwrap();
                assert_eq!(machine.bus().read8(0x24FF_FFFF), Ok(0x5A));
                machine.set_cartridge(Cartridge::Backup).unwrap();
                assert_eq!(machine.bus().read8(0x24FF_FFFF), Ok(0x21));
                assert_eq!(machine.cartridge_backup_ram().unwrap(), external);
                assert_eq!(
                    [
                        machine.backup_revision(),
                        machine.cartridge_backup_revision()
                    ],
                    revisions
                );
                assert!(
                    machine
                        .import_cartridge_backup_ram(&[0; BACKUP_SIZE])
                        .is_err()
                );
                assert_eq!(machine.cartridge_backup_ram().unwrap(), external);
                // The cold slot change still boots the installed reset vector.
                run(&mut machine, TRACE + DONE * 4);
                assert_eq!(words(&machine, TRACE + 12, 2), [0xFF22_FF44, 0xFFB2_FFD4]);
            } else if expansion {
                machine.reset(Boot::ExternalBios(&rom)).unwrap();
                assert_eq!(machine.bus().read32(0x2240_0000), Ok(0));
            }
        }
    }
}

#[test]
fn guest_clock_and_settings_survive_restore_and_calendar_rollovers() {
    let mut bios = rom();
    let mut source = Machine::new();
    source.reset(Boot::ExternalBios(&bios)).unwrap();
    run(&mut source, TRACE + DONE * 4);
    let snapshot = source.export_smpc_battery();
    assert_eq!(
        &snapshot[..12],
        &[
            0x20, 0x24, 0x32, 0x29, 0x23, 0x59, 0x59, 1, 0x12, 0x34, 0x56, 0x78
        ]
    );
    bios[..4].copy_from_slice(&0x1400_u32.to_be_bytes());
    let mut restored = Machine::new();
    for (elapsed, calendar) in [
        (0, [0x20, 0x24, 0x32, 0x29, 0x23, 0x59, 0x59]),
        (1000, [0x20, 0x24, 0x43, 0x01, 0, 0, 0]),
        (306 * 86_400_000 + 1000, [0x20, 0x25, 0x21, 0x01, 0, 0, 0]),
    ] {
        restored.restore_smpc_battery(&snapshot, elapsed).unwrap();
        restored.reset(Boot::ExternalBios(&bios)).unwrap();
        run(&mut restored, TRACE + 21 * 4);
        let report = words(&restored, TRACE + 5 * 4, 16);
        assert_eq!(&report[..8], &[&[0xC0], &calendar[..]].concat());
        assert_eq!(&report[12..], &[0x12, 0x34, 0x56, 0x78]);
    }
    let accepted = restored.export_smpc_battery();
    for (offset, value) in [(1, 0xFA), (2, 0x3D), (3, 0x30), (7, 2), (12, 0xFF)] {
        let mut damaged = snapshot;
        damaged[offset] = value;
        assert!(restored.restore_smpc_battery(&damaged, 0).is_err());
        assert_eq!(restored.export_smpc_battery(), accepted);
    }
}

#[test]
fn sysres_and_watchdog_manual_reset_retain_powered_cartridge_memory_and_smem() {
    let mut rom = rom();
    rom[..4].copy_from_slice(&0x1C00_u32.to_be_bytes());
    rom[8..12].copy_from_slice(&0x2400_u32.to_be_bytes());
    for (kind, expansion, backup) in [
        (Cartridge::Backup, OPEN, 0xA5),
        (Cartridge::Ram1MiB, 0xC0DE_BEEF, 0xFF),
        (Cartridge::Ram4MiB, 0xC0DE_BEEF, 0xFF),
    ] {
        let mut machine = Machine::new();
        machine.set_cartridge(kind).unwrap();
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        run(&mut machine, TRACE + 38 * 4);
        for start in [TRACE, TRACE + 19 * 4] {
            assert_eq!(words(&machine, start, 3), [expansion, backup, 1]);
            assert_eq!(machine.bus().read32(start + 15 * 4), Ok(0x67));
        }
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        assert_eq!(
            machine.bus().read32(0x2240_0000),
            Ok(if kind == Cartridge::Backup { OPEN } else { 0 })
        );
        assert_eq!(machine.bus().read8(0x2400_0009), Ok(backup as u8));
    }
}
