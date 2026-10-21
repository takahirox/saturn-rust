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
fn digital_selection_settles_after_two_microseconds_independently_on_both_ports() {
    // ST-169 §3.4, table3.36; no assumed success from immediate readback.
    for chunk in [1, 3, 8, 53] {
        let mut s = Smpc::default();
        s.write(SMPC + 0x7D, 3).unwrap();
        s.write(SMPC + 0x79, 0x60).unwrap();
        s.write(SMPC + 0x7B, 0x60).unwrap();
        s.write(SMPC + 0x75, 0x60).unwrap();
        s.write(SMPC + 0x77, 0x60).unwrap();
        s.set_buttons(0x842);
        s.set_port2_buttons(0x184);
        // Nibbles: port1 2/4/8 pressed in the three banks, port2 4/8/1.
        let mut old = [0xC, 0xC];
        for (select, pins) in [
            (0x20, [0xD, 0xB]),
            (0x40, [0xB, 7]),
            (0, [7, 0xE]),
            (0x60, [0xC, 0xC]),
        ] {
            s.write(SMPC + 0x75, select).unwrap();
            let mut left = 53;
            while left != 0 {
                let n = left.min(chunk);
                s.advance_clocks(n);
                left -= n;
            }
            assert_eq!(s.read(SMPC + 0x75).unwrap() & 15, old[0]);
            assert_eq!(s.read(SMPC + 0x77).unwrap() & 15, old[1]);
            // Port 2 starts one clock before port 1's deadline.
            s.write(SMPC + 0x77, select).unwrap();
            s.advance_clocks(1);
            assert_eq!(s.read(SMPC + 0x75).unwrap() & 15, pins[0]);
            assert_eq!(s.read(SMPC + 0x77).unwrap() & 15, old[1]);
            // Rewriting the same electrical selection cannot restart settling.
            s.write(SMPC + 0x77, select).unwrap();
            s.advance_clocks(52);
            assert_eq!(s.read(SMPC + 0x77).unwrap() & 15, old[1]);
            s.advance_clocks(1);
            assert_eq!(s.read(SMPC + 0x77).unwrap() & 15, pins[1]);
            old = pins;
        }
        s.write(SMPC + 0x75, 0x20).unwrap();
        s.advance_clocks(53);
        s.reset_volatile();
        s.advance_clocks(1);
        s.write(SMPC + 0x7D, 1).unwrap();
        assert_eq!(s.read(SMPC + 0x75), Ok(0x7C));
    }
}
#[test]
fn clock_commands_latch_dotsel_at_the_deadline_and_reset_sound_without_an_irq() {
    // ST-169 §§2.3.9/10, §2.4.2 OREG10 bit6/bit0.
    for chunk in [1, 8, 607, Smpc::SYSTEM_RESET_CLOCKS] {
        let mut s = Smpc::default();
        for (command, expected) in [(0x0E, 0x75), (0x0F, 0x35), (0x0E, 0x75)] {
            issue(&mut s, 6);
            s.advance_clocks(Smpc::CPU_COMMAND_CLOCKS);
            assert!(s.sound_enabled);
            let before = s.clock_352;
            issue(&mut s, command);
            let mut left = Smpc::SYSTEM_RESET_CLOCKS - 1;
            while left != 0 {
                let n = left.min(chunk);
                assert_eq!(s.advance_clocks(n), None);
                left -= n;
            }
            assert_eq!(s.clock_352, before);
            assert_eq!(s.read(SMPC + 0x63), Ok(1));
            assert_eq!(s.advance_clocks(1), Some(command));
            assert_eq!(s.read(SMPC + 0x63), Ok(0));
            assert!(!s.take_irq());
            assert!(!s.sound_enabled);
            input(&mut s, &[1, 0, 0xF0]);
            issue(&mut s, 0x10);
            s.advance_clocks(Smpc::STATUS_CLOCKS);
            assert_eq!(s.output[10], expected);
            assert!(s.take_irq());
        }
        s.system_reset();
        assert!(!s.clock_352);
    }
}
#[test]
fn sysres_has_a_hardware_deadline_and_retains_battery_calendar_memory_and_input() {
    // ST-169 §2.3.8: no IRQ, power-on controls, 256 Kbit backup is external.
    for chunk in [1, 8, 1024, Smpc::SYSTEM_RESET_CLOCKS] {
        let mut s = Smpc {
            battery: Battery::parse([0x20, 0x24, 0x11, 1, 0x12, 0, 0]).unwrap(),
            ..Smpc::default()
        };
        s.battery.memory = [1, 2, 3, 4];
        s.set_buttons(0x1001);
        issue(&mut s, 0x0D);
        assert_eq!(s.clocks_to_event(), Smpc::SYSTEM_RESET_CLOCKS);
        let mut left = Smpc::SYSTEM_RESET_CLOCKS - 1;
        while left != 0 {
            let n = left.min(chunk);
            assert_eq!(s.advance_clocks(n), None);
            left -= n;
        }
        assert_eq!(s.read(SMPC + 0x63).unwrap(), 1);
        assert_eq!(s.advance_clocks(1), Some(0x0D));
        assert!(!s.take_irq());
        s.system_reset();
        assert_eq!(s.output[31], 13);
        assert_eq!(s.battery.memory, [1, 2, 3, 4]);
        assert_eq!(s.battery.phase, u64::from(Smpc::SYSTEM_RESET_CLOCKS));
        assert_eq!(s.buttons[0], 0x1001);
        assert!(s.reset_disabled);
        assert!(!s.sound_enabled);
        assert!(!s.busy());
    }
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
    assert_eq!(output(&s, 5), [0xF1, 2, 0xEE, 0xF7, 0xFF]);
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
fn fast_clock_optimized_intback_uses_its_own_raster_deadline() {
    let mut s = Smpc::default();
    issue(&mut s, 0x0E);
    s.advance_clocks(Smpc::SYSTEM_RESET_CLOCKS);
    for cycle in 0..2 {
        input(&mut s, &[0, 8, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out();
        let clocks = if cycle == 0 { 8600 } else { 355717 };
        assert_eq!(s.clocks_to_event(), clocks);
        assert_eq!(s.advance_clocks(clocks - 1), None);
        assert!(!s.take_irq());
        assert_eq!(s.advance_clocks(1), Some(0x10));
        assert!(s.take_irq());
    }
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

#[test]
fn three_d_pad_packet_has_four_channels_and_documented_trigger_hysteresis() {
    // STN-43, Analog mode and L/R button diagram: engage at 146, release at 85.
    let mut s = Smpc::default();
    s.set_buttons(0x15);
    s.set_port2_buttons(0x80);
    for (right, left, expected) in [
        (145, 0, 0xFF),
        (146, 145, 0x7F),
        (100, 146, 0x77),
        (85, 100, 0xF7),
        (0, 85, 0xFF),
    ] {
        s.set_3d_pad(0, Some([17, 239, right, left]));
        input(&mut s, &[0, 0xA, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out();
        s.advance_clocks(8600);
        assert_eq!(
            &output(&s, 12)[..8],
            &[0xF1, 0x16, 0xAE, expected, 17, 239, right, left]
        );
        assert_eq!(&output(&s, 12)[8..], &[0xF1, 2, 0xF7, 0xFF]);
        s.take_irq();
    }
    s.set_3d_pad(0, None);
    input(&mut s, &[0, 0xA, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out();
    s.advance_clocks(8600);
    assert_eq!(output(&s, 4), [0xF1, 2, 0xAE, 0xFF]);
}
#[test]
fn direct_analog_handshake_has_timed_acknowledgement_snapshot_abort_and_independent_ports() {
    // ST-169 §3.4, table3.37. Start TH↓, then toggle TR and wait for TL.
    for pad in [false, true] {
        let mut s = Smpc::default();
        if pad {
            s.set_3d_pad(0, Some([0x12, 0x34, 0xAB, 0xCD]));
        } else {
            s.set_analog(0, Some([0x12, 0x34, 0xAB]));
        }
        s.set_analog(1, Some([0xFE, 0xDC, 0xBA]));
        s.set_buttons(1 | 16 | 4096);
        s.set_port2_buttons(8 | 128 | 2048);
        for (off, value) in [
            (0x79, 0x60),
            (0x7B, 0x60),
            (0x7D, 3),
            (0x75, 0x60),
            (0x77, 0x60),
        ] {
            s.write(SMPC + off, value).unwrap();
        }
        assert_eq!(s.read(SMPC + 0x75).unwrap(), 0x71);
        s.write(SMPC + 0x75, 0x20).unwrap();
        s.advance_clocks(54);
        // Buttons/axes change mid-transaction; the transferred packet must be latched.
        s.set_buttons(0);
        if pad {
            s.set_3d_pad(0, Some([128, 128, 0, 0]));
        } else {
            s.set_analog(0, Some([128, 128, 0]));
        }
        let expected: &[u8] = if pad {
            &[1, 6, 0xE, 0xE, 7, 7, 1, 2, 3, 4, 0xA, 0xB, 0xC, 0xD, 0, 1]
        } else {
            &[1, 5, 0xE, 0xE, 0xF, 7, 1, 2, 3, 4, 0xA, 0xB, 0, 1]
        };
        for (i, nibble) in expected.iter().enumerate() {
            let pins = if i & 1 == 0 { 0 } else { 0x20 };
            let before = s.read(SMPC + 0x75).unwrap() & 31;
            s.write(SMPC + 0x75, pins).unwrap();
            s.advance_clocks(53);
            assert_eq!(s.read(SMPC + 0x75).unwrap() & 31, before);
            s.advance_clocks(1);
            assert_eq!(
                s.read(SMPC + 0x75).unwrap(),
                pins | if i & 1 == 0 { 0 } else { 16 } | nibble
            );
            assert_eq!(s.read(SMPC + 0x77).unwrap(), 0x71); // other port untouched
        }
        s.write(SMPC + 0x75, 0x60).unwrap();
        assert_eq!(s.read(SMPC + 0x75).unwrap(), 0x71);
        // Abort before the acknowledgement deadline discards the queued response.
        s.write(SMPC + 0x75, 0x20).unwrap();
        s.advance_clocks(54);
        s.write(SMPC + 0x75, 0).unwrap();
        s.advance_clocks(53);
        s.write(SMPC + 0x75, 0x60).unwrap();
        s.advance_clocks(1);
        assert_eq!(s.read(SMPC + 0x75).unwrap(), 0x71);
    }
}

#[test]
fn all_documented_regions_survive_resets_and_reserved_codes_are_rejected() {
    // ST-169 §2.4.2, OREG9 table (physical area straps, not disc metadata).
    for code in 0..=255 {
        if ![1, 2, 4, 5, 6, 10, 12, 13].contains(&code) {
            assert!(Region::try_from(code).is_err());
            continue;
        }
        let mut s = Smpc::default();
        s.set_region(Region::try_from(code).unwrap());
        for reset in [0, 1, 2] {
            match reset {
                1 => s.system_reset(),
                2 => s.reset_volatile(),
                _ => (),
            }
            input(&mut s, &[1, 0, 0xF0]);
            issue(&mut s, 0x10);
            assert_eq!(s.advance_clocks(Smpc::STATUS_CLOCKS - 1), None);
            assert_eq!(s.advance_clocks(1), Some(0x10));
            assert_eq!(s.read(SMPC + 0x33), Ok(code));
            assert!(s.take_irq());
        }
    }
}

#[test]
fn intback_reports_cd_reset_independently_of_sound_reset_at_command_deadlines() {
    // ST-169 §2.3.6/7 (40µs), §2.4.2 OREG10/11 control-signal status.
    for chunk in [1, 8, 1074] {
        let mut s = Smpc::default();
        for (command, sound, cd, deadline) in [
            (9, false, false, 1075),
            (6, true, false, 807),
            (8, true, true, 1075),
            (7, false, true, 807),
        ] {
            let old_cd = s.cd_enabled;
            let old_sound = s.sound_enabled;
            issue(&mut s, command);
            let mut remaining = deadline - 1;
            while remaining != 0 {
                let n = remaining.min(chunk);
                assert_eq!(s.advance_clocks(n), None);
                remaining -= n;
            }
            assert_eq!(s.cd_enabled, old_cd);
            assert_eq!(s.sound_enabled, old_sound);
            assert_eq!(s.advance_clocks(1), Some(command));
            assert_eq!(s.cd_enabled, cd);
            assert_eq!(s.sound_enabled, sound);
            assert!(!s.take_irq());
            input(&mut s, &[1, 0, 0xF0]);
            issue(&mut s, 0x10);
            s.advance_clocks(Smpc::STATUS_CLOCKS);
            assert_eq!(s.read(SMPC + 0x35), Ok(if sound { 0x34 } else { 0x35 }));
            assert_eq!(s.read(SMPC + 0x37), Ok(if cd { 0 } else { 0x40 }));
            assert!(s.take_irq());
        }
    }
}

#[test]
fn optimized_intback_uses_selected_display_interval_and_reset_cancels_it() {
    // ST-169 §3.2: measured first packet, subsequent packet one millisecond
    // before VBlank-in. The bus supplies the selected TV/PLL interval.
    for active in [224 * 1708, 240 * 1708, 256 * 1820, 240 * 1820] {
        let mut s = Smpc::default();
        input(&mut s, &[0, 8, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out_after(active);
        assert_eq!(s.advance_clocks(8600), Some(0x10));
        assert!(s.take_irq());
        input(&mut s, &[0, 8, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out_after(active);
        assert_eq!(s.clocks_to_event(), active - 26875);
        assert_eq!(s.advance_clocks(active - 26876), None);
        assert!(!s.take_irq());
        assert_eq!(s.advance_clocks(1), Some(0x10));
        assert!(s.take_irq());
        input(&mut s, &[0, 8, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out_after(active);
        s.reset_volatile();
        assert_eq!(s.advance_clocks(active), None);
        assert!(!s.take_irq());
        assert!(!s.busy());
    }
}

#[test]
fn zero_byte_modes_pack_only_selected_ports_for_both_digital_and_analog_reports() {
    // ST-169 fig3.11/table3.4 and fig3.13; STN-43 analog ID16.
    for (mode, expected) in [
        (
            0,
            vec![
                0xF1, 2, 0xEE, 0xFF, 0xF1, 0x16, 0x77, 0xFF, 0x12, 0x34, 0x56, 0x78,
            ],
        ),
        (0x30, vec![0xF1, 0x16, 0x77, 0xFF, 0x12, 0x34, 0x56, 0x78]),
        (0xC0, vec![0xF1, 2, 0xEE, 0xFF]),
        (0xF0, vec![]),
    ] {
        let mut s = Smpc::default();
        s.set_buttons(0x11);
        s.set_port2_buttons(0x88);
        s.set_3d_pad(1, Some([0x12, 0x34, 0x56, 0x78]));
        input(&mut s, &[0, mode | 0x0A, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out();
        s.advance_clocks(8599);
        assert!(!s.take_irq());
        s.advance_clocks(1);
        let actual = output(&s, 16);
        assert_eq!(&actual[..expected.len()], &expected);
        assert!(actual[expected.len()..].iter().all(|v| *v == 0xFF));
        assert_eq!(s.status, 0xC0 | (mode >> 4));
        assert!(!s.sf);
    }
}

#[test]
fn intback_break_cancels_every_collection_phase_without_later_irq_or_stale_output() {
    // ST-169 §3.2/table3.2: break can end a wait or collection.
    for phase in 0..3 {
        let mut s = Smpc::default();
        input(&mut s, &[u8::from(phase == 0), 0x0A, 0xF0]);
        issue(&mut s, 0x10);
        if phase == 0 {
            s.advance_clocks(8600);
            assert!(s.take_irq());
        }
        if phase == 2 {
            s.vblank_out();
            s.advance_clocks(8599);
        }
        let before = s.output;
        s.write(SMPC + 1, 0x40 | u8::from(phase == 0)).unwrap();
        assert!(!s.sf);
        assert!(!s.busy());
        s.vblank_in();
        s.vblank_out();
        s.advance_clocks(100_000);
        assert!(!s.take_irq());
        assert_eq!(s.output, before);
        input(&mut s, &[0, 0x0A, 0xF0]);
        issue(&mut s, 0x10);
        s.vblank_out();
        s.advance_clocks(8600);
        assert!(s.take_irq());
        assert_eq!(output(&s, 4), [0xF1, 2, 0xFF, 0xFF]);
    }
}

#[test]
fn vblank_timeout_discards_incomplete_collection_and_remeasures_next_request() {
    // ST-169 §§3.2/3.3: timeout terminates acquisition and disables reuse
    // of its previous optimization; no collection output arrives next frame.
    let mut s = Smpc::default();
    input(&mut s, &[0, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out();
    s.advance_clocks(8600);
    assert!(s.take_irq());
    assert!(s.measured_collection);
    input(&mut s, &[0, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out_after(100_000);
    s.advance_clocks(1000);
    let before = s.output;
    s.vblank_in();
    assert!(!s.sf);
    assert!(!s.busy());
    assert!(!s.measured_collection);
    s.advance_clocks(100_000);
    assert!(!s.take_irq());
    assert_eq!(s.output, before);
    input(&mut s, &[0, 8, 0xF0]);
    issue(&mut s, 0x10);
    s.vblank_out_after(100_000);
    s.advance_clocks(8599);
    assert!(!s.take_irq());
    s.advance_clocks(1);
    assert!(s.take_irq());
}

#[test]
fn direct_analog_route_changes_cancel_pending_edges_only_on_the_affected_port() {
    for (offset, value) in [(0x79, 0), (0x7D, 2), (0x7F, 1)] {
        let mut s = Smpc::default();
        for port in 0..2 {
            s.set_analog(port, Some([1, 2, 3]));
        }
        for (at, value) in [
            (0x79, 0x60),
            (0x7B, 0x60),
            (0x7D, 3),
            (0x75, 0x60),
            (0x77, 0x60),
        ] {
            s.write(SMPC + at, value).unwrap();
        }
        for at in [0x75, 0x77] {
            s.write(SMPC + at, 0x20).unwrap();
            s.advance_clocks(54);
            s.write(SMPC + at, 0).unwrap();
        }
        s.write(SMPC + offset, value).unwrap();
        s.advance_clocks(54);
        assert_eq!(s.handshake[0].pins(), 0x11);
        assert_eq!(s.handshake[1].pins(), 1);
        assert_eq!(s.handshake[0].clocks_to_event(), u32::MAX);
        assert_eq!(s.handshake[1].clocks_to_event(), u32::MAX);
    }
}
