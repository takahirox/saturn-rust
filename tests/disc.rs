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
    assert_eq!(m.cpu_state().pc, AIP_ENTRY);
    assert_eq!(m.cpu_state().r[15], parsed.stack);
    assert_eq!(m.bus().read32(IP_ADDRESS), Ok(0x53454741));
    assert!(m.bus().read32(0).is_err());
    m.run_steps(3).unwrap();
    assert_eq!(m.cpu_state().pc, parsed.load_address);
    assert_eq!(m.cpu_state().instructions, 3);
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
