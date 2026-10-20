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
    assert!(matches!(
        bus.read8(BIOS_SIZE as u32),
        Err(BusError::Unmapped { .. })
    ));
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
        LOW_WRAM + WRAM_SIZE as u32,
        HIGH_WRAM - 4,
        HIGH_WRAM + WRAM_SIZE as u32,
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
