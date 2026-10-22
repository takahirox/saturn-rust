//! H §§9.1.1/9.3.3–9.3.8: independently authored bus-boundary vectors.
use super::*;
use crate::onchip::{DMAC, IPRA};

const SOURCE: u32 = HIGH_WRAM + 0x100;
const DEST: u32 = HIGH_WRAM + 0x200;

#[test]
fn internal_peripheral_dma_has_cpu_local_one_state_phases_parallel_to_external_bus() {
    // H §§7.11.2–7.11.3/9.3.5: SCI SMR -> BRR is an internal transfer.
    // A TAS-held external transaction must neither delay it nor inherit any
    // of its phase time. Both physical SH7604s have independent internal buses.
    for chunk in [1, 2, 7] {
        let mut bus = setup(Processor::Master, 0, 1, false);
        bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
        assert_eq!(
            bus.replay_cpu(Processor::Slave).tas_read8(DEST),
            Err(BusError::Pending)
        );
        bus.advance_clocks(2).unwrap();
        assert_eq!(bus.replay_cpu(Processor::Slave).tas_read8(DEST), Ok(0));
        for p in [Processor::Master, Processor::Slave] {
            let chip = &mut bus.onchip[p as usize];
            chip.write8(0xFFFFFE00, 0x31 + p as u8).unwrap();
            for (off, value) in [
                (0, 0xFFFFFE00),
                (4, 0xFFFFFE01),
                (8, 1),
                (12, 0x205),
                (0x20, 0x44),
                (0x30, 1),
            ] {
                chip.write32(DMAC + off, value).unwrap();
            }
            chip.write16(IPRA, 0x600).unwrap();
        }
        bus.advance_clocks(1).unwrap(); // only the internal read completes
        for p in [Processor::Master, Processor::Slave] {
            let chip = &mut bus.onchip[p as usize];
            assert_eq!(chip.read8(0xFFFFFE01), Ok(0xFF));
            assert_eq!(chip.read32(DMAC + 8), Ok(1));
            assert_eq!(chip.interrupt(), None);
            chip.write8(0xFFFFFE00, 0x71).unwrap(); // cannot change sampled data
        }
        bus.advance_clocks(chunk).unwrap();
        for p in [Processor::Master, Processor::Slave] {
            let chip = &bus.onchip[p as usize];
            assert_eq!(chip.read8(0xFFFFFE01), Ok(0x31 + p as u8));
            assert_eq!(chip.read32(DMAC + 8), Ok(0));
            assert_eq!(chip.interrupt(), Some((6, 0x44)));
        }
        // The external lock is still live; internal DMA never acquired it.
        assert_eq!(
            bus.replay_cpu(Processor::Master).read32(SOURCE),
            Err(BusError::Pending)
        );
        bus.advance_clocks(8).unwrap();
        assert!(!bus.take_cpu_transaction_completion(Processor::Master));
        bus.finish_cpu_transaction(Processor::Slave);
        bus.advance_clocks(8).unwrap();
        assert_eq!(
            bus.replay_cpu(Processor::Master).read32(SOURCE),
            Ok(0x11223344)
        );
        assert_eq!(bus.take_dma_errors(), [false; 2]);
    }
}

#[test]
fn slave_reset_cancels_its_granted_internal_phase_without_canceling_master_or_external_bus() {
    let mut bus = setup(Processor::Slave, 0, 1, false);
    for (off, value) in [(0, 0xFFFFFE00), (4, 0xFFFFFE01), (12, 0x205)] {
        bus.onchip[1].write32(DMAC + off, value).unwrap();
    }
    bus.onchip[1].write8(0xFFFFFE00, 0x53).unwrap();
    bus.grant_bus_transaction();
    bus.set_slave_enabled(false); // cancel the granted internal read
    bus.set_slave_enabled(true);
    bus.advance_clocks(8).unwrap();
    assert_eq!(bus.onchip[1].read8(0xFFFFFE01), Ok(0xFF));
    assert_eq!(bus.onchip[1].interrupt(), None);
    // Repeat with a completed read and a granted write.
    for (off, value) in [
        (0, 0xFFFFFE00),
        (4, 0xFFFFFE01),
        (8, 1),
        (12, 0x205),
        (0x30, 1),
    ] {
        bus.onchip[1].write32(DMAC + off, value).unwrap();
    }
    bus.advance_clocks(1).unwrap();
    bus.grant_bus_transaction();
    bus.set_slave_enabled(false);
    // A post-reset value exposes a late write from the canceled phase.
    bus.onchip[1].write8(0xFFFFFE01, 0x67).unwrap();
    bus.advance_clocks(8).unwrap();
    assert_eq!(bus.onchip[1].read8(0xFFFFFE01), Ok(0x67));
    assert_eq!(bus.onchip[1].interrupt(), None);
    assert_eq!(bus.take_dma_errors(), [false; 2]);
}

