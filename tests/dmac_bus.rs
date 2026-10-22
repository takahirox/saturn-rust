//! H §§9.1.1/9.3.4–9.3.8. Original guest/device work, no host result writes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};

#[test]
fn independent_cpus_execute_all_dma_widths_short_bursts_overlapping_reads_and_irqs() {
    let mut original = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/dmac-bus.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            original[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let golden: Vec<u32> = include_str!("fixtures/dmac-bus.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for changed in [false, true] {
        let mut rom = original.clone();
        let mut replacements = 0;
        if changed {
            for bytes in rom.as_chunks_mut::<4>().0 {
                if *bytes == 0x11223344u32.to_be_bytes() {
                    bytes.copy_from_slice(&0x91223344u32.to_be_bytes());
                    replacements += 1;
                }
            }
            assert_eq!(replacements, 2);
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.run_steps(10000).unwrap();
            for base in [0x0601C000, 0x0601C100] {
                for (i, &word) in golden.iter().enumerate() {
                    let expected = if changed {
                        match word {
                            0x11223344 => 0x91223344,
                            0x44332211 => 0x44332291,
                            0x33441122 => 0x33449122,
                            other => other,
                        }
                    } else {
                        word
                    };
                    assert_eq!(
                        m.bus().read32(base + i as u32 * 4),
                        Ok(expected),
                        "record {base:08X} word {i}"
                    );
                }
            }
            for state in [m.cpu_state(), m.slave_state()] {
                assert_eq!(state.status, CpuStatus::Sleeping, "pc={:08X}", state.pc);
                assert_eq!(state.exceptions, 8);
                assert_eq!(state.last_exception.as_ref().unwrap().vector, 0x44);
            }
            assert_eq!(m.fault(), None);
        }
    }
}
