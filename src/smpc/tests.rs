//! Independent ST-169 §§2.4/2.5 and tables 3.18/3.19 packet and timing vectors.
use super::*;
fn input(s: &mut Smpc, bytes: &[u8]) {
    for (i, b) in bytes.iter().enumerate() {
        s.write(SMPC + 1 + i as u32 * 2, *b).unwrap();
    }
}
fn issue(s: &mut Smpc, command: u8) {
    s.write(SMPC + 0x63, 1).unwrap();
    s.write(SMPC + 0x1F, command).unwrap();
}
fn output(s: &Smpc, count: usize) -> Vec<u8> {
    (0..count)
        .map(|i| s.read(SMPC + 0x21 + i as u32 * 2).unwrap())
        .collect()
}
#[test]
fn status_time_memory_region_reset_enable_and_irq_are_effects_not_stored_registers() {
    let mut s = Smpc::default();
    input(&mut s, &[0x20, 0x24, 0x32, 0x29, 0x23, 0x59, 0x59]);
    issue(&mut s, 0x16);
    assert_eq!(s.advance_clocks(1881), None);
    assert_eq!(s.advance_clocks(1), Some(0x16));
    assert!(!s.take_irq());
    s.advance_clocks(REFERENCE_HZ as u32);
    input(&mut s, &[0x12, 0x34, 0x56, 0x78]);
    issue(&mut s, 0x17);
    s.advance_clocks(1075);
    issue(&mut s, 0x19);
    s.advance_clocks(807);
    input(&mut s, &[1, 0, 0xF0]);
    issue(&mut s, 0x10);
    assert_eq!(s.advance_clocks(8599), None);
    assert!(!s.take_irq());
    s.advance_clocks(1);
    assert_eq!(
        output(&s, 16),
        [
            0x80, 0x20, 0x24, 0x43, 1, 0, 0, 0, 0, 1, 0x35, 0, 0x12, 0x34, 0x56, 0x78
        ]
    );
    assert_eq!(s.read(SMPC + 0x61).unwrap(), 0x40);
    assert_eq!(s.read(SMPC + 0x63).unwrap(), 0);
    assert!(s.take_irq());
    assert!(!s.take_irq());
}
#[test]
fn rtc_gregorian_boundaries_and_settime_atomicity_do_not_depend_on_clock_chunks() {
    for (date, expected) in [
        (
            [0x20, 0x00, 0x12, 0x28, 0x23, 0x59, 0x59],
            [0x20, 0x00, 0x22, 0x29, 0, 0, 0],
        ),
        (
            [0x21, 0x00, 0x12, 0x28, 0x23, 0x59, 0x59],
            [0x21, 0x00, 0x23, 1, 0, 0, 0],
        ),
        (
            [0x20, 0x99, 0x6C, 0x31, 0x23, 0x59, 0x59],
            [0x21, 0, 1, 1, 0, 0, 0],
        ),
    ] {
        let mut b = Battery::parse(date).unwrap();
        b.advance(REFERENCE_HZ as u32);
        assert_eq!(b.packet(), expected);
    }
    let mut s = Smpc::default();
    let before = s.battery.packet();
    for bad in [
        [0x20, 0x24, 0x02, 0x30, 0, 0, 0],
        [0x20, 0x24, 0x02, 0x29, 0x24, 0, 0],
        [0x20, 0x2A, 0x02, 1, 0, 0, 0],
    ] {
        input(&mut s, &bad);
        s.write(SMPC + 0x63, 1).unwrap();
        assert!(s.write(SMPC + 0x1F, 0x16).is_err());
        assert_eq!(s.battery.packet(), before);
    }
    fn trace(chunk: u32) -> [u8; 7] {
        let mut s = Smpc::default();
        input(&mut s, &[0x20, 0x24, 0x11, 1, 0, 0, 0]);
        issue(&mut s, 0x16);
        let mut left = REFERENCE_HZ as u32 + 1882;
        while left > 0 {
            let n = left.min(chunk);
            s.advance_clocks(n);
            left -= n;
        }
        s.battery.packet()
    }
    for chunk in [3, 807, 10000, REFERENCE_HZ as u32 + 1882] {
        assert_eq!(trace(chunk), [0x20, 0x24, 0x11, 1, 0, 0, 1]);
    }
}
#[test]
fn intback_digital_analog_and_port_modes_use_vblank_continue_and_break_handshakes() {
    let mut s = Smpc::default();
    s.set_buttons((1 << 0) | (1 << 4) | (1 << 12));
    s.set_port2_buttons((1 << 3) | (1 << 7) | (1 << 11));
    s.set_analog(1, Some([0, 128, 255]));
    input(&mut s, &[1, 0x0A, 0xF0]);
    issue(&mut s, 0x10);
    s.advance_clocks(8600);
    assert!(s.take_irq());
    assert_eq!(s.status, 0x60);
    assert!(s.sf);
    assert!(s.busy());
    assert!(s.write(SMPC + 3, 0).is_err());
    s.write(SMPC + 1, 0x81).unwrap();
    s.advance_clocks(10000);
    assert!(!s.take_irq());
    s.vblank_out();
    s.advance_clocks(8600);
    assert_eq!(
        output(&s, 11),
        [0xF1, 2, 0xEE, 0xF7, 0xF1, 0x15, 0x77, 0x7F, 0, 128, 255]
    );
    assert_eq!(s.status, 0xC0);
    assert!(!s.sf);
    assert!(s.take_irq());
    assert!(!s.busy());
    input(&mut s, &[0, 0xCA, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out();
    s.advance_clocks(8600);
    assert_eq!(output(&s, 5), [0xF1, 2, 0xEE, 0xF7, 0xF0]);
    assert_eq!(s.status, 0xCC);
    input(&mut s, &[1, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.advance_clocks(8600);
    s.take_irq();
    s.write(SMPC + 1, 0x41).unwrap();
    assert!(!s.busy());
    assert!(!s.sf);
    s.vblank_out();
    s.advance_clocks(100000);
    assert!(!s.take_irq());
}
#[test]
fn battery_calendar_and_smem_survive_volatile_reset_but_input_and_commands_do_not() {
    let mut s = Smpc::default();
    input(&mut s, &[0x20, 0x24, 0x11, 1, 0, 0, 0]);
    issue(&mut s, 0x16);
    s.advance_clocks(1882);
    input(&mut s, &[1, 2, 3, 4]);
    issue(&mut s, 0x17);
    s.advance_clocks(1075);
    s.set_buttons(0x1FFF);
    input(&mut s, &[1, 0, 0xF0]);
    issue(&mut s, 0x10);
    s.reset_volatile();
    assert_eq!(s.battery.packet(), [0x20, 0x24, 0x11, 1, 0, 0, 0]);
    assert_eq!(s.battery.memory, [1, 2, 3, 4]);
    assert!(!s.busy());
    assert_eq!(s.buttons, [0; 2]);
    assert_eq!(s.output, [0; 32]);
    assert!(s.reset_disabled);
}

#[test]
fn optimized_collection_has_a_measured_first_frame_and_a_late_second_frame() {
    let mut s = Smpc::default();
    input(&mut s, &[0, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out();
    s.advance_clocks(8600);
    assert!(s.take_irq());
    input(&mut s, &[0, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out();
    s.advance_clocks(8600);
    assert!(!s.take_irq());
    s.advance_clocks(224 * 1708 - 26875 - 8600);
    assert!(s.take_irq());
}

#[test]
fn reset_button_nmi_is_edge_triggered_masked_by_resd_and_status_latches_at_vblank() {
    let mut s = Smpc::default();
    assert!(!s.set_reset_button(true));
    assert_eq!(s.read(SMPC + 0x61).unwrap() & 16, 0);
    s.vblank_in();
    assert_eq!(s.read(SMPC + 0x61).unwrap() & 16, 16);
    issue(&mut s, 0x19);
    s.advance_clocks(807);
    assert!(!s.set_reset_button(true));
    s.set_reset_button(false);
    assert!(s.set_reset_button(true));
    assert!(!s.set_reset_button(true));
    issue(&mut s, 0x1A);
    s.advance_clocks(807);
    s.set_reset_button(false);
    assert!(!s.set_reset_button(true));
}
