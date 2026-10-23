//! Source-authored board vectors. ST-058-R2 §3.3 fig.3.2/table3.3;
//! SH7604 H §§7.3/9.3 and SCU ST-097 §2.1 supply bus completion costs.
use super::*;

fn board() -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    for p in [Processor::Master, Processor::Slave] {
        // Ordinary 32-bit CS3, zero software/idle waits. CS2 stays
        // 16-bit with its reset six Tw states: a VRAM beat takes eight.
        b.onchip[p as usize]
            .write32(0xFFFFFFE8, 0xA55A0030)
            .unwrap();
        b.onchip[p as usize]
            .write32(0xFFFFFFEC, 0xA55A0000)
            .unwrap();
    }
    for (at, value) in [
        (0, 0x8000),
        (0x10, 0x4EEE),
        (0x12, 0xEEEE),
        (0x20, 0x101),
        (0x28, 2),
        (0x78, 1),
        (0xF8, 1),
    ] {
        b.write16(VDP2 + at, value).unwrap();
    }
    b.write16(crate::vdp2::CRAM + 2, 0x801F).unwrap();
    b.write16(crate::vdp2::CRAM + 4, 0x83E0).unwrap();
    for at in (0..160).step_by(2) {
        b.write16(VRAM + at, 0x1111).unwrap();
    }
    b.write32(HIGH_WRAM + 0x100, 0x22221111).unwrap();
    b
}
fn advance(b: &mut Bus, mut clocks: u32, chunk: u32) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_clocks(n).unwrap();
        clocks -= n;
    }
}
fn retained(b: &Bus, address: usize) -> u16 {
    b.vdp2
        .display_fetch
        .borrow()
        .trace
        .iter()
        .find(|r| r.path == 1 && r.layer == 0 && r.address == address)
        .unwrap()
        .words[0]
}

#[test]
fn repeated_tile_addresses_refetch_after_cpu_publication_before_the_next_packet() {
    // ST-058-R2 §3.3 tables3.2–3.4: legal NBG0/16-color T0 name
    // and T0 character accesses in separate banks. The explicit two-cycle
    // lead primes line1 at1644 and its next packet at1676. A CPU write
    // beginning at1650 publishes at1658, between those acquisitions.
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut b = board();
            for (at, value) in [
                (0xE, 0x300),
                (0x10, 0x0EEE),
                (0x28, 0),
                (0x30, 0x8008),
                (0x18, 0x4EEE),
                (0x1A, 0xEEEE),
            ] {
                b.write16(VDP2 + at, value).unwrap();
            }
            for at in (0..160).step_by(2) {
                b.write16(VRAM + at, 0).unwrap();
            }
            b.write32(VRAM + 0x40000, 0x11111111).unwrap();
            advance(&mut b, 1650, chunk);
            assert_eq!(retained(&b, 0x40000), 0x1111);
            assert_eq!(
                b.replay_cpu(p).write16(0x25E40000, 0x2222),
                Err(BusError::Pending)
            );
            b.grant_bus_transaction();
            advance(&mut b, 7, chunk);
            assert_eq!(b.read16(VRAM + 0x40000), Ok(0x1111));
            advance(&mut b, 1, chunk);
            assert_eq!(b.read16(VRAM + 0x40000), Ok(0x2222));
            assert!(b.take_cpu_transaction_completion(p));
            assert_eq!(b.replay_cpu(p).write16(0x25E40000, 0x2222), Ok(()));
            b.finish_cpu_transaction(p);
            advance(&mut b, 18, chunk);
            let chars: Vec<_> = b
                .vdp2
                .display_fetch
                .borrow()
                .trace
                .iter()
                .filter(|r| r.path == 1)
                .map(|r| (r.phase, r.address, r.words))
                .collect();
            assert_eq!(
                chars,
                [(0, 0x40000, [0x1111; 2]), (32, 0x40000, [0x2222, 0x1111]),]
            );
            advance(&mut b, 1708 + 1280 - 1676, chunk);
            for x in 0..16 {
                let at = (320 + x) * 4;
                assert_eq!(
                    &b.vdp2.scanning[at..at + 4],
                    if (8..12).contains(&x) {
                        [0, 255, 0, 255]
                    } else {
                        [255, 0, 0, 255]
                    },
                    "processor={p:?}, chunk={chunk}, x={x}"
                );
            }
        }
    }
}

#[test]
fn cpu_writes_publish_before_at_or_after_a_display_access_on_both_processors() {
    // The unprimed reset line reads x8..15 (VRAM+4) at T0=32.
    // Eight-state physical writes beginning at22/24/25 finish30/32/33.
    // Display samples before a simultaneous bus completion.
    for p in [Processor::Master, Processor::Slave] {
        for (start, end, expected) in [(22, 30, 0x2222), (24, 32, 0x1111), (25, 33, 0x1111)] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                advance(&mut b, start, chunk);
                assert_eq!(
                    b.replay_cpu(p).write16(0x25E00004, 0x2222),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
                assert_eq!(b.transactions.active.as_ref().unwrap().remaining, 8);
                advance(&mut b, 7, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x1111));
                advance(&mut b, 1, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x2222));
                assert!(b.take_cpu_transaction_completion(p));
                assert_eq!(b.replay_cpu(p).write16(0x25E00004, 0x2222), Ok(()));
                b.finish_cpu_transaction(p);
                advance(&mut b, 40 - end, chunk);
                assert_eq!(retained(&b, 4), expected);
            }
        }
    }
}

