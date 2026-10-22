//! SH7604 §§8.2–8.5, tables 8.2–8.4 and figures 8.10/8.11.
use saturn_rust::{
    bus::{Bus, Memory, Processor},
    cache::CCR,
};

#[test]
fn separate_caches_write_through_cache_through_and_purge_obey_software_coherence() {
    let mut bus = Bus::default();
    bus.write32(0x06001000, 0x12345678).unwrap();
    for processor in [Processor::Master, Processor::Slave] {
        let mut view = bus.for_cpu(processor);
        view.write8(CCR, 0x11).unwrap();
        assert_eq!(view.read8(CCR).unwrap(), 1); // CP self-clears.
        assert_eq!(view.read32(0x06001000).unwrap(), 0x12345678);
    }
    bus.for_cpu(Processor::Slave)
        .write32(0x06001000, 0x87654321)
        .unwrap();
    assert_eq!(bus.read32(0x06001000).unwrap(), 0x87654321); // Write-through.
    let mut master = bus.for_cpu(Processor::Master);
    assert_eq!(master.read32(0x06001000).unwrap(), 0x12345678); // No snooping.
    assert_eq!(master.read32(0x26001000).unwrap(), 0x87654321); // P1 bypass.
    master.write32(0x26001000, 0x11223344).unwrap();
    assert_eq!(master.read32(0x06001000).unwrap(), 0x12345678); // P1 write does not update data array.
    master.write32(0x46001000, 0).unwrap();
    assert_eq!(master.read32(0x06001000).unwrap(), 0x11223344);
    master.write16(0x06001002, 0xABCD).unwrap();
    assert_eq!(master.read32(0x06001000).unwrap(), 0x1122ABCD);
    assert_eq!(master.read32(0x26001000).unwrap(), 0x1122ABCD);
}

#[test]
fn cache_arrays_use_address_tag_valid_bit_and_lru_data_and_support_all_widths() {
    let mut bus = Bus::default();
    let mut view = bus.for_cpu(Processor::Master);
    view.write8(CCR, 0x10).unwrap();
    for way in 0..4u32 {
        view.write8(CCR, (way as u8) << 6).unwrap();
        let data = 0xC0000000 + way * 1024 + 0x30;
        view.write32(data, 0x10203040 + way).unwrap();
        view.write8(data + 1, 0xA0 + way as u8).unwrap();
        assert_eq!(view.read16(data).unwrap(), 0x10A0 + way as u16);
        assert_eq!(view.read32(data).unwrap(), 0x10A03040 + way * 0x10000 + way);
        // Figure 8.11: tag and valid come from the write ADDRESS, not data.
        view.write32(0x66001034, 0x12345000).unwrap();
        assert_eq!(view.read32(0x60000030).unwrap(), 0x06001004);
        assert!(view.read16(0x60000030).is_err());
        view.write32(0x66001030, 0).unwrap(); // Clear V using address bit 2.
        assert_eq!(view.read32(0x60000030).unwrap(), 0x06001000);
    }
}

#[test]
fn lru_initial_order_replacement_and_two_way_ram_are_independent() {
    let mut bus = Bus::default();
    for i in 0..6 {
        bus.write32(0x06001000 + i * 1024, i + 1).unwrap();
    }
    let mut view = bus.for_cpu(Processor::Master);
    view.write8(CCR, 0x11).unwrap();
    // Manual tables 8.3/8.4: initial victims 3,2,1,0; after touching 3,
    // replacement victim is 2. All addresses deliberately use the same set.
    for i in 0..4 {
        assert_eq!(view.read32(0x06001000 + i * 1024).unwrap(), i + 1);
    }
    for (way, tag) in [
        (3, 0x06001000),
        (2, 0x06001400),
        (1, 0x06001800),
        (0, 0x06001C00),
    ] {
        view.write8(CCR, 1 | (way << 6)).unwrap();
        assert_eq!(view.read32(0x60000000).unwrap() & 0x1FFFFC04, tag | 4);
    }
    view.read32(0x06001000).unwrap();
    view.read32(0x06002000).unwrap();
    view.write8(CCR, 0x81).unwrap();
    assert_eq!(view.read32(0x60000000).unwrap() & 0x1FFFFC04, 0x06002004);
    // Ways 0/1 form RAM in two-way mode and survive CP invalidation.
    view.write32(0xC0000000, 0xDEADBEEF).unwrap();
    view.write8(CCR, 0x19).unwrap();
    for i in 0..6 {
        view.read32(0x06001000 + i * 1024).unwrap();
    }
    assert_eq!(view.read32(0xC0000000).unwrap(), 0xDEADBEEF);
    view.write8(CCR, 9).unwrap();
    assert_eq!(view.read32(0x60000000).unwrap() & 4, 0); // Way 0 never filled.
}

#[test]
fn instruction_and_operand_replacement_controls_and_tas_bypass_are_distinct() {
    let mut bus = Bus::default();
    bus.write32(0x06001000, 0x12345678).unwrap();
    let mut view = bus.for_cpu(Processor::Master);
    view.write8(CCR, 0x13).unwrap(); // ID, CE, CP.
    assert_eq!(view.fetch16(0x06001000).unwrap(), 0x1234);
    view.write32(0x26001000, 0xABCD5678).unwrap();
    assert_eq!(view.fetch16(0x06001000).unwrap(), 0xABCD); // ID didn't allocate.
    assert_eq!(view.read16(0x06001000).unwrap(), 0xABCD); // Data allocates.
    view.write32(0x26001000, 0x98765678).unwrap();
    assert_eq!(view.fetch16(0x06001000).unwrap(), 0xABCD); // ID still permits hit.
    assert_eq!(view.tas_read8(0x06001000).unwrap(), 0x98); // TAS reads external bus.
    view.write8(0x06001000, 0x98).unwrap();
    assert_eq!(view.read16(0x06001000).unwrap(), 0x98CD); // Write updates hit only.
    view.write8(CCR, 0x15).unwrap(); // OD, CE, CP.
    view.read16(0x06001000).unwrap();
    view.write32(0x26001000, 0x11223344).unwrap();
    assert_eq!(view.read16(0x06001000).unwrap(), 0x1122); // OD didn't allocate.
    view.fetch16(0x06001000).unwrap();
    view.write32(0x26001000, 0x55663344).unwrap();
    assert_eq!(view.read16(0x06001000).unwrap(), 0x1122); // Fetch did allocate.
}

#[test]
fn original_guest_excludes_ram_ways_from_tag_hits_and_write_through_updates() {
    use saturn_rust::{Boot, Machine, cpu::CpuStatus};
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/cache.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    for (old, value) in [(0x12345678u32, 0x12345678u32), (0x12345678, 0x76543210)] {
        let mut image = rom.clone();
        let at = image
            .windows(4)
            .position(|w| w == old.to_be_bytes())
            .unwrap();
        assert!(image[at + 4..].windows(4).all(|w| w != old.to_be_bytes()));
        image[at..at + 4].copy_from_slice(&value.to_be_bytes());
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(1000).unwrap();
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(m.cpu_state().exceptions, 0);
            for (i, expected) in [
                value, 0xABCDEF01, 0xBAD0BAD0, 0xC0DEC0DE, 0xABCDEF01, 0x600D,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(m.bus().read32(0x0600A500 + i as u32 * 4), Ok(expected));
            }
        }
    }
}
