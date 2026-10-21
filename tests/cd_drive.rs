//! Original guest commands and polls: ST-162 §§3.4/4.2/4.3/8.2.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-drive.hex")
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[at..at + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    rom
}
#[test]
fn guest_scans_reverses_pauses_waits_and_reopens_same_disc_without_reading_sectors() {
    let mut machine = Machine::new();
    let rom = rom();
    let mut previous = None;
    for _ in 0..2 {
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        machine.mount_media(Media::iso(&vec![0; 2048 * 100]).unwrap());
        for _ in 0..80 {
            machine.run_steps(10000).unwrap();
        }
        let read = |offset: u32| machine.bus().read32(0x06009000 + offset).unwrap();
        assert_eq!(read(0xF0), 0x600D);
        for (offset, status) in [
            (0, 0x0180),
            (0x10, 0x0500),
            (0x30, 0x0500),
            (0x50, 0x0400),
            (0x60, 0x0180),
            (0x70, 0x8180),
            (0x80, 0x0600),
            (0x90, 0x0180),
            (0xA0, 0x4180),
        ] {
            assert_eq!(read(offset), status, "offset {offset:x}");
        }
        assert!(read(0x2C) >= 170 && read(0x2C) < 250);
        assert!(read(0x4C) < 160 && read(0x4C) >= 150);
        assert_eq!(read(0xA4), 204);
        assert_eq!(read(0xB4), 2); // Two TOC words transferred by actual guest MOV.L.
        assert_eq!(read(0xCC), 0); // Scan never routes data to a partition.
        assert_eq!(read(0xE0), 0x41000096);
        let trace = (0..64).map(|i| read(i * 4)).collect::<Vec<_>>();
        if let Some(old) = previous {
            assert_eq!(trace, old);
        }
        previous = Some(trace);
    }
}

#[test]
fn guest_runtime_directory_commands_reject_malformed_extents_without_trapping() {
    let mut fixture = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-directory.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        let address = usize::from_str_radix(address, 16).unwrap();
        for (i, word) in fields.enumerate() {
            fixture[address + i * 2..address + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let original = include_bytes!("../web/star-collector.iso");
    for field in [None, Some(16 * 2048 + 158), Some(20 * 2048 + 70)] {
        let mut image = original.to_vec();
        if let Some(offset) = field {
            image[offset..offset + 8].fill(255);
        }
        let mut machine = Machine::new();
        machine.reset(Boot::ExternalBios(&fixture)).unwrap();
        machine.mount_media(Media::iso(&image).unwrap());
        machine.run_steps(3000).unwrap();
        let read = |offset: u32| machine.bus().read32(0x06009000 + offset).unwrap();
        assert_eq!(read(0x30), 0x600D);
        assert_eq!(read(0), if field.is_some() { 0xFF00 } else { 0x0180 });
        assert_eq!(read(0x10), if field.is_some() { 0xFF00 } else { 0x0180 });
        assert_eq!(read(0x24), if field.is_some() { 0 } else { 2 });
    }
}
