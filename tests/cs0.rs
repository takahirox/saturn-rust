//! Original MIT dual-SH-2 guest; H §7.8 figs7.44–46.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn image(payload: u32, waits: u32, omit: bool) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/cs0.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom[0x1F000..0x1F004].copy_from_slice(&payload.to_be_bytes());
    let mut changed = 0;
    for at in (0..rom.len()).step_by(4) {
        if rom[at..at + 4] == 0xA55A0001u32.to_be_bytes() {
            rom[at..at + 4].copy_from_slice(&(0xA55A0000 | waits).to_be_bytes());
            changed += 1;
        }
    }
    assert_eq!(changed, 2);
    if omit {
        let mut changed = 0;
        for at in (0x8000..0x8100).step_by(2) {
            if rom[at..at + 2] == [0x66, 0xA2] {
                // MOV.L @R10,R6 -> NOP
                rom[at..at + 2].copy_from_slice(&[0, 9]);
                changed += 1;
            }
        }
        assert_eq!(changed, 16);
    }
    rom
}

#[test]
fn dual_guest_computes_cs0_payloads_timing_and_low_ram_synchronization() {
    let mut m = Machine::new();
    for waits in [0, 1, 2] {
        for payload in [0x11223344, 0x89ABCDEF] {
            for omit in [false, true] {
                for _ in 0..2 {
                    m.reset(Boot::ExternalBios(&image(payload, waits, omit)))
                        .unwrap();
                    for _ in 0..5 {
                        m.run_steps(10000).unwrap();
                    }
                    let ticks = match (waits, omit) {
                        (0, false) => 16,
                        (1, false) => 20,
                        (2, false) => 28,
                        (0, true) => 10,
                        (1, true) => 12,
                        (2, true) => 16,
                        _ => unreachable!(),
                    };
                    for (cpu, at) in [(m.cpu_state(), 0x0601C000), (m.slave_state(), 0x0601C100)] {
                        assert_eq!(cpu.exceptions, 0);
                        assert_eq!(cpu.status, CpuStatus::Sleeping);
                        let words: Vec<_> = (0..if at == 0x0601C000 { 5 } else { 6 })
                            .map(|i| m.bus().read32(at + i * 4).unwrap())
                            .collect();
                        let rom_value = if omit { 0 } else { payload };
                        let mut expected =
                            vec![ticks, rom_value, 0x13579BDF, rom_value ^ 0x13579BDF];
                        if at == 0x0601C100 {
                            expected.push(0x51);
                        }
                        expected.push(0x600D);
                        assert_eq!(words, expected, "waits{waits} omit{omit} at{at:X}");
                    }
                    assert_eq!(m.bus().read32(0x00212000), Ok(0xA3));
                }
            }
        }
    }
}
