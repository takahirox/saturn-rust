//! Physical read-unit bridge used only by the continuation-kernel experiment.
use super::*;

impl Bus {
    pub(crate) fn kernel_deadline(&self) -> u32 {
        self.main_deadline()
    }

    /// A strictly bounded interval in which private cache/register work has
    /// no board observer. Every device still advances through ordinary edges.
    pub(crate) fn kernel_private_horizon(&self) -> u32 {
        if !self.cached_wait_allowed()
            || !self.system_clock.is_reference()
            || self.pending_main_clocks != 0
            || self.transactions.active.is_some()
            || self.transactions.internal.iter().any(Option::is_some)
            || self.transactions.writes.iter().any(Option::is_some)
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
