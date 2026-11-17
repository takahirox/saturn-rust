//! Physical read-unit bridge used only by the continuation-kernel experiment.
use super::*;

impl Bus {
    fn kernel_wram_transfer(transfer: &Transfer) -> bool {
        let address = Bus::physical(transfer.address);
        matches!(transfer.width, 1 | 2 | 4)
            && [LOW_WRAM, HIGH_WRAM].into_iter().any(|base| {
                (base..base + WRAM_SIZE as u32).contains(&address)
                    && address + transfer.width as u32 <= base + WRAM_SIZE as u32
            })
    }

    pub(crate) fn kernel_wram_horizon(&mut self) -> u32 {
        self.sample_interrupts();
        if !self.cached_wait_allowed()
            || !self.system_clock.is_reference()
            || self.pending_main_clocks != 0
            || self.cpu_dma_errors.iter().any(|&error| error)
            || self.transactions.lock.is_some()
            || self
                .transactions
                .resetting
                .iter()
                .any(|&resetting| resetting)
            || self.transactions.active.as_ref().is_some_and(|active| {
                !matches!(active.owner, Owner::Cpu(_) | Owner::WriteBuffer(_, false))
                    || !Self::kernel_wram_transfer(&active.transfer)
            })
            || self.transactions.internal.iter().flatten().any(|active| {
                !matches!(active.owner, Owner::WriteAccept(_, false))
                    || !Self::kernel_wram_write(&active.transfer)
            })
            || self
                .transactions
                .writes
                .iter()
                .flatten()
                .any(|write| write.dmac || !write.beats.iter().all(Self::kernel_wram_write))
            || self.transactions.journals[..2].iter().any(|journal| {
                let j = journal.borrow();
                !j.operations.is_empty()
                    || j.pipeline_wait.is_some()
                    || j.irq.is_some()
                    || j.vector_waiting.is_some()
                    || j.synchronous_stack
                    || j.waiting
                        .as_ref()
                        .is_some_and(|t| !Self::kernel_wram_transfer(t))
                    || j.pending_write
                        .as_ref()
                        .is_some_and(|write| !write.iter().all(Self::kernel_wram_write))
            })
        {
            return 0;
        }
        let fixed = self
            .cd
            .clocks_to_event()
            .min(self.smpc.clocks_to_event())
            .min(self.sound.cached_cpu_deadline());
        self.observable_main_deadline()
            .saturating_sub(1)
            .min(self.system_clock.preview(fixed.saturating_sub(1)))
    }

    /// Only within an admitted WRAM segment. CPU/BSC transfer state advances
    /// now; independent device domains are caught up before the observer edge.
    pub(crate) fn kernel_advance_wram_bus(&mut self, clocks: u32) -> Result<(), ScuError> {
        self.main_clocks += u64::from(clocks);
        self.onchip[0].kernel_advance_bus(clocks);
        if self.slave_enabled {
            self.onchip[1].kernel_advance_bus(clocks);
        }
        self.advance_bus_transaction(clocks)
    }

    pub(crate) fn kernel_bus_deadline(&self) -> u32 {
        self.bus_main_deadline()
    }

    fn kernel_wram_write(transfer: &Transfer) -> bool {
        let address = Bus::physical(transfer.address);
        transfer.write.is_some()
            && matches!(transfer.width, 1 | 2 | 4)
            && [LOW_WRAM, HIGH_WRAM].into_iter().any(|base| {
                (base..base + WRAM_SIZE as u32).contains(&address)
                    && address + transfer.width as u32 <= base + WRAM_SIZE as u32
            })
    }

    /// The CPU owns the immutable store latch. Retry only buffer admission;
    /// accepted physical byte/halfword draining stays owned by the board.
    pub(crate) fn resume_kernel_store(
        &mut self,
        owner: Processor,
        address: u32,
        width: u32,
        value: u32,
        pc: u32,
    ) -> Result<(), BusError> {
        self.resume_planned_write(owner, address, width, value, pc)
    }

    pub(crate) fn kernel_deadline(&self) -> u32 {
        self.main_deadline()
    }

    /// A strictly bounded interval in which private cache/register work has
    /// no board observer. Every device still advances through ordinary edges.
    pub(crate) fn kernel_private_horizon(&self) -> u32 {
        if !self.cached_wait_allowed()
            || !self.system_clock.is_reference()
            || self.pending_main_clocks != 0
            || self.transactions.lock.is_some()
            || self.transactions.active.as_ref().is_some_and(|active| {
                !matches!(active.owner, Owner::WriteBuffer(_, false))
                    || !Self::kernel_wram_write(&active.transfer)
            })
            || self.transactions.internal.iter().any(Option::is_some)
            || self
                .transactions
                .writes
                .iter()
                .flatten()
                .any(|write| write.dmac || !write.beats.iter().all(Self::kernel_wram_write))
            || self.transactions.journals[..2].iter().any(|journal| {
                let j = journal.borrow();
                j.waiting.is_some() || j.pending_write.is_some() || j.operand_completion.is_some()
            })
        {
            return 0;
        }
        let fixed = self
            .cd
            .clocks_to_event()
            .min(self.smpc.clocks_to_event())
            .min(self.sound.cached_cpu_deadline());
        self.observable_main_deadline()
            .saturating_sub(1)
            .min(self.system_clock.preview(fixed.saturating_sub(1)))
    }

    pub(crate) fn kernel_retired(&self, owner: Processor, count: u64) {
        self.onchip[owner as usize].count_cached_instructions(count);
    }

    pub(crate) fn schedule_kernel_operand(&mut self, owner: Processor, end: u64) {
        self.transactions.journals[owner as usize]
            .get_mut()
            .operand_completion = Some(end);
    }

