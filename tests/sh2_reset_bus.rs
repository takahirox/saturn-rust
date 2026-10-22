//! H §§4.2.2–3/7.11.1: original dual-SH-2 watchdog/reset/buffer guest.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn image(manual: bool, payload: u32) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/sh2-reset-bus.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let mut controls = 0;
    let mut operands = 0;
    for at in (0..rom.len()).step_by(4) {
        let value = u32::from_be_bytes(rom[at..at + 4].try_into().unwrap());
        if value == 0x5A7F {
            controls += 1;
            if !manual {
                rom[at..at + 4].copy_from_slice(&0x5A5Fu32.to_be_bytes());
            }
        }
        if value == 0x12345678 {
            operands += 1;
            rom[at..at + 4].copy_from_slice(&payload.to_be_bytes());
        }
    }
    assert_eq!((controls, operands), (2, 2));
    rom
}

#[test]
fn both_guest_cpus_observe_finished_manual_reset_write_and_aborted_power_reset_write() {
    let mut machine = Machine::new();
    for manual in [true, false] {
        for payload in [0x12345678, 0x89ABCDEF] {
            for _ in 0..2 {
                machine
                    .reset(Boot::ExternalBios(&image(manual, payload)))
                    .unwrap();
                machine.run_steps(10000).unwrap();
                for cpu in [machine.cpu_state(), machine.slave_state()] {
                    assert_eq!(cpu.status, CpuStatus::Sleeping);
                    assert_eq!(cpu.exceptions, 0);
                }
                let expected = if manual {
                    [2, payload, 0x7C, 0, 0xFF, 0x600D]
                } else {
                    [1, 0, 0xFC, 0, 0xDF, 0x600D]
                };
                for base in [0x0601E000, 0x0601E100] {
                    let actual: Vec<_> = (0..6)
                        .map(|i| machine.bus().read32(base + i * 4).unwrap())
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "manual={manual} payload={payload:X} base={base:X}"
                    );
                }
            }
        }
    }
}
