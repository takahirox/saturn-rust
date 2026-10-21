use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/smpc.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let mut a = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            let v = u16::from_str_radix(word, 16).unwrap();
            bytes[a..a + 2].copy_from_slice(&v.to_be_bytes());
            a += 2;
        }
    }
    bytes
}
#[test]
fn guest_sets_rtc_smem_reads_status_and_timed_digital_analog_packets_and_handles_smpc_irqs() {
    let mut m = Machine::new();
    let rom = rom();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        m.set_buttons(1 | 16 | 4096);
        m.set_port2_buttons(8 | 128 | 2048);
        m.set_analog(1, Some([0, 128, 255]));
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        let packet =
            |a: u32| -> Vec<u8> { (0..16).map(|i| m.bus().read8(a + i).unwrap()).collect() };
        assert_eq!(
            packet(0x06008000),
            [
                0xC0, 0x20, 0x24, 0x32, 0x29, 0x12, 0x34, 0x56, 0, 1, 0x35, 0, 0x12, 0x34, 0x56,
                0x78
            ]
        );
        assert_eq!(
            packet(0x06008020),
            [
                0xF1, 2, 0xEE, 0xF7, 0xF1, 0x15, 0x77, 0x7F, 0, 128, 255, 255, 255, 255, 255, 255
            ]
        );
        assert_eq!(m.bus().read32(0x06008040).unwrap(), 2);
        assert_eq!(m.bus().read32(0x06008044).unwrap(), 0x600D);
        assert_eq!(m.cpu_state().exceptions, 2);
        assert_eq!(m.cpu_state().r[15], 0x06100000);
    }
}
