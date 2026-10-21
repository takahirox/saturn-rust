//! Independent SH7604 §§7.2.7/7.5.7/7.10/9.3.3 arbitration vectors.
//! Literal deadlines are main-clock bus states, not measured golden output.
use super::*;

const BSC: u32 = 0xFFFF_FFE0;
const DMAC: u32 = 0xFFFF_FF80;
const TARGET: u32 = HIGH_WRAM + 0x800;

fn board() -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    for p in [Processor::Master, Processor::Slave] {
        let chip = &mut b.onchip[p as usize];
        // SDRAM on CS2; CS3 ordinary, no Tw/IW. Normal refresh six states.
        chip.write32(BSC, 0xA55A_0004).unwrap();
        chip.write32(BSC + 8, 0xA55A_0000).unwrap();
        chip.write32(BSC + 12, 0xA55A_0048).unwrap();
    }
    b
}
fn timer(b: &mut Bus, p: Processor, compare: u32) {
    let chip = &mut b.onchip[p as usize];
    chip.write32(BSC + 24, 0xA55A_0000 | compare).unwrap();
    chip.write32(BSC + 20, 0xA55A_0000).unwrap();
    chip.write32(BSC + 16, 0xA55A_0008).unwrap(); // phi/4
}
fn stop(b: &mut Bus, p: Processor) {
    let chip = &mut b.onchip[p as usize];
    chip.read32(BSC + 16).unwrap();
    chip.write32(BSC + 16, 0xA55A_0000).unwrap(); // also acknowledges CMF
}
fn owner(b: &Bus, expected: Owner, remaining: u32) {
    let a = b.transactions.active.as_ref().expect("granted bus cycle");
    assert!(a.owner == expected);
    assert_eq!(a.remaining, remaining);
}
fn advance(b: &mut Bus, mut clocks: u32, chunk: u32) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        b.advance_clocks(n).unwrap();
        clocks -= n;
    }
}

#[test]
fn refresh_is_six_owned_states_before_cpu_dma_or_scu_and_local_io_still_runs() {
    for chunk in [1, 3, 7, 64] {
        for contender in 0..3 {
            let mut b = board();
            timer(&mut b, Processor::Master, 1);
            advance(&mut b, 4, chunk);
            stop(&mut b, Processor::Master);
            assert_eq!(b.onchip[0].read32(BSC + 16).unwrap() & 0x80, 0);
            // Clearing CMF / stopping the counter cannot cancel the request.
            match contender {
                0 => assert_eq!(
                    b.replay_cpu(Processor::Master).write32(TARGET, 42),
                    Err(BusError::Pending)
                ),
                1 => {
                    for (off, value) in [
                        (0, TARGET + 4),
                        (4, TARGET),
                        (8, 1),
                        (12, 0x5A05),
                        (0x30, 1),
                    ] {
                        b.onchip[0].write32(DMAC + off, value).unwrap();
                    }
                    b.write32(TARGET + 4, 42).unwrap();
                }
                _ => {
                    b.write32(TARGET + 4, 42).unwrap();
                    for (off, value) in [
                        (0, TARGET + 4),
                        (4, TARGET),
                        (8, 4),
                        (12, 0x102),
                        (20, 7),
                        (16, 0x101),
                    ] {
                        b.write32(SCU + off, value).unwrap();
                    }
                }
            }
            // Independent slave peripheral write completes during refresh.
            assert_eq!(
                b.replay_cpu(Processor::Slave).write8(0xFFFF_FE01, 0x5A),
                Err(BusError::Pending)
            );
            b.grant_bus_transaction();
            owner(&b, Owner::Refresh, 6);
            advance(&mut b, 5, chunk);
            assert_eq!(b.read32(TARGET), Ok(0));
            owner(&b, Owner::Refresh, 1);
            assert_eq!(b.onchip[1].read8(0xFFFF_FE01), Ok(0x5A));
            advance(&mut b, 1, chunk);
            assert!(b.transactions.active.is_none());
            // Refresh does not consume a CPU journal slot or complete its I/O.
            assert!(!b.take_cpu_transaction_completion(Processor::Master));
            advance(&mut b, 1, chunk);
            assert_eq!(b.read32(TARGET), Ok(0));
            advance(&mut b, 30, chunk);
            assert_eq!(b.read32(TARGET), Ok(42));
        }
    }
}

