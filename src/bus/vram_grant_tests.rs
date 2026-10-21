//! ST-058 §3.3: real CPU data paths accumulate waits from the bank grants.
use super::*;
#[test]
fn both_cpus_wait_for_the_selected_vram_port_and_cache_hits_issue_no_new_request() {
    for processor in [Processor::Master, Processor::Slave] {
        for (ramctl, a0, a1, expected) in [
            (0, 0xFFFFFFFF, 0, 6),
            (0, 0, 0, 1280),
            (0x100, 0xFEEE0000, 0xFEEE0000, 6),
            (0x100, 0xFEEE0000, 0xF0000000, 1280),
        ] {
            let mut b = Bus::default();
            for (offset, value) in [(0, 0x8000), (0x20, 1), (0xE, ramctl)] {
                b.write16(VDP2 + offset, value).unwrap();
            }
            b.write32(VDP2 + 0x10, a0).unwrap();
            b.write32(VDP2 + 0x14, a1).unwrap();
            b.write32(VRAM, 0x13579BDF).unwrap();
            assert_eq!(b.for_cpu(processor).read32(VRAM), Ok(0x13579BDF));
            assert_eq!(b.take_wait_clocks(), expected);
            // The other physical port is free: disabled NBG0's command is
            // replaced with CPU access, independently of A0/A1's grant.
            b.write32(VDP2 + 0x18, 0xFFFFFFFF).unwrap();
            b.write32(VDP2 + 0x1C, 0xFFFFFFFF).unwrap();
            assert_eq!(b.for_cpu(processor).read16(VRAM + 0x40000), Ok(0));
            assert_eq!(b.take_wait_clocks(), 6);
        }
        let mut b = Bus::default();
        b.write16(VDP2, 0x8000).unwrap();
        b.write16(VDP2 + 0x20, 1).unwrap();
        b.write32(VDP2 + 0x10, 0xFFFFFFFF).unwrap();
        b.for_cpu(processor).write8(CCR, 1).unwrap();
        b.for_cpu(processor).read32(VRAM).unwrap();
        assert!(b.take_wait_clocks() > 0);
        b.write32(VDP2 + 0x10, 0).unwrap();
        b.for_cpu(processor).read32(VRAM).unwrap();
        assert_eq!(b.take_wait_clocks(), 0); // resident CPU-local cache line
        b.for_cpu(processor).read32(VRAM + 0x20000000).unwrap();
        assert_eq!(b.take_wait_clocks(), 1280); // P1 still requests the bus
        // Four cache-refill beats queue one wait into blanking. They must
        // not each repeat the entire active-line stall at the old beam phase.
        b.for_cpu(processor).write8(CCR, 0x11).unwrap();
        b.for_cpu(processor).read32(VRAM).unwrap();
        assert_eq!(b.take_wait_clocks(), 1298);
        b.reset_volatile();
        assert_eq!(b.read32(VDP2 + 0x10), Ok(0));
        b.for_cpu(processor).read16(VRAM).unwrap();
        assert_eq!(b.take_wait_clocks(), 6); // reset display releases the port
    }
}
