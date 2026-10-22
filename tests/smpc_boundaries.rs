//! ST-169 §3.2/table3.2 and figs3.11/3.13: original SH-2 command and
//! packet guest. Input reaches only peripheral devices, never guest RAM.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom(text: &str) -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[at..at + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    rom
}
#[test]
fn guest_cancels_each_intback_phase_and_parses_compacted_digital_and_analog_packets() {
    let rom = rom(include_str!("fixtures/smpc-boundaries.hex"));
    let expected: Vec<_> = include_str!("fixtures/smpc-boundaries.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let marker = 0x0600EC00 + (expected.len() as u32 - 1) * 4;
    for pal in [false, true] {
        let mut m = Machine::new();
        m.set_pal(pal);
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.set_buttons(0x11);
            m.set_port2_buttons(0x88);
            m.set_3d_pad(1, Some([0x12, 0x34, 0x56, 0x78]));
            for _ in 0..100 {
                m.run_steps(10000).unwrap();
                if m.bus().read32(marker).unwrap() == 0x600D {
                    break;
                }
            }
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0600EC00 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected, "PAL={pal}");
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}

#[test]
fn guest_reads_command_reports_while_busy_and_after_each_status_result() {
    let rom = rom(include_str!("fixtures/smpc-command-report.hex"));
    let expected: Vec<_> = include_str!("fixtures/smpc-command-report.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for pal in [false, true] {
        let mut m = Machine::new();
        m.set_pal(pal);
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.run_steps(10000).unwrap();
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0600ED00 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected, "PAL={pal}");
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}
