//! H §§9.1.1/9.3.3–9.3.8: independently authored bus-boundary vectors.
use super::*;
use crate::onchip::{DMAC, IPRA};

const SOURCE: u32 = HIGH_WRAM + 0x100;
const DEST: u32 = HIGH_WRAM + 0x200;

fn setup(processor: Processor, size: u32, count: u32, burst: bool) -> Bus {
    let mut bus = Bus::default();
    bus.set_slave_enabled(true);
    for p in [Processor::Master, Processor::Slave] {
        // H §7.3: ordinary 32-bit, no software waits, no programmed idle
        // states. A read-to-write transition still requires one empty state.
        let chip = &mut bus.onchip[p as usize];
        chip.write32(0xFFFFFFE0, 0xA55A0000).unwrap();
        chip.write32(0xFFFFFFE8, 0xA55A0000).unwrap();
    }
    for i in 0..8 {
        bus.write32(SOURCE + i * 4, 0x11223344 + i).unwrap();
    }
    let chip = &mut bus.onchip[processor as usize];
    chip.write16(IPRA, 0x600).unwrap();
    for (off, value) in [
        (0, SOURCE),
        (4, DEST),
        (8, count),
        (12, 0x5205 | size << 10 | if burst { 0x10 } else { 0 }),
        (0x20, 0x44),
        (0x30, 1),
    ] {
        chip.write32(DMAC + off, value).unwrap();
    }
    bus
}

#[test]
fn dmac_samples_reads_then_publishes_writes_count_and_irq_at_separate_deadlines() {
    for p in [Processor::Master, Processor::Slave] {
        let mut bus = setup(p, 2, 1, false);
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.write32(SOURCE, 0xAABBCCDD).unwrap();
        bus.advance_clocks(1).unwrap(); // read completes, latching changed input
        bus.write32(SOURCE, 0xDEADBEEF).unwrap();
        bus.advance_clocks(2).unwrap(); // destination T1 + turnaround, not T2
        assert_eq!(bus.read32(DEST), Ok(0));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(1));
        assert_eq!(bus.onchip[p as usize].interrupt(), None);
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.read32(DEST), Ok(0xAABBCCDD));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
        assert_eq!(bus.onchip[p as usize].read32(DMAC), Ok(SOURCE + 4));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 4), Ok(DEST + 4));
        assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
        assert_eq!(bus.take_dma_errors(), [false; 2]);
    }
}

#[test]
fn sixteen_byte_units_read_the_whole_source_before_writing_even_for_short_final_counts() {
    for count in 1..=4 {
        for p in [Processor::Master, Processor::Slave] {
            let mut bus = setup(p, 3, count, false);
            // Overlap by one longword: interleaved reads/writes corrupt the
            // second and later values. H §9.1.1 requires four reads first.
            bus.onchip[p as usize]
                .write32(DMAC + 4, SOURCE + 4)
                .unwrap();
            bus.advance_clocks(8).unwrap();
            for i in 0..4 {
                assert_eq!(bus.read32(SOURCE + (i + 1) * 4), Ok(0x11223345 + i));
            }
            assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(count));
            bus.advance_clocks(1 + 2 * count).unwrap();
            for i in 0..4 {
                let expected = if i < count {
                    0x11223344 + i
                } else {
                    0x11223345 + i
                };
                assert_eq!(bus.read32(SOURCE + (i + 1) * 4), Ok(expected));
            }
            assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
            assert_eq!(bus.onchip[p as usize].read32(DMAC), Ok(SOURCE + 16));
            assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
        }
    }
}

