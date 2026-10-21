//! Independent vectors from H §§5,9,10,12,13. Constants are source-authored.
use super::*;
use crate::bus::{Bus, Memory, Processor};

#[test]
fn division_signed_vectors_and_both_widths_match_truncation_towards_zero() {
    // Expected quotients/remainders are literals, not calls to a second copy of
    // the implementation. Includes negative dividends/divisors and 64-bit input.
    for (high, low, divisor, quotient, remainder) in [
        (0, 100, 7, 14, 2),
        (
            u32::MAX,
            (-100i32) as u32,
            7,
            (-14i32) as u32,
            (-2i32) as u32,
        ),
        (0, 100, (-7i32) as u32, (-14i32) as u32, 2),
        (
            u32::MAX,
            (-100i32) as u32,
            (-7i32) as u32,
            14,
            (-2i32) as u32,
        ),
        (1, 0, 3, 0x5555_5555, 1),
        (0xFFFF_FFFF, 0, 3, 0xAAAA_AAAB, 0xFFFF_FFFF),
        (0, 0x7FFF_FFFF, 1, 0x7FFF_FFFF, 0),
        (0xFFFF_FFFF, 0x8000_0000, 1, 0x8000_0000, 0),
    ] {
        let mut chip = OnChip::default();
        chip.write32(DIVU, divisor).unwrap();
        chip.write32(DIVU + 0x10, high).unwrap();
        chip.write32(DIVU + 0x14, low).unwrap();
        chip.advance_clocks(38);
        assert!(chip.division.borrow().busy());
        chip.advance_clocks(1);
        assert!(!chip.division.borrow().busy());
        assert_eq!(chip.read32(DIVU + 4).unwrap(), quotient);
        assert_eq!(chip.read32(DIVU + 0x14).unwrap(), quotient);
        assert_eq!(chip.read32(DIVU + 0x10).unwrap(), remainder);
        assert_eq!(chip.read32(DIVU + 8).unwrap(), 0);
    }
    let mut chip = OnChip::default();
    chip.write32(DIVU, 7).unwrap();
    chip.write32(DIVU + 4, (-100i32) as u32).unwrap();
    // A dependent register read interlocks for all remaining division clocks.
    chip.advance_clocks(9);
    assert_eq!(chip.read32(DIVU + 4).unwrap(), (-14i32) as u32);
    assert_eq!(chip.take_wait_clocks(), 30);
    assert_eq!(chip.read32(DIVU + 0x10).unwrap(), (-2i32) as u32);
}

#[test]
fn division_overflow_latency_saturation_sticky_flag_and_interrupt_mask() {
    for (high, low, divisor, expected) in [
        (0, 1, 0, 0x7FFF_FFFF),
        (0xFFFF_FFFF, 0xFFFF_FFFF, 0, 0x8000_0000),
        (1, 0, 1, 0x7FFF_FFFF),
        (0xFFFF_FFFF, 0x8000_0000, 0xFFFF_FFFF, 0x7FFF_FFFF),
    ] {
        let mut chip = OnChip::default();
        chip.write16(IPRA, 0xA000).unwrap();
        chip.write32(DIVU + 0xC, 0x45).unwrap();
        chip.write32(DIVU, divisor).unwrap();
        chip.write32(DIVU + 0x10, high).unwrap();
        chip.write32(DIVU + 0x14, low).unwrap();
        chip.advance_clocks(5);
        assert_eq!(chip.interrupt(), None);
        chip.advance_clocks(1);
        assert_eq!(chip.read32(DIVU + 4).unwrap(), expected);
        assert_eq!(chip.interrupt(), None); // OVFIE=0, saturation enabled.
        assert_eq!(chip.read32(DIVU + 8).unwrap(), 1);
        chip.write32(DIVU + 8, 3).unwrap();
        assert_eq!(chip.interrupt(), Some((10, 0x45)));
        chip.write16(IPRA, 0).unwrap();
        assert_eq!(chip.interrupt(), None);
        chip.write32(DIVU + 8, 0).unwrap();
        assert_eq!(chip.read32(DIVU + 8).unwrap(), 0);
    }
}

