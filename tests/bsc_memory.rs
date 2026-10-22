//! Original guest SH7604 §§7.5-7.7 timing evidence. Both CPUs run identical
//! independently authored BIOS code, using their own FRT and actual bus cycles.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/bsc-memory.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
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
fn replace(rom: &mut [u8], from: u32, to: u32) {
    let places: Vec<_> = rom
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == from.to_be_bytes()).then_some(i))
        .collect();
    assert!(!places.is_empty());
    for place in places {
        rom[place..place + 4].copy_from_slice(&to.to_be_bytes());
    }
}

#[test]
fn both_cpus_measure_memory_protocol_and_width_effects_and_reproduce_after_cold_reset() {
    for line in include_str!("fixtures/bsc-memory.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let name = words.next().unwrap();
        let values: Vec<_> = words.map(|v| u32::from_str_radix(v, 16).unwrap()).collect();
        let original_rom = rom();
        let mut rom = original_rom.clone();
        for (original, value) in [0xA55A0401_u32, 0xA55A0040, 0xA55A0000]
            .into_iter()
            .zip(&values[..3])
        {
            let places: Vec<_> = original_rom
                .windows(4)
                .enumerate()
                .filter_map(|(i, b)| (b == original.to_be_bytes()).then_some(i))
                .collect();
            assert_eq!(places.len(), 1);
            rom[places[0]..places[0] + 4].copy_from_slice(&(0xA55A0000 | value).to_be_bytes());
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.run_steps(4000).unwrap();
            assert_eq!(m.cpu_state().exceptions, 0, "{name}");
            assert_eq!(m.slave_state().exceptions, 0, "{name}");
            for base in [0x0600E800, 0x0600E900] {
                let actual: Vec<_> = (0..6)
                    .map(|i| m.bus().read32(base + i * 4).unwrap())
                    .collect();
                assert_eq!(actual, values[3..], "{name} CPU mailbox {base:X}");
            }
        }
    }
}

#[test]
fn changed_guest_operand_changes_actual_computed_data_on_both_cpus() {
    let mut rom = rom();
    replace(&mut rom, 0x12345600, 0x87654300);
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.run_steps(4000).unwrap();
    for base in [0x0600E800, 0x0600E900] {
        assert_eq!(m.bus().read32(base + 12).unwrap(), 0x87654308);
        assert_eq!(m.bus().read32(base + 16).unwrap(), 0x87654308);
        assert_eq!(m.bus().read32(base + 20).unwrap(), 0x600D);
    }
}
