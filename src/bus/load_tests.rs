//! Executing MOV loads through the production journal/arbiter.
use super::*;
use crate::cpu::{Cpu, CpuError};

fn board(p: Processor, address: u32) -> (Bus, Cpu) {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.write16(HIGH_WRAM, 0x60F6).unwrap(); // MOV.L @R15+,R0
    b.for_cpu(p).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
    b.for_cpu(p).write8(CCR, 0x11).unwrap();
    b.for_cpu(p).fetch16(HIGH_WRAM).unwrap();
    b.cpu_delay(p, 0);
    b.advance_clocks(32).unwrap();
    let mut c = Cpu::start(HIGH_WRAM, address);
    c.prime_fetch(&b.for_cpu(p));
    (b, c)
}
fn attempt(b: &mut Bus, c: &mut Cpu, p: Processor) -> bool {
    let mut next = c.clone();
    match next.step(&mut b.replay_cpu(p)) {
        Ok(()) => {
            *c = next;
            b.finish_cpu_transaction(p);
            true
        }
        Err(CpuError::Bus {
            source: BusError::Pending,
            ..
        }) => false,
        other => panic!("unexpected MOV result {other:?}"),
    }
}

#[test]
fn executing_mov_refill_retains_its_early_beats_across_chunks_and_passive_inspection() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let address = HIGH_WRAM + 0x2400;
                let (mut b, mut c) = board(p, address);
                b.write32(address, 3).unwrap();
                b.write32(address + 4, 0x123).unwrap();
                let mut grants = Vec::new();
                let mut last = None;
                while b.main_clocks < 50 {
                    if attempt(&mut b, &mut c, p) {
                        assert_eq!(c.state().r[0], 7);
                        break;
                    }
                    b.grant_bus_transaction();
                    if let Some(a) = &b.transactions.active {
                        let at = (b.main_clocks, a.transfer.address);
                        if last != Some(a.transfer.address) {
                            grants.push(at);
                            last = Some(a.transfer.address);
                        }
                    }
                    if b.main_clocks == 40 {
                        b.write32(address, 7).unwrap();
                    }
                    if b.main_clocks >= 35 {
                        b.write32(address + 4, 0x456).unwrap();
                        assert_eq!(
                            b.transactions.journals[p as usize].borrow().beats[0].1,
                            Ok(0x123)
                        );
                    }
                    if inspect {
                        assert_eq!(b.debug_read32(address), b.read32(address));
                        let _ = b.debug_read16(0x05818000);
                    }
                    if c.state().instructions != 0 {
                        break;
                    }
                    // Service pipeline/refill boundaries even for a large outer chunk.
                    let n = chunk
                        .min(if b.main_clocks < 40 {
                            (40 - b.main_clocks) as u32
                        } else {
                            1
                        })
                        .min(b.bus_transaction_deadline())
                        .min(50 - b.main_clocks as u32)
                        .max(1);
                    b.advance_clocks(n).unwrap();
                }
                assert_eq!(c.state().instructions, 1);
                assert_eq!(c.state().r[0], 7);
                assert_eq!(c.state().r[15], address + 4);
                assert_eq!(
                    grants,
                    [
                        (33, address + 4),
                        (35, address + 8),
                        (37, address + 12),
                        (39, address)
                    ]
                );
                assert_eq!(b.main_clocks, 41);
                // Fixed horizon, including post-completion inspection and idling.
                while b.main_clocks < 50 {
                    if inspect {
                        assert_eq!(b.debug_read32(address), Ok(7));
                    }
                    b.advance_clocks(chunk.min((50 - b.main_clocks) as u32))
                        .unwrap();
                }
                assert_eq!(
                    (b.main_clocks, c.state().instructions, c.state().r[0]),
                    (50, 1, 7)
                );
            }
        }
    }
}

