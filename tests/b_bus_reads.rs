//! Original MIT guest, executed identically in browser Wasm.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};

fn bios() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/b-bus-reads.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let mut at = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
        for word in fields {
            rom[at..at + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    rom
}
#[test]
fn guest_bridge_reads_both_cpus_self_initialize_mutate_and_measure_frt() {
    let original = bios();
    let golden: Vec<_> = include_str!("fixtures/b-bus-reads.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    let mut normal_times = Vec::new();
    for (a, b, omit) in [
        (0xA1B2u16, 0xC3D4u16, false),
        (0x1122, 0x3344, false),
        (0xA1B2, 0xC3D4, true),
    ] {
        let mut rom = original.clone();
        rom[0x100..0x102].copy_from_slice(&a.to_be_bytes());
        rom[0x102..0x104].copy_from_slice(&b.to_be_bytes());
        if omit {
            let mut count = 0;
            for at in (0x200..0x2000).step_by(2) {
                if rom[at..at + 2] == [0x60, 0x41] {
                    rom[at..at + 2].copy_from_slice(&[0x60, 0xC3]); // MOV R12,R0
                    count += 1;
                }
            }
            assert_eq!(count, 80);
        }
        for repeat in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            for _ in 0..100 {
                if m.bus().read32(0x0600E190) == Ok(0x600D) {
                    break;
                }
                m.run_steps(5000).unwrap();
            }
            let mut times = Vec::new();
            for base in [0x0600E000, 0x0600E100] {
                let mut actual: Vec<_> = (0..golden.len())
                    .map(|i| m.bus().read32(base + i as u32 * 4).unwrap())
                    .collect();
                for index in [3, 11, 19, 27, 35] {
                    times.push(actual[index]);
                    assert!(
                        (if omit { 10..64 } else { 32..128 }).contains(&actual[index]),
                        "{actual:X?}"
                    );
                    actual[index] = 0;
                }
                let mut expected = vec![
                    u32::from(a),
                    u32::from(b),
                    (u32::from(a) << 16) | u32::from(b),
                    0,
                ];
                for _ in 0..4 {
                    expected.extend([
                        u32::from(a >> 8),
                        u32::from(a & 255),
                        u32::from(b >> 8),
                        u32::from(b & 255),
                        u32::from(a),
                        u32::from(b),
                        (u32::from(a) << 16) | u32::from(b),
                        0,
                    ]);
                }
                expected.push(0x600D);
                if a == 0xA1B2 {
                    assert_eq!(expected, golden);
                }
                assert_eq!(actual, expected);
            }
            if !omit {
                if normal_times.is_empty() {
                    normal_times = times;
                } else {
                    assert_eq!(times, normal_times);
                }
            } else {
                for (short, long) in times.iter().zip(&normal_times) {
                    assert!(short + 20 < *long, "repeat={repeat}: {short}/{long}");
                }
            }
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.slave_state().exceptions, 0);
            assert!(m.slave_state().instructions > 100);
        }
    }
}
