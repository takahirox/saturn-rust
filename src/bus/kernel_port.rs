//! Physical read-unit bridge used only by the continuation-kernel experiment.
use super::*;

impl Bus {
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