#[test]
fn executing_mov_keeps_existing_peer_dmac_and_scu_bus_priority() {
    for (competitor, p) in [0, 1, 2]
        .into_iter()
        .flat_map(|k| [Processor::Master, Processor::Slave].map(|p| (k, p)))
    {
        let other = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        let address = 0x26002400;
        let (mut b, mut c) = board(p, address);
        b.write32(address, 3).unwrap();
        b.write32(address + 4, 0x55667788).unwrap();
        let mut peer = Cpu::start(HIGH_WRAM, address + 4);
        if competitor == 0 {
            b.for_cpu(other).write8(CCR, 0x11).unwrap();
            b.for_cpu(other).fetch16(HIGH_WRAM).unwrap();
            peer.prime_fetch(&b.for_cpu(other));
            b.cpu_delay(other, 0);
            assert!(!attempt(&mut b, &mut peer, other));
        }
        assert!(!attempt(&mut b, &mut c, p));
        b.advance_clocks(1).unwrap(); // EX completed; requests meet at MA33.
        assert!(!attempt(&mut b, &mut c, p));
        b.write32(HIGH_WRAM + 0x3000, 0x11223344).unwrap();
        b.for_cpu(other).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
        match competitor {
            0 => assert!(!attempt(&mut b, &mut peer, other)),
            1 => {
                for (off, v) in [
                    (0, HIGH_WRAM + 0x3000),
                    (4, HIGH_WRAM + 0x4000),
                    (8, 1),
                    (12, 0xA05),
                    (0x30, 1),
                ] {
                    b.for_cpu(other)
                        .write32(crate::onchip::DMAC + off, v)
                        .unwrap();
                }
            }
            _ => {
                for (off, v) in [
                    (0, HIGH_WRAM + 0x3000),
                    (4, HIGH_WRAM + 0x4000),
                    (8, 4),
                    (12, 0x102),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(SCU + off, v).unwrap();
                }
            }
        }
        b.grant_bus_transaction();
        let a = b.transactions.active.as_ref().unwrap();
        assert_eq!(
            a.transfer.address,
            match competitor {
                0 =>
                    if p == Processor::Master {
                        address
                    } else {
                        address + 4
                    },
                1 => HIGH_WRAM + 0x3000,
                _ => 0,
            }
        );
        assert!(match competitor {
            0 => a.owner == Owner::Cpu(Processor::Master),
            1 => a.owner == Owner::Dmac(other),
            _ => a.owner == Owner::Scu,
        });
        let mut sample = None;
        while b.main_clocks < 80 {
            if attempt(&mut b, &mut c, p) {
                sample = Some(b.main_clocks);
                break;
            }
            b.grant_bus_transaction();
            b.advance_clocks(1).unwrap();
        }
        assert_eq!(
            sample,
            Some(match competitor {
                0 =>
                    if p == Processor::Master {
                        35
                    } else {
                        37
                    },
                1 => 40,
                _ => 42,
            })
        );
        assert_eq!(c.state().r[0], 3);
        assert_eq!(c.state().r[15], address + 4);
        b.advance_clocks((80 - b.main_clocks) as u32).unwrap();
        if competitor == 0 {
            assert!(attempt(&mut b, &mut peer, other));
            assert_eq!(
                (peer.state().r[0], peer.state().r[15]),
                (0x55667788, address + 8)
            );
        } else {
            assert_eq!(b.read32(HIGH_WRAM + 0x4000), Ok(0x11223344));
        }
        assert!(
            b.transactions.journals[p as usize]
                .borrow()
                .operations
                .is_empty()
        );
    }
}

#[test]
fn executing_mov_fifo_effect_is_retained_on_replay_and_cancelled_load_reset() {
    for p in [Processor::Master, Processor::Slave] {
        for reset in [
            None,
            Some(crate::onchip::ResetRequest::PowerOn),
            Some(crate::onchip::ResetRequest::Manual),
        ] {
            let cancel = reset.is_some();
            let (mut b, _) = board(p, crate::cd::DATA);
            b.cd.mount(
                crate::media::Media::iso(include_bytes!("../../web/star-collector.iso")).unwrap(),
            );
            for (off, value) in [(0x18, 0x0200), (0x1C, 0), (0x20, 0), (0x24, 0)] {
                b.cd.write16(crate::cd::REGISTERS + off, value).unwrap();
            }
            b.advance_clocks(1075).unwrap();
            b.for_cpu(p).write16(HIGH_WRAM, 0x60F5).unwrap(); // MOV.W @R15+,R0
            b.cpu_delay(p, 0);
            let mut c = Cpu::start(HIGH_WRAM, crate::cd::DATA | 0x20000000);
            c.prime_fetch(&b.for_cpu(p));
            let mut completed = false;
            for _ in 0..64 {
                let mut next = c.clone();
                match next.step(&mut b.replay_cpu(p)) {
                    Ok(()) => {
                        assert_eq!(next.state().r[0], 0x4100);
                        assert_eq!(b.debug_read16(crate::cd::DATA), Ok(150));
                        let mut replay = c.clone();
                        replay.step(&mut b.replay_cpu(p)).unwrap();
                        assert_eq!(replay.state(), next.state());
                        assert_eq!(b.debug_read16(crate::cd::DATA), Ok(150));
                        if let Some(reset) = reset {
                            // Reset can cancel the boundary after hardware sampling,
                            // before architectural replay. Completed I/O stays done.
                            b.onchip[p as usize].reset_from_watchdog(reset);
                            b.hold_cpu_reset(p);
                        } else {
                            c = next;
                            b.finish_cpu_transaction(p);
                        }
                        completed = true;
                        break;
                    }
                    Err(CpuError::Bus {
                        source: BusError::Pending,
                        ..
                    }) => (),
                    other => panic!("unexpected MOV result {other:?}"),
                }
                b.grant_bus_transaction();
                b.advance_clocks(1).unwrap();
            }
            assert!(completed);
            b.advance_clocks(32).unwrap();
            assert_eq!(c.state().r[0], if cancel { 0 } else { 0x4100 });
            assert_eq!(
                c.state().r[15],
                (crate::cd::DATA | 0x20000000) + if cancel { 0 } else { 2 }
            );
            assert_eq!(b.cd.read16(crate::cd::DATA), Ok(150));
        }
    }
}
