//! Relaxed playback keeps the exact scheduler's write buffering and VDP1
//! port ownership in aggregate, although devices catch up after the CPUs.
use super::*;
use crate::vdp1::{REGISTERS, VRAM};

const TEXTURE: u32 = VRAM | 0x2000_0000;

fn relaxed() -> Bus {
    let mut bus = Bus::default();
    bus.set_fast_video(true);
    bus
}

/// The store's own bus time, as the serial model charged it to the CPU.
fn serial_store_clocks() -> u32 {
    let mut bus = Bus::default();
    bus.for_cpu(Processor::Master)
        .write32(TEXTURE, 0x1234_5678)
        .unwrap();
    bus.cpu_main_delay(Processor::Master, 1)
}

#[test]
fn relaxed_cpu_stores_are_posted_and_the_next_external_access_waits() {
    let store = serial_store_clocks();
    // Reset BCR1 gives CS2 six Tw states plus T1/T2.
    assert_eq!(store, 8);
    let mut bus = relaxed();
    bus.set_fast_cpu_time(Processor::Master, 100);
    bus.for_cpu(Processor::Master)
        .write32(TEXTURE, 0x1234_5678)
        .unwrap();
    // One issue state plus the internal-bus acceptance clock.
    assert_eq!(bus.cpu_main_delay(Processor::Master, 1), 2);
    // A second store two clocks later first waits for the BSC beats.
    bus.set_fast_cpu_time(Processor::Master, 102);
    bus.for_cpu(Processor::Master)
        .write32(TEXTURE + 4, 0)
        .unwrap();
    assert_eq!(bus.cpu_main_delay(Processor::Master, 1), 2 + (store - 1));
    // A read waits for the pending write; once drained it pays only itself.
    bus.set_fast_cpu_time(Processor::Master, 104);
    bus.for_cpu(Processor::Master).read32(TEXTURE).unwrap();
    let waited = bus.cpu_main_delay(Processor::Master, 1);
    bus.set_fast_cpu_time(Processor::Master, 10_000);
    bus.for_cpu(Processor::Master).read32(TEXTURE).unwrap();
    let drained = bus.cpu_main_delay(Processor::Master, 1);
    // Each store is accepted after its predecessor drains, then takes its beats.
    assert_eq!(waited - drained, 100 + 2 * (1 + store) - 104);
}

#[test]
fn exact_scheduler_functional_stores_keep_the_serial_charge() {
    let mut bus = Bus::default();
    bus.set_fast_cpu_time(Processor::Master, 100);
    bus.for_cpu(Processor::Master)
        .write32(TEXTURE, 0x1234_5678)
        .unwrap();
    assert_eq!(bus.cpu_main_delay(Processor::Master, 1), 8);
    assert_eq!(bus.fast_vdp1_busy[0].get(), 0);
}

#[test]
fn relaxed_vdp1_stores_hold_the_port_for_both_b_bus_halfwords() {
    let store = serial_store_clocks();
    let mut bus = relaxed();
    bus.for_cpu(Processor::Master)
        .write32(TEXTURE, 0x1234_5678)
        .unwrap();
    bus.for_cpu(Processor::Master)
        .write16(TEXTURE + 4, 0x1234)
        .unwrap();
    let halfword = {
        let mut serial = Bus::default();
        serial
            .for_cpu(Processor::Master)
            .write16(TEXTURE, 0x1234)
            .unwrap();
        serial.cpu_main_delay(Processor::Master, 1)
    };
    assert_eq!(bus.fast_vdp1_busy[0].get(), 2 * store + halfword);
    assert_eq!(bus.fast_vdp1_busy[1].get(), 0);
    bus.for_cpu(Processor::Master)
        .write16(crate::vdp1::FRAMEBUFFER | 0x2000_0000, 0)
        .unwrap();
    assert_ne!(bus.fast_vdp1_busy[1].get(), 0);
}

/// One 64x64 textured sprite, enough VRAM reads to see a stall.
fn drawing() -> Bus {
    let mut bus = relaxed();
    for (offset, value) in [
        // System clipping to the 320x224 screen.
        (0, 9),
        (0x14, 319),
        (0x16, 223),
        // Normal sprite: RGB texture at 0x8000, 64x64 at the origin.
        (0x20, 0),
        (0x24, 0xE8),
        (0x28, 0x1000),
        (0x2A, 0x0840),
        (0x40, 0x8000),
    ] {
        bus.write16(VRAM + offset, value).unwrap();
    }
    for offset in (0x8000..0x8000 + 64 * 64 * 2).step_by(2) {
        bus.write16(VRAM + offset, 0x8000 | offset as u16).unwrap();
    }
    bus.write16(REGISTERS + 2, 2).unwrap();
    bus.write16(REGISTERS + 4, 1).unwrap();
    bus
}

fn draw_clocks(bus: &mut Bus) -> u32 {
    let mut clocks = 0;
    while bus.vdp1.busy() {
        bus.fast_devices(1).unwrap();
        clocks += 1;
        assert!(clocks < 100_000);
    }
    clocks
}

#[test]
fn relaxed_vdp1_drawing_waits_for_charged_port_clocks_at_catch_up() {
    let free = draw_clocks(&mut drawing());
    let mut stalled = drawing();
    stalled.charge_fast_vdp1_busy((true, false), 1000);
    // Charges spill across catch-ups until all are spent.
    assert_eq!(draw_clocks(&mut stalled), free + 1000);
    assert_eq!(stalled.fast_vdp1_busy[0].get(), 0);
    // A framebuffer-only hold does not stop texture fetches, only plotting.
    let mut plotted = drawing();
    plotted.charge_fast_vdp1_busy((false, true), 1000);
    assert!(draw_clocks(&mut plotted) > free);
}
