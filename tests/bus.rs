use saturn_rust::bus::*;

#[test]
fn ram_banks_have_independent_big_endian_byte_word_and_long_access() {
    let mut bus = Bus::default();
    for base in [LOW_WRAM, HIGH_WRAM] {
        assert_eq!(bus.read32(base), Ok(0));
        bus.write32(base, 0x1234_ABCD).unwrap();
        assert_eq!(bus.read8(base), Ok(0x12));
        assert_eq!(bus.read8(base + 3), Ok(0xCD));
        assert_eq!(bus.read16(base), Ok(0x1234));
        assert_eq!(bus.read16(base + 2), Ok(0xABCD));
        bus.write8(base + 1, 0xFE).unwrap();
        bus.write16(base + 2, 0xCAFE).unwrap();
        assert_eq!(bus.read32(base), Ok(0x12FE_CAFE));
        bus.write8(base + WRAM_SIZE as u32 - 1, 0xFF).unwrap();
        assert_eq!(bus.read8(base + WRAM_SIZE as u32 - 1), Ok(0xFF));
        bus.write16(base + WRAM_SIZE as u32 - 2, 0x1234).unwrap();
        assert_eq!(bus.read16(base + WRAM_SIZE as u32 - 2), Ok(0x1234));
        bus.write32(base + WRAM_SIZE as u32 - 4, 0x5678_ABCD)
            .unwrap();
        assert_eq!(bus.read32(base + WRAM_SIZE as u32 - 4), Ok(0x5678_ABCD));
    }
    bus.write8(LOW_WRAM, 0xEE).unwrap();
    assert_eq!(bus.read8(HIGH_WRAM), Ok(0x12));
}

#[test]
fn bios_is_exactly_512_kib_big_endian_and_read_only_at_all_widths() {
    let mut bus = Bus::default();
    assert!(matches!(bus.read8(0), Err(BusError::Unmapped { .. })));
    let mut bytes = vec![0; BIOS_SIZE];
    bytes[..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    bytes[BIOS_SIZE - 4..].copy_from_slice(&[0xAB, 0xCD, 0xEF, 0x01]);
    bus.load_bios(&bytes).unwrap();
    assert_eq!(bus.read8(0), Ok(0x12));
    assert_eq!(bus.read16(0), Ok(0x1234));
    assert_eq!(bus.read32(0), Ok(0x1234_5678));
    assert_eq!(bus.read8(BIOS_SIZE as u32 - 1), Ok(1));
    assert_eq!(bus.read16(BIOS_SIZE as u32 - 2), Ok(0xEF01));
    assert_eq!(bus.read32(BIOS_SIZE as u32 - 4), Ok(0xABCD_EF01));
    for result in [bus.write8(0, 0), bus.write16(0, 0), bus.write32(0, 0)] {
        assert!(matches!(result, Err(BusError::ReadOnly { .. })));
    }
    assert_eq!(bus.read32(0), Ok(0x1234_5678));
    assert_eq!(bus.read32(BIOS_SIZE as u32), Ok(0x12345678));
    assert_eq!(bus.read32(0x20080000), Ok(0x12345678));
    assert!(bus.write32(0x80000, 0).is_err());
}

#[test]
fn invalid_and_unaligned_accesses_fail_without_partial_writes() {
    let mut bus = Bus::default();
    for base in [LOW_WRAM, HIGH_WRAM] {
        bus.write32(base, 0x1234_5678).unwrap();
        assert!(matches!(
            bus.read16(base + 1),
            Err(BusError::Unaligned { width: 2, .. })
        ));
        assert!(matches!(
            bus.read32(base + 2),
            Err(BusError::Unaligned { width: 4, .. })
        ));
        assert!(matches!(
            bus.write16(base + 1, 0),
            Err(BusError::Unaligned { .. })
        ));
        assert!(matches!(
            bus.write32(base + 2, 0),
            Err(BusError::Unaligned { .. })
        ));
        assert_eq!(bus.read32(base), Ok(0x1234_5678));
    }
    for address in [
        LOW_WRAM - 4,
        0x0040_0000,
        HIGH_WRAM - 4,
        0x0800_0000,
        0x2000_0000,
        0xFFFF_FFFC,
    ] {
        assert!(matches!(bus.read8(address), Err(BusError::Unmapped { .. })));
        assert!(matches!(
            bus.read16(address),
            Err(BusError::Unmapped { .. })
        ));
        assert!(matches!(
            bus.read32(address),
            Err(BusError::Unmapped { .. })
        ));
        assert!(matches!(
            bus.write8(address, 0),
            Err(BusError::Unmapped { .. })
        ));
        assert!(matches!(
            bus.write16(address, 0),
            Err(BusError::Unmapped { .. })
        ));
        assert!(matches!(
            bus.write32(address, 0),
            Err(BusError::Unmapped { .. })
        ));
    }
    assert!(matches!(
        bus.read32(u32::MAX),
        Err(BusError::Unaligned { .. })
    ));
}

#[test]
fn ram_cs_mirrors_share_physical_bytes_on_both_cache_through_aliases() {
    let mut b = Bus::default();
    for high in [false, true] {
        let base = if high { HIGH_WRAM } else { LOW_WRAM };
        let mirrors = if high { 32 } else { 2 };
        for n in 0..mirrors {
            b.write32(base + n * 0x100000 + 0x80, n + 0x12340000)
                .unwrap();
            for m in 0..mirrors {
                assert_eq!(b.read32(base + m * 0x100000 + 0x80), Ok(n + 0x12340000));
                assert_eq!(
                    b.read32(base + m * 0x100000 + 0x20000080),
                    Ok(n + 0x12340000)
                );
            }
        }
    }
}

#[test]
fn sound_video_and_smpc_chip_select_mirrors_and_lanes() {
    let mut b = Bus::default();
    for (base, mirror) in [
        (0x05A00000, 0x05A80000),
        (0x05C80000, 0x05CC0000),
        (0x05E00000, 0x05E80000),
    ] {
        b.write32(base + 0x10, 0x12345678).unwrap();
        assert_eq!(b.read32(mirror + 0x10), Ok(0x12345678));
        b.write16(mirror + 0x12, 0xCAFE).unwrap();
        assert_eq!(b.read32(base + 0x10), Ok(0x1234CAFE));
    }
    b.write8(0x00100162, 0).unwrap(); // no connected even SMPC lane
    b.write16(0x00100162, 1).unwrap();
    assert_eq!(b.read16(0x00100062), Ok(0xFF01));
    b.write16(0x0010011E, 0).unwrap(); // MSHON through mirrored word bus
    assert_eq!(b.read16(0x0010005E), Ok(0xFF00));
}
