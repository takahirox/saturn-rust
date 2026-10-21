use saturn_rust::{
    Boot, BootMode, Machine,
    bus::{HIGH_WRAM, Memory},
    cpu::CpuStatus,
    disc::*,
};
const ISO: &[u8] = include_bytes!("../web/star-collector.iso");

#[test]
fn boot_uses_saturn_metadata_iso_first_file_and_original_aip_instructions() {
    let parsed = parse(ISO).unwrap();
    assert_eq!(parsed.ip.len(), 4096);
    assert_eq!(parsed.stack, 0x060FF000);
    assert_eq!(parsed.load_address, 0x06004000);
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    assert_eq!(m.mode(), Some(BootMode::HleDisc));
    assert_eq!(m.cpu_state().pc, 0x100);
    assert_eq!(m.cpu_state().r[15], parsed.stack);
    assert_eq!(m.bus().read32(IP_ADDRESS), Ok(0x53454741));
    assert_eq!(m.bus().read32(0), Ok(0x100)); // original independent HLE reset trampoline
    for _ in 0..32 {
        if m.cpu_state().pc == AIP_ENTRY {
            break;
        }
        m.step().unwrap();
    }
    assert_eq!(m.cpu_state().pc, AIP_ENTRY);
    assert_eq!(m.cpu_state().vbr, 0x06000000);
    let before = m.cpu_state().instructions;
    m.run_steps(3).unwrap();
    assert_eq!(m.cpu_state().pc, parsed.load_address);
    assert_eq!(m.cpu_state().instructions - before, 3);
}
fn rejects(mut edit: impl FnMut(&mut Vec<u8>)) {
    let mut image = ISO.to_vec();
    edit(&mut image);
    assert!(parse(&image).is_err());
    let mut m = Machine::new();
    m.reset(Boot::HlePrototype).unwrap();
    m.run_steps(6).unwrap();
    assert!(m.reset(Boot::HleDisc(&image)).is_err());
    assert_eq!(m.mode(), Some(BootMode::HleDisc));
    assert_eq!(m.cpu_state().status, CpuStatus::Reset);
    assert_eq!(m.bus().read32(HIGH_WRAM), Ok(0));
    assert_eq!(m.frames(), 0);
}
#[test]
fn malformed_and_unsupported_discs_fail_before_execution_without_stale_state() {
    rejects(|b| b.truncate(2048));
    rejects(|b| {
        b.pop();
    });
    rejects(|b| b.resize(MAX_DISC_SIZE + SECTOR, 0));
    rejects(|b| b[0] = 0);
    rejects(|b| b[0x41] = b'J');
    rejects(|b| b[0xE0..0xE4].copy_from_slice(&0xFFFFu32.to_be_bytes()));
    rejects(|b| b[0xE8..0xEC].copy_from_slice(&0x06002004u32.to_be_bytes()));
    rejects(|b| b[0xE8..0xEC].copy_from_slice(&0x060FF001u32.to_be_bytes()));
    rejects(|b| b[0xF0..0xF4].copy_from_slice(&0x06002000u32.to_be_bytes()));
    rejects(|b| b[0xF0..0xF4].copy_from_slice(&0x060FFFFCu32.to_be_bytes()));
    rejects(|b| b[16 * SECTOR + 1] = 0);
    rejects(|b| b[17 * SECTOR] = 2);
    rejects(|b| b[16 * SECTOR + 80] = 60);
    rejects(|b| b[16 * SECTOR + 128] = 4);
    rejects(|b| b[16 * SECTOR + 156] = 33);
    rejects(|b| b[20 * SECTOR] = 255);
    rejects(|b| b[20 * SECTOR + 33] = 2);
    rejects(|b| b[20 * SECTOR + 34 + 25] = 0);
    rejects(|b| b[20 * SECTOR + 68 + 25] = 2);
    rejects(|b| b[20 * SECTOR + 68 + 25] = 0x80); // multi-extent file
    rejects(|b| b[20 * SECTOR + 68 + 32] = 255); // truncated name
    rejects(|b| b[20 * SECTOR + 68 + 26] = 1); // interleaving
    rejects(|b| {
        let record = 20 * SECTOR + 68;
        b[record + 2..record + 6].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        b[record + 6..record + 10].copy_from_slice(&0xFFFFFFFFu32.to_be_bytes());
    });
    rejects(|b| b[20 * SECTOR..21 * SECTOR].fill(0));
}

