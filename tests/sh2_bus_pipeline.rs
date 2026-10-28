//! SH7604 §§7.3.1/7.11.2: original executing fetch/write-buffer guests.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-bus-pipeline.hex")
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
fn both_guest_cpus_execute_retained_words_branch_refetch_and_buffered_cpu_and_dma_writes() {
    for (width_code, changed) in [1, 2, 3]
        .into_iter()
        .flat_map(|code| [false, true].map(|changed| (code, changed)))
    {
        let mut image = rom();
        let mut widths = 0;
        for bytes in image.as_chunks_mut::<4>().0 {
            if *bytes == 0xA55A00FCu32.to_be_bytes() {
                *bytes = (0xA55A003Cu32 | width_code << 6).to_be_bytes();
                widths += 1;
            }
        }
        assert_eq!(widths, 2);
        let mut expected: Vec<_> = include_str!("fixtures/sh2-bus-pipeline.tsv")
            .lines()
            .map(|s| u32::from_str_radix(s, 16).unwrap())
            .collect();
        if changed {
            let mut changes = [0; 2];
            for at in (0..image.len() - 4).step_by(2) {
                if image[at..at + 4] == 0x0000E62Au32.to_be_bytes() {
                    image[at + 3] = 0x3D;
                    changes[0] += 1;
                }
                if image[at..at + 4] == [0xE0, 0x31, 0x29, 0] {
                    image[at + 1] = 0x52;
                    changes[1] += 1;
                }
            }
            assert_eq!(changes, [2, 2]);
            expected[1] = 61;
            expected[6] = 0x52520000;
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            for cpu in [m.cpu_state(), m.slave_state()] {
                assert_eq!(cpu.status, CpuStatus::Sleeping, "{:08X}", cpu.pc);
                assert_eq!(cpu.exceptions, 0);
            }
            for base in [0x0601D000, 0x0601D100] {
                let words: Vec<_> = (0..expected.len())
                    .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
                    .collect();
                assert_eq!(words, expected, "CPU mailbox {base:08X}");
            }
        }
    }
}
