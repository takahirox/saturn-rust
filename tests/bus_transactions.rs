//! Original guest mutual exclusion under multi-clock external data phases.
//! P TAS.B, H §§7.3/7.9: no competing cycle may split read/modify/write.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};
fn rom() -> Vec<u8> {
    let mut image = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/bus-transactions.hex")
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
fn both_guest_cpus_use_tas_to_protect_shared_counter_across_real_bus_waits_and_resets() {
    for increments in [32u8, 17] {
        let mut image = rom();
        let at = image.windows(2).position(|b| b == [0xE6, 32]).unwrap();
        image[at + 1] = increments;
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            for (state, exceptions) in [(m.cpu_state(), 3), (m.slave_state(), 0)] {
                assert_eq!(state.status, CpuStatus::Sleeping, "pc={:X}", state.pc);
                assert_eq!(state.exceptions, exceptions);
            }
            for base in [0x0600ED00, 0x0600ED10] {
                assert_eq!(m.bus().read32(base), Ok(u32::from(increments)));
                assert_eq!(m.bus().read32(base + 4), Ok(0x600D));
                assert_eq!(m.bus().read32(base + 8), Ok(6));
                assert_eq!(
                    m.bus().read32(base + 12),
                    Ok(0),
                    "overlapping TAS critical sections"
                );
            }
            assert_eq!(m.bus().read8(0x0600F000), Ok(0));
            assert_eq!(m.bus().read32(0x0600ED30), Ok(0x600D));
            assert_eq!(m.cpu_state().exceptions, 3);
            for i in 0..3 {
                assert_eq!(m.bus().read32(0x0600ED20 + i * 4), Ok(1));
                assert_eq!(m.bus().read32(0x0600ED40 + i * 8), Ok(0x11223344));
                assert_eq!(m.bus().read32(0x0600ED44 + i * 8), Ok(0xAABBCCDD));
            }
            assert_eq!(m.bus().read32(0x0600F004), Ok(u32::from(increments) * 2));
        }
    }
}
