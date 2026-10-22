//! Original SH-2 guest: ST-162 §§3.2–3.4/8.2.7 selector/host concurrency.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-copy.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        let at = usize::from_str_radix(address, 16).unwrap();
        for (i, word) in fields.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn guest_copies_and_moves_real_sectors_while_host_transfers_and_selector_commands_run() {
    let mut m = Machine::new();
    let mut previous = None;
    for payload in [0x1357u16, 0x1357, 0x2468, 0x2468] {
        let mut image = rom();
        image[0x100..0x102].copy_from_slice(&payload.to_be_bytes());
        m.reset(Boot::ExternalBios(&image)).unwrap();
        m.mount_media(Media::iso(&vec![0; 2048 * 100]).unwrap());
        for _ in 0..100 {
            m.run_steps(10000).unwrap();
            if m.bus().read32(0x0600_A80C) == Ok(0x600D) {
                break;
            }
        }
        let read = |command: u32, word: u32| {
            m.bus()
                .read32(0x0600_A000 + command * 16 + word * 4)
                .unwrap()
        };
        assert_eq!(m.bus().read32(0x0600_A80C), Ok(0x600D));
        assert_eq!(m.cpu_state().exceptions, 0);
        for (command, expected) in [
            (4, 255),
            (7, 0),
            (19, 255),
            (20, 0),
            (30, 255),
            (32, 1),
            (37, 0),
            (43, 1),
        ] {
            assert_eq!(
                read(command, 0) & 255,
                expected,
                "error/status at command{command}"
            );
        }
        for (command, word, expected) in [
            (8, 3, 32),
            (9, 1, 1),
            (12, 1, 1),
            (14, 1, 1),
            (21, 3, 0),
            (22, 3, 32),
            (23, 1, 0),
            (25, 1, 1),
            (33, 3, 32),
            (38, 3, 0),
            (45, 1, 0),
        ] {
            assert_eq!(
                read(command, word),
                expected,
                "response{command} word{word}"
            );
        }
        assert_eq!(read(17, 0) & 0x8000, 0x8000); // capacity WAIT
        for command in [3, 5, 6, 18, 29, 31, 35, 41, 42] {
            assert_eq!(read(command, 0) & 0x8000, 0, "accepted command{command}");
        }
        for (at, expected) in [
            (0x0600_A800, u32::from(payload)),
            (0x0600_A804, 0x5678),
            (0x0600_A808, u32::from(payload)),
        ] {
            assert_eq!(m.bus().read32(at), Ok(expected));
        }
        let trace = (0..46 * 4)
            .map(|i| m.bus().read32(0x0600_A000 + i * 4).unwrap())
            .collect::<Vec<_>>();
        if let Some(old) = previous {
            assert_eq!(trace, old);
        }
        previous = Some(trace);
    }
}
