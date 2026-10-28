//! Programming Manual table7.2 and SH7604 §14.3: original FRT-timed kernels.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-multiplier.hex")
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
    for _ in 0..30 {
        m.run_steps(10000).unwrap();
    }
    let fast = image.windows(4).any(|b| b == [0xEB, 1, 0x2B, 0xB8]);
    for (i, cpu) in [m.cpu_state(), m.slave_state()].into_iter().enumerate() {
        assert_eq!(cpu.status, CpuStatus::Sleeping);
        assert_eq!(cpu.exceptions, u64::from(fast && i == 0));
    }
    [0x0600E900, 0x0600EB00].map(|base| {
        (0..include_str!("fixtures/sh2-multiplier.tsv").lines().count())
            .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
            .collect()
    })
}
#[test]
fn both_guest_cpus_measure_multiplier_interlocks_with_their_own_frt() {
    for fast in [false, true] {
        let mut image = rom();
        if fast {
            let at = image
                .windows(4)
                .position(|b| b == [0xEB, 0, 0x2B, 0xB8])
                .unwrap();
            image[at + 1] = 1;
        }
        let expected: Vec<_> = include_str!("fixtures/sh2-multiplier.tsv")
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
}

#[test]
fn multiplier_guest_operand_spacing_and_omitted_operation_controls() {
    let expected: Vec<u32> = include_str!("fixtures/sh2-multiplier.tsv")
        .lines()
        .map(|line| u32::from_str_radix(line, 16).unwrap())
        .collect();
    for variant in 0..3 {
        let mut image = rom();
        let mut changed = expected.clone();
        let start = 0x400C;
        match variant {
            0 => {
                // MULS.W R2,R2 instead of R1,R2: 7*7 rather than 3*7.
                for i in 0..16 {
                    image[start + i * 4 + 1] = 0x2F;
                }
                changed[1] = 49;
            }
            1 => {
                // Keep the original sixteen products and STS, inserting two
                // independent NOPs per pair. Primary: 16*(1+1+1+2)=80.
                image.copy_within(start + 64..start + 74, start + 128);
                for i in 0..16 {
                    image[start + i * 8..start + i * 8 + 8]
                        .copy_from_slice(&[0x22, 0x1F, 0, 9, 0, 9, 0x06, 0x1A]);
                }
                changed[0] = 11; // floor((8+80)/8)
            }
            _ => {
                for i in 0..16 {
                    image[start + i * 4..start + i * 4 + 2].copy_from_slice(&[0, 9]);
                }
                changed[0] = 7; // floor((8+16*3)/8)
                changed[1] = 0;
            }
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            for trace in run(&mut m, &image) {
                assert_eq!(trace, changed, "mutation {variant}");
            }
        }
    }
}
