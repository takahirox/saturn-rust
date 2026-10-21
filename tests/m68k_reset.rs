//! Guest SNDON/SNDOFF follows MC68000UM §§5.5/6.3.1, ST-169 §§2.3.4–5.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/m68k-reset.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
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
    for _ in 0..10 {
        machine.run_steps(10000).unwrap();
    }
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 0);
    assert_eq!(machine.sound_state().exceptions, 0);
    assert!(machine.sound_state().stopped && !machine.sound_state().halted);
    assert_eq!(machine.sound_state().a[7], 0x7FE00);
    (0..19)
        .map(|i| machine.bus().read32(0x0600EB00 + i * 4).unwrap())
        .collect()
}
fn expected() -> Vec<u32> {
    include_str!("fixtures/m68k-reset.tsv")
        .lines()
        .map(|word| u32::from_str_radix(word, 16).unwrap())
        .collect()
}

#[test]
fn both_guest_cpus_verify_retained_registers_and_reloaded_stack_after_sound_reset() {
    let original = rom();
    let mut machine = Machine::new();
    for _ in 0..2 {
        assert_eq!(run(&mut machine, &original), expected());
    }
}

#[test]
fn changing_the_pre_reset_guest_operand_changes_the_post_reset_record() {
    let mut changed = rom();
    // First-entry MOVE.L #13570013,D3 -> #13579BDF,D3. The reset entry
    // has no initialization or host-written expected result for D3.
    let before = [0x26, 0x3C, 0x13, 0x57, 0, 0x13];
    let at: Vec<_> = changed
        .windows(6)
        .enumerate()
        .filter_map(|(i, b)| (b == before).then_some(i))
        .collect();
    assert_eq!(at.len(), 1);
    changed[at[0] + 2..at[0] + 6].copy_from_slice(&0x13579BDFu32.to_be_bytes());
    let mut vectors = expected();
    vectors[3] = 0x13579BDF;
    let mut machine = Machine::new();
    for _ in 0..2 {
        assert_eq!(run(&mut machine, &changed), vectors);
    }
}
