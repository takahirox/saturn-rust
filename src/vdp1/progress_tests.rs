//! ST-013-R3 §2, §§4.2/4.4–4.8: independent main-clock/pixel vectors.
use crate::{
    bus::{Bus, Memory},
    scu::SCU,
    vdp1::{FRAMEBUFFER, REGISTERS, VRAM},
};

fn reg(bus: &mut Bus, offset: u32, value: u16) {
    bus.write16(REGISTERS + offset, value).unwrap();
}
fn command(bus: &mut Bus, at: u32, words: &[(u32, u16)]) {
    for &(offset, value) in words {
        bus.write16(VRAM + at + offset, value).unwrap();
    }
}
fn advance(bus: &mut Bus, clocks: u32, chunk: u32) {
    let mut remaining = clocks;
    while remaining != 0 {
        let n = remaining.min(chunk);
        bus.advance_clocks(n).unwrap();
        remaining -= n;
    }
}
fn line(bus: &mut Bus, length: u16) {
    command(bus, 0, &[(0, 9), (20, 511), (22, 255)]);
    command(bus, 32, &[(0, 6), (4, 0xC0), (6, 0x801F), (16, length - 1)]);
    command(bus, 64, &[(0, 0x8000)]);
    reg(bus, 2, 2); // manual erase, no implicit frame changes
    reg(bus, 4, 1);
}
#[test]
fn drawing_exposes_one_pixel_per_clock_and_end_irq_follows_the_last_pixel() {
    for chunk in [1, 3, 8, 31, 1024] {
        let mut bus = Bus::default();
        line(&mut bus, 32);
        advance(&mut bus, 64, chunk); // system clip and line command fetch
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0));
        advance(&mut bus, 1, chunk);
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0x801F0000));
        advance(&mut bus, 7, chunk);
        assert_eq!(bus.read32(FRAMEBUFFER + 12), Ok(0x801F801F));
        assert_eq!(bus.read16(FRAMEBUFFER + 16), Ok(0));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
        advance(&mut bus, 24, chunk);
        assert_eq!(bus.read16(FRAMEBUFFER + 62), Ok(0x801F));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
        advance(&mut bus, 31, chunk);
        assert_eq!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
        advance(&mut bus, 1, chunk);
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
        assert_ne!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
    }
}
#[test]
fn forced_stop_retains_partial_pixels_without_end_status_or_irq() {
    let mut bus = Bus::default();
    line(&mut bus, 128);
    advance(&mut bus, 68, 7);
    reg(&mut bus, 12, 0);
    advance(&mut bus, 29, 3);
    assert!(bus.vdp1.busy());
    assert_eq!(bus.read16(FRAMEBUFFER + 64), Ok(0x801F));
    assert_eq!(bus.read16(FRAMEBUFFER + 66), Ok(0));
    advance(&mut bus, 1, 1);
    assert!(!bus.vdp1.busy());
    advance(&mut bus, 1000, 17);
    assert_eq!(bus.read16(FRAMEBUFFER + 66), Ok(0));
    assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
    assert_eq!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
}
#[test]
fn texture_writes_between_pixel_clocks_change_only_unfetched_dots() {
    let mut bus = Bus::default();
    command(&mut bus, 0, &[(0, 9), (20, 511), (22, 255)]);
    command(&mut bus, 32, &[(4, 0xE8), (8, 0x100), (10, 0x101)]);
    command(&mut bus, 64, &[(0, 0x8000)]);
    for x in 0..8 {
        bus.write16(VRAM + 0x800 + x * 2, 0x801F).unwrap();
    }
    reg(&mut bus, 4, 1);
    // Four independently fetched RGB words (2+1 clocks each).
    advance(&mut bus, 76, 8);
    for x in 0..8 {
        bus.write16(VRAM + 0x800 + x * 2, 0x83E0).unwrap();
    }
    advance(&mut bus, 44, 3);
    for x in 0..8 {
        assert_eq!(
            bus.read16(FRAMEBUFFER + x * 2),
            Ok(if x < 4 { 0x801F } else { 0x83E0 })
        );
    }
}
fn erase_setup(mode: u16) -> Bus {
    let mut bus = Bus::default();
    reg(&mut bus, 0, mode);
    bus.vdp1.draw_buffer_mut().fill(255);
    reg(&mut bus, 2, 3);
    bus.vdp1.vblank_out(); // publish the authored pre-existing framebuffer
    reg(&mut bus, 2, 2);
    reg(&mut bus, 6, 0x1234);
    reg(&mut bus, 8, 0);
    reg(
        &mut bus,
        10,
        (if mode & 1 == 0 { 64 } else { 32 }) << 9 | 255,
    );
    bus
}
#[test]
fn vblank_erase_obeys_raster_budget_and_stops_at_field_end_in_both_pixel_depths() {
    // Table 4.5: (1708 - 200) * (263 - 224) = 58,812 word writes.
    for mode in [8, 11] {
        for chunk in [1, 17, 1708, 65535] {
            let mut bus = erase_setup(mode);
            advance(&mut bus, 224 * 1708, chunk);
            assert_eq!(
                bus.vdp1.display_pixel(0, 0),
                if mode == 8 { 65535 } else { 255 }
            );
            advance(&mut bus, 200, chunk);
            assert_eq!(
                bus.vdp1.display_pixel(0, 0),
                if mode == 8 { 65535 } else { 255 }
            );
            advance(&mut bus, 1, chunk);
            assert_eq!(
                bus.vdp1.display_pixel(0, 0),
                if mode == 8 { 0x1234 } else { 0x12 }
            );
            assert_eq!(
                bus.vdp1.display_pixel(1, 0),
                if mode == 8 { 65535 } else { 0x34 }
            );
            advance(&mut bus, 39 * 1708 - 201, chunk);
            let count = 58812 * if mode == 8 { 1 } else { 2 };
            let x = (count - 1) % 512;
            let y = (count - 1) / 512;
            assert_eq!(
                bus.vdp1.display_pixel(x, y),
                if mode == 8 { 0x1234 } else { 0x34 }
            );
            assert_eq!(
                bus.vdp1.display_pixel(count % 512, count / 512),
                if mode == 8 { 65535 } else { 255 }
            );
            advance(&mut bus, 1000, chunk);
            assert_eq!(
                bus.vdp1.display_pixel(count % 512, count / 512),
                if mode == 8 { 65535 } else { 255 }
            );
        }
    }
}
#[test]
fn display_erase_follows_each_readout_and_does_not_clear_future_scanlines() {
    let mut bus = erase_setup(0);
    // Latch a normal manual erase at the next VBlank-OUT.
    bus.vdp1.vblank_out();
    advance(&mut bus, 1279, 31);
    assert_eq!(bus.vdp1.display_pixel(0, 0), 0xFFFF);
    advance(&mut bus, 1, 1);
    assert_eq!(bus.vdp1.display_pixel(0, 0), 0x1234);
    assert_eq!(bus.vdp1.display_pixel(323, 0), 0x1234);
    assert_eq!(bus.vdp1.display_pixel(324, 0), 0xFFFF);
    assert_eq!(bus.vdp1.display_pixel(0, 1), 0xFFFF);
    advance(&mut bus, 1708, 17);
    assert_eq!(bus.vdp1.display_pixel(0, 1), 0x1234);
    assert_eq!(bus.vdp1.display_pixel(0, 2), 0xFFFF);
}

