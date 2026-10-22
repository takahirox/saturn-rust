//! SH7604 §§6.3.4/6.3.7/7.10: the external UBC monitor uses the
//! internal bus while a different master owns the shared external bus.
use super::*;
const BRCR: u32 = 0xFFFF_FF78;
const SCI: u32 = 0xFFFF_FE00;
const DMAC: u32 = 0xFFFF_FF80;

fn board(p: Processor, control: u16) -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.onchip[p as usize].write16(BRCR, control).unwrap();
    b.onchip[p as usize].read16(BRCR).unwrap(); // read-back barrier
    b
}
fn peer(p: Processor) -> Processor {
    if p == Processor::Master {
        Processor::Slave
    } else {
        Processor::Master
    }
}
fn peripheral(b: &mut Bus, p: Processor, dma: bool) {
    if dma {
        for (off, value) in [(0, SCI + 1), (4, SCI + 3), (8, 1), (12, 0x205), (0x30, 1)] {
            b.onchip[p as usize].write32(DMAC + off, value).unwrap();
        }
    } else {
        assert_eq!(b.replay_cpu(p).read8(SCI + 1), Err(BusError::Pending));
    }
}
fn completed(b: &mut Bus, p: Processor, dma: bool) -> bool {
    if dma {
        b.onchip[p as usize].read32(DMAC + 8).unwrap() == 0
    } else {
        b.take_cpu_transaction_completion(p)
    }
}

#[test]
fn external_monitor_stalls_cpu_and_dma_peripherals_only_in_active_sh2_ebbe_mode() {
    for p in [Processor::Master, Processor::Slave] {
        for control in [0, 0x1000, 0x2000, 0x3000] {
            for dma in [false, true] {
                for external in [0, 1, 2] {
                    // peer CPU, peer DMAC, SCU
                    let mut b = board(p, control);
                    match external {
                        0 => assert_eq!(
                            b.replay_cpu(peer(p)).write32(HIGH_WRAM, 42),
                            Err(BusError::Pending)
                        ),
                        1 => {
                            b.write32(HIGH_WRAM + 4, 42).unwrap();
                            for (off, value) in [
                                (0, HIGH_WRAM + 4),
                                (4, HIGH_WRAM),
                                (8, 1),
                                (12, 0xA05),
                                (0x30, 1),
                            ] {
                                b.onchip[peer(p) as usize]
                                    .write32(DMAC + off, value)
                                    .unwrap();
                            }
                        }
                        _ => {
                            b.write32(HIGH_WRAM + 4, 42).unwrap();
                            for (off, value) in [
                                (0, HIGH_WRAM + 4),
                                (4, HIGH_WRAM),
                                (8, 4),
                                (12, 0x102),
                                (20, 7),
                                (16, 0x101),
                            ] {
                                b.write32(SCU + off, value).unwrap();
                            }
                            b.advance_clocks(2).unwrap(); // setup, no bus pins
                        }
                    }
                    b.grant_bus_transaction();
                    assert!(b.transactions.active.is_some());
                    peripheral(&mut b, p, dma);
                    b.advance_clocks(2).unwrap();
                    assert_eq!(completed(&mut b, p, dma), control != 0x3000);
                    b.advance_clocks(100).unwrap();
                    // The monitor delays an access, never drops it.
                    if control == 0x3000 || dma {
                        assert!(completed(&mut b, p, dma));
                    }
                    if dma {
                        assert_eq!(b.onchip[p as usize].read8(SCI + 3), Ok(0xFF));
                    }
                }
            }
        }
    }
}

#[test]
fn an_accepted_peripheral_phase_finishes_before_bus_handoff_to_external_monitoring() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, 0x3000);
        peripheral(&mut b, p, false);
        b.grant_bus_transaction(); // CPU read has two internal states
        assert_eq!(
            b.replay_cpu(peer(p)).write32(HIGH_WRAM, 42),
            Err(BusError::Pending)
        );
        b.advance_clocks(2).unwrap();
        assert!(b.take_cpu_transaction_completion(p));
        assert!(b.transactions.active.is_none());
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        b.advance_clocks(7).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        b.advance_clocks(1).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(42));
    }
}

#[test]
fn monitor_uses_synchronized_conditions_and_partial_share_master_does_not_monitor() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, 0);
        b.onchip[p as usize].write16(BRCR, 0x3000).unwrap();
        assert!(!b.onchip[p as usize].monitors_external_bus());
        b.advance_clocks(2).unwrap();
        assert!(!b.onchip[p as usize].monitors_external_bus());
        b.advance_clocks(1).unwrap();
        assert!(b.onchip[p as usize].monitors_external_bus());
        b.onchip[p as usize]
            .write32(0xFFFFFFE0, 0xA55A_0400)
            .unwrap();
        assert_eq!(
            b.onchip[p as usize].monitors_external_bus(),
            p == Processor::Slave
        );
        b.onchip[p as usize].write16(BRCR, 0x1000).unwrap();
        b.onchip[p as usize].read16(BRCR).unwrap();
        assert!(!b.onchip[p as usize].monitors_external_bus());
    }
}

#[test]
fn retained_peer_group_reserves_monitor_bus_between_beats_and_cancel_releases_it() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, 0x3000);
        b.onchip[peer(p) as usize]
            .write32(0xFFFFFFE4, 0xA55A_007C)
            .unwrap();
        assert_eq!(
            b.replay_cpu(peer(p)).read32(HIGH_WRAM),
            Err(BusError::Pending)
        );
        b.advance_clocks(8).unwrap(); // first byte completed, group retained
        assert!(b.transactions.active.is_none());
        peripheral(&mut b, p, false);
        b.advance_clocks(32).unwrap();
        assert!(!b.take_cpu_transaction_completion(p));
        b.cancel_cpu_transaction(peer(p));
        b.advance_clocks(2).unwrap();
        assert!(b.take_cpu_transaction_completion(p));
    }
}