#[test]
fn watchdog_keys_widths_divisors_and_read_before_clear() {
    for (clock, divisor) in [2, 64, 128, 256, 512, 1024, 4096, 8192]
        .into_iter()
        .enumerate()
    {
        let mut chip = OnChip::default();
        assert_eq!(chip.read8(WDT).unwrap(), 0x18);
        assert_eq!(chip.read8(WDT + 3).unwrap(), 0x1F);
        assert!(chip.write8(WDT, 0x20).is_err());
        assert!(chip.read16(WDT).is_err());
        assert!(chip.write32(WDT, 0xA538).is_err());
        chip.write16(WDT, 0x0038).unwrap(); // Invalid key: no effect.
        assert_eq!(chip.read8(WDT).unwrap(), 0x18);
        chip.write16(IPRA, 0x0090).unwrap();
        chip.write16(VCRWDT, 0x4600).unwrap();
        chip.write16(WDT, 0xA538 | clock as u16).unwrap();
        chip.write16(WDT, 0x5AFF).unwrap();
        chip.advance_clocks(divisor - 1);
        assert_eq!(chip.read8(WDT + 1).unwrap(), 255);
        assert_eq!(chip.interrupt(), None);
        chip.advance_clocks(1);
        assert_eq!(chip.read8(WDT + 1).unwrap(), 0);
        assert_eq!(chip.interrupt(), Some((9, 0x46)));
        chip.write16(WDT, 0xA538 | clock as u16).unwrap(); // No status read.
        assert_eq!(chip.interrupt(), Some((9, 0x46)));
        assert_eq!(chip.read8(WDT).unwrap(), 0xB8 | clock as u8);
        chip.write16(WDT, 0xA538 | clock as u16).unwrap();
        assert_eq!(chip.interrupt(), None);
        chip.write16(WDT, 0xA518).unwrap();
        chip.advance_clocks(10000);
        assert_eq!(chip.read8(WDT + 1).unwrap(), 0);
    }
}

#[test]
fn watchdog_reset_selection_cause_retention_and_no_reset_mode() {
    for (reset_control, expected) in [
        (0x1F, None),
        (0x5F, Some(ResetRequest::PowerOn)),
        (0x7F, Some(ResetRequest::Manual)),
    ] {
        let mut chip = OnChip::default();
        chip.write16(WDT + 2, 0x5A00 | reset_control).unwrap();
        chip.write16(WDT, 0xA578).unwrap();
        chip.write16(WDT, 0x5AFF).unwrap();
        chip.advance_clocks(2);
        assert_eq!(chip.take_reset_request(), None);
        if expected.is_some() {
            assert!(chip.reset_active());
            chip.advance_clocks(511);
            assert_eq!(chip.take_reset_request(), None);
            chip.advance_clocks(1);
            assert!(!chip.reset_active());
        }
        assert_eq!(chip.take_reset_request(), expected);
        assert_eq!(chip.read8(WDT).unwrap(), 0x18);
        assert_eq!(chip.read8(WDT + 3).unwrap(), reset_control as u8 | 0x80);
        chip.reset_from_watchdog();
        chip.write16(WDT + 2, 0xA500).unwrap(); // Requires status read since reset.
        assert_eq!(chip.read8(WDT + 3).unwrap(), reset_control as u8 | 0x80);
        chip.write16(WDT + 2, 0xA500).unwrap();
        assert_eq!(chip.read8(WDT + 3).unwrap(), reset_control as u8);
    }
}