#[test]
fn unfinished_manual_drawing_crosses_a_field_without_fabricating_end_status() {
    let mut bus = Bus::default();
    command(&mut bus, 0, &[(0, 9), (20, 511), (22, 255)]);
    command(
        &mut bus,
        32,
        &[
            (0, 4),
            (4, 0x8C0), // Disable pre-clipping to retain the offscreen workload.
            (6, 0x801F),
            (16, 4095),
            (20, 4095),
            (22, 4095),
            (26, 4095),
        ],
    );
    command(&mut bus, 64, &[(0, 0x8000)]);
    reg(&mut bus, 2, 3); // one manual change, no auto restart
    reg(&mut bus, 4, 1);
    advance(&mut bus, 263 * 1708, 30000);
    assert!(bus.vdp1.busy());
    assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0)); // BEF=CEF=0, transfer over
    assert_eq!(bus.read16(REGISTERS + 0x12), Ok(4)); // previous in-progress command
    assert_eq!(bus.read16(REGISTERS + 0x14), Ok(4));
    assert_eq!(bus.vdp1.display_pixel(0, 0), 0x801F);
    assert_eq!(bus.read16(FRAMEBUFFER), Ok(0)); // next buffer begins intact
    advance(&mut bus, 100, 17);
    assert_eq!(bus.read16(FRAMEBUFFER), Ok(0)); // cursor did not restart at row zero
    assert_eq!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
    reg(&mut bus, 12, 0);
    advance(&mut bus, 30, 3);
    assert!(!bus.vdp1.busy());
    assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
}

#[test]
fn reset_cancels_partial_drawing_and_erase_without_later_writes_or_end_irq() {
    for erase in [false, true] {
        let mut bus = if erase {
            erase_setup(8)
        } else {
            Bus::default()
        };
        bus.write8(0x00180001, 0xA5).unwrap();
        if erase {
            advance(&mut bus, 224 * 1708 + 205, 1708);
        } else {
            line(&mut bus, 128);
            advance(&mut bus, 70, 7);
        }
        bus.system_reset();
        advance(&mut bus, 2000, 3);
        assert!(!bus.vdp1.busy());
        assert_eq!(bus.vdp1.clocks_to_event(), u32::MAX);
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0));
        assert_eq!(bus.vdp1.display_pixel(0, 0), 0);
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
        assert_eq!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
        assert_eq!(bus.read8(0x00180001), Ok(0xA5));
    }
}