#[test]
fn refresh_waits_for_an_accepted_cycle_and_slave_compare_never_refreshes() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board();
        timer(&mut b, p, 1);
        // Ordinary CS3 with six Tw: accepted write completes at t8.
        b.onchip[0].write32(BSC, 0xA55A_0304).unwrap();
        b.onchip[0].write32(BSC + 8, 0xA55A_00C0).unwrap();
        assert_eq!(
            b.replay_cpu(Processor::Master).write32(TARGET, 9),
            Err(BusError::Pending)
        );
        b.advance_clocks(7).unwrap();
        assert_eq!(b.read32(TARGET), Ok(0));
        owner(&b, Owner::Cpu(Processor::Master), 1);
        b.advance_clocks(1).unwrap();
        assert_eq!(b.read32(TARGET), Ok(9));
        stop(&mut b, p);
        b.finish_cpu_transaction(Processor::Master);
        b.grant_bus_transaction();
        if p == Processor::Master {
            owner(&b, Owner::Refresh, 6); // both t4/t8 matches coalesce
            b.advance_clocks(6).unwrap();
            b.grant_bus_transaction();
        }
        assert!(b.transactions.active.is_none());
    }
}

#[test]
fn cache_fill_and_narrow_operands_retain_all_beats_before_refresh_or_a_peer() {
    for p in [Processor::Master, Processor::Slave] {
        for cache in [false, true] {
            let mut b = board();
            if cache {
                b.for_cpu(p).write8(CCR, 1).unwrap();
            } else {
                b.onchip[p as usize].write32(BSC + 4, 0xA55A_007C).unwrap(); // CS3 byte bus
            }
            b.write32(TARGET, 0x12345678).unwrap();
            timer(&mut b, Processor::Master, 1);
            let peer = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            // Choose the tested CPU first, independent of round-robin history.
            b.transactions.previous = Some(peer);
            let at = TARGET | if cache { 0 } else { 0x2000_0000 };
            assert_eq!(b.replay_cpu(p).read32(at), Err(BusError::Pending));
            assert_eq!(
                b.replay_cpu(peer).write32(TARGET, 0xDEADBEEF),
                Err(BusError::Pending)
            );
            for beat in 0..4 {
                b.grant_bus_transaction();
                owner(&b, Owner::Cpu(p), 2);
                b.advance_clocks(2).unwrap();
                if beat != 3 {
                    // Long host gaps cannot erase a pending refresh request
                    // or hand the retained operand to another bus master.
                    if beat == 1 {
                        stop(&mut b, Processor::Master);
                        b.advance_clocks(32).unwrap();
                    }
                    assert_eq!(b.replay_cpu(p).read32(at), Err(BusError::Pending));
                }
                assert_eq!(b.read32(TARGET), Ok(0x12345678));
            }
            assert_eq!(b.replay_cpu(p).read32(at), Ok(0x12345678));
            b.finish_cpu_transaction(p);
            b.grant_bus_transaction();
            owner(&b, Owner::Refresh, 6);
            b.advance_clocks(6).unwrap();
            b.advance_clocks(2).unwrap();
            assert_eq!(b.read32(TARGET), Ok(0xDEADBEEF));
        }
    }
}

#[test]
fn tas_and_burst_dma_defer_refresh_without_losing_its_request() {
    for burst in [false, true] {
        let mut b = board();
        timer(&mut b, Processor::Master, 1);
        if burst {
            b.write32(TARGET, 0x11223344).unwrap();
            b.write32(TARGET + 4, 0x55667788).unwrap();
            for (off, value) in [
                (0, TARGET),
                (4, TARGET + 16),
                (8, 2),
                (12, 0x5A15),
                (0x30, 1),
            ] {
                b.onchip[0].write32(DMAC + off, value).unwrap();
            }
            b.advance_clocks(9).unwrap(); // read2, write3, read2, write2
            stop(&mut b, Processor::Master);
            assert_eq!(b.read32(TARGET + 16), Ok(0x11223344));
            assert_eq!(b.read32(TARGET + 20), Ok(0));
            b.advance_clocks(1).unwrap(); // second write (read/write turnaround)
            assert_eq!(b.read32(TARGET + 20), Ok(0x55667788));
        } else {
            let at = TARGET | 0x2000_0000;
            assert_eq!(
                b.replay_cpu(Processor::Master).tas_read8(at),
                Err(BusError::Pending)
            );
            b.advance_clocks(4).unwrap();
            stop(&mut b, Processor::Master);
            let mut mem = b.replay_cpu(Processor::Master);
            assert_eq!(mem.tas_read8(at), Ok(0));
            assert_eq!(mem.write8(at, 0x80), Err(BusError::Pending));
            b.advance_clocks(2).unwrap(); // idle time since read covers turnaround
            assert_eq!(b.read8(at), Ok(0x80));
            b.finish_cpu_transaction(Processor::Master);
        }
        b.grant_bus_transaction();
        owner(&b, Owner::Refresh, 6);
        b.advance_clocks(6).unwrap();
        b.grant_bus_transaction();
        assert!(b.transactions.active.is_none());
    }
}