#[test]
fn cycle_steal_yields_to_a_waiting_cpu_while_burst_retains_the_bus_and_cache_hits_proceed() {
    for burst in [false, true] {
        let mut bus = setup(Processor::Master, 2, 2, burst);
        // Populate a real cache line before either DMA or the queued CPU
        // starts, then discard only setup access timing.
        bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
        bus.for_cpu(Processor::Slave).write8(CCR, 1).unwrap();
        assert_eq!(bus.for_cpu(Processor::Slave).read32(SOURCE), Ok(0x11223344));
        bus.cpu_bus_clocks[1].set(0);
        bus.onchip[1].take_wait_clocks();
        bus.advance_clocks(16).unwrap();
        bus.onchip[0].write32(DMAC + 0x30, 1).unwrap();
        let at = DEST | 0x20000000;
        assert_eq!(
            bus.replay_cpu(Processor::Slave).read32(at),
            Err(BusError::Pending)
        );
        bus.advance_clocks(5).unwrap(); // first complete unit
        assert_eq!(bus.read32(DEST), Ok(0x11223344));
        bus.advance_clocks(2).unwrap();
        if burst {
            assert!(!bus.take_cpu_transaction_completion(Processor::Slave));
            assert_eq!(bus.read32(DEST + 4), Ok(0));
            // A resident instruction/data cache hit doesn't need bus ownership.
            assert_eq!(bus.for_cpu(Processor::Slave).read32(SOURCE), Ok(0x11223344));
            assert_eq!(bus.cpu_bus_clocks[1].get(), 0);
            assert_eq!(bus.onchip[1].take_wait_clocks(), 0);
        } else {
            assert!(bus.take_cpu_transaction_completion(Processor::Slave));
            assert_eq!(bus.replay_cpu(Processor::Slave).read32(at), Ok(0x11223344));
            bus.finish_cpu_transaction(Processor::Slave);
            assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(1));
        }
        bus.advance_clocks(64).unwrap();
        assert_eq!(bus.read32(DEST + 4), Ok(0x11223345));
        if burst {
            assert_eq!(bus.replay_cpu(Processor::Slave).read32(at), Ok(0x11223344));
        }
    }
}

#[test]
fn reset_cancels_both_pending_read_and_granted_write_without_late_dma_or_irq() {
    for p in [Processor::Master, Processor::Slave] {
        for clocks in [1, 2, 4] {
            let mut bus = setup(p, 2, 1, true);
            bus.advance_clocks(clocks).unwrap();
            if p == Processor::Slave {
                bus.set_slave_enabled(false);
            } else {
                bus.reset_volatile();
            }
            bus.advance_clocks(128).unwrap();
            assert_eq!(bus.read32(DEST), Ok(0));
            assert_eq!(bus.onchip[p as usize].interrupt(), None);
            assert_eq!(bus.take_dma_errors(), [false; 2]);
        }
    }
}

#[test]
fn tas_excludes_dmac_until_its_write_and_instruction_release() {
    let mut bus = setup(Processor::Master, 2, 0, false);
    bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
    assert_eq!(
        bus.replay_cpu(Processor::Slave).tas_read8(DEST),
        Err(BusError::Pending)
    );
    bus.advance_clocks(2).unwrap();
    assert_eq!(bus.replay_cpu(Processor::Slave).tas_read8(DEST), Ok(0));
    bus.onchip[0].write32(DMAC + 8, 1).unwrap();
    bus.onchip[0].write32(DMAC + 0x30, 1).unwrap();
    bus.advance_clocks(40).unwrap();
    assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(1));
    bus.finish_cpu_transaction(Processor::Slave);
    bus.advance_clocks(5).unwrap();
    assert_eq!(bus.read32(DEST), Ok(0x11223344));
}

#[test]
fn every_narrow_bus_and_software_wait_delays_actual_beats_and_the_final_irq() {
    // H §§7.2.2/7.3.1/9.3.5. Both endpoints use CS3. Each physical
    // beat is T1+T2+Tw; one read-to-write empty state is mandatory.
    for p in [Processor::Master, Processor::Slave] {
        for (code, beats) in [(1, 4), (2, 2), (3, 1)] {
            for waits in 0..=3 {
                let mut bus = setup(p, 2, 1, false);
                bus.onchip[p as usize]
                    .write32(0xFFFFFFE4, 0xA55A003C | code << 6)
                    .unwrap();
                bus.onchip[p as usize]
                    .write32(0xFFFFFFE8, 0xA55A0000 | waits << 6)
                    .unwrap();
                let end = (2 + waits) * beats * 2 + 1;
                bus.advance_clocks(end - 1).unwrap();
                assert_eq!(bus.read8(DEST + 3), Ok(0));
                assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(1));
                assert_eq!(bus.onchip[p as usize].interrupt(), None);
                bus.advance_clocks(1).unwrap();
                assert_eq!(
                    bus.read32(DEST),
                    Ok(0x11223344),
                    "{p:?}, width code {code}, waits {waits}"
                );
                assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
            }
        }
    }
}

#[test]
fn nmi_completes_the_accepted_unit_but_dme_abort_never_issues_another_cycle() {
    for nmi in [false, true] {
        for clocks in [1, 2, 4] {
            let mut bus = setup(Processor::Master, 2, 2, true);
            bus.advance_clocks(clocks).unwrap();
            if nmi {
                bus.signal_nmi(Processor::Master);
            } else {
                bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
            }
            bus.advance_clocks(64).unwrap();
            let completes = nmi || clocks >= 4;
            assert_eq!(bus.read32(DEST), Ok(if completes { 0x11223344 } else { 0 }));
            assert_eq!(bus.read32(DEST + 4), Ok(0));
            assert_eq!(
                bus.onchip[0].read32(DMAC + 8),
                Ok(if completes { 1 } else { 2 })
            );
            assert_eq!(bus.onchip[0].interrupt(), None);
            assert_eq!(bus.take_dma_errors(), [false; 2]);
        }
    }
}