#[test]
fn sequential_command_fetch_wraps_vram_without_an_illegal_address_or_false_end() {
    let mut bus = Bus::default();
    command(&mut bus, 0, &[(0, 0x5000), (2, 0xFFFC)]); // skip, assign last command
    command(&mut bus, 0x7FFE0, &[(0, 0x400A)]); // skipped local coordinates, NEXT
    reg(&mut bus, 4, 1);
    advance(&mut bus, 64, 17);
    assert_eq!(bus.read16(REGISTERS + 0x14), Ok(0xFFFC));
    advance(&mut bus, 32, 3);
    assert_eq!(bus.read16(REGISTERS + 0x14), Ok(0));
    assert!(bus.vdp1.busy());
    assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
}

#[test]
fn command_words_are_retained_on_separate_deadlines_and_restart_discards_partial_reads() {
    for chunk in [1, 3, 17, 10000] {
        let mut bus = Bus::default();
        line(&mut bus, 8);
        // The line command starts at clock32; CTRL is read at34 and COLR
        // at40. Changing CTRL after34 must not turn it into END. Changing
        // COLR before40 must affect this draw; later writes must not.
        advance(&mut bus, 34, chunk);
        bus.write16(VRAM + 32, 0x8000).unwrap();
        advance(&mut bus, 5, chunk);
        bus.write16(VRAM + 38, 0x83E0).unwrap();
        advance(&mut bus, 1, chunk);
        bus.write16(VRAM + 38, 0xFC00).unwrap();
        advance(&mut bus, 32 + 32, chunk);
        assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x83E0));
        assert_eq!(bus.read16(FRAMEBUFFER + 14), Ok(0x83E0));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));

        // PTMR discards an incomplete table and begins again at word zero.
        command(&mut bus, 0, &[(0, 9), (20, 511), (22, 255)]);
        reg(&mut bus, 4, 1);
        advance(&mut bus, 2, chunk);
        bus.write16(VRAM, 0x8000).unwrap();
        reg(&mut bus, 4, 1);
        advance(&mut bus, 31, chunk);
        assert_eq!(bus.read16(REGISTERS + 0x10).unwrap() & 2, 0);
        advance(&mut bus, 1, chunk);
        assert_eq!(bus.read16(REGISTERS + 0x10).unwrap() & 2, 2);
        assert_eq!(bus.read16(REGISTERS + 0x14), Ok(0));
    }
}

fn shaded_quad(bus: &mut Bus) {
    command(bus, 0, &[(0, 9), (20, 511), (22, 255)]);
    command(
        bus,
        32,
        &[
            (0, 4),
            (4, 0xC4),
            (6, 0x8008),
            (16, 3),
            (20, 3),
            (22, 1),
            (26, 1),
            (28, 0x100),
        ],
    );
    command(bus, 64, &[(0, 0x8000)]);
    for i in 0..4 {
        bus.write16(VRAM + 0x800 + i * 2, 0x4210).unwrap();
    }
    reg(bus, 2, 2);
    reg(bus, 4, 1);
}

#[test]
fn gouraud_vertices_have_individual_fetch_deadlines_and_retain_completed_words() {
    // ST-013 §5.3 tables5.2/5.3: A/B/C/D order, per-channel correction
    // -16..15. The second row interpolates D to C, not A to B.
    for chunk in [1, 3, 17, 10000] {
        let mut bus = Bus::default();
        shaded_quad(&mut bus);
        advance(&mut bus, 65, chunk);
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0));
        advance(&mut bus, 1, chunk); // A is now retained at neutral.
        bus.write16(VRAM + 0x800, 0x4200).unwrap();
        bus.write16(VRAM + 0x802, 0x421F).unwrap();
        advance(&mut bus, 2, chunk); // B is now retained at +15 red.
        bus.write16(VRAM + 0x802, 0x4200).unwrap();
        bus.write16(VRAM + 0x804, 0x4200).unwrap();
        advance(&mut bus, 4, chunk); // C=-16 red; D neutral.
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0));
        advance(&mut bus, 8, chunk);
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0x8008800D));
        assert_eq!(bus.read32(FRAMEBUFFER + 4), Ok(0x80128017));
        assert_eq!(bus.read32(FRAMEBUFFER + 1024), Ok(0x80088003));
        assert_eq!(bus.read32(FRAMEBUFFER + 1028), Ok(0x80008000));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
        advance(&mut bus, 32, chunk);
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
        assert_ne!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
    }
}