#[test]
fn canceling_a_partial_group_releases_it_but_does_not_cancel_refresh() {
    let mut b = board();
    b.onchip[1].write32(BSC + 4, 0xA55A_007C).unwrap();
    timer(&mut b, Processor::Master, 1);
    assert_eq!(
        b.replay_cpu(Processor::Slave).write32(TARGET, 0xAABBCCDD),
        Err(BusError::Pending)
    );
    b.advance_clocks(4).unwrap();
    stop(&mut b, Processor::Master);
    assert_eq!(b.read32(TARGET), Ok(0xAA000000));
    b.set_slave_enabled(false);
    b.grant_bus_transaction();
    owner(&b, Owner::Refresh, 6);
    b.cancel_cpu_transaction(Processor::Master);
    b.cancel_dmac_transaction(Processor::Master);
    b.advance_clocks(64).unwrap();
    assert_eq!(b.read32(TARGET), Ok(0xAA000000));
    assert!(b.transactions.active.is_none());
}

#[test]
fn self_refresh_holds_peer_bus_requests_while_cache_and_local_exit_execute() {
    for (mode, recovery) in [(4, 3), (3, 5)] {
        for chunk in [1, 2, 17, 128] {
            let mut b = board();
            b.onchip[0].write32(BSC, 0xA55A_0000 | mode).unwrap();
            b.onchip[0].write32(BSC + 12, 0xA55A_004C).unwrap();
            b.for_cpu(Processor::Master).write8(CCR, 9).unwrap();
            b.for_cpu(Processor::Master)
                .write32(0xC0000400, 0x12345678)
                .unwrap();
            assert_eq!(
                b.replay_cpu(Processor::Slave).write32(LOW_WRAM, 42),
                Err(BusError::Pending)
            );
            advance(&mut b, 128, chunk);
            assert!(b.transactions.active.is_none());
            assert_eq!(b.read32(LOW_WRAM), Ok(0));
            // Cache/RAM-array hit needs no external bus, even in self-refresh.
            assert_eq!(
                b.replay_cpu(Processor::Master).read32(0xC0000400),
                Ok(0x12345678)
            );
            b.finish_cpu_transaction(Processor::Master);
            assert_eq!(
                b.replay_cpu(Processor::Master)
                    .write32(BSC + 12, 0xA55A_0048),
                Err(BusError::Pending)
            );
            advance(&mut b, 1, chunk); // local peripheral write clears RMODE
            assert!(b.onchip[0].inhibits_external_bus());
            advance(&mut b, recovery, chunk);
            assert!(!b.onchip[0].inhibits_external_bus());
            assert_eq!(b.read32(LOW_WRAM), Ok(0));
            advance(&mut b, 1, chunk);
            assert_eq!(b.read32(LOW_WRAM), Ok(0));
            advance(&mut b, 1, chunk);
            assert_eq!(b.read32(LOW_WRAM), Ok(42));
        }
    }
}

#[test]
fn entering_self_refresh_does_not_cancel_an_already_accepted_bus_cycle() {
    let mut b = board();
    assert_eq!(
        b.replay_cpu(Processor::Slave).write32(LOW_WRAM, 42),
        Err(BusError::Pending)
    );
    b.advance_clocks(1).unwrap();
    b.onchip[0].write32(BSC + 12, 0xA55A_004C).unwrap();
    b.advance_clocks(1).unwrap();
    assert_eq!(b.read32(LOW_WRAM), Ok(42));
    b.finish_cpu_transaction(Processor::Slave);
    assert_eq!(
        b.replay_cpu(Processor::Slave).write32(LOW_WRAM, 99),
        Err(BusError::Pending)
    );
    b.advance_clocks(128).unwrap();
    assert_eq!(b.read32(LOW_WRAM), Ok(42));
    b.set_slave_enabled(false);
    assert!(b.onchip[0].inhibits_external_bus());
}
