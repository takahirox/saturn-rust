//! Resumable SH-2 bus cycles. H §§7.3–7.9, §8.4 and P TAS.B:
//! data is sampled/published at completion, and TAS retains ownership through
//! its write. Replaying an instruction never repeats an already completed I/O.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Read(u32, usize, bool, bool), // address, width, instruction, TAS
    Write(u32, usize, u32),
    Sleep,
    BreakPending,
    InstructionBreak(u32, bool),
    AcknowledgeBreak,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Transfer {
    address: u32,
    width: usize,
    write: Option<u32>,
    first: bool,
    last: bool,
    locked: bool,
}
#[derive(Default)]
pub(super) struct Journal {
    operations: Vec<(Operation, Result<u32, BusError>)>,
    cursor: usize,
    beats: Vec<(Transfer, Result<u32, BusError>)>,
    beat_cursor: usize,
    waiting: Option<Transfer>,
    completed: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Cpu(Processor),
    Scu,
}
struct Active {
    owner: Owner,
    transfer: Transfer,
    remaining: u32,
}
#[derive(Default)]
pub(super) struct Transactions {
    journals: [RefCell<Journal>; 2],
    active: Option<Active>,
    lock: Option<Processor>,
    previous: Option<Processor>,
    scu: Option<crate::scu::DmaCycle>,
}

