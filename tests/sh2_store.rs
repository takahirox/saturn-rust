//! Original guest records and mutations; the oracle is authored stage arithmetic.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn image() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-store.hex")
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
fn both_sh2s_execute_store_alignment_operand_dependency_and_omission_controls_across_cold_boots() {
    let vectors: Vec<_> = include_str!("fixtures/sh2-store.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for variant in 0..5 {
        let mut image = image();
        let mut expected = vectors.clone();
        match variant {
            1 => {
                // Change prepared store data in both kernel calls and the local load source.
                for at in 0x200..0x3000 {
                    if image[at..at + 2] == [0xE6, 3] || image[at..at + 4] == [0xE0, 3, 0x2D, 2] {
                        image[at + 1] = 11;
                    }
                }
                for i in [1, 3, 5, 7, 11] {
                    expected[i] = 11;
                }
            }
            2 => {
                // Fourth kernel stores independent R9 rather than loaded R6.
                for i in 0..16 {
                    image[0x4C0E + i * 4..0x4C10 + i * 4].copy_from_slice(&[0x2C, 0x92]);
                }
                expected[6] = 7; // (8+16*3)/8
                expected[7] = 7;
            }
            3 => {
                image.copy_within(0x400C..0x40F0, 0x400E);
                image[0x400C..0x400E].copy_from_slice(&[0, 9]);
                expected[0] = 7; // (8+1+16*3)/8
            }
            4 => {
                for i in 0..16 {
                    image[0x440E + i * 4..0x4410 + i * 4].copy_from_slice(&[0, 9]);
                }
                expected[2] = 5; // (8+33)/8
                expected[3] = u32::MAX; // sentinel survives omitted stores
            }
            _ => (),
        }
        for chunk in [1u32, 3, 17, 10000] {
            let mut m = Machine::new();
            for _ in 0..2 {
                m.reset(Boot::ExternalBios(&image)).unwrap();
                for _ in 0..10000u32.div_ceil(chunk) {
                    m.run_steps(chunk).unwrap();
                }
                for cpu in [m.cpu_state(), m.slave_state()] {
                    assert_eq!(cpu.status, CpuStatus::Sleeping);
                    assert_eq!(cpu.exceptions, 0);
                }
                for base in [0x0600EE00, 0x0600EF00] {
                    let actual: Vec<_> = (0..expected.len())
                        .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
                        .collect();
                    assert_eq!(actual, expected, "variant {variant}");
                }
            }
        }
    }
}
