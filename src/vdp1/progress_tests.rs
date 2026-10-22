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
    advance(&mut bus, 68, 8);
    for x in 0..8 {
        bus.write16(VRAM + 0x800 + x * 2, 0x83E0).unwrap();
    }
    advance(&mut bus, 36, 3);
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
            (4, 0xC0),
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