#[test]
fn gouraud_fetch_uses_vram_ownership_and_stop_or_restart_discards_partial_tables() {
    for sampled in 0..4 {
        let mut bus = Bus::default();
        shaded_quad(&mut bus);
        advance(&mut bus, 64 + sampled * 2, 1);
        for _ in 0..8 {
            bus.vdp1.advance_with_bus(1, true, false).unwrap();
        }
        // Framebuffer traffic cannot block a Gouraud VRAM read. Conversely,
        // VRAM traffic above cannot consume any of its remaining words.
        for _ in sampled..4 {
            bus.vdp1.advance_with_bus(2, false, true).unwrap();
        }
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0));
        bus.vdp1.advance_with_bus(1, false, false).unwrap();
        assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x8008));

        shaded_quad(&mut bus);
        advance(&mut bus, 64 + sampled * 2, 3);
        reg(&mut bus, 12, 0);
        bus.vdp1.advance_with_bus(30, true, false).unwrap();
        assert!(!bus.vdp1.busy());
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
        assert_eq!(bus.read32(SCU + 0xA4).unwrap() & (1 << 13), 0);
        // Restart re-reads the whole command and table; the partial shade
        // state from the stopped command cannot leak into the new draw.
        for i in 0..4 {
            bus.write16(VRAM + 0x800 + i * 2, 0x421F).unwrap();
        }
        reg(&mut bus, 4, 1);
        advance(&mut bus, 112, 17);
        assert_eq!(bus.read32(FRAMEBUFFER), Ok(0x80178017));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
    }
}

#[test]
fn framebuffer_calculations_take_six_clocks_and_retain_the_sampled_background() {
    // ST-013 §6.3: shadow and translucent modes are six times slower
    // than replacement. Gouraud saturation precedes averaging (color-calculation note3).
    for (mode, old, result) in [
        (1, 0xFC00, Some(0xBC00)),
        (1, 0x1234, None),
        (3, 0xFC00, Some(0xBC08)),
        (3, 0x1234, Some(0x8010)),
        (7, 0xFC00, Some(0xBC08)),
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut bus = Bus::default();
            shaded_quad(&mut bus);
            command(
                &mut bus,
                32,
                &[
                    (4, 0xC0 | mode),
                    (6, 0x8010),
                    (16, 0),
                    (20, 0),
                    (22, 0),
                    (26, 0),
                ],
            );
            bus.write16(FRAMEBUFFER, old).unwrap();
            let tables = if mode & 4 != 0 { 8 } else { 0 };
            advance(&mut bus, 65 + tables, chunk); // framebuffer read
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(old));
            bus.write16(FRAMEBUFFER, 0xFFFF).unwrap();
            advance(&mut bus, 4, chunk);
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0xFFFF));
            advance(&mut bus, 1, chunk); // captured RMW publishes at six
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(result.unwrap_or(0xFFFF)));
            advance(&mut bus, 31, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
        }
    }
}

#[test]
fn framebuffer_calculation_contention_stop_restart_and_reset_cancel_unpublished_writes() {
    for cancel in 0..4 {
        let mut bus = Bus::default();
        line(&mut bus, 1);
        bus.write16(VRAM + 36, 0xC3).unwrap();
        bus.write16(FRAMEBUFFER, 0xFC00).unwrap();
        advance(&mut bus, 65, 1); // read old framebuffer
        // Framebuffer ownership pauses publication, even after six clocks.
        bus.vdp1.advance_with_bus(8, false, true).unwrap();
        assert_eq!(bus.read16(FRAMEBUFFER), Ok(0xFC00));
        match cancel {
            0 => {
                // Texture/command VRAM is independent of the framebuffer
                // RMW already in flight. The red source=31 averages to15.
                bus.vdp1.advance_with_bus(5, true, false).unwrap();
                assert_eq!(bus.read16(FRAMEBUFFER), Ok(0xBC0F));
            }
            1 => {
                reg(&mut bus, 12, 0);
                bus.vdp1.advance_with_bus(30, false, true).unwrap();
                advance(&mut bus, 100, 17);
                assert_eq!(bus.read16(FRAMEBUFFER), Ok(0xFC00));
                assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            }
            2 => {
                command(&mut bus, 0, &[(0, 0x8000)]);
                reg(&mut bus, 4, 1);
                advance(&mut bus, 100, 17);
                assert_eq!(bus.read16(FRAMEBUFFER), Ok(0xFC00));
                assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
            }
            _ => {
                bus.system_reset();
                bus.write16(FRAMEBUFFER, 0x2468).unwrap();
                advance(&mut bus, 100, 17);
                assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x2468));
                assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            }
        }
    }
}
