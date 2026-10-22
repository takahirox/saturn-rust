//! Independent original master/slave guest evidence for SH7604 §§5/7/14.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/system-reset.hex")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let mut a = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[a..a + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            a += 2;
        }
    }
    rom
}
#[test]
fn sysres_restarts_the_original_rom_and_retains_only_battery_and_mounted_disc() {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom())).unwrap();
    let media = saturn_rust::media::Media::iso(&vec![0; 2048]).unwrap();
    m.mount_media(media);
    m.import_backup_ram(&vec![0xa5; 32768]).unwrap();
    m.run_steps(1000).unwrap();
    assert_eq!(m.bus().read32(0x06001000).unwrap(), 0x5a);
    assert_eq!(m.bus().read8(0x20100063).unwrap(), 1);
    for _ in 0..34 {
        m.run_steps(10000).unwrap();
    }
    let expected = [13, 0, 0, 0, 0, 0, 0, 0x12, 0x34, 0x56, 0x78, 0x600d];
    for (i, value) in expected.into_iter().enumerate() {
        assert_eq!(
            m.bus().read32(0x0600c000 + i as u32 * 4).unwrap(),
            value,
            "mailbox {i}"
        );
    }
    assert_eq!(m.backup_ram()[0], 0x5a);
    assert!(m.backup_ram()[1..].iter().all(|b| *b == 0xa5));
    assert_eq!(m.cpu_state().r[15], 0x06100000);
    assert!(m.bus().cd.media().is_some());
}

#[test]
fn hle_sysres_reloads_the_original_disc_program_without_entering_the_slave_trampoline() {
    // Reuse the source-authored position-independent SYSRES program as a first
    // read file in the original licensed ISO. No running guest RAM is edited.
    let mut image = include_bytes!("../web/star-collector.iso").to_vec();
    image[21 * 2048..21 * 2048 + 0x19C].copy_from_slice(&rom()[0x200..0x39C]);
    let mut machine = Machine::new();
    machine.reset(Boot::HleDisc(&image)).unwrap();
    for _ in 0..35 {
        machine.run_steps(10000).unwrap();
    }
    assert_eq!(machine.mode(), Some(saturn_rust::BootMode::HleDisc));
    assert_eq!(machine.bus().read32(0x0600c000).unwrap(), 13);
    assert_eq!(machine.bus().read32(0x0600c02c).unwrap(), 0x600d);
    assert_eq!(machine.cpu_state().r[15], 0x060ff000);
    assert_eq!(machine.backup_ram()[0], 0x5a);
    assert!(machine.bus().cd.media().is_some());
}