#[test]
fn serial_timing_transmit_receive_overrun_and_read_before_clear() {
    let mut chip = OnChip::default();
    assert_eq!(chip.read8(SCI + 4).unwrap(), 0x84);
    chip.write8(SCI + 1, 0).unwrap(); // 32 clocks/bit, 10-bit asynchronous frame.
    chip.write8(SCI + 2, 0xF4).unwrap();
    chip.write16(IPRB, 0xB000).unwrap();
    chip.write16(VCRA, 0x4041).unwrap();
    chip.write16(VCRB, 0x4243).unwrap();
    assert_eq!(chip.interrupt(), Some((11, 0x42))); // TXI before TEI.
    chip.write8(SCI + 3, 0xA5).unwrap();
    chip.read8(SCI + 4).unwrap();
    chip.write8(SCI + 4, 0).unwrap();
    assert_eq!(chip.interrupt(), None);
    chip.advance_clocks(319);
    assert_eq!(chip.take_serial_output(), None);
    assert_eq!(chip.read8(SCI + 4).unwrap() & 4, 0);
    chip.advance_clocks(1);
    assert_eq!(chip.take_serial_output(), Some(0xA5));
    assert_eq!(chip.take_serial_output(), None);
    assert_eq!(chip.read8(SCI + 4).unwrap(), 0x84);
    chip.receive_serial(0x36, 0, false);
    assert_eq!(chip.interrupt(), Some((11, 0x41))); // RXI before TXI.
    assert_eq!(chip.read8(SCI + 5).unwrap(), 0x36);
    chip.receive_serial(0x77, 0, false);
    assert_eq!(chip.interrupt(), Some((11, 0x40))); // ERI before RXI.
    assert_eq!(chip.read8(SCI + 5).unwrap(), 0x36); // Overrun does not replace RDR.
    chip.read8(SCI + 4).unwrap();
    chip.write8(SCI + 4, 0x84).unwrap();
    chip.receive_serial(0x77, 0x10, false);
    assert_eq!(chip.read8(SCI + 4).unwrap() & 0x78, 0x10); // Framing error, no RDRF.
    assert_eq!(chip.read8(SCI + 5).unwrap(), 0x77);
}

#[test]
fn serial_external_clock_and_disabled_receiver_do_not_invent_input() {
    let mut chip = OnChip::default();
    chip.receive_serial(0x42, 0, false);
    assert_eq!(chip.read8(SCI + 4).unwrap(), 0x84);
    chip.write8(SCI + 2, 0x22).unwrap(); // External clock, no clock edges supplied.
    chip.write8(SCI + 3, 0x55).unwrap();
    chip.read8(SCI + 4).unwrap();
    chip.write8(SCI + 4, 0).unwrap();
    chip.advance_clocks(1_000_000);
    assert_eq!(chip.take_serial_output(), None);
    assert_eq!(chip.read8(SCI + 4).unwrap() & 4, 0);
    chip.write8(SCI + 2, 0).unwrap();
    assert_eq!(chip.read8(SCI + 4).unwrap() & 0x84, 0x84);
}

#[test]
fn dma_widths_address_updates_flags_irq_and_read_before_clear() {
    for (size_code, size) in [(0, 1), (1, 2), (2, 4), (3, 16)] {
        let mut chip = OnChip::default();
        let mut memory = Bus::default();
        for i in 0..32 {
            memory.write8(0x06001000 + i, (i * 7 + 3) as u8).unwrap();
        }
        chip.write16(IPRA, 0x0600).unwrap();
        chip.write32(DMAC + 0x20, 0x44).unwrap();
        chip.write32(DMAC, 0x06001000).unwrap();
        chip.write32(DMAC + 4, 0x06002000).unwrap();
        chip.write32(DMAC + 8, if size == 16 { 8 } else { 2 })
            .unwrap();
        chip.write32(DMAC + 12, 0x5205 | size_code << 10).unwrap();
        chip.write32(DMAC + 0x30, 1).unwrap();
        for _ in 0..2 {
            assert_eq!(chip.dma.transfer(&mut memory, [false; 2]), Some(0));
        }
        for i in 0..size * 2 {
            assert_eq!(memory.read8(0x06002000 + i).unwrap(), (i * 7 + 3) as u8);
        }
        assert_eq!(chip.read32(DMAC).unwrap(), 0x06001000 + size * 2);
        assert_eq!(chip.read32(DMAC + 4).unwrap(), 0x06002000 + size * 2);
        assert_eq!(chip.read32(DMAC + 8).unwrap(), 0);
        assert_eq!(chip.interrupt(), Some((6, 0x44)));
        chip.write32(DMAC + 12, 0x5205 | size_code << 10).unwrap();
        assert_eq!(chip.interrupt(), Some((6, 0x44))); // TE read is required.
        assert_ne!(chip.read32(DMAC + 12).unwrap() & 2, 0);
        chip.write32(DMAC + 12, 0).unwrap();
        assert_eq!(chip.interrupt(), None);
    }
}

