use saturn_rust::{
    Machine,
    bus::{Bus, BusError, Memory},
    devices::*,
};

#[test]
fn digital_pad_direct_port_obeys_direction_selection_and_active_low_buttons() {
    let mut bus = Bus::default();
    assert!(matches!(
        bus.read8(SMPC + 0x75),
        Err(BusError::UnsupportedDevice { .. })
    ));
    bus.write8(SMPC + 0x79, 0x60).unwrap();
    bus.write8(SMPC + 0x7D, 1).unwrap();
    bus.write8(SMPC + 0x7F, 0).unwrap();
    for (select, bits) in [(0x20, 0), (0x40, 4), (0, 8)] {
        bus.write8(SMPC + 0x75, select).unwrap();
        bus.set_buttons(0);
        assert_eq!(bus.read8(SMPC + 0x75).unwrap() & 15, 15);
        for bit in 0..4 {
            bus.set_buttons(1 << (bit + bits));
            assert_eq!(bus.read8(0x20100075).unwrap() & 15, 15 ^ (1 << bit));
        }
    }
    bus.write8(SMPC + 0x75, 0x60).unwrap();
    bus.set_buttons(0);
    assert_eq!(bus.read8(SMPC + 0x75).unwrap(), 0x7C);
    bus.set_buttons(1 << 12);
    assert_eq!(bus.read8(SMPC + 0x75).unwrap(), 0x74);
    bus.write8(SMPC + 0x7D, 0).unwrap();
    assert!(bus.read8(SMPC + 0x75).is_err());
    assert!(bus.read16(SMPC + 0x74).is_err());
    assert!(bus.write8(SMPC + 0x1F, 0x10).is_err()); // no fake INTBACK
}

#[test]
fn both_digital_ports_have_independent_direction_selection_and_enable_bits() {
    let mut bus = Bus::default();
    bus.write8(SMPC + 0x79, 0x60).unwrap();
    bus.write8(SMPC + 0x7B, 0x60).unwrap();
    bus.write8(SMPC + 0x7D, 3).unwrap();
    for (select, shift) in [(0x20, 0), (0x40, 4), (0, 8)] {
        bus.write8(SMPC + 0x75, select).unwrap();
        bus.write8(SMPC + 0x77, select).unwrap();
        for bit in 0..4 {
            bus.set_buttons(1 << (shift + bit));
            bus.set_port2_buttons(1 << (shift + ((bit + 1) % 4)));
            assert_eq!(bus.read8(SMPC + 0x75).unwrap() & 15, 15 ^ (1 << bit));
            assert_eq!(
                bus.read8(0x2010_0077).unwrap() & 15,
                15 ^ (1 << ((bit + 1) % 4))
            );
        }
    }
    bus.write8(SMPC + 0x77, 0x60).unwrap();
    bus.set_port2_buttons(0xFFFF);
    assert_eq!(bus.read8(SMPC + 0x77), Ok(0x74));
    bus.set_port2_buttons(0);
    assert_eq!(bus.read8(SMPC + 0x77), Ok(0x7C));
    for enabled in [1, 2] {
        bus.write8(SMPC + 0x7D, enabled).unwrap();
        assert_eq!(bus.read8(SMPC + 0x75).is_ok(), enabled == 1);
        assert_eq!(bus.read8(SMPC + 0x77).is_ok(), enabled == 2);
    }
    bus.write8(SMPC + 0x7D, 3).unwrap();
    for latch in [1, 2] {
        bus.write8(SMPC + 0x7F, latch).unwrap();
        assert_eq!(bus.read8(SMPC + 0x75).is_ok(), latch == 2);
        assert_eq!(bus.read8(SMPC + 0x77).is_ok(), latch == 1);
    }
    bus.write8(SMPC + 0x7F, 0).unwrap();
    bus.write8(SMPC + 0x7B, 0).unwrap();
    assert!(bus.read8(SMPC + 0x77).is_err());
    assert!(bus.read8(SMPC + 0x75).is_ok());
    assert!(bus.read8(SMPC + 0x7B).is_err()); // Write-only DDR2.
    assert!(bus.read16(SMPC + 0x76).is_err());
}

#[test]
fn vram_aliases_endianness_alignment_register_boundaries_and_read_only_status() {
    let mut bus = Bus::default();
    bus.write32(VRAM, 0x801F83E0).unwrap();
    assert_eq!(bus.read16(VRAM), Ok(0x801F));
    assert_eq!(bus.read16(VRAM + 0x20000002), Ok(0x83E0));
    bus.write16(VRAM + 512 * 1024 - 2, 0xFC00).unwrap();
    assert_eq!(bus.read8(VRAM + 512 * 1024 - 1), Ok(0));
    assert!(bus.write32(VRAM + 1, 0).is_err());
    assert!(bus.read8(VRAM + 512 * 1024).is_err());
    assert_eq!(bus.read16(VDP2 + 4), Ok(0));
    assert!(bus.write16(VDP2 + 4, 8).is_err());
    assert!(bus.write32(VDP2, 0x80000000).is_err());
    assert_eq!(bus.read16(VDP2), Ok(0)); // no partial longword write
    assert!(bus.write16(VDP2 + 0x2A, 1).is_err());
    assert!(bus.read8(VDP2).is_err());
}

#[test]
fn rgb_bitmap_respects_pitch_and_configuration_and_has_no_game_state_dependency() {
    let mut vdp = Vdp2::default();
    assert_eq!(&vdp.framebuffer().unwrap()[..4], &[0, 0, 0, 255]);
    for (offset, value) in [
        (0, 0x8000),
        (0x20, 0x101),
        (0x28, 0x32),
        (0x78, 1),
        (0x7C, 1),
        (0xF8, 1),
    ] {
        vdp.write16(VDP2 + offset, value).unwrap();
    }
    vdp.vram[..6].copy_from_slice(&[0x80, 0x1F, 0x83, 0xE0, 0xFC, 0x00]);
    vdp.vram[1024..1026].copy_from_slice(&0xFFFFu16.to_be_bytes());
    let pixels = vdp.framebuffer().unwrap();
    assert_eq!(
        &pixels[..12],
        &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]
    );
    assert_eq!(&pixels[320 * 4..320 * 4 + 4], &[255, 255, 255, 255]);
    vdp.vblank = true;
    assert_eq!(vdp.read16(VDP2 + 4), Ok(8));
    vdp.write16(VDP2 + 0x28, 0x72).unwrap();
    assert!(vdp.framebuffer().unwrap_err().contains("Unsupported VDP2"));
    assert_eq!(
        Machine::new().framebuffer().unwrap().len(),
        WIDTH * HEIGHT * 4
    );
}
