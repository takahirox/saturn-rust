//! Original MC68000 family/EA programs on the actual sound-RAM/SCSP bus.
//! M68000PRM integer instruction entries, MC68000 only; same ROM runs in Wasm.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    sound::SOUND_RAM,
};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/m68k-families.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn sound_cpu_executes_independent_architectural_families_and_effective_addresses() {
    let rom = rom();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert!(
            m.sound_state().stopped && !m.sound_state().halted,
            "{:?}",
            m.sound_state()
        );
        assert_eq!(m.bus().read32(0x06007004), Ok(0x600D));
        assert_eq!(m.bus().read32(SOUND_RAM + 0x5800), Ok(3));
        assert_eq!(m.bus().read32(SOUND_RAM + 0x5804), Ok(2));
        assert_eq!(m.sound_state().a[7], 0x7FF00);
        for (i, line) in include_str!("fixtures/m68k-families.tsv")
            .lines()
            .enumerate()
        {
            let fields: Vec<_> = line.split('\t').collect();
            let address = SOUND_RAM + 0x5000 + i as u32 * 8;
            let value = u32::from_str_radix(fields[1], 16).unwrap();
            assert_eq!(m.bus().read32(address), Ok(value), "{} result", fields[0]);
            if fields[2] != "-" {
                let flags = u32::from_str_radix(fields[2], 16).unwrap();
                assert_eq!(
                    m.bus().read32(address + 4),
                    Ok(flags),
                    "{} flags",
                    fields[0]
                );
            }
        }
        for (i, line) in include_str!("fixtures/m68k-faults.tsv").lines().enumerate() {
            let fields: Vec<_> = line.split('\t').collect();
            for (j, word) in fields[1..].iter().enumerate() {
                assert_eq!(
                    m.bus().read32(SOUND_RAM + 0x5900 + (i * 32 + j * 4) as u32),
                    Ok(u32::from_str_radix(word, 16).unwrap()),
                    "{} field {j}",
                    fields[0]
                );
            }
        }
        assert!(m.drain_audio(8192).iter().any(|s| *s != 0));
    }
}

#[test]
fn unlk_stack_alias_uses_the_guest_loaded_pointer_and_preserves_other_results() {
    let mut image = rom();
    // MOVE.L #$6200,(A7); UNLK A7; MOVE.L A7,D0. Change the actual
    // frame longword, rather than a mailbox or an emulator register.
    let needle = [0x2E, 0xBC, 0, 0, 0x62, 0, 0x4E, 0x5F, 0x20, 0x0F];
    let positions: Vec<_> = image
        .windows(needle.len())
        .enumerate()
        .filter_map(|(at, bytes)| (bytes == needle).then_some(at))
        .collect();
    assert_eq!(positions.len(), 1);
    image[positions[0] + 4] = 0x63;
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x06007004), Ok(0x600D));
        assert!(m.sound_state().stopped && !m.sound_state().halted);
        assert_eq!(m.sound_state().a[7], 0x7FF00);
        for (i, line) in include_str!("fixtures/m68k-families.tsv")
            .lines()
            .enumerate()
        {
            let fields: Vec<_> = line.split('\t').collect();
            let expected = if fields[0] == "UNLK A7" {
                0x6304
            } else {
                u32::from_str_radix(fields[1], 16).unwrap()
            };
            assert_eq!(
                m.bus().read32(SOUND_RAM + 0x5000 + i as u32 * 8),
                Ok(expected),
                "{} result",
                fields[0]
            );
            if fields[2] != "-" {
                assert_eq!(
                    m.bus().read32(SOUND_RAM + 0x5004 + i as u32 * 8),
                    Ok(u32::from_str_radix(fields[2], 16).unwrap()),
                    "{} flags",
                    fields[0]
                );
            }
        }
        for (i, line) in include_str!("fixtures/m68k-faults.tsv").lines().enumerate() {
            for (j, word) in line.split('\t').skip(1).enumerate() {
                assert_eq!(
                    m.bus().read32(SOUND_RAM + 0x5900 + (i * 32 + j * 4) as u32),
                    Ok(u32::from_str_radix(word, 16).unwrap())
                );
            }
        }
        assert!(m.drain_audio(8192).iter().any(|s| *s != 0));
    }
}

#[test]
fn changing_guest_rmw_to_a_plain_write_changes_the_bus_error_access_and_ir() {
    for (opcode, replacement, index) in [(0x50D0u16, 0x1080u16, 10), (0x40D0, 0x3080, 11)] {
        let mut image = rom();
        let [high, low] = opcode.to_be_bytes();
        let needle = [high, low, 0x33, 0xFC, 0x12, 0x34];
        let positions: Vec<_> = image
            .windows(needle.len())
            .enumerate()
            .filter_map(|(at, bytes)| (bytes == needle).then_some(at))
            .collect();
        assert_eq!(positions.len(), 1);
        image[positions[0]..positions[0] + 2].copy_from_slice(&replacement.to_be_bytes());
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x06007004), Ok(0x600D));
        for (i, line) in include_str!("fixtures/m68k-faults.tsv").lines().enumerate() {
            let fields: Vec<_> = line.split('\t').collect();
            for (j, word) in fields[1..].iter().enumerate() {
                let expected = if i == index && j == 0 {
                    0x00050008
                } else if i == index && j == 1 {
                    u32::from(replacement)
                } else {
                    u32::from_str_radix(word, 16).unwrap()
                };
                assert_eq!(
                    m.bus().read32(SOUND_RAM + 0x5900 + (i * 32 + j * 4) as u32),
                    Ok(expected),
                    "{} field {j}",
                    fields[0]
                );
            }
        }
    }
}

#[test]
fn changing_movem_operand_width_removes_only_its_terminal_bus_error() {
    let mut image = rom();
    // MOVEM.L (A0)+,D0 at 7FFFC reads a long then faults at 80000.
    // MOVEM.W reads one word and its terminal read at 7FFFE succeeds.
    let needle = [0x4C, 0xD8, 0, 1, 0x33, 0xFC, 0x12, 0x34];
    let at = image
        .windows(needle.len())
        .position(|v| v == needle)
        .unwrap();
    assert!(!image[at + 1..].windows(needle.len()).any(|v| v == needle));
    image[at + 1] = 0x98;
    let vectors: Vec<_> = include_str!("fixtures/m68k-faults.tsv")
        .lines()
        .filter(|s| !s.starts_with("movem long terminal\t"))
        .collect();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&image)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x06007004), Ok(0x600D));
        assert!(m.sound_state().stopped && !m.sound_state().halted);
        for (i, line) in vectors.iter().enumerate() {
            let fields: Vec<_> = line.split('\t').collect();
            for (j, word) in fields[1..].iter().enumerate() {
                assert_eq!(
                    m.bus().read32(SOUND_RAM + 0x5900 + (i * 32 + j * 4) as u32),
                    Ok(u32::from_str_radix(word, 16).unwrap()),
                    "{} field {j}",
                    fields[0]
                );
            }
        }
        for j in 0..8 {
            assert_eq!(
                m.bus()
                    .read32(SOUND_RAM + 0x5900 + (vectors.len() * 32 + j * 4) as u32),
                Ok(0)
            );
        }
    }
}