#[test]
fn dma_fixed_decrement_round_robin_nmi_and_address_error() {
    let mut chip = OnChip::default();
    let mut memory = Bus::default();
    memory.write32(0x06001000, 0x11223344).unwrap();
    for channel in 0..2 {
        let base = DMAC + channel * 16;
        chip.write32(base, 0x06001000).unwrap();
        chip.write32(base + 4, 0x06002004 + channel * 0x10).unwrap();
        chip.write32(base + 8, 2).unwrap();
        chip.write32(base + 12, 0x8A01).unwrap(); // Fixed source, decrement dest.
    }
    chip.write32(DMAC + 0x30, 9).unwrap();
    chip.dma.transfer(&mut memory, [false; 2]); // Initial round-robin winner=1.
    assert_eq!(memory.read32(0x06002014).unwrap(), 0x11223344);
    assert_eq!(memory.read32(0x06002004).unwrap(), 0);
    chip.dma.transfer(&mut memory, [false; 2]);
    assert_eq!(memory.read32(0x06002004).unwrap(), 0x11223344);
    assert_eq!(chip.read32(DMAC + 4).unwrap(), 0x06002000);
    chip.signal_nmi();
    assert_eq!(chip.dma.transfer(&mut memory, [false; 2]), None);
    chip.write32(DMAC + 0x30, 9).unwrap(); // Without a read NMIF cannot clear.
    assert_eq!(chip.read32(DMAC + 0x30).unwrap(), 11);
    chip.write32(DMAC + 0x30, 9).unwrap();
    chip.dma.transfer(&mut memory, [false; 2]);
    assert_eq!(memory.read32(0x06002010).unwrap(), 0x11223344);
    chip.write32(DMAC, 0x06001001).unwrap();
    chip.dma.transfer(&mut memory, [false; 2]);
    assert!(chip.dma.address_error);
    assert_eq!(chip.read32(DMAC + 0x30).unwrap(), 13);
    assert_eq!(chip.read32(DMAC + 8).unwrap(), 1); // Failed transfer not counted.
}

#[test]
fn peripheral_registers_are_cpu_local_and_arbitration_uses_programmed_priority() {
    let mut bus = Bus::default();
    bus.for_cpu(Processor::Master).write32(DIVU, 7).unwrap();
    bus.for_cpu(Processor::Slave).write32(DIVU, 3).unwrap();
    bus.for_cpu(Processor::Master)
        .write32(DIVU + 4, 100)
        .unwrap();
    bus.for_cpu(Processor::Slave)
        .write32(DIVU + 4, 100)
        .unwrap();
    assert_eq!(bus.read32(DIVU + 4).unwrap(), 14);
    assert_eq!(bus.for_cpu(Processor::Slave).read32(DIVU + 4).unwrap(), 33);
    let mut chip = OnChip::default();
    chip.write16(IPRA, 0xAAA0).unwrap();
    chip.write16(IPRB, 0xAA00).unwrap();
    chip.write16(VCRWDT, 0x4400).unwrap();
    chip.write16(VCRB, 0x4546).unwrap();
    chip.write8(SCI + 2, 0x80).unwrap(); // TXI asserted.
    chip.write16(WDT, 0xA538).unwrap();
    chip.write16(WDT, 0x5AFF).unwrap();
    chip.advance_clocks(2);
    assert_eq!(chip.interrupt(), Some((10, 0x44))); // WDT before SCI at equal IPR.
    chip.write16(IPRB, 0xBA00).unwrap();
    assert_eq!(chip.interrupt(), Some((11, 0x45))); // Higher level overrides rank.
}