#[test]
fn malformed_inputs_never_panic_even_with_corrupt_directory_bytes() {
    // Deterministic bounded parser robustness exercise, no external corpus.
    for offset in (0..ISO.len()).step_by(127) {
        let mut image = ISO.to_vec();
        image[offset] ^= 0xFF;
        let _ = parse(&image);
    }
}

#[test]
fn first_read_name_and_destination_are_metadata_not_game_specific_hooks() {
    let mut image = ISO.to_vec();
    image[20 * SECTOR + 68 + 33..20 * SECTOR + 68 + 43].copy_from_slice(b"ELSE.BIN;1");
    image[0xF0..0xF4].copy_from_slice(&0x06008000u32.to_be_bytes());
    image[0xE8..0xEC].fill(0); // documented default master stack
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    let parsed = parse(&image).unwrap();
    assert_eq!(parsed.stack, 0x06002000);
    assert_eq!(
        m.bus().read16(0x06008000),
        Ok(u16::from_be_bytes(
            parsed.executable[..2].try_into().unwrap()
        ))
    );
    assert_eq!(m.bus().read16(0x06004000), Ok(0));
}

#[test]
fn descriptor_sets_padding_and_hidden_first_read_preserve_guest_boot() {
    // ST-040 §3.2.1: primary need not be the first/only descriptor, and
    // supplementary namespaces must not replace its first-read file ID2.
    let mut image = ISO.to_vec();
    let primary = image[16 * SECTOR..17 * SECTOR].to_vec();
    image[16 * SECTOR..17 * SECTOR].fill(0);
    image[16 * SECTOR..16 * SECTOR + 7].copy_from_slice(b"\0CD001\x01");
    image[17 * SECTOR..18 * SECTOR].copy_from_slice(&primary);
    image[18 * SECTOR..19 * SECTOR].fill(0);
    image[18 * SECTOR..18 * SECTOR + 7].copy_from_slice(b"\x02CD001\x01");
    image[19 * SECTOR..20 * SECTOR].fill(0);
    image[19 * SECTOR..19 * SECTOR + 7].copy_from_slice(b"\xFFCD001\x01");
    image[20 * SECTOR + 68 + 25] = 1; // hidden regular file
    image.resize(image.len() + SECTOR * 2, 0xA5); // outside logical volume
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    for _ in 0..32 {
        if m.cpu_state().pc == AIP_ENTRY {
            break;
        }
        m.step().unwrap();
    }
    assert_eq!(m.cpu_state().pc, AIP_ENTRY);
    m.run_steps(3).unwrap();
    assert_eq!(m.cpu_state().pc, parse(ISO).unwrap().load_address);
    assert_eq!(
        m.bus().read32(0x06004000),
        Ok(u32::from_be_bytes(
            parse(ISO).unwrap().executable[..4].try_into().unwrap()
        ))
    );
    // A duplicate primary, missing terminator, or an extent in physical padding
    // must fail, even though the underlying bytes exist in the mounted medium.
    let mut duplicate = image.clone();
    duplicate[18 * SECTOR] = 1;
    assert!(parse(&duplicate).is_err());
    let mut missing = image.clone();
    missing[19 * SECTOR] = 3;
    assert!(parse(&missing).is_err());
    let volume_end = ISO.len() / SECTOR;
    for first in [2, 10] {
        let mut escaped = image.clone();
        let record = 20 * SECTOR + 68;
        let n = if first == 2 {
            volume_end as u32
        } else {
            (ISO.len() + 1) as u32
        };
        escaped[record + first..record + first + 4].copy_from_slice(&n.to_le_bytes());
        escaped[record + first + 4..record + first + 8].copy_from_slice(&n.to_be_bytes());
        assert!(parse(&escaped).is_err());
    }
}

#[test]
fn zero_first_read_address_executes_original_aip_without_loading_a_file() {
    // ST-040 §4.3: original guest code, no Sega security/area object.
    let mut image = ISO.to_vec();
    image[0xF0..0xF4].fill(0);
    image[20 * SECTOR + 68..21 * SECTOR].fill(0); // no first-read entry
    for (i, word) in [0xE02Au16, 0xD101, 0x2102, 0x001B, 0x0600, 0x8000]
        .into_iter()
        .enumerate()
    {
        image[0xE20 + i * 2..0xE22 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    assert!(parse(&image).unwrap().executable.is_empty());
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    m.run_steps(100).unwrap();
    assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(m.bus().read32(0x06008000), Ok(42));
    assert_eq!(m.bus().read32(0x06004000), Ok(0));
    assert_eq!(m.cpu_state().instructions, 27); // 23 reset + four original AIP instructions
}