#[test]
fn internal_dma_completes_concurrently_with_another_cpus_granted_external_read() {
    let mut bus = setup(Processor::Master, 0, 1, false);
    for (off, value) in [(0, 0xFFFFFE00), (4, 0xFFFFFE01), (12, 0x205)] {
        bus.onchip[0].write32(DMAC + off, value).unwrap();
    }
    bus.onchip[0].write8(0xFFFFFE00, 0x35).unwrap();
    let address = SOURCE | 0x20000000;
    assert_eq!(
        bus.replay_cpu(Processor::Slave).read32(address),
        Err(BusError::Pending)
    );
    bus.advance_clocks(1).unwrap();
    assert!(!bus.take_cpu_transaction_completion(Processor::Slave));
    assert_eq!(bus.onchip[0].read8(0xFFFFFE01), Ok(0xFF));
    bus.write32(SOURCE, 0x88776655).unwrap();
    bus.advance_clocks(1).unwrap();
    assert!(bus.take_cpu_transaction_completion(Processor::Slave));
    assert_eq!(
        bus.replay_cpu(Processor::Slave).read32(address),
        Ok(0x88776655)
    );
    assert_eq!(bus.onchip[0].read8(0xFFFFFE01), Ok(0x35));
    assert_eq!(bus.onchip[0].interrupt(), Some((6, 0x44)));
}

#[test]
fn external_level_request_is_not_accepted_while_tas_retains_the_bus() {
    let mut bus = setup(Processor::Master, 2, 1, false);
    bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
    assert_eq!(
        bus.replay_cpu(Processor::Slave).tas_read8(DEST),
        Err(BusError::Pending)
    );
    bus.advance_clocks(2).unwrap();
    assert_eq!(bus.replay_cpu(Processor::Slave).tas_read8(DEST), Ok(0));
    bus.onchip[0].write32(DMAC + 12, 0x5805).unwrap(); // active-low level DREQ
    bus.onchip[0].write32(DMAC + 0x30, 1).unwrap();
    bus.onchip[0].set_dma_request_pin(0, false);
    bus.advance_clocks(16).unwrap(); // request while the other CPU owns TAS
    bus.onchip[0].set_dma_request_pin(0, true);
    bus.finish_cpu_transaction(Processor::Slave);
    bus.advance_clocks(16).unwrap();
    assert_eq!(bus.read32(DEST), Ok(0));
    assert_eq!(bus.onchip[0].read32(DMAC + 8), Ok(1));
    bus.onchip[0].set_dma_request_pin(0, false);
    bus.advance_clocks(5).unwrap();
    assert_eq!(bus.read32(DEST), Ok(0x11223344));
    assert_eq!(bus.onchip[0].interrupt(), Some((6, 0x44)));
}

