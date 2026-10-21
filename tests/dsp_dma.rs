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
    machine.run_steps(10000).unwrap();
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 0);
    (0..13)
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
        (0x8C000039, 0x8C00004D, 8, 77),
        (0x00001207, 0x0000120B, 7, 11),
        (0x8C00006B, 0x8C00007D, 11, 125),
        (0xC000C400, 0xC000C401, 11, 0), // Only word zero is loaded; FF remains NOP.
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

#[test]
fn guest_access_to_a_live_dma_bank_faults_without_canned_parallel_results() {
    use saturn_rust::{MachineError, scu::ScuError};
    let mut changed = rom();
    let needle = 0x00003204u32.to_be_bytes(); // legal MC0 -> MC2 during M1 DMA
    let at = changed.windows(4).position(|b| b == needle).unwrap();
    assert!(changed[at + 1..].windows(4).all(|b| b != needle));
    changed[at..at + 4].copy_from_slice(&0x00003205u32.to_be_bytes()); // MC1 -> MC2
    let mut machine = Machine::new();
    for _ in 0..2 {
        machine.reset(Boot::ExternalBios(&changed)).unwrap();
        let error = machine.run_steps(4000).unwrap_err();
        assert!(matches!(
            error,
            MachineError::Scu(ScuError::DspDmaConflict { pc: 2, bank: 1 })
        ));
        assert_eq!(machine.bus().read32(0x0600E400).unwrap(), 0);
        let clocks = machine.clocks();
        let cpu = machine.cpu_state().clone();
        for _ in 0..3 {
            assert_eq!(machine.run_steps(4000), Err(error.clone()));
            assert_eq!(machine.clocks(), clocks);
            assert_eq!(machine.cpu_state(), &cpu);
        }
    }
    assert_eq!(run(&mut machine, &rom())[2], 0x1234);
}
