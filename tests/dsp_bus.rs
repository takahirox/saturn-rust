//! Original SH-2 executes/upload SCU DSP DMA on real sound/VDP1 RAM buses.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/dsp-bus.hex")
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
fn expected() -> Vec<u32> {
    include_str!("fixtures/dsp-bus.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect()
}
fn run(machine: &mut Machine, rom: &[u8]) -> Vec<u32> {
    machine.reset(Boot::ExternalBios(rom)).unwrap();
    machine.run_steps(10000).unwrap();
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 0);
    (0..expected().len())
        .map(|i| machine.bus().read32(0x0600e800 + i as u32 * 4).unwrap())
        .collect()
}
#[test]
fn guest_b_bus_dma_all_output_strides_wrap_and_sound_ram_input_repeat_after_reset() {
    let mut machine = Machine::new();
    for _ in 0..2 {
        assert_eq!(run(&mut machine, &rom()), expected());
    }
}
#[test]
fn changed_guest_data_and_dma_stride_change_only_the_authored_records() {
    let original = rom();
    let mut machine = Machine::new();
    let mut changed = original.clone();
    let needle = 0x11223344u32.to_be_bytes();
    let positions: Vec<_> = original
        .windows(4)
        .enumerate()
        .filter(|(_, b)| *b == needle)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(positions.len(), 8); // One original data operand per stride.
    for at in positions {
        changed[at..at + 4].copy_from_slice(&0x55667788u32.to_be_bytes());
    }
    let mut words = expected();
    words[1] = 0x55667788;
    for i in [3, 7, 11, 15, 19, 23] {
        words[i] = 0x5566eeee;
        words[i + 1] = 0x7788eeee;
    }
    assert_eq!(run(&mut machine, &changed), words);

    let mut changed = original.clone();
    let needle = 0xc0009002u32.to_be_bytes(); // First incrementing B-bus output.
    let at = original.windows(4).position(|b| b == needle).unwrap();
    changed[at..at + 4].copy_from_slice(&0xc0001002u32.to_be_bytes()); // add0: low word overwrites high.
    let mut words = expected();
    words[1] = 0xccddeeee;
    words[2] = 0xeeeeeeee;
    assert_eq!(run(&mut machine, &changed), words);
}
