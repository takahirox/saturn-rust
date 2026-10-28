//! Independent ST-077 fig4.18/4.30/4.31 guest oracle. Original MIT fixture.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/scsp-pipeline.hex")
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

#[test]
fn real_cpus_upload_a_dsp_that_distinguishes_early_late_and_sample_wrap_publication() {
    let original = rom();
    let operands: Vec<_> = include_str!("fixtures/scsp-pipeline.tsv")
        .split_whitespace()
        .map(|v| usize::from_str_radix(v, 16).unwrap())
        .collect();
    for (a, b) in [(0x2000u16, 0x4000u16), (0x6000, 0xC000), (0xA000, 0x2000)] {
        for omit in [0, 1, 2] {
            // full, omitted slot31 send, omitted slot31 KEYON
            let mut image = original.clone();
            image[operands[0]..operands[0] + 2].copy_from_slice(&a.to_be_bytes());
            image[operands[1]..operands[1] + 2].copy_from_slice(&b.to_be_bytes());
            if omit != 0 {
                let at = operands[if omit == 1 { 2 } else { 3 }];
                image[at..at + 2].copy_from_slice(&0u16.to_be_bytes());
            }
            let (left, right) = (if omit == 0 { b as i16 / 2 } else { 0 }, a as i16 / 2);
            let expected = [
                u32::from(right as u16),
                u32::from(left as u16),
                0x30060000,
                0x600D,
            ];
            let mut m = Machine::new();
            for _ in 0..2 {
                m.reset(Boot::ExternalBios(&image)).unwrap();
                for _ in 0..10 {
                    m.run_steps(10000).unwrap();
                }
                let records: Vec<_> = (0..4)
                    .map(|i| m.bus().read32(0x0600EF00 + i * 4).unwrap())
                    .collect();
                assert_eq!(records, expected, "a={a:04x} b={b:04x} omit={omit}");
                assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
                assert_eq!(m.cpu_state().exceptions, 0);
                assert!(m.sound_state().stopped && !m.sound_state().halted);
                assert_eq!(m.sound_state().exceptions, 0);
                m.drain_audio(8192);
                m.run_steps(10000).unwrap();
                let audio = m.drain_audio(8192);
                assert!(audio.len() >= 128);
                assert!(audio.as_chunks::<2>().0.iter().all(|s| *s == [left, right]));
            }
        }
    }
}