#[test]
fn edge_requests_are_consumed_at_acceptance_and_burst_edges_start_the_whole_block() {
    for burst in [false, true] {
        let mut bus = setup(Processor::Master, 2, 3, burst);
        bus.onchip[0]
            .write32(DMAC + 12, 0x5845 | if burst { 0x10 } else { 0 })
            .unwrap();
        bus.advance_clocks(8).unwrap();
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.onchip[0].set_dma_request_pin(0, false);
        bus.advance_clocks(1).unwrap(); // accepted, data read still pending
        bus.onchip[0].set_dma_request_pin(0, true);
        bus.onchip[0].set_dma_request_pin(0, false); // a second edge while busy
        bus.advance_clocks(64).unwrap();
        assert_eq!(bus.read32(DEST), Ok(0x11223344));
        assert_eq!(bus.read32(DEST + 4), Ok(0x11223345));
        assert_eq!(bus.read32(DEST + 8), Ok(if burst { 0x11223346 } else { 0 }));
        assert_eq!(
            bus.onchip[0].read32(DMAC + 8),
            Ok(if burst { 0 } else { 1 })
        );
    }
}

#[test]
fn dmac_cannot_access_its_own_bsc_ubc_or_cache_registers_or_change_them() {
    for address in [CCR, 0xFFFFFF40, DMAC, DMAC + 0x30, 0xFFFFFFE0] {
        let mut bus = setup(
            Processor::Master,
            if address == CCR { 0 } else { 2 },
            1,
            false,
        );
        bus.onchip[0].write32(DMAC + 4, address).unwrap();
        bus.advance_clocks(128).unwrap();
        assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(1));
        assert_eq!(bus.take_dma_errors(), [true, false]);
        assert_eq!(bus.onchip[0].interrupt(), None);
        assert_eq!(bus.onchip[0].read32(0xFFFFFFE0), Ok(0));
        assert_eq!(bus.cache[0].borrow().control(), 0);
        assert_eq!(bus.read32(DEST), Ok(0));
    }
}

#[test]
fn fixed_cycle_steal_alternates_and_burst_priorities_follow_the_programmed_mode() {
    for (burst, round_robin, winners) in [
        (false, false, [0, 1, 0, 1]),
        (true, false, [0, 0, 1, 1]),
        (true, true, [1, 0, 1, 0]),
    ] {
        let mut bus = setup(Processor::Master, 2, 2, burst);
        for (off, value) in [
            (0x10, SOURCE + 16),
            (0x14, DEST + 16),
            (0x18, 2),
            (0x1C, 0x5A05 | if burst { 0x10 } else { 0 }),
            (0x30, if round_robin { 9 } else { 1 }),
        ] {
            bus.onchip[0].write32(DMAC + off, value).unwrap();
        }
        let mut counts = [2, 2];
        for winner in winners {
            bus.advance_clocks(5).unwrap();
            counts[winner] -= 1;
            for (i, &count) in counts.iter().enumerate() {
                assert_eq!(bus.onchip[0].read32(DMAC + 8 + i as u32 * 16), Ok(count));
            }
        }
        assert_eq!(bus.read32(DEST), Ok(0x11223344));
        assert_eq!(bus.read32(DEST + 4), Ok(0x11223345));
        assert_eq!(bus.read32(DEST + 16), Ok(0x11223348));
        assert_eq!(bus.read32(DEST + 20), Ok(0x11223349));
    }
}

#[test]
fn zero_tcr_means_sixteen_million_longwords_and_standby_discards_burst_state() {
    let mut bus = setup(Processor::Master, 3, 0, true);
    bus.advance_clocks(17).unwrap();
    for i in 0..4 {
        assert_eq!(bus.read32(DEST + i * 4), Ok(0x11223344 + i));
    }
    assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(0xFFFFFC));
    assert_eq!(bus.onchip[0].interrupt(), None);
    bus.onchip[0].dma.standby();
    bus.advance_clocks(128).unwrap();
    assert_eq!(bus.read32(DEST + 16), Ok(0));
    assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(0xFFFFFC));
    assert_eq!(bus.onchip[0].read32(DMAC + 12), Ok(0));
}
