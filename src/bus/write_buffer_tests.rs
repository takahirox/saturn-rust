//! H §7.11.2: buffer acceptance, external data, and internal-bus ownership
//! are distinct events, for both CPU and DMAC masters.
use super::*;

fn board(p: Processor) -> Bus {
    let mut b = Bus::default();
    b.set_slave_enabled(true);
    b.for_cpu(p).write32(0xFFFFFFE8, 0xA55A00C0).unwrap(); // CS3 six Tw
    b
}

#[test]
fn accepted_cpu_write_allows_peripheral_work_but_dependent_external_read_drains_it() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 11, 64] {
            let mut b = board(p);
            let address = HIGH_WRAM | 0x20000000;
            assert_eq!(
                b.replay_cpu(p).write32(address, 0x12345678),
                Err(BusError::Pending)
            );
            b.advance_clocks(1).unwrap();
            assert_eq!(b.replay_cpu(p).write32(address, 0x12345678), Ok(()));
            assert_eq!(b.read32(address), Ok(0));
            b.finish_cpu_transaction(p);
            assert_eq!(b.replay_cpu(p).read8(0xFFFFFE01), Err(BusError::Pending));
            b.advance_clocks(2).unwrap();
            assert_eq!(b.replay_cpu(p).read8(0xFFFFFE01), Ok(0xFF));
            assert_eq!(b.read32(address), Ok(0));
            b.finish_cpu_transaction(p);
            assert_eq!(b.replay_cpu(p).read32(address), Err(BusError::Pending));
            // External write ends at8. The later external read cannot sample
            // before16, and no read forwarding circumvents the dummy barrier.
            for clock in 4..=16 {
                b.advance_clocks(1).unwrap();
                assert_eq!(
                    b.read32(address),
                    Ok(if clock < 8 { 0 } else { 0x12345678 })
                );
                if clock < 16 {
                    assert!(!b.take_cpu_transaction_completion(p));
                }
            }
            assert_eq!(b.replay_cpu(p).read32(address), Ok(0x12345678));
            b.finish_cpu_transaction(p);
            assert_eq!(
                b.replay_cpu(p).write32(address, 0xDEADBEEF),
                Err(BusError::Pending)
            );
            let mut left = 100;
            while left != 0 {
                let n = left.min(chunk);
                b.advance_clocks(n).unwrap();
                left -= n;
            }
            assert_eq!(b.read32(address), Ok(0xDEADBEEF));
        }
    }
}

#[test]
fn one_entry_backpressure_and_each_reset_phase_keep_only_already_published_bytes() {
    for p in [Processor::Master, Processor::Slave] {
        for elapsed in [0, 1, 7, 8, 15, 16, 23, 24, 31] {
            let mut b = board(p);
            b.for_cpu(p).write32(0xFFFFFFE4, 0xA55A007C).unwrap(); // byte bus
            let address = HIGH_WRAM | 0x20000000;
            assert_eq!(
                b.replay_cpu(p).write32(address, 0x12345678),
                Err(BusError::Pending)
            );
            b.grant_bus_transaction();
            b.advance_clocks(elapsed).unwrap();
            let published = match elapsed / 8 {
                0 => 0,
                1 => 0x12000000,
                2 => 0x12340000,
                _ => 0x12345600,
            };
            assert_eq!(b.read32(address), Ok(published));
            if elapsed > 0 {
                assert_eq!(b.replay_cpu(p).write32(address, 0x12345678), Ok(()));
                b.finish_cpu_transaction(p);
                assert_eq!(
                    b.replay_cpu(p).write32(address + 4, 0xDEADBEEF),
                    Err(BusError::Pending)
                );
            }
            // A reset of this CPU cancels its admission, remaining byte beats
            // and a second queued write. It leaves the peer and shared RAM.
            b.cancel_cpu_transaction(p);
            b.advance_clocks(100).unwrap();
            assert_eq!(b.read32(address), Ok(published));
            assert_eq!(b.read32(address + 4), Ok(0));
        }
    }
}

