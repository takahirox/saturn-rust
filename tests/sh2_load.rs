//! Original guest records and mutations; the oracle is authored stage arithmetic.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn image() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-load.hex")
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

#[test]
fn both_sh2s_execute_load_alignment_operand_and_dependency_controls_across_cold_boots() {
    let vectors: Vec<_> = include_str!("fixtures/sh2-load.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for variant in 0..4 {
        let mut image = image();
        let mut expected = vectors.clone();
        match variant {
            1 => {
                let at = image
                    .windows(4)
                    .position(|b| b == [0xE0, 3, 0x2C, 2])
                    .unwrap();
                image[at + 1] = 7;
                for i in [1, 3, 9] {
                    expected[i] = 7;
                }
                for i in [5, 7, 11] {
                    expected[i] = 8;
                }
            }
            2 => {
                // Change only the dependent ADD destination in the third kernel.
                for i in 0..16 {
                    image[0x480E + i * 4] = 0x79;
                }
                expected[4] = 5;
                expected[5] = 3; // (8+16*2)/8
            }
            3 => {
                // Move the first kernel body and epilogue by a word, inserting NOP.
                image.copy_within(0x400C..0x40F0, 0x400E);
                image[0x400C..0x400E].copy_from_slice(&[0, 9]);
                expected[0] = 7; // (8+1+16*3)/8
            }
            _ => (),
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            for cpu in [m.cpu_state(), m.slave_state()] {
                assert_eq!(cpu.status, CpuStatus::Sleeping);
                assert_eq!(cpu.exceptions, 0);
            }
            for base in [0x0600EC00, 0x0600ED00] {
                let actual: Vec<_> = (0..expected.len())
                    .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
                    .collect();
                assert_eq!(actual, expected, "variant {variant}");
            }
        }
    }
}