#[test]
fn internal_divu_dma_waits_for_division_without_premature_result_irq_or_cpu_waits() {
    use crate::onchip::DIVU;
    for p in [Processor::Master, Processor::Slave] {
        let mut bus = setup(p, 2, 1, false);
        let chip = &mut bus.onchip[p as usize];
        chip.write16(IPRA, 0x6600).unwrap();
        chip.write32(DIVU + 0xC, 0x45).unwrap();
        chip.write32(DIVU + 8, 2).unwrap(); // unsaturated overflow result + IRQ
        chip.write32(DIVU, 0).unwrap();
        chip.write32(DIVU + 4, 5).unwrap();
        chip.write32(DMAC, DIVU + 4).unwrap();
        // H table10.2: three setup + three non-restoring steps. Zero divisor,
        // initial low=5 -> partial low=(5<<3)|7=47; interrupt at clock6.
        bus.advance_clocks(5).unwrap();
        assert_eq!(bus.onchip[p as usize].interrupt(), None);
        assert_eq!(bus.read32(DEST), Ok(0));
        assert_eq!(bus.onchip[p as usize].take_wait_clocks(), 0);
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x45)));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(1));
        bus.advance_clocks(2).unwrap(); // internal read done; external write T1
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.read32(DEST), Ok(47));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
        assert_eq!(bus.onchip[p as usize].take_wait_clocks(), 0);
    }
}

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
fn dmac_samples_reads_then_accepts_buffered_writes_before_external_publication() {
    for p in [Processor::Master, Processor::Slave] {
        let mut bus = setup(p, 2, 1, false);
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.write32(SOURCE, 0xAABBCCDD).unwrap();
        bus.advance_clocks(1).unwrap(); // read completes, latching changed input
        bus.write32(SOURCE, 0xDEADBEEF).unwrap();
        bus.advance_clocks(2).unwrap(); // destination T1 + turnaround, not T2
        assert_eq!(bus.read32(DEST), Ok(0));
        // H §7.11.2: DMAC has handed the write to the BSC. Count/TE
        // report acceptance; a dependent external read still drains the buffer.
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
        assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
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
fn every_narrow_bus_and_software_wait_delays_publication_after_buffer_acceptance() {
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
                assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
                assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
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

#[test]
fn cpu_peripheral_reads_sample_at_completion_and_share_only_the_local_dma_bus() {
    // H §7.11.2: a CPU peripheral read adds a cycle to determine the
    // access. Both SH7604 internal buses operate while the external bus is
    // TAS-locked. DMA has priority when requesting the same peripheral bus.
    for chunk in [1, 2, 7] {
        let mut bus = setup(Processor::Master, 0, 1, false);
        for (off, value) in [(0, 0xFFFFFE00), (4, 0xFFFFFE01), (12, 0x205)] {
            bus.onchip[0].write32(DMAC + off, value).unwrap();
        }
        bus.onchip[0].write8(0xFFFFFE00, 0x35).unwrap();
        for p in [Processor::Master, Processor::Slave] {
            assert_eq!(bus.replay_cpu(p).read8(0xFFFFFE01), Err(BusError::Pending));
        }
        bus.advance_clocks(1).unwrap(); // master DMA read; both CPU reads wait
        for p in [Processor::Master, Processor::Slave] {
            assert!(!bus.take_cpu_transaction_completion(p));
        }
        bus.advance_clocks(1).unwrap(); // DMA write and independent slave CPU read
        assert!(bus.take_cpu_transaction_completion(Processor::Slave));
        assert_eq!(bus.replay_cpu(Processor::Slave).read8(0xFFFFFE01), Ok(0xFF));
        assert!(!bus.take_cpu_transaction_completion(Processor::Master));
        bus.advance_clocks(1).unwrap(); // master read: determine cycle
        bus.onchip[0].write8(0xFFFFFE01, 0x67).unwrap();
        bus.advance_clocks(chunk).unwrap();
        assert!(bus.take_cpu_transaction_completion(Processor::Master));
        assert_eq!(
            bus.replay_cpu(Processor::Master).read8(0xFFFFFE01),
            Ok(0x67)
        );
        // Replay is stable after subsequent hardware changes.
        bus.onchip[0].write8(0xFFFFFE01, 0x89).unwrap();
        assert_eq!(
            bus.replay_cpu(Processor::Master).read8(0xFFFFFE01),
            Ok(0x67)
        );
    }
}

#[test]
fn cpu_internal_transactions_do_not_acquire_external_bus_and_cancel_by_owner() {
    for p in [Processor::Master, Processor::Slave] {
        let mut bus = setup(p, 0, 1, false);
        bus.onchip[p as usize].write32(DMAC + 0x30, 0).unwrap();
        let other = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        assert_eq!(
            bus.replay_cpu(other).tas_read8(DEST),
            Err(BusError::Pending)
        );
        bus.advance_clocks(2).unwrap();
        assert_eq!(bus.replay_cpu(other).tas_read8(DEST), Ok(0));
        assert_eq!(
            bus.replay_cpu(p).write8(0xFFFFFE01, 0x53),
            Err(BusError::Pending)
        );
        bus.grant_bus_transaction();
        // Canceling the DMA owner cannot discard the CPU's granted write.
        bus.cancel_dmac_transaction(p);
        bus.advance_clocks(1).unwrap();
        assert_eq!(bus.onchip[p as usize].read8(0xFFFFFE01), Ok(0x53));
        assert_eq!(bus.replay_cpu(p).write8(0xFFFFFE01, 0x53), Ok(()));
        bus.finish_cpu_transaction(p);
        for granted in [false, true] {
            assert_eq!(
                bus.replay_cpu(p).write8(0xFFFFFE01, 0x71),
                Err(BusError::Pending)
            );
            if granted {
                bus.grant_bus_transaction();
            }
            bus.cancel_cpu_transaction(p);
            bus.advance_clocks(8).unwrap();
            assert_eq!(bus.onchip[p as usize].read8(0xFFFFFE01), Ok(0x53));
        }
        assert_eq!(bus.replay_cpu(p).read32(SOURCE), Err(BusError::Pending));
        bus.advance_clocks(8).unwrap();
        assert!(!bus.take_cpu_transaction_completion(p));
        bus.finish_cpu_transaction(other);
        bus.advance_clocks(8).unwrap();
        assert_eq!(bus.replay_cpu(p).read32(SOURCE), Ok(0x11223344));
    }
}

#[test]
fn cpu_divu_interlock_keeps_irq_and_other_devices_on_real_clock_boundaries() {
    // H §§10.3.3/10.4: overflow publishes at clock six. A CPU result
    // read issued at clock zero cannot make its quotient or IRQ visible early.
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 2, 4] {
            let mut bus = setup(p, 0, 1, false);
            let i = p as usize;
            bus.onchip[i].write32(DMAC + 0x30, 0).unwrap();
            bus.onchip[i].write16(IPRA, 0xA000).unwrap();
            bus.onchip[i].write32(0xFFFFFF0C, 0x45).unwrap();
            bus.onchip[i].write32(0xFFFFFF08, 2).unwrap();
            bus.onchip[i].write32(0xFFFFFF00, 0).unwrap();
            bus.onchip[i].write32(0xFFFFFF04, 99).unwrap();
            assert_eq!(bus.replay_cpu(p).read32(0xFFFFFF04), Err(BusError::Pending));
            for _ in 0..5 {
                bus.advance_clocks(1).unwrap();
                assert_eq!(bus.onchip[i].interrupt(), None);
                assert!(!bus.take_cpu_transaction_completion(p));
            }
            bus.advance_clocks(1).unwrap();
            assert_eq!(bus.onchip[i].interrupt(), Some((10, 0x45)));
            assert!(!bus.take_cpu_transaction_completion(p));
            bus.advance_clocks(chunk).unwrap();
            if chunk == 1 {
                assert!(!bus.take_cpu_transaction_completion(p));
                bus.advance_clocks(1).unwrap();
            }
            assert!(bus.take_cpu_transaction_completion(p));
            assert_eq!(bus.replay_cpu(p).read32(0xFFFFFF04), Ok(799));
            assert_eq!(bus.onchip[i].take_wait_clocks(), 0); // not charged twice
        }
    }
}

#[test]
fn cpu_peripheral_byte_word_and_long_writes_have_one_completion_and_reads_have_two_states() {
    for p in [Processor::Master, Processor::Slave] {
        for (address, width, value, initial) in [
            (0xFFFFFE01, 1, 0x35, 0xFF),
            (IPRA, 2, 0x6500, 0x600),
            (0xFFFFFF40, 4, 0x12345678, 0),
        ] {
            let mut bus = setup(p, 0, 1, false);
            bus.onchip[p as usize].write32(DMAC + 0x30, 0).unwrap();
            let read = |bus: &mut Bus| {
                let memory = bus.replay_cpu(p);
                match width {
                    1 => memory.read8(address).map(u32::from),
                    2 => memory.read16(address).map(u32::from),
                    _ => memory.read32(address),
                }
            };
            let mut memory = bus.replay_cpu(p);
            let write = match width {
                1 => memory.write8(address, value as u8),
                2 => memory.write16(address, value as u16),
                _ => memory.write32(address, value),
            };
            assert_eq!(write, Err(BusError::Pending));
            let chip = &bus.onchip[p as usize];
            assert_eq!(
                match width {
                    1 => chip.read8(address).map(u32::from),
                    2 => chip.read16(address).map(u32::from),
                    _ => chip.read32(address),
                },
                Ok(initial)
            );
            bus.advance_clocks(1).unwrap();
            assert!(bus.take_cpu_transaction_completion(p));
            bus.finish_cpu_transaction(p);
            assert_eq!(read(&mut bus), Err(BusError::Pending));
            bus.advance_clocks(1).unwrap();
            assert!(!bus.take_cpu_transaction_completion(p));
            bus.advance_clocks(1).unwrap();
            assert!(bus.take_cpu_transaction_completion(p));
            assert_eq!(read(&mut bus), Ok(value));
        }
    }
}

#[test]
fn local_external_read_holds_internal_bus_until_completion_but_not_the_other_cpu_bus() {
    // H §7.11.2: an unbuffered read holds the requesting processor's
    // internal bus; the other SH7604 has a separate internal/peripheral bus.
    let mut bus = setup(Processor::Master, 0, 1, false);
    bus.onchip[0].write32(DMAC + 0x30, 0).unwrap();
    assert_eq!(
        bus.replay_cpu(Processor::Master).read32(SOURCE),
        Err(BusError::Pending)
    );
    bus.grant_bus_transaction();
    for (off, value) in [(0, 0xFFFFFE00), (4, 0xFFFFFE01), (12, 0x205), (0x30, 1)] {
        bus.onchip[0].write32(DMAC + off, value).unwrap();
    }
    bus.onchip[0].write8(0xFFFFFE00, 0x35).unwrap();
    assert_eq!(
        bus.replay_cpu(Processor::Slave).write8(0xFFFFFE01, 0x67),
        Err(BusError::Pending)
    );
    bus.advance_clocks(1).unwrap();
    assert_eq!(bus.onchip[1].read8(0xFFFFFE01), Ok(0x67));
    assert!(!bus.take_cpu_transaction_completion(Processor::Master));
    assert_eq!(bus.onchip[0].read8(0xFFFFFE01), Ok(0xFF));
    bus.advance_clocks(1).unwrap();
    assert!(bus.take_cpu_transaction_completion(Processor::Master));
    assert_eq!(
        bus.replay_cpu(Processor::Master).read32(SOURCE),
        Ok(0x11223344)
    );
    bus.advance_clocks(1).unwrap(); // DMA internal read
    assert_eq!(bus.onchip[0].read8(0xFFFFFE01), Ok(0xFF));
    bus.advance_clocks(1).unwrap(); // DMA internal write
    assert_eq!(bus.onchip[0].read8(0xFFFFFE01), Ok(0x35));
}

#[test]
fn external_dma_buffer_releases_local_internal_bus_while_external_burst_continues() {
    // H §7.11.2: a buffered destination does not retain the internal bus.
    // DMA wins simultaneous internal requests; CPU peripheral work follows
    // acceptance and overlaps external write(3), before next external read.
    for p in [Processor::Master, Processor::Slave] {
        let other = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        let mut bus = setup(p, 2, 2, true);
        assert_eq!(bus.replay_cpu(p).read8(0xFFFFFE01), Err(BusError::Pending));
        assert_eq!(
            bus.replay_cpu(other).read8(0xFFFFFE01),
            Err(BusError::Pending)
        );
        bus.advance_clocks(2).unwrap();
        assert!(bus.take_cpu_transaction_completion(other));
        assert!(!bus.take_cpu_transaction_completion(p));
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.advance_clocks(1).unwrap(); // write accepted, internal bus released
        assert!(!bus.take_cpu_transaction_completion(p));
        bus.advance_clocks(1).unwrap();
        assert!(!bus.take_cpu_transaction_completion(p));
        assert_eq!(bus.read32(DEST), Ok(0));
        bus.advance_clocks(1).unwrap(); // peripheral read + external write finish
        assert!(bus.take_cpu_transaction_completion(p));
        assert_eq!(bus.replay_cpu(p).read8(0xFFFFFE01), Ok(0xFF));
        assert_eq!(bus.read32(DEST), Ok(0x11223344));
        bus.advance_clocks(5).unwrap();
        assert_eq!(bus.read32(DEST + 4), Ok(0x11223345));
        assert_eq!(bus.onchip[p as usize].read32(DMAC + 8), Ok(0));
        assert_eq!(bus.onchip[p as usize].interrupt(), Some((6, 0x44)));
    }
}
