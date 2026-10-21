//! Independent original master/slave guest evidence for SH7604 §§5/7/14.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/bsc.hex")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let mut a = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[a..a + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            a += 2;
        }
    }
    rom
}
#[test]
fn both_guest_cpus_use_hardware_identification_refresh_irq_rte_and_module_stops() {
    let mut m = Machine::new();
    let rom = rom();
    let mut previous = None;
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..3000 {
            m.step().unwrap();
        }
        let read = |a| m.bus().read32(a).unwrap();
        for (base, id) in [(0x06008000, 0x3F0), (0x06008040, 0x83F0)] {
            assert_eq!(read(base), id);
            assert_eq!(read(base + 4), id); // pin not writable
            assert_eq!(read(base + 8), 0xD0); // IRQ handler read CMF|CMIE|/16
            assert_eq!(read(base + 12), 1); // actual IRQ handler computation
            assert_eq!(read(base + 20), 0); // stopped FRT reset
            assert!(read(base + 24) > 0); // resumed FRT
            assert_eq!(read(base + 28), 0x600D);
        }
        assert_eq!(m.cpu_state().exceptions, 1);
        assert_eq!(m.slave_state().exceptions, 1);
        assert_eq!(m.cpu_state().r[15], 0x06100000);
        assert_eq!(m.slave_state().r[15], 0x060FE000);
        let trace = (m.cpu_state().clone(), m.slave_state().clone(), m.clocks());
        if let Some(old) = previous {
            assert_eq!(trace, old);
        }
        previous = Some(trace);
    }
}
