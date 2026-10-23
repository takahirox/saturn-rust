//! H §§4.2.2–3/7.11.1: manual reset finishes the current external unit;
//! power-on reset aborts it. Expectations follow literal bus beats and clocks.
use super::*;
use crate::onchip::{ResetRequest, WDT};

fn board(p: Processor, select: u32) -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.for_cpu(p)
        .write32(0xFFFFFFE4, 0xA55A003C | select << 6)
        .unwrap();
    b
}
fn arm(b: &mut Bus, p: Processor, manual: bool, ticks: u8) {
    let chip = &mut b.onchip[p as usize];
    chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
        .unwrap();
    chip.write16(WDT, 0xA578).unwrap();
    chip.write16(WDT, 0x5A00 | u16::from(0u8.wrapping_sub(ticks)))
        .unwrap();
}

#[test]
fn manual_reset_finishes_buffered_operand_but_power_reset_keeps_only_completed_lanes() {
    for p in [Processor::Master, Processor::Slave] {
        for (select, width) in [(1, 1), (2, 2), (3, 4)] {
            for manual in [false, true] {
                for assertion in [2, 8, 10, 16, 24] {
                    if usize::from(assertion) > 8 * 4 / width {
                        continue;
                    }
                    for chunk in [1, 3, 19, 600] {
                        let mut b = board(p, select);
                        let address = HIGH_WRAM | 0x20000000;
                        assert_eq!(
                            b.replay_cpu(p).write32(address, 0x12345678),
                            Err(BusError::Pending)
                        );
                        arm(&mut b, p, manual, assertion / 2);
                        b.advance_clocks(1).unwrap();
                        assert_eq!(b.replay_cpu(p).write32(address, 0x12345678), Ok(()));
                        b.finish_cpu_transaction(p);
                        // This second operand has not reached the BSC buffer.
                        assert_eq!(
                            b.replay_cpu(p).write32(address + 4, 0xDEADBEEF),
                            Err(BusError::Pending)
                        );
                        let mut elapsed = 1;
                        while elapsed < 600 {
                            let n = chunk.min(600 - elapsed);
                            b.advance_clocks(n).unwrap();
                            elapsed += n;
                        }
                        let complete_bytes = if manual {
                            4
                        } else {
                            (usize::from(assertion) / 8 * width).min(4)
                        };
                        let mask = u32::MAX
                            .checked_shl((8 * (4 - complete_bytes)) as u32)
                            .unwrap_or(0);
                        assert_eq!(
                            b.read32(address),
                            Ok(0x12345678 & mask),
                            "{p:?} width{width} manual{manual} t{assertion} chunk{chunk}"
                        );
                        assert_eq!(b.read32(address + 4), Ok(0));
                        assert_eq!(
                            b.take_cpu_reset(p),
                            Some(if manual {
                                ResetRequest::Manual
                            } else {
                                ResetRequest::PowerOn
                            })
                        );
                        assert_eq!(b.take_cpu_reset(p), None);
                        assert!(!b.cpu_reset_active(p));
                        assert_eq!(
                            b.for_cpu(p).read32(0xFFFFFFE4),
                            Ok(if manual { 0x3C | select << 6 } else { 0xFC })
                        );
                        assert!(b.transactions.active.is_none());
                        assert!(b.transactions.reset_drains[p as usize].is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn manual_reset_stops_cache_fill_at_current_longword_and_never_installs_partial_data() {
    for p in [Processor::Master, Processor::Slave] {
        for (select, width) in [(1, 1), (2, 2), (3, 4)] {
            for assertion in [2, 8] {
                let mut b = board(p, select);
                b.for_cpu(p).write8(CCR, 0x11).unwrap();
                assert_eq!(b.replay_cpu(p).read32(HIGH_WRAM), Err(BusError::Pending));
                arm(&mut b, p, true, assertion / 2);
                let mut trace = Vec::new();
                for clock in 0..550 {
                    b.grant_bus_transaction();
                    if let Some(a) = &b.transactions.active
                        && a.remaining == 1
                    {
                        trace.push((clock + 1, a.transfer.address, a.transfer.width));
                    }
                    b.advance_clocks(1).unwrap();
                }
                let expected: Vec<_> = (0..4 / width)
                    .map(|i| (8 * (i + 1), HIGH_WRAM + 4 + (i * width) as u32, width))
                    .collect();
                assert_eq!(trace, expected, "{p:?} width{width} assertion{assertion}");
                assert_eq!(b.take_cpu_reset(p), Some(ResetRequest::Manual));
                assert_eq!(b.for_cpu(p).read8(CCR), Ok(0));
                for way in 0..4 {
                    b.for_cpu(p).write8(CCR, way << 6).unwrap();
                    assert_eq!(b.for_cpu(p).read32(0x60000000).unwrap() & 4, 0);
                }
                assert!(b.transactions.cpu_group.is_none());
                assert!(b.transactions.lock.is_none());
            }
        }
    }
}

#[test]
fn manual_reset_during_narrow_word_read_stops_at_word_end_instead_of_longword_end() {
    for p in [Processor::Master, Processor::Slave] {
        for offset in [0, 2] {
            let mut b = board(p, 1);
            assert_eq!(
                b.replay_cpu(p).read16((HIGH_WRAM | 0x20000000) + offset),
                Err(BusError::Pending)
            );
            arm(&mut b, p, true, 1);
            let mut addresses = Vec::new();
            for _ in 0..40 {
                b.grant_bus_transaction();
                if let Some(a) = &b.transactions.active
                    && a.remaining == 1
                {
                    addresses.push(a.transfer.address);
                }
                b.advance_clocks(1).unwrap();
            }
            assert_eq!(
                addresses,
                [
                    (HIGH_WRAM | 0x20000000) + offset,
                    (HIGH_WRAM | 0x20000000) + offset + 1
                ]
            );
        }
    }
}

#[test]
fn reset_cancels_unaccepted_buffer_and_system_reset_cancels_manual_drain() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, 1);
        arm(&mut b, p, true, 1);
        b.advance_clocks(2).unwrap();
        assert_eq!(
            b.replay_cpu(p).write32(HIGH_WRAM | 0x20000000, 0x12345678),
            Err(BusError::Pending)
        );
        b.advance_clocks(100).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        assert!(b.transactions.writes[p as usize].is_none());

        let mut b = board(p, 1);
        assert_eq!(
            b.replay_cpu(p).write32(HIGH_WRAM | 0x20000000, 0x12345678),
            Err(BusError::Pending)
        );
        arm(&mut b, p, true, 1);
        b.advance_clocks(2).unwrap();
        assert!(
            matches!(b.transactions.active.as_ref().unwrap().owner, Owner::ResetDrain(owner) if owner == p)
        );
        b.system_reset();
        b.advance_clocks(600).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        assert!(b.transactions.active.is_none());
        assert_eq!(b.take_cpu_reset(p), None);
    }
}

#[test]
fn slow_video_cycle_holds_manual_reset_past_watchdog_pulse_without_stopping_peer_peripherals() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p, 3);
        b.write16(VDP2, 0x8000).unwrap();
        b.write16(VDP2 + 0x20, 1).unwrap();
        b.write32(VDP2 + 0x10, 0).unwrap(); // no CPU A-bank grants until HBlank
        assert_eq!(
            b.replay_cpu(p).read16(VRAM | 0x20000000),
            Err(BusError::Pending)
        );
        arm(&mut b, p, true, 1);
        b.grant_bus_transaction();
        // Ordinary bus base2 + 1280 blanking wait (ST-058 §3.3).
        assert_eq!(b.transactions.active.as_ref().unwrap().remaining, 1282);
        b.advance_clocks(514).unwrap();
        assert!(b.cpu_reset_active(p));
        assert_eq!(b.take_cpu_reset(p), None); // pulse ended, bus still draining
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        assert_eq!(b.replay_cpu(peer).read8(0xFFFFFE01), Err(BusError::Pending));
        b.advance_clocks(2).unwrap();
        assert_eq!(b.replay_cpu(peer).read8(0xFFFFFE01), Ok(0xFF));
        b.finish_cpu_transaction(peer);
        b.advance_clocks(765).unwrap();
        assert_eq!(b.take_cpu_reset(p), None);
        b.advance_clocks(1).unwrap();
        // ST-210 item32: this narrow read drains its second B-bus word
        // after the blanking-delayed first word. Reset BSC timing adds
        // T1/T2 plus six Tw for that newly granted physical phase.
        assert_eq!(b.take_cpu_reset(p), None);
        b.advance_clocks(8).unwrap();
        assert_eq!(b.take_cpu_reset(p), Some(ResetRequest::Manual));
        assert!(b.transactions.active.is_none());
    }
}

#[test]
fn reset_drain_preserves_cpu_and_exception_diagnostic_provenance() {
    for p in [Processor::Master, Processor::Slave] {
        for vector in [None, Some(0x41)] {
            let mut b = board(p, 3);
            let address = crate::scu::SCU + 0xCC;
            {
                let mut memory = b.replay_cpu(p);
                memory.sh2_write_context(0x26004002, vector);
                assert_eq!(memory.write8(address, 1), Err(BusError::Pending));
            }
            arm(&mut b, p, true, 1);
            b.advance_clocks(514).unwrap();
            assert_eq!(b.take_cpu_reset(p), None); // report the diagnostic first
            let source = BusError::UnsupportedDevice { address, width: 1 };
            assert_eq!(
                b.take_cpu_write_error(p),
                Some(match vector {
                    Some(vector) => crate::cpu::CpuError::ExceptionEntry {
                        pc: 0x26004002,
                        vector,
                        source
                    },
                    None => crate::cpu::CpuError::Bus {
                        pc: 0x26004002,
                        source
                    },
                })
            );
            assert_eq!(b.take_cpu_reset(p), Some(ResetRequest::Manual));
        }
    }
}

#[test]
fn manual_reset_finishes_only_current_dma_longword_without_resuming_sixteen_byte_request() {
    use crate::onchip::DMAC;
    for p in [Processor::Master, Processor::Slave] {
        for (select, width) in [(1, 1), (2, 2), (3, 4)] {
            let mut b = board(p, select);
            for (offset, value) in [
                (0, HIGH_WRAM + 0x100),
                (4, HIGH_WRAM + 0x200),
                (8, 4),
                (12, 0x5E15),
                (0x30, 1),
            ] {
                b.onchip[p as usize].write32(DMAC + offset, value).unwrap();
            }
            arm(&mut b, p, true, 1);
            let mut trace = Vec::new();
            for clock in 0..550 {
                b.grant_bus_transaction();
                if let Some(a) = &b.transactions.active
                    && a.remaining == 1
                {
                    trace.push((clock + 1, a.transfer.address, a.transfer.width));
                }
                b.advance_clocks(1).unwrap();
            }
            let expected: Vec<_> = (0..4 / width)
                .map(|i| (8 * (i + 1), HIGH_WRAM + 0x100 + (i * width) as u32, width))
                .collect();
            assert_eq!(trace, expected);
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
            assert_eq!(b.take_cpu_reset(p), Some(ResetRequest::Manual));
            assert_eq!(b.onchip[p as usize].read32(DMAC + 12), Ok(0));
            assert_eq!(b.onchip[p as usize].interrupt(), None);
            assert!(b.transactions.dmac_owner.is_none());
        }
    }
}

#[test]
fn reset_assertion_initializes_modules_and_power_reset_releases_self_refresh_before_pulse_ends() {
    for p in [Processor::Master, Processor::Slave] {
        for manual in [true, false] {
            let mut b = board(p, 3);
            b.for_cpu(p).write8(CCR, 1).unwrap();
            b.for_cpu(p).write8(0xFFFFFE01, 0x31).unwrap();
            b.for_cpu(p).write32(0xFFFFFF40, 0x06003000).unwrap();
            arm(&mut b, p, manual, 1);
            b.advance_clocks(2).unwrap();
            assert!(b.cpu_reset_active(p));
            assert_eq!(b.for_cpu(p).read8(CCR), Ok(0));
            assert_eq!(b.for_cpu(p).read8(0xFFFFFE01), Ok(0xFF));
            assert_eq!(
                b.for_cpu(p).read32(0xFFFFFF40),
                Ok(if manual { 0x06003000 } else { 0 })
            );
            b.advance_clocks(511).unwrap();
            assert_eq!(b.take_cpu_reset(p), None);
            b.advance_clocks(1).unwrap();
            assert_eq!(
                b.take_cpu_reset(p),
                Some(if manual {
                    ResetRequest::Manual
                } else {
                    ResetRequest::PowerOn
                })
            );
        }
    }
    for manual in [true, false] {
        let p = Processor::Master;
        let mut b = board(p, 3);
        b.for_cpu(p).write32(0xFFFFFFE0, 0xA55A0001).unwrap();
        b.for_cpu(p).write32(0xFFFFFFEC, 0xA55A004C).unwrap();
        assert_eq!(
            b.replay_cpu(Processor::Slave)
                .read32(HIGH_WRAM | 0x20000000),
            Err(BusError::Pending)
        );
        arm(&mut b, p, manual, 1);
        b.advance_clocks(2).unwrap();
        assert!(b.cpu_reset_active(p));
        b.advance_clocks(8).unwrap();
        assert_eq!(b.take_cpu_transaction_completion(Processor::Slave), !manual);
        if manual {
            assert!(b.transactions.active.is_none());
        } else {
            assert_eq!(
                b.replay_cpu(Processor::Slave)
                    .read32(HIGH_WRAM | 0x20000000),
                Ok(0)
            );
        }
    }
}

#[test]
fn retained_manual_reset_buffer_cannot_break_the_other_processors_tas_lock() {
    for p in [Processor::Master, Processor::Slave] {
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        let mut b = board(p, 1);
        let lock = (HIGH_WRAM | 0x20000000) + 0x100;
        assert_eq!(b.replay_cpu(peer).tas_read8(lock), Err(BusError::Pending));
        b.advance_clocks(8).unwrap();
        assert_eq!(b.replay_cpu(peer).tas_read8(lock), Ok(0));
        assert_eq!(
            b.replay_cpu(p).write32(HIGH_WRAM | 0x20000000, 0x12345678),
            Err(BusError::Pending)
        );
        b.advance_clocks(1).unwrap(); // buffer admitted; peer still owns TAS
        arm(&mut b, p, true, 1);
        b.advance_clocks(30).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        assert_eq!(b.transactions.lock, Some(peer));
        {
            let mut memory = b.replay_cpu(peer);
            assert_eq!(memory.tas_read8(lock), Ok(0));
            assert_eq!(memory.write8(lock, 0x80), Err(BusError::Pending));
        }
        b.advance_clocks(10).unwrap();
        assert_eq!(b.read8(lock), Ok(0x80));
        assert_eq!(b.read32(HIGH_WRAM), Ok(0));
        b.finish_cpu_transaction(peer);
        b.advance_clocks(100).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0x12345678));
        assert!(b.cpu_reset_active(p));
    }
}