#[test]
fn dma_internal_source_is_sampled_while_its_previous_external_write_is_buffered() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 64] {
            let mut b = board(p);
            let chip = &mut b.onchip[p as usize];
            chip.write8(0xFFFFFE00, 0x31).unwrap();
            for (offset, value) in [
                (0, 0xFFFFFE00),
                (4, HIGH_WRAM),
                (8, 2),
                (12, 0x4201),
                (0x30, 1),
            ] {
                chip.write32(0xFFFFFF80 + offset, value).unwrap();
            }
            b.advance_clocks(3).unwrap(); // read1, accept1, second read1
            b.onchip[p as usize].write8(0xFFFFFE00, 0x67).unwrap();
            assert_eq!(b.read16(HIGH_WRAM), Ok(0));
            let mut left = 100;
            while left != 0 {
                let n = left.min(chunk);
                b.advance_clocks(n).unwrap();
                left -= n;
            }
            assert_eq!(b.read16(HIGH_WRAM), Ok(0x3131));
            assert_eq!(b.onchip[p as usize].read32(0xFFFFFF88), Ok(0));
            assert_eq!(b.take_dma_errors(), [false; 2]);
        }
    }
}

#[test]
fn accepting_a_write_updates_only_the_issuing_cache_before_external_memory() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = board(p);
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        b.write32(HIGH_WRAM, 0x12345678).unwrap();
        for cpu in [p, peer] {
            b.for_cpu(cpu).write8(CCR, 0x11).unwrap();
            assert_eq!(b.for_cpu(cpu).read32(HIGH_WRAM), Ok(0x12345678));
            b.cpu_delay(cpu, 0);
        }
        b.advance_clocks(100).unwrap();
        assert_eq!(
            b.replay_cpu(p).write32(HIGH_WRAM, 0xAABBCCDD),
            Err(BusError::Pending)
        );
        b.advance_clocks(1).unwrap();
        assert_eq!(b.replay_cpu(p).write32(HIGH_WRAM, 0xAABBCCDD), Ok(()));
        b.finish_cpu_transaction(p);
        assert_eq!(b.replay_cpu(p).read32(HIGH_WRAM), Ok(0xAABBCCDD));
        b.finish_cpu_transaction(p);
        assert_eq!(b.for_cpu(peer).read32(HIGH_WRAM), Ok(0x12345678));
        assert_eq!(b.read32(HIGH_WRAM), Ok(0x12345678));
        b.advance_clocks(7).unwrap();
        assert_eq!(b.read32(HIGH_WRAM), Ok(0xAABBCCDD));
        assert_eq!(b.for_cpu(peer).read32(HIGH_WRAM), Ok(0x12345678));
    }
}

#[test]
fn deferred_diagnostics_keep_the_issuer_pc_and_exception_context_and_reset_discards_them() {
    for p in [Processor::Master, Processor::Slave] {
        for vector in [None, Some(0x42)] {
            let mut b = board(p);
            {
                let mut memory = b.replay_cpu(p);
                memory.sh2_write_context(0x200, vector);
                assert_eq!(memory.write8(SCU + 0xCC, 42), Err(BusError::Pending));
            }
            b.advance_clocks(1).unwrap();
            {
                let mut memory = b.replay_cpu(p);
                memory.sh2_write_context(0x200, vector);
                assert_eq!(memory.write8(SCU + 0xCC, 42), Ok(()));
            }
            b.finish_cpu_transaction(p);
            b.advance_clocks(100).unwrap();
            let source = BusError::UnsupportedDevice {
                address: SCU + 0xCC,
                width: 1,
            };
            let expected = match vector {
                Some(vector) => crate::cpu::CpuError::ExceptionEntry {
                    pc: 0x200,
                    vector,
                    source,
                },
                None => crate::cpu::CpuError::Bus { pc: 0x200, source },
            };
            assert_eq!(b.take_cpu_write_error(p), Some(expected));
            b.cancel_cpu_transaction(p);
            assert_eq!(b.take_cpu_write_error(p), None);
        }
    }
}
