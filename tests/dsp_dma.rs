//! Original guest execution of ST-097 §§4.3–4.5 DMA and transferred DSP code.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/dsp-dma.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

fn run(machine: &mut Machine, rom: &[u8]) -> Vec<u32> {
    machine.reset(Boot::ExternalBios(rom)).unwrap();
    machine.run_steps(4000).unwrap();
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 0);
    (0..7)
        .map(|i| machine.bus().read32(0x0600E400 + i * 4).unwrap())
        .collect()
}

#[test]
fn guest_register_count_fixed_destination_and_dma_loaded_dsp_code_execute_after_reset() {
    let rom = rom();
    let expected: Vec<_> = include_str!("fixtures/dsp-dma.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut machine = Machine::new();
    for _ in 0..2 {
        assert_eq!(run(&mut machine, &rom), expected);
    }
}

#[test]
fn changed_dma_data_count_and_loaded_opcode_change_computed_guest_results() {
    let original = rom();
    let mut machine = Machine::new();
    let baseline = run(&mut machine, &original);
    for (before, after, index, value) in [
        (0x11u32, 0x35u32, 0, 0x35),
        (3, 2, 0, 0), // Two input words leave M1[2]=0 for the third fixed write.
        (0x8400002A, 0x84000063, 5, 99),
    ] {
        let needle = before.to_be_bytes();
        let at = original
            .windows(4)
            .position(|bytes| bytes == needle)
            .unwrap();
        assert!(original[at + 1..].windows(4).all(|bytes| bytes != needle));
        let mut changed = original.clone();
        changed[at..at + 4].copy_from_slice(&after.to_be_bytes());
        let mut expected = baseline.clone();
        expected[index] = value;
        assert_eq!(run(&mut machine, &changed), expected);
    }
}
