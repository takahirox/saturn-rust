//! Source-authored ST-058-R2 §2.5 status and ST-097 table2.1 IRQ vectors.
use saturn_rust::{
    Boot, Machine,
    bus::{Bus, Memory},
    devices::VDP2,
    scu::SCU,
    timing::{CLOCKS_PER_FIELD, CLOCKS_PER_LINE},
};

#[test]
fn disp_forces_status_without_moving_raster_counters_or_interrupt_edges() {
    for chunk in [1, 7, 128, 10000] {
        let mut enabled = Bus::default();
        let mut toggled = Bus::default();
        for bus in [&mut enabled, &mut toggled] {
            bus.write16(VDP2, 0x8000).unwrap();
            bus.advance_clocks(100).unwrap();
            assert_eq!(bus.read16(VDP2 + 4), Ok(2));
        }
        toggled.write16(VDP2, 0).unwrap();
        assert_eq!(toggled.read16(VDP2 + 4), Ok(10));
        let mut time = 100;
        for end in [
            1280,
            u64::from(CLOCKS_PER_LINE) * 224 - 1,
            u64::from(CLOCKS_PER_LINE) * 224,
            CLOCKS_PER_FIELD - 1,
            CLOCKS_PER_FIELD,
            CLOCKS_PER_FIELD * 2,
        ] {
            while time < end {
                let n = chunk.min((end - time) as u32);
                for bus in [&mut enabled, &mut toggled] {
                    bus.advance_clocks(n).unwrap();
                }
                time += u64::from(n);
            }
            let status = enabled.read16(VDP2 + 4).unwrap();
            assert_eq!(toggled.read16(VDP2 + 4), Ok(status | 8));
            for bus in [&enabled, &toggled] {
                bus.read16(VDP2 + 2).unwrap();
            }
            assert_eq!(toggled.read32(VDP2 + 8), enabled.read32(VDP2 + 8));
            assert_eq!(toggled.read32(SCU + 0xA4), enabled.read32(SCU + 0xA4));
            let expected = if end < u64::from(CLOCKS_PER_LINE) * 224 {
                0
            } else if end < CLOCKS_PER_FIELD {
                1
            } else {
                3
            };
            assert_eq!(toggled.read32(SCU + 0xA4).unwrap() & 3, expected);
            toggled.write16(VDP2, 0x8000).unwrap();
            assert_eq!(toggled.read16(VDP2 + 4), Ok(status));
            toggled.write16(VDP2, 0).unwrap();
        }
    }
}

#[test]
fn original_bios_guest_observes_disp_status_and_independent_scu_raster_irqs() {
    let mut original = vec![0; 524288];
    for line in include_str!("fixtures/vdp2-status.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            original[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let mut machine = Machine::new();
    for operand in [0u16, 0, 0x8000, 0x8000] {
        let mut rom = original.clone();
        rom[0x100..0x102].copy_from_slice(&operand.to_be_bytes());
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..100 {
            machine.run_steps(5000).unwrap();
            if machine.bus().read32(0x0600E02C) == Ok(0x600D) {
                break;
            }
        }
        let results: Vec<_> = (0..12)
            .map(|i| machine.bus().read32(0x0600E000 + i * 4).unwrap())
            .collect();
        let blank = if operand == 0 { 8 } else { 0 };
        assert_eq!(results, [0, 0, blank, 0, 8, 1, 8, 0, 3, blank, 3, 0x600D]);
        assert_eq!(machine.cpu_state().exceptions, 0);
    }
}
