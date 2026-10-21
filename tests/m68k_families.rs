//! Original MC68000 family/EA programs on the actual sound-RAM/SCSP bus.
//! M68000PRM integer instruction entries, MC68000 only; same ROM runs in Wasm.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    sound::SOUND_RAM,
};

#[test]
fn sound_cpu_executes_independent_architectural_families_and_effective_addresses() {
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
