//! Original SH-2 MMIO guest exercises both digital multiplexers and settling.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};
fn bios() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/digital-timing.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
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
fn guest_observes_selection_propagation_and_two_independent_button_sources() {
    let rom = bios();
    let expected: Vec<_> = include_str!("fixtures/digital-timing.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.set_buttons(0x842);
        m.set_port2_buttons(0x184);
        m.run_steps(1000).unwrap();
        assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(m.cpu_state().exceptions, 0);
        let actual: Vec<_> = (0..expected.len())
            .map(|i| m.bus().read32(0x0600E000 + i as u32 * 4).unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.set_buttons(0);
    m.set_port2_buttons(0);
    m.run_steps(1000).unwrap();
    for (bank, select) in [0x20, 0x40, 0, 0x60].into_iter().enumerate() {
        for port in 0..2 {
            let late = m
                .bus()
                .read32(0x0600E008 + (bank * 16 + port * 4) as u32)
                .unwrap();
            assert_eq!(late, select | 0x10 | if select == 0x60 { 0xC } else { 15 });
        }
    }
}
