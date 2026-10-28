//! Same original bytes and manual-derived slot oracle as production Wasm.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn image() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/sh2-fetch.hex")
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

fn variant(n: usize) -> (Vec<u8>, Vec<u32>) {
    let mut rom = image();
    let mut expected: Vec<_> = include_str!("fixtures/sh2-fetch.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    if n == 1 {
        // Guest local source data changes; instruction timing is unchanged.
        let mut found = 0;
        for at in (0x200..0x7000).step_by(2) {
            if rom[at..at + 4] == [0xE0, 3, 0x2C, 2] {
                rom[at + 1] = 11;
                found += 1;
            }
        }
        assert_eq!(found, 1);
        for i in [2, 3, 6, 7, 9, 11, 13] {
            expected[2 * i + 1] = 11;
        }
    } else if n == 2 || n == 3 {
        // Modify both the ROM kernels and the guest's RAM installation literals.
        let (before, after) = if n == 2 {
            (0x7601u16, 0x7602u16)
        } else {
            (0x66C2, 9)
        };
        for at in (0x8000..0x9000).step_by(2) {
            if rom[at..at + 2] == before.to_be_bytes() {
                rom[at..at + 2].copy_from_slice(&after.to_be_bytes());
            }
        }
        let mut found = 0;
        for at in (0x200..0x7000).step_by(4) {
            if rom[at..at + 4] == u32::from(before).to_be_bytes() {
                rom[at..at + 4].copy_from_slice(&u32::from(after).to_be_bytes());
                found += 1;
            }
        }
        assert!(found > 0);
        if n == 2 {
            for i in [0, 1, 4, 5, 8, 10, 12] {
                expected[2 * i + 1] = 64;
            }
        } else {
            for i in [2, 3, 6, 7, 9, 11, 13] {
                expected[2 * i + 1] = 0;
            }
            for (i, ticks) in [(3, 4), (7, 7), (9, 11), (11, 11), (13, 9)] {
                expected[2 * i] = ticks;
            }
        }
    } else if n == 4 {
        let mut found = 0;
        for at in (0x200..0x7000).step_by(4) {
            if rom[at..at + 4] == 0xA55A0080u32.to_be_bytes() {
                rom[at..at + 4].copy_from_slice(&0xA55A0040u32.to_be_bytes());
                found += 1;
            }
        }
        assert_eq!(found, 2);
        expected[16] = 9;
        expected[18] = 11;
    }
    (rom, expected)
}

#[test]
fn both_cpus_measure_cached_cold_p1_waited_and_cs0_fetch_streams_and_handoff() {
    for control in 0..5 {
        let (image, expected) = variant(control);
        for chunk in [1u32, 3, 17, 10000] {
            let mut m = Machine::new();
            for _ in 0..2 {
                m.reset(Boot::ExternalBios(&image)).unwrap();
                let mut remaining = 30000;
                while remaining != 0 {
                    let n = remaining.min(chunk);
                    m.run_steps(n).unwrap();
                    // Passive inspection is deliberately interleaved with advancement.
                    let _ = m.cpu_state();
                    let _ = m.slave_state();
                    let _ = m.bus().debug_read32(0x0600DA00);
                    remaining -= n;
                }
                for (p, base) in [0x0600D800, 0x0600D900].into_iter().enumerate() {
                    let actual: Vec<_> = (0..expected.len())
                        .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "CPU {p}, control {control}, chunk {chunk}"
                    );
                }
                assert_eq!(
                    m.bus().read32(0x0600D800 + expected.len() as u32 * 4),
                    Ok(0x600D)
                );
                assert_eq!(
                    m.bus().read32(0x0600D804 + expected.len() as u32 * 4),
                    Ok(0x12345679)
                );
                assert_eq!(
                    m.bus().read32(0x0600D900 + expected.len() as u32 * 4),
                    Ok(0x12345679)
                );
                assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
                assert_eq!(m.slave_state().status, CpuStatus::Sleeping);
                assert_eq!(
                    (m.cpu_state().exceptions, m.slave_state().exceptions),
                    (1, 0)
                );
            }
        }
    }
}
