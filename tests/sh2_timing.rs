//! Programming Manual table7.2 and SH7604 §14.3: original FRT-timed kernels.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-timing.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}
fn run(m: &mut Machine, image: &[u8]) -> [Vec<u32>; 2] {
    m.reset(Boot::ExternalBios(image)).unwrap();
    m.run_steps(10000).unwrap();
    for cpu in [m.cpu_state(), m.slave_state()] {
        assert_eq!(cpu.status, CpuStatus::Sleeping);
        assert_eq!(cpu.exceptions, 0);
    }
    [0x0600E600, 0x0600E700].map(|base| {
        (0..include_str!("fixtures/sh2-timing.tsv").lines().count())
            .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
            .collect()
    })
}
#[test]
fn both_guest_cpus_measure_instruction_states_with_their_own_frt() {
    let image = rom();
    let expected: Vec<_> = include_str!("fixtures/sh2-timing.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    for _ in 0..2 {
        for trace in run(&mut m, &image) {
            assert_eq!(trace, expected);
        }
    }
}
#[test]
fn changed_guest_control_flow_changes_elapsed_time_and_computation() {
    let mut image = rom();
    let occurrences: Vec<_> = image
        .windows(2)
        .enumerate()
        .filter_map(|(i, b)| (b == [0x76, 1]).then_some(i))
        .collect();
    assert_eq!(occurrences.len(), 120); // arithmetic, branch/slot and STS WB successors
    // Replace 16 of the 32 ADDs by eight BRA-to-successor/NOP pairs. This adds eight
    // real states and removes sixteen increments without a host result write.
    let start = occurrences[0];
    for i in 0..8 {
        image[start + i * 4..start + i * 4 + 4].copy_from_slice(&[0xA0, 0, 0, 9]);
    }
    let expected: Vec<_> = include_str!("fixtures/sh2-timing.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut changed = expected;
    changed[0] = 6;
    changed[1] = 16;
    for trace in run(&mut Machine::new(), &image) {
        assert_eq!(trace, changed);
    }
}

#[test]
fn guest_peripheral_operand_mutation_changes_read_data_on_both_cpus_without_changing_time() {
    let mut image = rom();
    let mut count = 0;
    for at in (0..image.len() - 4).step_by(2) {
        // MOV #-1,R0; MOV.B R0,@R10 -> MOV #$5A,R0, then real SCI BRR.
        if image[at..at + 4] == [0xE0, 0xFF, 0x2A, 0] {
            image[at + 1] = 0x5A;
            count += 1;
        }
    }
    assert_eq!(count, 34);
    let mut expected: Vec<_> = include_str!("fixtures/sh2-timing.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    expected[21] = 0x5A;
    let mut machine = Machine::new();
    for _ in 0..2 {
        for trace in run(&mut machine, &image) {
            assert_eq!(trace, expected);
        }
    }
}

#[test]
fn system_register_dependency_controls_preserve_real_if_splits() {
    for (pattern, replacement, time_index) in [
        ([0x06, 0x1A, 0x76, 1], [0x79, 1], 24), // STS MACL,R6; ADD #1,R9
        ([0x4C, 0x26, 0x06, 0x2A], [0xE6, 0], 30), // LDS.L PR; MOV #0,R6
    ] {
        let mut image = rom();
        let mut changes = 0;
        for at in (0..image.len() - 4).step_by(2) {
            if image[at..at + 4] == pattern {
                image[at + 2..at + 4].copy_from_slice(&replacement);
                changes += 1;
            }
        }
        assert_eq!(changes, 16);
        let mut expected: Vec<_> = include_str!("fixtures/sh2-timing.tsv")
            .lines()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        // STS MACL's independent pairs take 32 clocks. PR's first LDS.L
        // is at 4n+2, so all sixteen pairs retain a real IF/MA split:
        // 32+16=48 clocks, even without a PR dependency (P figs7.6/7.9).
        // Its seven-state wrapper gives floor((7+48)/8)=6 FRT ticks.
        expected[time_index] = if time_index == 30 { 6 } else { 5 };
        expected[time_index + 1] = 0;
        let mut m = Machine::new();
        for _ in 0..2 {
            for trace in run(&mut m, &image) {
                assert_eq!(trace, expected);
            }
        }
    }
}
