//! Original guest computes all results through SH-2, SCU MMIO, DSP and DMA.
//! ST-097 §§2.1–2.3/3.2–3.5/4.5; ST-210 #7/13–27/30/31.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/scu.hex")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut parts = line.split_whitespace();
        let mut address = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for word in parts {
            rom[address..address + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            address += 2;
        }
    }
    rom
}
#[test]
fn guest_configures_scu_dma_dsp_and_raster_timers_with_real_interrupt_handlers() {
    let mut m = Machine::new();
    let rom = rom();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.run_steps(10000).unwrap();
        let actual: Vec<_> = (0..11)
            .map(|i| m.bus().read32(0x06004000 + i * 4).unwrap())
            .collect();
        assert_eq!(
            actual,
            [2, 2, 1, 84, 7, 48, 0x600D, 1, 0x40015, 7, 11],
            "pc={:08X}",
            m.cpu_state().pc
        );
        assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(m.cpu_state().r[15], 0x060FF000);
        assert_eq!(m.cpu_state().exceptions, 6);
        assert_eq!(m.bus().read32(0x05E00090).unwrap(), 7);
        assert_eq!(m.bus().read32(0x05E00094).unwrap(), 11);
        assert_eq!(m.bus().read32(0x05E00098).unwrap(), 13);
        assert_eq!(m.bus().read32(0x05E0009C).unwrap(), 17);
        assert_eq!(m.bus().read32(0x06006000).unwrap(), 48);
        // No post-boot host writes; timing, interrupts and DSP perform the work.
    }
}
#[test]
fn invalid_dsp_stops_boundedly_without_discarding_fault_on_retry() {
    let mut rom = rom();
    // Patch only an original DSP literal, not host state after boot.
    let old = 0x02084000u32.to_be_bytes();
    let at = rom.windows(4).position(|w| w == old).unwrap();
    rom[at..at + 4].copy_from_slice(&0x40000000u32.to_be_bytes());
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    let error = m.run_steps(1000).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("SCU DSP illegal instruction 40000000 at 00")
    );
    let instructions = m.cpu_state().instructions;
    assert_eq!(m.step(), Err(error));
    assert_eq!(m.cpu_state().instructions, instructions);
    m.reset(Boot::HlePrototype).unwrap();
    m.step().unwrap();
    assert!(m.fault().is_none());
}
