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

#[test]
fn three_d_pad_reports_and_both_direct_analog_handshakes_run_as_guest_instructions() {
    // The existing INTBACK guest also parses the longer ID16 report correctly.
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom())).unwrap();
    m.set_buttons(1 | 16 | 4096);
    m.set_port2_buttons(8 | 128 | 2048);
    m.set_3d_pad(1, Some([17, 239, 146, 85]));
    for _ in 0..10 {
        m.run_steps(10000).unwrap();
    }
    let packet: Vec<_> = (0..12)
        .map(|i| m.bus().read8(0x06008020 + i).unwrap())
        .collect();
    assert_eq!(
        packet,
        [
            0xF1, 2, 0xEE, 0xF7, 0xF1, 0x16, 0x77, 0x7F, 17, 239, 146, 85
        ]
    );
    let mut bios = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/controller.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let at = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for (i, w) in parts.enumerate() {
            bios[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&bios)).unwrap();
        m.set_buttons(1 | 16 | 4096);
        m.set_port2_buttons(8 | 128 | 2048);
        m.set_3d_pad(0, Some([0x12, 0x34, 0xAB, 0xCD]));
        m.set_analog(1, Some([0xFE, 0xDC, 0xBA]));
        m.run_steps(10000).unwrap();
        assert_eq!(m.bus().read32(0x0600B040).unwrap(), 0x600D);
        let first: Vec<_> = (0..16)
            .map(|i| m.bus().read8(0x0600B000 + i).unwrap())
            .collect();
        let second: Vec<_> = (0..14)
            .map(|i| m.bus().read8(0x0600B020 + i).unwrap())
            .collect();
        assert_eq!(
            first,
            [
                1, 0x36, 0xE, 0x3E, 7, 0x37, 1, 0x32, 3, 0x34, 0xA, 0x3B, 0xC, 0x3D, 0, 0x31
            ]
        );
        assert_eq!(
            second,
            [
                1, 0x35, 7, 0x37, 7, 0x3F, 0xF, 0x3E, 0xD, 0x3C, 0xB, 0x3A, 0, 0x31
            ]
        );
        assert_eq!(m.cpu_state().r[15], 0x06100000);
    }
}