#[test]
fn onchip_dmac_writes_obey_the_same_acquisition_boundary_and_irq_order() {
    for p in [Processor::Master, Processor::Slave] {
        for (start, end, expected) in [(19, 30, 0x2222), (21, 32, 0x1111), (22, 33, 0x1111)] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                advance(&mut b, start, chunk);
                // Word auto-request: zero-wait CS3 source takes two clocks;
                // the physical CS2 destination takes eight after one buffer-accept
                // state. No host result
                // writes occur after arming either DMA controller.
                for (at, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, VRAM + 4),
                    (8, 1),
                    (12, 0x5605),
                    (0x20, 0x44),
                    (0x30, 1),
                ] {
                    b.onchip[p as usize]
                        .write32(0xFFFFFF80 + at, value)
                        .unwrap();
                }
                b.onchip[p as usize]
                    .write16(crate::onchip::IPRA, 0x600)
                    .unwrap();
                advance(&mut b, 3, chunk);
                // SH7604 §7.11.2 reports TE/IRQ at buffer acceptance,
                // preceding publication of the accepted physical write.
                assert_eq!(b.onchip[p as usize].interrupt(), Some((6, 0x44)));
                advance(&mut b, 7, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x1111));
                advance(&mut b, 1, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x2222));
                assert_eq!(b.onchip[p as usize].read32(0xFFFFFF88), Ok(0));
                assert_eq!(b.onchip[p as usize].interrupt(), Some((6, 0x44)));
                advance(&mut b, 40 - end, chunk);
                assert_eq!(retained(&b, 4), expected);
            }
        }
    }
}

#[test]
fn each_scu_channel_samples_and_completes_against_the_same_display_edges() {
    // Setup2 + zero-wait CS3 read2 + turnaround1 + CS2 write8.
    // The second halfword waits for a CPU slot when its grant coincides
    // with T0 (four clocks), or is inside T0 (three), then raises IRQ.
    for channel in 0..3 {
        for (start, end, finish, expected) in [
            (17, 30, 38, 0x2222),
            (19, 32, 44, 0x1111),
            (20, 33, 44, 0x1111),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut b = board();
                advance(&mut b, start, chunk);
                let base = SCU + channel * 0x20;
                for (at, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, VRAM + 4),
                    (8, 4),
                    (12, 0x102),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(base + at, value).unwrap();
                }
                advance(&mut b, 12, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x1111));
                advance(&mut b, 1, chunk);
                assert_eq!(b.read16(VRAM + 4), Ok(0x2222));
                assert_eq!(b.read32(SCU + 0xA4), Ok(0));
                advance(&mut b, finish - end - 1, chunk);
                assert_eq!(b.read32(SCU + 0xA4), Ok(0));
                advance(&mut b, 1, chunk);
                assert_eq!(b.read32(SCU + 0xA4), Ok(1 << (11 - channel)));
                assert_eq!(retained(&b, 4), expected, "channel={channel}, end={end}");
            }
        }
    }
}

#[test]
fn acquisition_and_completed_pixels_are_independent_of_chunks_and_debugger_reads() {
    let mut reference = None;
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let mut b = board();
            // Prime line1 during line0's final two blanking cycles.
            advance(&mut b, 1708, chunk);
            advance(&mut b, 77, chunk);
            if inspect {
                for _ in 0..3 {
                    let _ = b.vdp2.framebuffer().unwrap();
                }
            }
            advance(&mut b, 1280 - 77, chunk);
            let trace = b.vdp2.display_fetch.borrow().trace.clone();
            let pixels = b.vdp2.scanning.clone();
            let result = (trace, pixels);
            if let Some(expected) = &reference {
                assert_eq!(&result, expected);
            } else {
                reference = Some(result);
            }
            b.reset_volatile();
            advance(&mut b, 1800, chunk);
            assert!(b.vdp2.display_fetch.borrow().trace.is_empty());
        }
    }
}

#[test]
fn a_busy_display_bank_does_not_stall_the_other_cpu_port() {
    let mut b = board();
    b.write16(VDP2 + 0x28, 0x42).unwrap();
    b.write32(VDP2 + 0x10, 0x44444444).unwrap();
    b.write32(VDP2 + 0x18, 0xFFFFFFFF).unwrap();
    assert_eq!(b.vdp2.cpu_access_slots(VRAM), 0);
    assert_eq!(b.vdp2.cpu_access_slots(VRAM + 0x40000), 255);
    assert_eq!(
        b.replay_cpu(Processor::Slave).write16(0x25E40000, 0x1234),
        Err(BusError::Pending)
    );
    advance(&mut b, 7, 3);
    assert_eq!(b.read16(VRAM + 0x40000), Ok(0));
    advance(&mut b, 1, 3);
    assert_eq!(b.read16(VRAM + 0x40000), Ok(0x1234));
    assert!(
        b.vdp2
            .display_fetch
            .borrow()
            .trace
            .iter()
            .any(|r| r.path == 1 && r.address == 8)
    );
}