    /// Experimental owned-port bridge: retry only the unfinished physical
    /// byte/halfword unit, never CPU decode or earlier cache-fill longwords.
    pub(crate) fn resume_kernel_read(
        &mut self,
        transfer: crate::kernel::ReadTransfer,
    ) -> Result<u32, BusError> {
        let processor = transfer.ticket.owner();
        let journal = self.transactions.journals[processor as usize].get_mut();
        journal.beat_cursor = 0;
        journal.current_operation = Some(Operation::Read(
            transfer.address,
            transfer.width,
            transfer.instruction,
            false,
        ));
        let memory = CpuBus {
            bus: self,
            processor,
            cached: true,
            scheduled: true,
        };
        let result = match transfer.width {
            1 => memory
                .external_read_group::<1>(transfer.address, transfer.first, transfer.last, false)
                .map(|v| u32::from(v[0])),
            2 => memory
                .external_read_group::<2>(transfer.address, transfer.first, transfer.last, false)
                .map(|v| u32::from(u16::from_be_bytes(v))),
            4 => memory
                .external_read_group::<4>(transfer.address, transfer.first, transfer.last, false)
                .map(u32::from_be_bytes),
            _ => Err(BusError::UnsupportedDevice {
                address: transfer.address,
                width: transfer.width,
            }),
        };
        if result.is_ok() {
            let journal = self.transactions.journals[processor as usize].get_mut();
            journal.beats.clear();
            journal.beat_cursor = 0;
            journal.completed = false;
            journal.current_operation = None;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wram_domain_admission_rejects_other_memory_faults_reset_pll_and_dma() {
        let mut bus = Bus::default();
        assert!(bus.kernel_wram_horizon() > 1);
        bus.cpu_dma_errors[0] = true;
        assert_eq!(bus.kernel_wram_horizon(), 0);
        bus.cpu_dma_errors[0] = false;
        bus.transactions.resetting[1] = true;
        assert_eq!(bus.kernel_wram_horizon(), 0);
        bus.transactions.resetting[1] = false;
        bus.change_system_clock(true);
        assert_eq!(bus.kernel_wram_horizon(), 0);
        bus.change_system_clock(false);
        assert_eq!(
            bus.replay_cpu(Processor::Master).read32(VRAM),
            Err(BusError::Pending)
        );
        assert_eq!(
            bus.kernel_wram_horizon(),
            0,
            "queued B-bus reads are observers"
        );
        let mut bus = Bus::default();
        for (offset, value) in [
            (0, HIGH_WRAM),
            (4, HIGH_WRAM + 0x1000),
            (8, 4),
            (12, 0xA05),
            (0x30, 1),
        ] {
            bus.for_cpu(Processor::Master)
                .write32(crate::onchip::DMAC + offset, value)
                .unwrap();
        }
        assert_eq!(
            bus.kernel_wram_horizon(),
            0,
            "on-chip DMA owns its actual observations"
        );
    }

    fn accepted(address: u32, byte_bus: bool) -> Bus {
        let mut bus = Bus::default();
        if byte_bus {
            bus.for_cpu(Processor::Master)
                .write32(0xFFFFFFE4, 0xA55A007C)
                .unwrap();
        }
        bus.for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        assert_eq!(
            bus.replay_cpu(Processor::Master)
                .write32(address, 0xAABBCCDD),
            Err(BusError::Pending)
        );
        bus.advance_clocks(1).unwrap();
        assert_eq!(
            bus.replay_cpu(Processor::Master)
                .write32(address, 0xAABBCCDD),
            Ok(())
        );
        bus.finish_cpu_transaction(Processor::Master);
        bus.grant_bus_transaction();
        bus
    }

    #[test]
    fn private_cache_work_can_cross_only_wram_write_drains_and_keeps_each_byte_edge() {
        for address in [HIGH_WRAM, HIGH_WRAM | 0x20000000] {
            let mut bus = accepted(address, true);
            assert!(bus.kernel_private_horizon() > bus.bus_main_deadline());
            assert_eq!(bus.read32(address), Ok(0));
            for (edge, expected) in [
                (8, 0xAA000000),
                (16, 0xAABB0000),
                (24, 0xAABBCC00),
                (32, 0xAABBCCDD),
            ] {
                while bus.main_clocks < edge {
                    let (_, command) = bus
                        .advance_cpu_clocks_with_sound((edge - bus.main_clocks) as u32, true)
                        .unwrap();
                    assert!(command.is_none());
                }
                assert_eq!(bus.main_clocks, edge);
                assert_eq!(bus.read32(address), Ok(expected));
            }
        }
        let low = accepted(LOW_WRAM, false);
        assert!(
            low.kernel_private_horizon() > 0,
            "CS0 WRAM keeps its own physical width/timing"
        );
        for address in [vdp1::VRAM, VRAM] {
            let bus = accepted(address, false);
            assert_eq!(
                bus.kernel_private_horizon(),
                0,
                "non-WRAM writer {address:X}"
            );
        }
        let mut bus = accepted(HIGH_WRAM, false);
        assert!(bus.kernel_private_horizon() > 0);
        bus.for_cpu(Processor::Master)
            .write16(0xFFFFFF48, 0x56)
            .unwrap();
        bus.cpu_delay(Processor::Master, 0);
        assert!(
            bus.kernel_private_horizon() <= 2,
            "stop before queued UBC activation"
        );
        assert_eq!(bus.for_cpu(Processor::Master).read16(0xFFFFFF48), Ok(0x56));
        bus.cpu_delay(Processor::Master, 0);
        assert_eq!(
            bus.kernel_private_horizon(),
            0,
            "UBC comparison is a real observer"
        );
    }
}
