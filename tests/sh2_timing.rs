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
        (0..13)
            .map(|i| m.bus().read32(base + i * 4).unwrap())
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
    assert_eq!(occurrences.len(), 56); // 32 arithmetic + 8×3 branch/slot adds
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
    changed[0] = 5;
    changed[1] = 16;
    for trace in run(&mut Machine::new(), &image) {
        assert_eq!(trace, changed);
    }
}