struct ScheduledMemory<'a>(CpuBus<'a>);
impl ScheduledMemory<'_> {
    fn operation(&mut self, op: Operation) -> Result<u32, BusError> {
        let i = self.0.processor as usize;
        {
            let mut journal = self.0.bus.transactions.journals[i].borrow_mut();
            let cursor = journal.cursor;
            if let Some(&(recorded, result)) = journal.operations.get(cursor) {
                assert_eq!(op, recorded, "instruction replay changed its bus sequence");
                journal.cursor += 1;
                return result;
            }
            journal.beat_cursor = 0;
        }
        let result = match op {
            Operation::Read(a, 1, _, true) => self.0.tas_read8(a).map(u32::from),
            Operation::Read(a, 1, _, _) => self.0.read8(a).map(u32::from),
            Operation::Read(a, 2, true, _) => self.0.fetch16(a).map(u32::from),
            Operation::Read(a, 2, _, _) => self.0.read16(a).map(u32::from),
            Operation::Read(a, _, _, _) => self.0.read32(a),
            Operation::Write(a, 1, v) => self.0.write8(a, v as u8).map(|_| 0),
            Operation::Write(a, 2, v) => self.0.write16(a, v as u16).map(|_| 0),
            Operation::Write(a, _, v) => self.0.write32(a, v).map(|_| 0),
            Operation::Sleep => {
                self.0.enter_sleep();
                Ok(0)
            }
            Operation::BreakPending => Ok(u32::from(self.0.break_pending())),
            Operation::InstructionBreak(pc, after) => {
                Ok(u32::from(self.0.instruction_break(pc, after)))
            }
            Operation::AcknowledgeBreak => {
                self.0.acknowledge_break();
                Ok(0)
            }
        };
        if result != Err(BusError::Pending) {
            let mut journal = self.0.bus.transactions.journals[i].borrow_mut();
            journal.operations.push((op, result));
            journal.cursor += 1;
            journal.beats.clear();
        }
        result
    }
}
// Memory reads take &self. RefCell contains the adapter, never a memory pointer;
// a resumed interpreter uses exactly the same checked Memory boundary.
pub(crate) struct ReplayMemory<'a>(RefCell<ScheduledMemory<'a>>);
impl Memory for ReplayMemory<'_> {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::Read(a, 1, false, false))
            .map(|v| v as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::Read(a, 2, false, false))
            .map(|v| v as u16)
    }
    fn fetch16(&self, a: u32) -> Result<u16, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::Read(a, 2, true, false))
            .map(|v| v as u16)
    }
    fn tas_read8(&self, a: u32) -> Result<u8, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::Read(a, 1, false, true))
            .map(|v| v as u8)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::Read(a, 4, false, false))
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.0
            .get_mut()
            .operation(Operation::Write(a, 1, u32::from(v)))
            .map(|_| ())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.0
            .get_mut()
            .operation(Operation::Write(a, 2, u32::from(v)))
            .map(|_| ())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.0
            .get_mut()
            .operation(Operation::Write(a, 4, v))
            .map(|_| ())
    }
    fn enter_sleep(&mut self) {
        self.0.get_mut().operation(Operation::Sleep).unwrap();
    }
    fn break_pending(&self) -> bool {
        self.0
            .borrow_mut()
            .operation(Operation::BreakPending)
            .unwrap()
            != 0
    }
    fn instruction_break(&self, pc: u32, after: bool) -> bool {
        self.0
            .borrow_mut()
            .operation(Operation::InstructionBreak(pc, after))
            .unwrap()
            != 0
    }
    fn acknowledge_break(&self) {
        self.0
            .borrow_mut()
            .operation(Operation::AcknowledgeBreak)
            .unwrap();
    }
}
impl Bus {
    pub(crate) fn replay_cpu(&mut self, processor: Processor) -> ReplayMemory<'_> {
        self.transactions.journals[processor as usize]
            .borrow_mut()
            .cursor = 0;
        ReplayMemory(RefCell::new(ScheduledMemory(CpuBus {
            bus: self,
            processor,
            cached: true,
            scheduled: true,
        })))
    }
    pub(crate) fn finish_cpu_transaction(&mut self, processor: Processor) {
        let journal = self.transactions.journals[processor as usize].get_mut();
        journal.operations.clear();
        journal.beats.clear();
        journal.cursor = 0;
        journal.beat_cursor = 0;
        journal.waiting = None;
        journal.completed = false;
        if self.transactions.lock == Some(processor) {
            self.transactions.lock = None;
        }
    }
    pub(super) fn cancel_scu_transaction(&mut self) {
        if self
            .transactions
            .active
            .as_ref()
            .is_some_and(|a| a.owner == Owner::Scu)
        {
            self.transactions.active = None;
        }
        self.transactions.scu = None;
    }
    pub(crate) fn cancel_cpu_transaction(&mut self, processor: Processor) {
        if self
            .transactions
            .active
            .as_ref()
            .is_some_and(|a| a.owner == Owner::Cpu(processor))
        {
            self.transactions.active = None;
        }
        self.finish_cpu_transaction(processor);
    }
    pub(crate) fn take_cpu_transaction_completion(&self, processor: Processor) -> bool {
        std::mem::take(
            &mut self.transactions.journals[processor as usize]
                .borrow_mut()
                .completed,
        )
    }
    pub(crate) fn grant_bus_transaction(&mut self) {
        if self.transactions.active.is_some() {
            return;
        }
        if self.transactions.lock.is_none()
            && let Some(cycle) = self.scu.prepare_dma_cycle()
        {
            let mut remaining = 2;
            if cycle.width != 0 {
                let write = cycle.value.is_some();
                if self.external_owner.get().is_some() {
                    self.release_external_bus();
                }
                // SCU accesses are physical, with the master BSC driving
                // CPU-bus RAM and the SCU driving A/B-bus selects.
                let base = self.onchip[0].external_access(cycle.address, write, true, true);
                let mut extra = self.scu.abus_access(cycle.address, cycle.width, write);
                if (VRAM..VRAM + 0x80000).contains(&cycle.address) {
                    extra = extra.saturating_add(
                        self.raster
                            .vram_wait_clocks(self.vdp2.cpu_access_slots(cycle.address), 0),
                    );
                }
                self.onchip[0].add_external_wait_clocks(extra);
                remaining = base
                    .saturating_add(self.onchip[0].take_wait_clocks())
                    .max(1);
            }
            self.transactions.scu = Some(cycle);
            self.transactions.active = Some(Active {
                owner: Owner::Scu,
                transfer: Transfer {
                    address: cycle.address,
                    width: cycle.width,
                    write: cycle.value,
                    first: true,
                    last: true,
                    locked: false,
                },
                remaining,
            });
            return;
        }
        // Round-robin at beat boundaries; TAS is the only retained CPU lock.
        let order = if self.transactions.previous == Some(Processor::Master) {
            [Processor::Slave, Processor::Master]
        } else {
            [Processor::Master, Processor::Slave]
        };
        for processor in order {
            if self
                .transactions
                .lock
                .is_some_and(|owner| owner != processor)
            {
                continue;
            }
            let Some(transfer) = self.transactions.journals[processor as usize]
                .borrow()
                .waiting
            else {
                continue;
            };
            if transfer.locked {
                self.transactions.lock = Some(processor);
            }
            let memory = self.for_cpu(processor);
            memory.reserve_external(
                transfer.address,
                transfer.width,
                transfer.write.is_some(),
                transfer.first,
                transfer.last,
            );
            let i = processor as usize;
            let remaining = self.cpu_bus_clocks[i]
                .replace(0)
                .saturating_add(self.onchip[i].take_wait_clocks())
                .max(1);
            self.transactions.active = Some(Active {
                owner: Owner::Cpu(processor),
                transfer,
                remaining,
            });
            self.transactions.previous = Some(processor);
            return;
        }
    }
    pub(crate) fn bus_transaction_deadline(&self) -> u32 {
        self.transactions
            .active
            .as_ref()
            .map_or(u32::MAX, |a| self.system_clock.deadline(a.remaining).max(1))
    }
    pub(super) fn bus_main_deadline(&self) -> u32 {
        self.transactions
            .active
            .as_ref()
            .map_or(u32::MAX, |a| a.remaining)
    }
    pub(super) fn bus_busy(&self) -> bool {
        self.transactions.active.is_some() || self.transactions.lock.is_some()
    }
    pub(super) fn advance_bus_transaction(&mut self, clocks: u32) -> Result<(), ScuError> {
        let Some(active) = &mut self.transactions.active else {
            return Ok(());
        };
        active.remaining = active.remaining.saturating_sub(clocks);
        if active.remaining != 0 {
            return Ok(());
        }
        let active = self.transactions.active.take().unwrap();
        let t = active.transfer;
        let processor = match active.owner {
            Owner::Cpu(p) => p,
            Owner::Scu => Processor::Master,
        };
        let result = if t.width == 0 {
            Ok(0)
        } else if t.width == 2 && t.write.is_some() && (0xFFFF8000..0xFFFFC000).contains(&t.address)
        {
            self.onchip[processor as usize]
                .write_sdram_mode(t.address)
                .map(|_| t.write.unwrap())
        } else if let Some(value) = t.write {
            let result = match t.width {
                1 => self.write(t.address, [value as u8]),
                2 => self.write(t.address, (value as u16).to_be_bytes()),
                _ => self.write(t.address, value.to_be_bytes()),
            };
            match result {
                Ok(()) | Err(BusError::ReadOnly { .. }) => Ok(value),
                Err(e) => Err(e),
            }
        } else {
            match t.width {
                1 => self.read::<1>(t.address).map(|b| u32::from(b[0])),
                2 => self
                    .read::<2>(t.address)
                    .map(|b| u32::from(u16::from_be_bytes(b))),
                _ => self.read::<4>(t.address).map(u32::from_be_bytes),
            }
        };
        if active.owner == Owner::Scu {
            if let Ok(value) = result
                && t.width != 0
            {
                for chip in &self.onchip {
                    chip.observe_access(
                        Bus::physical(t.address),
                        t.width,
                        value,
                        t.write.is_some(),
                        false,
                        true,
                    );
                }
            }
            let cycle = self.transactions.scu.take().unwrap();
            self.scu.complete_dma_cycle(cycle, result)?;
        } else {
            if let Ok(value) = result {
                let memory = self.for_cpu(processor);
                memory.observe_external_data(t.address, t.width, value, t.write.is_some());
            }
            let journal = self.transactions.journals[processor as usize].get_mut();
            journal.beats.push((t, result));
            journal.waiting = None;
            journal.completed = true;
        }
        Ok(())
    }
}
impl CpuBus<'_> {
    pub(super) fn scheduled_external(
        &self,
        address: u32,
        width: usize,
        write: Option<u32>,
        first: bool,
        last: bool,
        locked: bool,
    ) -> Result<u32, BusError> {
        let transfer = Transfer {
            address,
            width,
            write,
            first,
            last,
            locked,
        };
        let mut journal = self.bus.transactions.journals[self.processor as usize].borrow_mut();
        let cursor = journal.beat_cursor;
        if let Some(&(recorded, result)) = journal.beats.get(cursor) {
            assert_eq!(
                transfer, recorded,
                "instruction replay changed its physical bus sequence"
            );
            journal.beat_cursor += 1;
            return result;
        }
        journal.waiting = Some(transfer);
        Err(BusError::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(b: &mut Bus, p: Processor, address: u32) -> Result<u8, BusError> {
        b.replay_cpu(p).read8(address)
    }
    fn write(b: &mut Bus, p: Processor, address: u32, value: u8) -> Result<(), BusError> {
        b.replay_cpu(p).write8(address, value)
    }
    fn complete(b: &mut Bus) {
        b.grant_bus_transaction();
        let clocks = b.bus_transaction_deadline();
        assert_ne!(clocks, u32::MAX);
        b.advance_clocks(clocks).unwrap();
    }
    #[test]
    fn queued_reads_sample_at_completion_and_waiting_writes_never_publish_early() {
        let mut b = Bus::default();
        let at = HIGH_WRAM | 0x20000000;
        assert_eq!(
            write(&mut b, Processor::Master, at, 42),
            Err(BusError::Pending)
        );
        assert_eq!(read(&mut b, Processor::Slave, at), Err(BusError::Pending));
        b.grant_bus_transaction();
        b.advance_clocks(7).unwrap();
        assert_eq!(b.read8(at), Ok(0));
        b.advance_clocks(1).unwrap();
        assert_eq!(b.read8(at), Ok(42));
        assert_eq!(write(&mut b, Processor::Master, at, 42), Ok(()));
        b.finish_cpu_transaction(Processor::Master);
        complete(&mut b);
        assert_eq!(read(&mut b, Processor::Slave, at), Ok(42));
        // Completed results are sampled data, never fresh host reads on replay.
        b.write8(at, 99).unwrap();
        assert_eq!(read(&mut b, Processor::Slave, at), Ok(42));
        b.finish_cpu_transaction(Processor::Slave);
        assert_eq!(read(&mut b, Processor::Slave, at), Err(BusError::Pending));
        complete(&mut b);
        assert_eq!(read(&mut b, Processor::Slave, at), Ok(99));
    }
    #[test]
    fn replay_does_not_consume_a_cd_fifo_word_twice_when_a_later_operand_stalls() {
        let mut b = Bus::default();
        b.cd.mount(
            crate::media::Media::iso(include_bytes!("../../web/star-collector.iso")).unwrap(),
        );
        for (offset, value) in [(0x18, 0x0200), (0x1C, 0), (0x20, 0), (0x24, 0)] {
            b.cd.write16(crate::cd::REGISTERS + offset, value).unwrap();
        }
        b.advance_clocks(1075).unwrap();
        assert_eq!(
            b.replay_cpu(Processor::Master).read16(crate::cd::DATA),
            Err(BusError::Pending)
        );
        complete(&mut b);
        {
            let m = b.replay_cpu(Processor::Master);
            assert_eq!(m.read16(crate::cd::DATA), Ok(0x4100));
            assert_eq!(m.read32(HIGH_WRAM | 0x20000000), Err(BusError::Pending));
        }
        complete(&mut b);
        {
            let m = b.replay_cpu(Processor::Master);
            assert_eq!(m.read16(crate::cd::DATA), Ok(0x4100));
            assert_eq!(m.read32(HIGH_WRAM | 0x20000000), Ok(0));
        }
        b.finish_cpu_transaction(Processor::Master);
        // The next word is still track1's FAD150, not a third FIFO word.
        assert_eq!(b.cd.read16(crate::cd::DATA), Ok(150));
    }
    #[test]
    fn tas_owns_read_write_interval_and_cancellation_releases_a_waiting_peer() {
        let mut b = Bus::default();
        let at = HIGH_WRAM | 0x20000000;
        assert_eq!(
            b.replay_cpu(Processor::Master).tas_read8(at),
            Err(BusError::Pending)
        );
        assert_eq!(read(&mut b, Processor::Slave, at), Err(BusError::Pending));
        complete(&mut b);
        {
            let mut m = b.replay_cpu(Processor::Master);
            assert_eq!(m.tas_read8(at), Ok(0));
            assert_eq!(m.write8(at, 0x80), Err(BusError::Pending));
        }
        complete(&mut b); // round robin cannot hand the locked bus to the slave
        assert_eq!(b.read8(at), Ok(0x80));
        b.cancel_cpu_transaction(Processor::Master);
        complete(&mut b);
        assert_eq!(read(&mut b, Processor::Slave, at), Ok(0x80));
    }
    #[test]
    fn pending_and_granted_writes_are_cancelled_by_cpu_reset_without_later_effects() {
        for grant in [false, true] {
            let mut b = Bus::default();
            let at = HIGH_WRAM | 0x20000000;
            assert_eq!(
                write(&mut b, Processor::Slave, at, 42),
                Err(BusError::Pending)
            );
            if grant {
                b.grant_bus_transaction();
                b.advance_clocks(3).unwrap();
            }
            b.set_slave_enabled(false);
            b.advance_clocks(100).unwrap();
            assert_eq!(b.read8(at), Ok(0));
            assert_eq!(b.bus_transaction_deadline(), u32::MAX);
        }
    }
}

#[cfg(test)]
mod dma_tests {
    use super::*;
    fn channel(b: &mut Bus, i: u32, indirect: bool) {
        let base = SCU + i * 32;
        for (offset, value) in [
            (0, HIGH_WRAM + 0x100),
            (
                4,
                if indirect {
                    HIGH_WRAM + 0x80
                } else {
                    HIGH_WRAM + 0x200
                },
            ),
            (8, 4),
            (12, 0x102),
            (20, if indirect { 0x01000107 } else { 7 }),
            (16, 0x101),
        ] {
            b.write32(base + offset, value).unwrap();
        }
    }
    #[test]
    fn all_three_channels_defer_direct_and_indirect_data_and_irqs_through_cpu_bus_waits() {
        // H WCR reset=AAFF: each high-RAM beat has T1/T2+six Tw,
        // and read->write IW3=2. SCU setup=one internal clock (two main).
        for i in 0..3 {
            for indirect in [false, true] {
                for chunk in [1, 3, 17, 64] {
                    let mut b = Bus::default();
                    b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
                    for (n, value) in [4, HIGH_WRAM + 0x200, 0x80000000 | (HIGH_WRAM + 0x100)]
                        .into_iter()
                        .enumerate()
                    {
                        b.write32(HIGH_WRAM + 0x80 + n as u32 * 4, value).unwrap();
                    }
                    channel(&mut b, i, indirect);
                    let completion = if indirect { 44 } else { 20 };
                    let mut elapsed = 0;
                    while elapsed < completion - 1 {
                        let delta = chunk.min(completion - 1 - elapsed);
                        b.advance_clocks(delta).unwrap();
                        elapsed += delta;
                    }
                    assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
                    assert_eq!(b.read32(SCU + 0xA4).unwrap() & (1 << (11 - i)), 0);
                    b.advance_clocks(1).unwrap();
                    assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x12345678));
                    assert_ne!(b.read32(SCU + 0xA4).unwrap() & (1 << (11 - i)), 0);
                }
            }
        }
    }
    #[test]
    fn tas_blocks_dma_grants_until_write_completion_and_instruction_release() {
        let mut b = Bus::default();
        let lock = (HIGH_WRAM + 0x10) | 0x20000000;
        assert_eq!(
            b.replay_cpu(Processor::Master).tas_read8(lock),
            Err(BusError::Pending)
        );
        b.grant_bus_transaction();
        b.advance_clocks(8).unwrap();
        channel(&mut b, 0, false);
        {
            let mut m = b.replay_cpu(Processor::Master);
            assert_eq!(m.tas_read8(lock), Ok(0));
            assert_eq!(m.write8(lock, 0x80), Err(BusError::Pending));
        }
        b.grant_bus_transaction();
        b.advance_clocks(10).unwrap();
        assert_eq!(b.read8(lock), Ok(0x80));
        b.grant_bus_transaction();
        assert_eq!(b.bus_transaction_deadline(), u32::MAX);
        // Neither the SCU's source nor its destination cycle can split TAS.
        assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
        b.finish_cpu_transaction(Processor::Master);
        b.grant_bus_transaction();
        assert_eq!(b.bus_transaction_deadline(), 2);
    }
    #[test]
    fn dsp_output_latches_at_grant_before_later_host_writes_and_keeps_both_halves() {
        let mut b = Bus::default();
        b.write32(SCU + 0x88, 0).unwrap();
        b.write32(SCU + 0x8C, 0x11223344).unwrap();
        for word in [0x9d700400, 0xc0009001, 0xf0000000] {
            b.write32(SCU + 0x84, word).unwrap();
        }
        b.write32(SCU + 0x80, 0x18000).unwrap();
        b.advance_clocks(7).unwrap(); // DMA granted t4; END executes at t6
        assert_eq!(b.read32(vdp1::VRAM + 0x1000), Ok(0));
        b.write32(SCU + 0x88, 0).unwrap();
        b.write32(SCU + 0x8C, 0xAABBCCDD).unwrap();
        b.advance_clocks(5).unwrap();
        assert_eq!(b.read32(vdp1::VRAM + 0x1000), Ok(0x11220000));
        b.advance_clocks(8).unwrap();
        assert_eq!(b.read32(vdp1::VRAM + 0x1000), Ok(0x11223344));
    }
}
