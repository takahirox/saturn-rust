//! Guest SNDON/SNDOFF follows MC68000UM §§5.5/6.3.1, ST-169 §§2.3.4–5.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    load(include_str!("fixtures/m68k-reset.hex"))
}
fn load(text: &str) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in text.lines().filter(|line| !line.starts_with('#')) {
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
fn sh2_vector_write_after_sndon_is_seen_by_scheduled_sound_cpu_reset_reads() {
    let original = load(include_str!("fixtures/m68k-reset-phases.hex"));
    for operand in [0x13579BDFu32, 0x2468ACE0] {
        let mut rom = original.clone();
        rom[0x2202..0x2206].copy_from_slice(&operand.to_be_bytes());
        let mut machine = Machine::new();
        for _ in 0..2 {
            machine.reset(Boot::ExternalBios(&rom)).unwrap();
            for _ in 0..10 {
                machine.run_steps(10000).unwrap();
            }
            assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(machine.cpu_state().exceptions, 0);
            assert_eq!(machine.sound_state().exceptions, 0);
            assert!(!machine.sound_state().halted);
            assert_eq!(machine.bus().read32(0x0600ED00), Ok(operand));
        }
    }
    let mut without_write = original;
    let at: Vec<_> = without_write
        .windows(2)
        .enumerate()
        .filter_map(|(i, word)| (word == [0x25, 0x62]).then_some(i))
        .collect();
    assert_eq!(at.len(), 1);
    without_write[at[0]..at[0] + 2].copy_from_slice(&[0, 9]); // guest NOP
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&without_write)).unwrap();
    for _ in 0..10 {
        machine.run_steps(10000).unwrap();
    }
    assert_eq!(machine.bus().read32(0x0600ED00), Ok(0xBAD));
    assert_eq!(machine.sound_state().exceptions, 0);
}

#[test]
fn traced_guest_reset_completes_then_stacks_and_vectors_without_running_the_following_store() {
    let original = load(include_str!("fixtures/m68k-traced-reset.hex"));
    let mut machine = Machine::new();
    for operand in [0x13579BDFu32, 0x2468ACE0] {
        let mut rom = original.clone();
        rom[0x2202..0x2206].copy_from_slice(&operand.to_be_bytes());
        for _ in 0..2 {
            machine.reset(Boot::ExternalBios(&rom)).unwrap();
            for _ in 0..10 {
                machine.run_steps(10000).unwrap();
            }
            let records: Vec<_> = (0..4)
                .map(|i| machine.bus().read32(0x0600EE00 + i * 4).unwrap())
                .collect();
            assert_eq!(records, [0xA700, 0x20C, operand, 0x600D]);
            assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(machine.cpu_state().exceptions, 0);
            assert_eq!(machine.sound_state().exceptions, 1);
            assert_eq!(machine.sound_state().last_vector, Some(9));
            assert!(machine.sound_state().stopped && !machine.sound_state().halted);
        }
    }
}

#[test]
fn sh2_changes_the_trace_vector_during_signed_and_unsigned_sound_cpu_division() {
    for (text, original, changed, result, changed_result, sr) in [
        (
            include_str!("fixtures/m68k-retire-divu.hex"),
            7u32,
            9u32,
            0x10003,
            0x10004,
            0xA700,
        ),
        (
            include_str!("fixtures/m68k-retire-divs.hex"),
            0xFFFFFFF9,
            0xFFFFFFF7,
            0xFFFFFFFD,
            0xFFFFFFFC,
            0xA708,
        ),
    ] {
        for (operand, expected, write_vector) in [
            (original, result, true),
            (changed, changed_result, true),
            (original, result, false),
        ] {
            let mut rom = load(text);
            rom[0x2202..0x2206].copy_from_slice(&operand.to_be_bytes());
            if !write_vector {
                let stores: Vec<_> = rom
                    .windows(2)
                    .enumerate()
                    .filter_map(|(at, word)| (at % 2 == 0 && word == [0x21, 0x02]).then_some(at))
                    .collect();
                assert_eq!(stores.len(), 1);
                rom[stores[0]..stores[0] + 2].copy_from_slice(&[0, 9]);
            }
            let mut machine = Machine::new();
            for _ in 0..2 {
                machine.reset(Boot::ExternalBios(&rom)).unwrap();
                for _ in 0..10 {
                    machine.run_steps(10000).unwrap();
                }
                let records: Vec<_> = (0..4)
                    .map(|i| machine.bus().read32(0x0600EF00 + i * 4).unwrap())
                    .collect();
                assert_eq!(
                    records,
                    [
                        expected,
                        sr,
                        0x20E,
                        if write_vector { 0x600D } else { 0xBAD }
                    ],
                    "SH2={:?} sound={:?} token={:?}",
                    machine.cpu_state(),
                    machine.sound_state(),
                    machine.bus().read32(0x05a0090c)
                );
                assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
                assert_eq!(machine.cpu_state().exceptions, 0);
                assert_eq!(machine.sound_state().exceptions, 1);
                assert_eq!(machine.sound_state().last_vector, Some(9));
                assert!(machine.sound_state().stopped && !machine.sound_state().halted);
            }
        }
    }
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