#[test]
fn dma_module_requests_wait_for_pins_and_sci() {
    let mut chip = OnChip::default();
    let mut memory = Bus::default();
    memory.write8(0x06001000, 0x5A).unwrap();
    chip.write32(DMAC, 0x06001000).unwrap();
    chip.write32(DMAC + 4, 0x06002000).unwrap();
    chip.write32(DMAC + 8, 1).unwrap();
    chip.write32(DMAC + 12, 0x5001).unwrap(); // External low-level DREQ.
    chip.write32(DMAC + 0x30, 1).unwrap();
    assert_eq!(chip.dma.transfer(&mut memory, [false; 2]), None);
    chip.set_dma_request_pin(0, false);
    assert_eq!(chip.dma.transfer(&mut memory, [false; 2]), Some(0));
    assert_eq!(memory.read8(0x06002000).unwrap(), 0x5A);
    chip.read32(DMAC + 12).unwrap();
    chip.write32(DMAC + 12, 0).unwrap();
    chip.write32(DMAC, SCI + 5).unwrap();
    chip.write32(DMAC + 4, 0x06002001).unwrap();
    chip.write32(DMAC + 8, 1).unwrap();
    chip.write8(0xFFFFFE71, 1).unwrap(); // RXI request.
    chip.write32(DMAC + 12, 0x4001).unwrap();
    chip.write8(SCI + 2, 0x10).unwrap();
    assert_eq!(
        chip.dma.transfer(&mut memory, chip.serial_dma_requests()),
        None
    );
    chip.receive_serial(0x33, 0, false);
    assert_eq!(chip.serial_dma_requests(), [true, false]);
    assert!(chip.write32(DMAC + 12, 0x4009).is_err()); // Unimplemented DACK mode is explicit.
}

#[test]
fn sci_dma_requests_move_real_data_acknowledge_flags_and_bypass_warm_cpu_cache() {
    let mut bus = Bus::default();
    {
        let mut view = bus.for_cpu(Processor::Master);
        view.write8(crate::cache::CCR, 0x11).unwrap();
        assert_eq!(view.read8(0x06002000).unwrap(), 0); // Cache destination.
        view.write8(SCI + 2, 0x10).unwrap();
        view.write32(DMAC, SCI + 5).unwrap();
        view.write32(DMAC + 4, 0x06002000).unwrap();
        view.write32(DMAC + 8, 1).unwrap();
        view.write8(0xFFFFFE71, 1).unwrap();
        view.write32(DMAC + 12, 0x4001).unwrap();
        view.write32(DMAC + 0x30, 1).unwrap();
    }
    assert!(!bus.advance_dma(Processor::Master));
    assert_eq!(bus.read32(DMAC + 8).unwrap(), 1);
    bus.supply_serial_character(Processor::Master, 0x35, 0, false);
    assert!(!bus.advance_dma(Processor::Master));
    assert_eq!(bus.read8(0x06002000).unwrap(), 0x35);
    assert_eq!(bus.read8(SCI + 4).unwrap() & 0x40, 0); // RDRF acknowledged.
    assert_eq!(bus.for_cpu(Processor::Master).read8(0x06002000).unwrap(), 0); // DMA does not snoop.
    {
        let mut view = bus.for_cpu(Processor::Master);
        view.read32(DMAC + 12).unwrap();
        view.write32(DMAC + 12, 0).unwrap();
        view.write32(DMAC, 0x06002000).unwrap();
        view.write32(DMAC + 4, SCI + 3).unwrap();
        view.write32(DMAC + 8, 1).unwrap();
        view.write8(0xFFFFFE71, 2).unwrap();
        view.write8(SCI + 1, 0).unwrap();
        view.write8(SCI + 2, 0x20).unwrap();
        view.write32(DMAC + 12, 1).unwrap();
    }
    assert!(!bus.advance_dma(Processor::Master));
    assert_eq!(bus.read8(SCI + 4).unwrap() & 0x84, 0); // TDRE/TEND cleared by DMAC.
    bus.advance_clocks(319).unwrap();
    assert_eq!(bus.take_serial_output(Processor::Master), None);
    bus.advance_clocks(1).unwrap();
    assert_eq!(bus.take_serial_output(Processor::Master), Some(0x35));
    assert_eq!(bus.read8(SCI + 4).unwrap() & 0x84, 0x84);
}
