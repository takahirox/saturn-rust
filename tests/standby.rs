//! Independent original master/slave guest evidence for SH7604 §§5/7/14.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/standby.hex")
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
fn guest_standby_halts_local_peripherals_until_timed_nmi_wake() {
    let mut m = Machine::new();
    let mut previous = None;
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom())).unwrap();
        while m.cpu_state().status != saturn_rust::cpu::CpuStatus::Sleeping {
            assert!(m.clocks() < 3000);
            m.step().unwrap();
        }
        assert_eq!(m.cpu_state().pc, 0x1002);
        let retired = m.cpu_state().instructions;
        // Until the NMI settling interval ends the CPU and FRT do not advance.
        let sleep_time = m.clocks();
        for _ in 0..60 {
            m.step().unwrap();
            assert_eq!(m.cpu_state().instructions, retired);
            assert_eq!(m.bus().read8(0xfffffe12).unwrap(), 0);
            assert_eq!(m.bus().read8(0xfffffe13).unwrap(), 0);
        }
        m.run_steps(1000).unwrap();
        assert!(m.clocks() >= sleep_time + 512);
        let expected = [0x1002, 0xf0, 0xffffffc8, 0xffffffff, 0x600d];
        for (i, value) in expected.into_iter().enumerate() {
            assert_eq!(
                m.bus().read32(0x0600d000 + i as u32 * 4).unwrap(),
                value,
                "mailbox {i}"
            );
        }
        assert_eq!(m.cpu_state().exceptions, 1);
        assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 11);
        assert_eq!(m.cpu_state().r[15], 0x06100000);
        let trace = (m.cpu_state().clone(), m.clocks());
        if let Some(previous) = previous {
            assert_eq!(trace, previous);
        }
        previous = Some(trace);
    }
}
