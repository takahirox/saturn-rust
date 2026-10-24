//! Resumable SH-2 bus cycles. H §§7.3–7.9, §8.4 and P TAS.B:
//! data is sampled/published at completion, and TAS retains ownership through
//! its write. Replaying an instruction never repeats an already completed I/O.
use super::*;
use std::collections::VecDeque;
#[cfg(test)]
#[path = "bridge_tests.rs"]
mod bridge_tests;
#[cfg(test)]
#[path = "cs0_tests.rs"]
mod cs0_tests;
#[cfg(test)]
#[path = "dma_startup_tests.rs"]
mod dma_startup_tests;
#[cfg(test)]
#[path = "fetch_tests.rs"]
mod fetch_tests;
#[cfg(test)]
#[path = "monitor_tests.rs"]
mod monitor_tests;
#[cfg(test)]
#[path = "refresh_tests.rs"]
mod refresh_tests;
#[cfg(test)]
#[path = "reset_tests.rs"]
mod reset_tests;
#[cfg(test)]
#[path = "sound_tests.rs"]
mod sound_tests;
#[cfg(test)]
#[path = "vdp1_tests.rs"]
mod vdp1_tests;
#[cfg(test)]
#[path = "video_write_tests.rs"]
mod video_write_tests;
#[cfg(test)]
#[path = "write_buffer_tests.rs"]
mod write_buffer_tests;

#[cfg(test)]
#[path = "vdp2_tests.rs"]
mod vdp2_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Clock(u64),
    PipelineWait(u32),
    MemoryAccess(u32, bool, u64), // address, write, earliest MA completion
    Read(u32, usize, bool, bool), // address, width, instruction, TAS
    Write(u32, usize, u32),
    DmaRead(u32, bool, bool),
    Sleep,
    BreakPending,
    InstructionBreak(u32, bool),
    AcknowledgeBreak,
    DiscardPrefetch,
    WriteContext(u32, Option<u8>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Transfer {
    pub(super) address: u32,
    pub(super) width: usize,
    pub(super) write: Option<u32>,
    pub(super) first: bool,
    pub(super) last: bool,
    pub(super) unit_end: u32, // byte/word/longword end, independently of burst end
    pub(super) locked: bool,
    // One of two physical words belonging to an aligned CPU-side longword.
    // UBC external observations describe that longword, not these words.
    pub(super) bridge_read: bool,
    // Retain the CPU-facing longword while publishing its device words.
    pub(super) bridge_write: Option<u32>,
}
#[derive(Default)]
pub(super) struct Journal {
    operations: Vec<(Operation, Result<u32, BusError>)>,
    cursor: usize,
    beats: Vec<(Transfer, Result<u32, BusError>)>,
    beat_cursor: usize,
    waiting: Option<Transfer>,
    completed: bool,
    pipeline_wait: Option<(u32, u32)>, // requested clocks, remaining clocks
    operand_completion: Option<u64>,
    pending_write: Option<VecDeque<Transfer>>,
    write_accepted: bool,
    write_context: (u32, Option<u8>),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Cpu(Processor),
    Dmac(Processor),
    Scu,
    Refresh,
    ResetDrain(Processor),
    WriteAccept(Processor, bool), // processor, DMAC
    WriteBuffer(Processor, bool),
}
struct BufferedWrite {
    dmac: bool,
    beats: VecDeque<Transfer>,
    context: (u32, Option<u8>),
}
struct Active {
    owner: Owner,
    transfer: Transfer,
    remaining: u32,
}
struct ScuUnit {
    cycle: crate::scu::DmaCycle,
    width: usize,
    offset: usize,
    sampled: u32,
}
#[derive(Default)]
pub(super) struct Transactions {
    journals: [RefCell<Journal>; 4],
    dmac_owner: Option<Processor>,
    dmac_yield: bool,
    previous_dmac: Option<Processor>,
    active: Option<Active>,
    // H §7.11.3: on-chip peripheral cycles do not drive or acquire the
    // external bus. Each SH7604 has its own internal/peripheral buses.
    internal: [Option<Active>; 2],
    writes: [Option<BufferedWrite>; 2],
    resetting: [bool; 2],
    reset_drains: [VecDeque<Transfer>; 2],
    reset_owner: Option<Processor>,
    reset_context: [(u32, Option<u8>); 2],
    write_errors: [Option<crate::cpu::CpuError>; 2],
    lock: Option<Processor>,
    // H §7.10: narrow operands and cache fills are indivisible even
    // though the adapter completes each physical beat separately.
    cpu_group: Option<Processor>,
    previous: Option<Processor>,
    scu: Option<crate::scu::DmaCycle>,
    // Each channel may be preempted between physical beats of its accepted
    // logical unit. Keep already sampled data and bus width per channel.
    scu_units: [Option<ScuUnit>; 4],
}

struct ScheduledMemory<'a>(CpuBus<'a>);
impl ScheduledMemory<'_> {
    fn operation(&mut self, op: Operation) -> Result<u32, BusError> {
        let i = self.0.journal_index();
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
            Operation::Clock(_) => unreachable!("clock observations are journaled directly"),
            Operation::PipelineWait(clocks) => {
                let mut journal = self.0.bus.transactions.journals[i].borrow_mut();
                journal.pipeline_wait.get_or_insert((clocks, clocks));
                Err(BusError::Pending)
            }
            Operation::MemoryAccess(address, write, completion) => {
                let synchronous = self.0.cached
                    && if write {
                        matches!(address >> 29, 2 | 3 | 6)
                    } else {
                        matches!(address >> 29, 3 | 6)
                            || self.0.bus.cache[self.0.processor as usize]
                                .borrow()
                                .read_hit(address)
                    };
                if !synchronous && !write {
                    self.0.bus.transactions.journals[i]
                        .borrow_mut()
                        .operand_completion = Some(completion);
                }
                Ok(u32::from(!synchronous))
            }
            Operation::DmaRead(a, first, last) => self.0.dma_read32(a, first, last),
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
            Operation::DiscardPrefetch => {
                self.0.sh2_discard_prefetch();
                Ok(0)
            }
            Operation::WriteContext(pc, vector) => {
                self.0.bus.transactions.journals[i]
                    .borrow_mut()
                    .write_context = (pc, vector);
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
    fn sh2_clock(&self) -> Option<u64> {
        let memory = self.0.borrow_mut();
        let mut journal = memory.0.bus.transactions.journals[memory.0.journal_index()].borrow_mut();
        let cursor = journal.cursor;
        let clock = if let Some(&(Operation::Clock(clock), _)) = journal.operations.get(cursor) {
            clock
        } else {
            assert_eq!(
                cursor,
                journal.operations.len(),
                "replay lost a clock observation"
            );
            let clock = memory.0.bus.main_clocks;
            journal.operations.push((Operation::Clock(clock), Ok(0)));
            clock
        };
        journal.cursor += 1;
        Some(clock)
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_schedule_memory_access(&self, address: u32, write: bool, completion: u64) -> bool {
        self.0
            .borrow_mut()
            .operation(Operation::MemoryAccess(address, write, completion))
            .unwrap()
            != 0
    }
    fn sh2_write_context(&self, pc: u32, vector: Option<u8>) {
        self.0
            .borrow_mut()
            .operation(Operation::WriteContext(pc, vector))
            .unwrap();
    }
    fn sh2_discard_prefetch(&self) {
        self.0
            .borrow_mut()
            .operation(Operation::DiscardPrefetch)
            .unwrap();
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        if clocks == 0 {
            return Ok(());
        }
        self.0
            .get_mut()
            .operation(Operation::PipelineWait(clocks))
            .map(|_| ())
    }
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
    fn dma_read32(&self, a: u32, first: bool, last: bool) -> Result<u32, BusError> {
        self.0
            .borrow_mut()
            .operation(Operation::DmaRead(a, first, last))
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
    // ST-077 §3.1/§3.2: a buffered main-interface request precedes SCPU
    // memory access. Device/DMA/refresh ownership can still delay this beat.
    pub(super) fn sound_ram_request(&self) -> Option<crate::sound::memory::Request> {
        let active = self.transactions.active.as_ref()?;
        let address = active.transfer.address & 0x1FFF_FFFF;
        if matches!(active.owner, Owner::WriteAccept(_, _))
            || !(crate::sound::SOUND_RAM..crate::sound::SCSP).contains(&address)
        {
            return None;
        }
        Some(crate::sound::memory::Request {
            address: (address - crate::sound::SOUND_RAM) as usize,
            width: active.transfer.width,
            write: active.transfer.write,
        })
    }
    /// H §§4.2.3/7.11.1: power-on reset aborts; manual reset completes
    /// the BSC's current operand, stopping a refill/16-byte DMA at a longword.
    /// Retain physical phases, not an interpreter that could issue more work.
    pub(crate) fn hold_cpu_reset(&mut self, processor: Processor) {
        let i = processor as usize;
        if self.transactions.resetting[i] {
            return;
        }
        let mut active = None;
        let mut drain = VecDeque::new();
        if self.onchip[i].reset_kind() == Some(crate::onchip::ResetRequest::Manual) {
            if self.transactions.active.as_ref().is_some_and(|a| {
                matches!(a.owner, Owner::Cpu(p) | Owner::Dmac(p) | Owner::WriteBuffer(p, _) if p == processor)
            }) {
                active = self.transactions.active.take();
            }
            let admitting = self.transactions.internal[i]
                .as_ref()
                .is_some_and(|a| matches!(a.owner, Owner::WriteAccept(_, _)));
            if let Some(write) = &self.transactions.writes[i]
                && (!admitting || active.is_some())
            {
                drain = write.beats.clone();
                if active.is_some() {
                    drain.pop_front();
                }
            } else {
                let completed = if self.transactions.cpu_group == Some(processor) {
                    self.transactions.journals[i]
                        .borrow()
                        .beats
                        .last()
                        .map(|(t, _)| *t)
                } else if self.transactions.dmac_owner == Some(processor) {
                    self.transactions.journals[i + 2]
                        .borrow()
                        .beats
                        .last()
                        .map(|(t, _)| *t)
                } else {
                    None
                };
                if let Some(t) = active.as_ref().map(|a| a.transfer).or(completed) {
                    let mut address = t.address + t.width as u32;
                    while address < t.unit_end {
                        drain.push_back(Transfer {
                            address,
                            first: false,
                            last: address + t.width as u32 == t.unit_end,
                            locked: false,
                            ..t
                        });
                        address += t.width as u32;
                    }
                }
            }
        }
        let context = self.transactions.writes[i].as_ref().map_or_else(
            || self.transactions.journals[i].borrow().write_context,
            |write| write.context,
        );
        self.cancel_cpu_transaction(processor);
        self.cancel_dmac_transaction(processor);
        self.transactions.reset_context[i] = context;
        if self.transactions.lock == Some(processor) {
            self.transactions.lock = None;
        }
        if self.transactions.cpu_group == Some(processor) {
            self.transactions.cpu_group = None;
        }
        self.transactions.resetting[i] = true;
        self.transactions.reset_drains[i] = drain;
        if let Some(reset) = self.onchip[i].reset_kind() {
            // Initialize modules at assertion, retaining the watchdog pulse.
            // A power reset also immediately releases BSC self-refresh.
            self.onchip[i].reset_from_watchdog(reset);
            self.cache[i].get_mut().reset_control();
        }
        if let Some(mut active) = active {
            active.owner = Owner::ResetDrain(processor);
            self.transactions.active = Some(active);
            self.transactions.reset_owner = Some(processor);
        }
    }
    pub(crate) fn reset_drain_complete(&self, processor: Processor) -> bool {
        self.transactions.reset_drains[processor as usize].is_empty()
            && self.transactions.write_errors[processor as usize].is_none()
            && self.transactions.reset_owner != Some(processor)
    }
    fn grant_reset_drain(&mut self, processor: Processor) {
        let Some(transfer) = self.transactions.reset_drains[processor as usize].pop_front() else {
            self.transactions.reset_owner = None;
            return;
        };
        self.grant_cpu_beat(processor, transfer, false);
        self.transactions.active.as_mut().unwrap().owner = Owner::ResetDrain(processor);
        self.transactions.cpu_group = None;
        self.transactions.reset_owner = Some(processor);
    }
    pub(super) fn vdp1_bus_ownership(&self) -> (bool, bool) {
        let Some(active) = &self.transactions.active else {
            return (false, false);
        };
        if active.owner == Owner::Refresh {
            return (false, false);
        }
        let address = Bus::physical(active.transfer.address);
        (
            (vdp1::VRAM..vdp1::FRAMEBUFFER).contains(&address),
            (vdp1::FRAMEBUFFER..vdp1::REGISTERS).contains(&address),
        )
    }
    fn external_monitor_owns_internal(&self, processor: Processor) -> bool {
        self.onchip[processor as usize].monitors_external_bus()
            && (self
                .transactions
                .active
                .as_ref()
                .is_some_and(|a| match a.owner {
                    Owner::Cpu(p)
                    | Owner::Dmac(p)
                    | Owner::WriteBuffer(p, _)
                    | Owner::ResetDrain(p) => p != processor,
                    Owner::WriteAccept(_, _) => false,
                    Owner::Scu => true,
                    Owner::Refresh => false,
                })
                || [
                    self.transactions.cpu_group,
                    self.transactions.lock,
                    self.transactions.dmac_owner,
                    self.transactions.reset_owner,
                ]
                .into_iter()
                .flatten()
                .any(|p| p != processor))
    }
    fn external_monitor_waits_for_internal(&self) -> bool {
        [Processor::Master, Processor::Slave].into_iter().any(|p| {
            self.onchip[p as usize].monitors_external_bus()
                && self.transactions.internal[p as usize].is_some()
        })
    }
    fn prepare_dmac(&mut self, processor: Processor) {
        let i = processor as usize;
        if (i == 1 && !self.slave_enabled)
            || self.onchip[i].reset_active()
            || self.onchip[i].dma_stopped()
        {
            return;
        }
        self.transactions.journals[i + 2].get_mut().cursor = 0;
        let requests = self.onchip[i].serial_dma_requests();
        let mut dma = std::mem::take(&mut self.onchip[i].dma);
        let acknowledged = dma.transfer(
            &mut ReplayMemory(RefCell::new(ScheduledMemory(CpuBus {
                bus: self,
                processor,
                cached: false,
                scheduled: true,
            }))),
            requests,
        );
        self.cpu_dma_errors[i] |= std::mem::take(&mut dma.address_error);
        let in_progress = dma.in_progress();
        let burst = dma.burst_requested(requests);
        self.onchip[i].dma = dma;
        if let Some(source) = acknowledged {
            self.onchip[i].acknowledge_serial_dma(source);
        }
        if !in_progress {
            *self.transactions.journals[i + 2].get_mut() = Journal::default();
            if self.transactions.dmac_owner == Some(processor) && !burst {
                self.transactions.dmac_owner = None;
                self.transactions.dmac_yield = acknowledged.is_some();
            }
        }
    }
    pub(super) fn cancel_dmac_transaction(&mut self, processor: Processor) {
        if self.transactions.internal[processor as usize]
            .as_ref()
            .is_some_and(|a| matches!(a.owner, Owner::Dmac(p) | Owner::WriteAccept(p, true) if p == processor))
        {
            self.transactions.internal[processor as usize] = None;
        }
        if self.transactions.active.as_ref().is_some_and(
            |a| matches!(a.owner, Owner::Dmac(p) | Owner::WriteBuffer(p, true) if p == processor),
        ) {
            self.transactions.active = None;
        }
        *self.transactions.journals[processor as usize + 2].get_mut() = Journal::default();
        if self.transactions.writes[processor as usize]
            .as_ref()
            .is_some_and(|w| w.dmac)
        {
            self.transactions.writes[processor as usize] = None;
        }
        if self.transactions.dmac_owner == Some(processor) {
            self.transactions.dmac_owner = None;
        }
    }
    fn grant_dmac(&mut self) -> bool {
        let order = if let Some(p) = self.transactions.dmac_owner {
            [p, p]
        } else if self.transactions.previous_dmac == Some(Processor::Master) {
            [Processor::Slave, Processor::Master]
        } else {
            [Processor::Master, Processor::Slave]
        };
        for processor in order {
            if self.onchip[processor as usize].reset_active() {
                continue;
            }
            if self.transactions.writes[processor as usize]
                .as_ref()
                .is_some_and(|w| w.dmac)
            {
                return self.grant_buffered_write(processor);
            }
            if self.transactions.internal[processor as usize].is_some()
                || self.transactions.writes[processor as usize].is_some()
            {
                continue;
            }
            self.prepare_dmac(processor);
            let Some(transfer) = self.transactions.journals[processor as usize + 2]
                .borrow()
                .waiting
            else {
                continue;
            };
            if transfer.address >= 0xFFFF_FE00 {
                continue;
            }
            self.transactions.dmac_owner = Some(processor);
            self.transactions.previous_dmac = Some(processor);
            self.grant_cpu_beat(processor, transfer, true);
            return true;
        }
        false
    }
    fn grant_cpu_beat(&mut self, processor: Processor, transfer: Transfer, dmac: bool) {
        if !dmac && !transfer.last {
            self.transactions.cpu_group = Some(processor);
        }
        self.for_cpu(processor).reserve_external(
            transfer.address,
            transfer.width,
            transfer.write.is_some(),
            transfer.first,
            transfer.last,
        );
        let i = processor as usize;
        let mut remaining = self.cpu_bus_clocks[i]
            .replace(0)
            .saturating_add(self.onchip[i].take_wait_clocks())
            .max(1);
        if !dmac && transfer.last {
            remaining = remaining.max(self.operand_completion_wait(processor));
        }
        self.transactions.active = Some(Active {
            owner: if dmac {
                Owner::Dmac(processor)
            } else {
                Owner::Cpu(processor)
            },
            transfer,
            remaining,
        });
    }
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
        journal.pipeline_wait = None;
        journal.operand_completion = None;
        journal.pending_write = None;
        journal.write_accepted = false;
        if self.transactions.lock == Some(processor) {
            self.transactions.lock = None;
        }
        if self.transactions.cpu_group == Some(processor)
            && self.transactions.writes[processor as usize].is_none()
        {
            self.transactions.cpu_group = None;
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
        self.transactions.scu_units = std::array::from_fn(|_| None);
    }
    pub(crate) fn cancel_cpu_transaction(&mut self, processor: Processor) {
        self.transactions.resetting[processor as usize] = false;
        self.transactions.reset_drains[processor as usize].clear();
        if self.transactions.reset_owner == Some(processor) {
            self.transactions.reset_owner = None;
        }
        self.instruction_word[processor as usize].set(None);
        self.transactions.write_errors[processor as usize] = None;
        if self.transactions.internal[processor as usize]
            .as_ref()
            .is_some_and(|a| matches!(a.owner, Owner::Cpu(p) | Owner::WriteAccept(p, false) if p == processor))
        {
            self.transactions.internal[processor as usize] = None;
        }
        if self.transactions.active.as_ref().is_some_and(
            |a| matches!(a.owner, Owner::Cpu(p) | Owner::WriteBuffer(p, false) | Owner::ResetDrain(p) if p == processor),
        ) {
            self.transactions.active = None;
        }
        if self.transactions.writes[processor as usize]
            .as_ref()
            .is_some_and(|w| !w.dmac)
        {
            self.transactions.writes[processor as usize] = None;
        }
        self.finish_cpu_transaction(processor);
    }
    fn accept_buffered_write(&mut self, processor: Processor, dmac: bool) -> bool {
        let i = processor as usize;
        let journal = self.transactions.journals[i + if dmac { 2 } else { 0 }].get_mut();
        if self.transactions.writes[i].is_some() || journal.pending_write.is_none() {
            return false;
        }
        // The internal-bus acceptance state precedes notification. The BSC
        // owns the external data until its final physical beat completes.
        self.transactions.internal[i] = Some(Active {
            owner: Owner::WriteAccept(processor, dmac),
            transfer: *journal.pending_write.as_ref().unwrap().front().unwrap(),
            remaining: 1,
        });
        self.transactions.writes[i] = Some(BufferedWrite {
            dmac,
            beats: journal.pending_write.take().unwrap(),
            context: journal.write_context,
        });
        true
    }
    fn grant_buffered_write(&mut self, processor: Processor) -> bool {
        if self.transactions.lock.is_some_and(|p| p != processor) {
            return false;
        }
        let Some(write) = &self.transactions.writes[processor as usize] else {
            return false;
        };
        let transfer = *write.beats.front().unwrap();
        let dmac = write.dmac;
        self.grant_cpu_beat(processor, transfer, dmac);
        self.transactions.active.as_mut().unwrap().owner = Owner::WriteBuffer(processor, dmac);
        self.transactions.previous = Some(processor);
        true
    }
    pub(crate) fn take_cpu_transaction_completion(&self, processor: Processor) -> bool {
        std::mem::take(
            &mut self.transactions.journals[processor as usize]
                .borrow_mut()
                .completed,
        )
    }
    pub(crate) fn take_cpu_write_error(
        &mut self,
        processor: Processor,
    ) -> Option<crate::cpu::CpuError> {
        self.transactions.write_errors[processor as usize].take()
    }
    fn grant_cpu_internal(&mut self) {
        // H §§7.11.2–7.11.3/9.3: CPU and DMAC share this
        // processor's peripheral bus; the DMAC wins a simultaneous request.
        // Peripheral cycles are independent of the external bus unless UBC
        // monitoring reserves their internal bus for an external owner.
        // An already granted access cannot be displaced by a later request.
        for processor in [Processor::Master, Processor::Slave] {
            let i = processor as usize;
            if self.onchip[i].reset_active()
                || self.transactions.internal[i].is_some()
                || self.external_monitor_owns_internal(processor)
                || (self.transactions.writes[i].is_none()
                    && !self.onchip[i].dma_stopped()
                    && !self.onchip[i].reset_active()
                    && self.onchip[i]
                        .dma
                        .requests_internal_bus(self.onchip[i].serial_dma_requests()))
                || self.transactions.active.as_ref().is_some_and(
                    |a| matches!(a.owner, Owner::Cpu(p) | Owner::Dmac(p) if p == processor),
                )
            {
                continue;
            }
            if self.accept_buffered_write(processor, false) {
                continue;
            }
            let waiting = self.transactions.journals[i].borrow().waiting;
            if let Some(transfer) = waiting.filter(|t| t.address >= 0xFFFF_FE00) {
                self.transactions.internal[i] = Some(Active {
                    owner: Owner::Cpu(processor),
                    transfer,
                    // CPU reads first determine the cycle on the cache bus;
                    // writes and DMAC cycles need no such extra state.
                    remaining: ((if transfer.write.is_some() { 1 } else { 2 })
                        + self.onchip[i].cpu_peripheral_wait(
                            transfer.address,
                            transfer.width,
                            transfer.write.is_some(),
                        ))
                    .max(self.operand_completion_wait(processor)),
                });
            }
        }
    }
    pub(crate) fn grant_bus_transaction(&mut self) {
        // Drop canceled DRAM work before allocating a replacement bus beat.
        // A retained manual-reset drain is still active and keeps its grant.
        self.sound.main_ram_request(self.sound_ram_request());
        self.allocate_bus_transaction();
        self.sound.main_ram_request(self.sound_ram_request());
    }
    fn allocate_bus_transaction(&mut self) {
        for processor in [Processor::Master, Processor::Slave] {
            let i = processor as usize;
            if self.onchip[i].reset_active()
                || self.transactions.internal[i].is_some()
                || self.external_monitor_owns_internal(processor)
                || self.transactions.active.as_ref().is_some_and(
                    |a| matches!(a.owner, Owner::Cpu(p) | Owner::Dmac(p) if p == processor),
                )
            {
                continue;
            }
            let waiting = self.transactions.journals[i + 2].borrow().waiting;
            let pending_write = self.transactions.journals[i + 2]
                .borrow()
                .pending_write
                .is_some();
            if waiting.is_some_and(|t| t.address < 0xFFFF_FE00)
                || (waiting.is_none()
                    && !pending_write
                    && !self.onchip[i]
                        .dma
                        .internal_request(self.onchip[i].serial_dma_requests()))
            {
                // Do not accept a new external request merely to find its
                // address while another master owns the external bus.
                continue;
            }
            self.prepare_dmac(processor);
            if self.accept_buffered_write(processor, true) {
                continue;
            }
            let transfer = self.transactions.journals[i + 2].borrow().waiting;
            if let Some(transfer) = transfer.filter(|t| t.address >= 0xFFFF_FE00) {
                // H §9.3.5: internal peripheral access is one state, with
                // neither external T1/T2 nor BSC software/turnaround waits.
                self.transactions.internal[i] = Some(Active {
                    owner: Owner::Dmac(processor),
                    transfer,
                    remaining: 1 + self.onchip[i].dma_wait_clocks(transfer.address, transfer.width),
                });
                if self.transactions.dmac_owner == Some(processor) {
                    self.transactions.dmac_owner = None;
                }
            }
        }
        self.grant_cpu_internal();
        if self.transactions.active.is_some() {
            return;
        }
        if let Some(processor) = self.transactions.reset_owner {
            if !self.external_monitor_waits_for_internal() {
                self.grant_reset_drain(processor);
            }
            return;
        }
        if let Some(processor) = self.transactions.cpu_group {
            if self.transactions.writes[processor as usize].is_some() {
                if !self.external_monitor_waits_for_internal() {
                    self.grant_buffered_write(processor);
                }
                return;
            }
            // The interpreter resumes the same operand to queue the next
            // beat. An empty queue here is not a bus release.
            let transfer = self.transactions.journals[processor as usize]
                .borrow()
                .waiting;
            if let Some(transfer) = transfer
                && !self.external_monitor_waits_for_internal()
            {
                self.grant_cpu_beat(processor, transfer, false);
            }
            self.grant_cpu_internal();
            return;
        }
        // A narrow operand already accepted by the BSC finishes before
        // self-refresh can inhibit a new external request (§§7.5.7/7.10).
        if self.onchip[0].inhibits_external_bus() {
            self.grant_cpu_internal();
            return;
        }
        if self.transactions.lock.is_none()
            && self.transactions.dmac_owner.is_none()
            && let Some(remaining) = self.onchip[0].grant_refresh()
        {
            // H §7.10: refresh precedes simultaneous external-master,
            // DMAC and CPU requests. A latched request cannot age while
            // waiting for a burst, narrow operand, cache fill or TAS.
            self.release_external_bus();
            self.scu.refresh_abus();
            self.transactions.active = Some(Active {
                owner: Owner::Refresh,
                transfer: Transfer {
                    address: 0,
                    width: 0,
                    write: None,
                    first: true,
                    last: true,
                    unit_end: 0,
                    locked: false,
                    bridge_read: false,
                    bridge_write: None,
                },
                remaining,
            });
            self.grant_cpu_internal();
            return;
        }
        if self.external_monitor_waits_for_internal() {
            // Do not pass the external bus to a new master until an
            // already accepted peripheral cycle has released its monitor bus.
            // Refresh is BSC-local and does not drive an external data cycle.
            self.grant_cpu_internal();
            return;
        }
        if self.transactions.lock.is_none()
            && self.transactions.dmac_owner.is_some()
            && self.grant_dmac()
        {
            self.grant_cpu_internal();
            return;
        }
        if self.transactions.lock.is_none()
            && let Some(cycle) = self.scu.prepare_dma_cycle(self.cpu_ab_write_pending())
        {
            let mut first = true;
            let mut last = true;
            let cycle = if cycle.width == 0 {
                cycle
            } else {
                let physical_width = self
                    .scu
                    .bus_width(cycle.address)
                    .unwrap_or_else(|| self.onchip[0].external_bus_width(cycle.address))
                    .min(cycle.width);
                let unit = self.transactions.scu_units[cycle.channel].get_or_insert(ScuUnit {
                    cycle,
                    width: physical_width,
                    offset: 0,
                    sampled: 0,
                });
                first = unit.offset == 0;
                last = unit.offset + unit.width == unit.cycle.width;
                crate::scu::DmaCycle {
                    channel: cycle.channel,
                    address: unit.cycle.address + unit.offset as u32,
                    width: unit.width,
                    value: unit.cycle.value.map(|v| {
                        let shift = (unit.cycle.width - unit.offset - unit.width) * 8;
                        (v >> shift)
                            & if unit.width == 4 {
                                u32::MAX
                            } else {
                                (1 << (unit.width * 8)) - 1
                            }
                    }),
                }
            };
            let mut remaining = 2;
            if cycle.width != 0 {
                let write = cycle.value.is_some();
                if self.external_owner.get().is_some() {
                    self.release_external_bus();
                }
                // SCU accesses are physical, with the master BSC driving
                // CPU-bus RAM and the SCU driving A/B-bus selects.
                let base =
                    self.onchip[0].external_access(cycle.address, cycle.width, write, first, last);
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
                    first,
                    last,
                    unit_end: 0,
                    locked: false,
                    bridge_read: false,
                    bridge_write: None,
                },
                remaining,
            });
            self.grant_cpu_internal();
            return;
        }
        if self.transactions.lock.is_none() && !self.transactions.dmac_yield && self.grant_dmac() {
            self.grant_cpu_internal();
            return;
        }
        self.grant_cpu_internal();
        // Round-robin at beat boundaries; TAS is the only retained CPU lock.
        let order = if self.transactions.previous == Some(Processor::Master) {
            [Processor::Slave, Processor::Master]
        } else {
            [Processor::Master, Processor::Slave]
        };
        for processor in order {
            if !self.transactions.reset_drains[processor as usize].is_empty()
                && self
                    .transactions
                    .lock
                    .is_none_or(|owner| owner == processor)
            {
                self.grant_reset_drain(processor);
                return;
            }
            if self.onchip[processor as usize].reset_active() {
                continue;
            }
            if self.transactions.writes[processor as usize]
                .as_ref()
                .is_some_and(|w| !w.dmac)
                && self.grant_buffered_write(processor)
            {
                self.transactions.dmac_yield = false;
                return;
            }
            if self.transactions.internal[processor as usize].is_some()
                || self.transactions.writes[processor as usize].is_some()
            {
                continue;
            }
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
            if transfer.address >= 0xFFFF_FE00 {
                continue;
            }
            if transfer.locked {
                self.transactions.lock = Some(processor);
            }
            self.grant_cpu_beat(processor, transfer, false);
            self.transactions.dmac_yield = false;
            self.transactions.previous = Some(processor);
            return;
        }
        // Sleeping/cache-only CPUs need no external cycle. Cycle-steal must
        // continue in that case, without introducing a fabricated CPU access.
        if self.transactions.lock.is_none() {
            self.transactions.dmac_yield = false;
            self.grant_dmac();
        }
    }
    fn cpu_ab_write_pending(&self) -> bool {
        // ST-210 §3 item12. Inspect both SH-2 queues at this arbitration
        // opportunity, including a second write waiting for buffer admission.
        // SCU registers, CPU-bus RAM and SH-2 DMAC writes do not qualify.
        let qualifies = |t: &Transfer| {
            t.write.is_some() && (0x0200_0000..0x05FC_0000).contains(&Self::physical(t.address))
        };
        (0..2).any(|i| {
            self.transactions.writes[i]
                .as_ref()
                .is_some_and(|w| !w.dmac && w.beats.iter().any(qualifies))
                || self.transactions.reset_drains[i].iter().any(qualifies)
                || {
                    let journal = self.transactions.journals[i].borrow();
                    journal.waiting.as_ref().is_some_and(qualifies)
                        || journal
                            .pending_write
                            .as_ref()
                            .is_some_and(|w| w.iter().any(qualifies))
                }
        })
    }
    fn operand_completion_wait(&self, processor: Processor) -> u32 {
        self.transactions.journals[processor as usize]
            .borrow()
            .operand_completion
            .unwrap_or(0)
            .saturating_sub(self.main_clocks) as u32
    }
    pub(crate) fn bus_transaction_deadline(&self) -> u32 {
        self.system_clock.deadline(self.bus_main_deadline()).max(1)
    }
    pub(super) fn bus_main_deadline(&self) -> u32 {
        let bus = self
            .transactions
            .active
            .iter()
            .chain(self.transactions.internal.iter().flatten())
            .map(|a| a.remaining)
            .min()
            .unwrap_or(u32::MAX);
        self.transactions.journals[..2]
            .iter()
            .filter_map(|j| j.borrow().pipeline_wait.map(|(_, remaining)| remaining))
            .fold(bus, u32::min)
    }
    pub(super) fn advance_bus_transaction(&mut self, clocks: u32) -> Result<(), ScuError> {
        // Decrement every already-granted phase before completing any of
        // them: a completion may queue a new phase, never retroactively age it.
        for active in self
            .transactions
            .internal
            .iter_mut()
            .flatten()
            .chain(self.transactions.active.iter_mut())
        {
            active.remaining = active.remaining.saturating_sub(clocks);
        }
        for journal in &mut self.transactions.journals[..2] {
            let journal = journal.get_mut();
            if let Some((requested, remaining)) = journal.pipeline_wait {
                let remaining = remaining.saturating_sub(clocks);
                journal.pipeline_wait = if remaining == 0 {
                    journal
                        .operations
                        .push((Operation::PipelineWait(requested), Ok(0)));
                    journal.completed = true;
                    None
                } else {
                    Some((requested, remaining))
                };
            }
        }
        for i in 0..2 {
            if self.transactions.internal[i]
                .as_ref()
                .is_some_and(|a| a.remaining == 0)
            {
                let active = self.transactions.internal[i].take().unwrap();
                self.complete_bus_transaction(active)?;
            }
        }
        if self
            .transactions
            .active
            .as_ref()
            .is_some_and(|a| a.remaining == 0)
        {
            let sound_memory = self.transactions.active.as_ref().is_some_and(|active| {
                let physical = active.transfer.address & 0x1FFF_FFFF;
                (crate::sound::SOUND_RAM..crate::sound::SCSP).contains(&physical)
            });
            if sound_memory && !self.sound.main_ram_ready() {
                // Keep the accepted beat and sampled halves intact until the
                // retained memory result is ready. Recheck on the next main-clock edge;
                // PLL selection must not convert a reference wait into an
                // incorrect number of main clocks.
                self.transactions.active.as_mut().unwrap().remaining = 1;
                return Ok(());
            }
            let active = self.transactions.active.take().unwrap();
            self.complete_bus_transaction(active)?;
        }
        Ok(())
    }
    fn complete_bus_transaction(&mut self, active: Active) -> Result<(), ScuError> {
        if active.owner == Owner::Refresh {
            return Ok(());
        }
        if let Owner::WriteAccept(processor, dmac) = active.owner {
            let i = processor as usize;
            let journal = self.transactions.journals[i + if dmac { 2 } else { 0 }].get_mut();
            journal.write_accepted = true;
            journal.completed = true;
            if dmac {
                self.prepare_dmac(processor);
            }
            return Ok(());
        }
        let t = active.transfer;
        let processor = match active.owner {
            Owner::Cpu(p) | Owner::Dmac(p) | Owner::WriteBuffer(p, _) | Owner::ResetDrain(p) => p,
            Owner::WriteAccept(_, _) => unreachable!(),
            Owner::Scu | Owner::Refresh => Processor::Master,
        };
        let result = if (crate::sound::SOUND_RAM..crate::sound::SCSP)
            .contains(&(t.address & 0x1FFF_FFFF))
        {
            self.sound.complete_main_ram()
        } else if matches!(active.owner, Owner::Cpu(_)) && t.address >= 0xFFFF_FE00 {
            let mut memory = self.for_cpu(processor);
            if let Some(value) = t.write {
                match t.width {
                    1 => memory.write8(t.address, value as u8),
                    2 => memory.write16(t.address, value as u16),
                    _ => memory.write32(t.address, value),
                }
                .map(|_| value)
            } else {
                match t.width {
                    1 => memory.read8(t.address).map(u32::from),
                    2 => memory.read16(t.address).map(u32::from),
                    _ => memory.read32(t.address),
                }
            }
        } else if matches!(active.owner, Owner::Dmac(_)) && t.address >= 0xFFFF_FE00 {
            let chip = &mut self.onchip[processor as usize];
            if let Some(value) = t.write {
                match t.width {
                    1 => chip.write8(t.address, value as u8),
                    2 => chip.write16(t.address, value as u16),
                    _ => chip.dma_write32(t.address, value),
                }
                .map(|_| value)
            } else {
                match t.width {
                    1 => chip.read8(t.address).map(u32::from),
                    2 => chip.read16(t.address).map(u32::from),
                    _ => chip.dma_read32(t.address),
                }
            }
        } else if t.width == 0 {
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
        if let Owner::ResetDrain(processor) = active.owner {
            if let Ok(value) = result
                && !t.bridge_read
            {
                self.for_cpu(processor).observe_write_phase(t, value);
            }
            // No architectural instruction or next burst unit resumes on reset.
            if let Err(source) = result {
                let i = processor as usize;
                let (pc, vector) = self.transactions.reset_context[i];
                self.transactions.write_errors[i] = Some(match vector {
                    Some(vector) => crate::cpu::CpuError::ExceptionEntry { pc, vector, source },
                    None => crate::cpu::CpuError::Bus { pc, source },
                });
                self.transactions.reset_drains[i].clear();
            }
            if self.transactions.reset_drains[processor as usize].is_empty() {
                self.transactions.reset_owner = None;
            }
        } else if let Owner::WriteBuffer(processor, dmac) = active.owner {
            // Invalid device accesses remain observable diagnostics even when
            // the issuing CPU has retired; they never become silent no-ops.
            let value = match result {
                Ok(value) => value,
                Err(source) => {
                    let i = processor as usize;
                    let (pc, vector) = self.transactions.writes[i].take().unwrap().context;
                    if dmac {
                        self.onchip[i].dma.buffer_error();
                        self.cpu_dma_errors[i] = true;
                        self.cancel_dmac_transaction(processor);
                    } else {
                        self.transactions.write_errors[i] = Some(match vector {
                            Some(vector) => {
                                crate::cpu::CpuError::ExceptionEntry { pc, vector, source }
                            }
                            None => crate::cpu::CpuError::Bus { pc, source },
                        });
                    }
                    if self.transactions.cpu_group == Some(processor) {
                        self.transactions.cpu_group = None;
                    }
                    return Ok(());
                }
            };
            self.for_cpu(processor).observe_write_phase(t, value);
            let i = processor as usize;
            let write = self.transactions.writes[i].as_mut().unwrap();
            assert_eq!(write.beats.pop_front(), Some(t));
            if write.beats.is_empty() {
                self.transactions.writes[i] = None;
                if self.transactions.cpu_group == Some(processor) {
                    self.transactions.cpu_group = None;
                }
            }
        } else if active.owner == Owner::Scu {
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
            if cycle.width == 0 {
                self.scu.complete_dma_cycle(cycle, result)?;
            } else {
                let unit = self.transactions.scu_units[cycle.channel].as_mut().unwrap();
                if let Ok(value) = result {
                    unit.sampled = if cycle.width == 4 {
                        value
                    } else {
                        unit.sampled << (cycle.width * 8) | value
                    };
                    unit.offset += cycle.width;
                }
                if result.is_err() || unit.offset == unit.cycle.width {
                    let unit = self.transactions.scu_units[cycle.channel].take().unwrap();
                    self.scu
                        .complete_dma_cycle(unit.cycle, result.map(|_| unit.sampled))?;
                }
            }
        } else {
            if matches!(active.owner, Owner::Cpu(_))
                && (t.last || result.is_err())
                && self.transactions.cpu_group == Some(processor)
            {
                self.transactions.cpu_group = None;
            }
            if let Ok(value) = result {
                if !t.bridge_read {
                    self.for_cpu(processor).observe_external_data(
                        t.address,
                        t.width,
                        value,
                        t.write.is_some(),
                    );
                } else if t.address & 2 != 0 {
                    let (previous, high) = *self.transactions.journals[processor as usize]
                        .borrow()
                        .beats
                        .last()
                        .expect("bridge high word");
                    assert!(previous.bridge_read && previous.address + 2 == t.address);
                    if let Ok(high) = high {
                        self.for_cpu(processor).observe_external_data(
                            t.address - 2,
                            4,
                            high << 16 | value,
                            false,
                        );
                    }
                }
            }
            let index = processor as usize
                + if matches!(active.owner, Owner::Dmac(_)) {
                    2
                } else {
                    0
                };
            let journal = self.transactions.journals[index].get_mut();
            journal.beats.push((t, result));
            journal.waiting = None;
            journal.completed = true;
            if matches!(active.owner, Owner::Dmac(_)) {
                self.prepare_dmac(processor);
            }
        }
        Ok(())
    }
}
impl CpuBus<'_> {
    fn observe_write_phase(&self, t: Transfer, value: u32) {
        if let Some(payload) = t.bridge_write {
            if t.address + t.width as u32 == t.unit_end {
                self.observe_external_data(t.unit_end - 4, 4, payload, true);
            }
        } else {
            self.observe_external_data(t.address, t.width, value, t.write.is_some());
        }
    }
    pub(super) fn buffered_write_locked(&self) -> bool {
        self.bus.transactions.lock == Some(self.processor)
    }
    pub(super) fn buffered_external_write<const N: usize>(
        &self,
        address: u32,
        bytes: [u8; N],
    ) -> Result<(), BusError> {
        let mut journal = self.bus.transactions.journals[self.journal_index()].borrow_mut();
        if journal.write_accepted {
            journal.write_accepted = false;
            return Ok(());
        }
        let cpu_width = self.bus.onchip[self.processor as usize]
            .external_bus_width(Bus::physical(address))
            .min(N);
        // ST-097 §3.2: memory devices transfer at most one word per phase.
        // ST-210 item32 preserves narrow strobes; item03 excludes VDP1 MMIO.
        let bridge = cpu_width == 4 && Self::video_memory(address);
        let width = if bridge { 2 } else { cpu_width };
        let payload = bridge.then(|| bytes.iter().fold(0, |v, b| v << 8 | u32::from(*b)));
        journal.pending_write.get_or_insert_with(|| {
            bytes
                .chunks(width)
                .enumerate()
                .map(|(i, bytes)| Transfer {
                    address: address + (i * width) as u32,
                    width,
                    write: Some(bytes.iter().fold(0, |v, b| v << 8 | u32::from(*b))),
                    first: i == 0,
                    last: (i + 1) * width == N,
                    unit_end: address + N as u32,
                    locked: false,
                    bridge_read: false,
                    bridge_write: payload,
                })
                .collect()
        });
        Err(BusError::Pending)
    }
    fn journal_index(&self) -> usize {
        self.processor as usize + if self.cached { 0 } else { 2 }
    }
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
            unit_end: address + width as u32,
            locked,
            bridge_read: false,
            bridge_write: None,
        };
        self.scheduled_transfer(transfer)
    }
    pub(super) fn scheduled_transfer(&self, transfer: Transfer) -> Result<u32, BusError> {
        let mut journal = self.bus.transactions.journals[self.journal_index()].borrow_mut();
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
    fn pipeline_waits_are_cpu_local_replayed_once_and_canceled_by_reset() {
        for p in [Processor::Master, Processor::Slave] {
            let other = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            let mut b = Bus::default();
            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(0), Ok(()));
            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(5), Err(BusError::Pending));
            // The wait has no external/internal bus owner. Another CPU's
            // SCI write finishes in one cycle while this CPU is stalled.
            assert_eq!(
                write(&mut b, other, 0xFFFFFE01, 0x5A),
                Err(BusError::Pending)
            );
            b.grant_bus_transaction();
            assert!(b.transactions.active.is_none());
            assert!(b.transactions.internal[p as usize].is_none());
            b.advance_clocks(1).unwrap();
            assert_eq!(write(&mut b, other, 0xFFFFFE01, 0x5A), Ok(()));
            assert!(!b.take_cpu_transaction_completion(p));
            b.finish_cpu_transaction(other);
            b.advance_clocks(3).unwrap();
            assert!(!b.take_cpu_transaction_completion(p));
            assert_eq!(b.bus_transaction_deadline(), 1);
            b.advance_clocks(1).unwrap();
            assert!(b.take_cpu_transaction_completion(p));
            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(5), Ok(()));
            b.advance_clocks(100).unwrap();
            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(5), Ok(()));
            assert!(!b.take_cpu_transaction_completion(p));
            b.finish_cpu_transaction(p);

            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(5), Err(BusError::Pending));
            b.advance_clocks(2).unwrap();
            b.cancel_cpu_transaction(p);
            b.advance_clocks(20).unwrap();
            assert!(!b.take_cpu_transaction_completion(p));
            assert_eq!(b.replay_cpu(p).sh2_pipeline_wait(5), Err(BusError::Pending));
            b.system_reset();
            b.advance_clocks(20).unwrap();
            assert!(!b.take_cpu_transaction_completion(p));
        }
    }
    #[test]
    fn operand_completion_bounds_are_cpu_local_replayed_and_canceled_by_reset() {
        use crate::onchip;

        fn bounded_read(b: &mut Bus, p: Processor, end: u64) -> Result<u32, BusError> {
            let memory = b.replay_cpu(p);
            assert!(memory.sh2_schedule_memory_access(onchip::DMAC, false, end));
            memory.read32(onchip::DMAC)
        }
        for p in [Processor::Master, Processor::Slave] {
            let other = if p == Processor::Master {
                Processor::Slave
            } else {
                Processor::Master
            };
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            b.for_cpu(p).write32(onchip::DMAC, 3).unwrap();
            assert_eq!(bounded_read(&mut b, p, 5), Err(BusError::Pending));
            assert_eq!(
                write(&mut b, other, 0xFFFFFE01, 0x5A),
                Err(BusError::Pending)
            );
            b.advance_clocks(1).unwrap();
            assert_eq!(write(&mut b, other, 0xFFFFFE01, 0x5A), Ok(()));
            b.finish_cpu_transaction(other);
            b.advance_clocks(3).unwrap();
            assert!(!b.take_cpu_transaction_completion(p));
            assert_eq!(bounded_read(&mut b, p, 5), Err(BusError::Pending));
            b.for_cpu(p).write32(onchip::DMAC, 7).unwrap();
            b.advance_clocks(1).unwrap();
            assert_eq!(bounded_read(&mut b, p, 5), Ok(7));
            b.for_cpu(p).write32(onchip::DMAC, 11).unwrap();
            assert_eq!(bounded_read(&mut b, p, 5), Ok(7));
            b.finish_cpu_transaction(p);

            assert_eq!(bounded_read(&mut b, p, 100), Err(BusError::Pending));
            b.advance_clocks(1).unwrap();
            b.cancel_cpu_transaction(p);
            b.advance_clocks(20).unwrap();
            assert!(!b.take_cpu_transaction_completion(p));
            assert_eq!(b.replay_cpu(p).read32(onchip::DMAC), Err(BusError::Pending));
            b.advance_clocks(2).unwrap();
            assert_eq!(b.replay_cpu(p).read32(onchip::DMAC), Ok(11));
            b.finish_cpu_transaction(p);

            assert_eq!(bounded_read(&mut b, p, 100), Err(BusError::Pending));
            b.system_reset();
            b.set_slave_enabled(true);
            assert_eq!(b.replay_cpu(p).read32(onchip::DMAC), Err(BusError::Pending));
            b.advance_clocks(2).unwrap();
            assert_eq!(b.replay_cpu(p).read32(onchip::DMAC), Ok(0));
        }
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
    #[test]
    fn scu_cpu_bus_units_use_programmed_byte_word_and_longword_physical_deadlines() {
        // H §§7.2.2/7.3.1; SCU §2.1. Reset ordinary CS3 timing:
        // setup2, N reads of8, N writes of8 plus initial read/write IW=2.
        for (config, width, expected, completion) in [
            (0x7c, 1, 0x11bbccdd, 68),
            (0xbc, 2, 0x1122ccdd, 36),
            (0xfc, 4, 0x11223344, 20),
        ] {
            for index in 0..3 {
                for chunk in [1, 3, 8, 64] {
                    let mut b = Bus::default();
                    b.for_cpu(Processor::Master)
                        .write32(0xffffffe4, 0xa55a0000 | config)
                        .unwrap();
                    b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                    channel(&mut b, index, false);
                    elapse(&mut b, 10, chunk);
                    b.write32(HIGH_WRAM + 0x100, 0xaabbccdd).unwrap();
                    let first_write = 12 + (4 / width) * 8;
                    elapse(&mut b, first_write - 11, chunk);
                    assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
                    elapse(&mut b, 1, chunk);
                    let mask = u32::MAX << ((4 - width) * 8);
                    assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(expected & mask));
                    if width < 4 {
                        assert_eq!(b.read32(SCU + 0xa4), Ok(0));
                        elapse(&mut b, completion - first_write - 1, chunk);
                        assert_eq!(b.read32(SCU + 0xa4), Ok(0));
                        elapse(&mut b, 1, chunk);
                    }
                    assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(expected));
                    assert_eq!(b.read32(SCU + 0xa4), Ok(1 << (11 - index)));
                }
            }
        }
    }
    #[test]
    fn narrow_scu_units_retain_sampled_bytes_across_preemption_and_discard_them_on_reset() {
        for chunk in [1, 3, 8, 64] {
            let mut b = Bus::default();
            b.for_cpu(Processor::Master)
                .write32(0xffffffe4, 0xa55a007c)
                .unwrap();
            b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
            channel(&mut b, 0, false);
            elapse(&mut b, 10, chunk);
            b.write32(HIGH_WRAM + 0x100, 0xaabbccdd).unwrap();
            b.write32(HIGH_WRAM + 0x300, 0x76543210).unwrap();
            for (o, v) in [
                (0, HIGH_WRAM + 0x300),
                (4, HIGH_WRAM + 0x400),
                (8, 4),
                (12, 0x102),
                (20, 7),
                (16, 0x101),
            ] {
                b.write32(SCU + 0x40 + o, v).unwrap();
            }
            elapse(&mut b, 68, chunk);
            assert_eq!(b.read32(HIGH_WRAM + 0x400), Ok(0x76543210));
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
            assert_eq!(b.read32(SCU + 0xa4), Ok(0x200));
            elapse(&mut b, 57, chunk);
            assert_eq!(b.read32(SCU + 0xa4), Ok(0x200));
            elapse(&mut b, 1, chunk);
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x11bbccdd));
            assert_eq!(b.read32(SCU + 0xa4), Ok(0xa00));

            for reset_at in [9, 10, 17, 18, 33, 34, 43, 44, 67] {
                let mut b = Bus::default();
                b.for_cpu(Processor::Master)
                    .write32(0xffffffe4, 0xa55a007c)
                    .unwrap();
                b.write32(HIGH_WRAM + 0x100, 0x11223344).unwrap();
                channel(&mut b, 0, false);
                elapse(&mut b, reset_at, chunk);
                b.system_reset();
                b.write32(HIGH_WRAM + 0x200, 0xabcdef01).unwrap();
                elapse(&mut b, 100, chunk);
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0xabcdef01));
                assert_eq!(b.read32(SCU + 0xa4), Ok(0));
                // Default32-bit width after reset must not resume a stale
                // narrow beat, sampled prefix, width or destination write.
                b.write32(HIGH_WRAM + 0x100, 0x76543210).unwrap();
                channel(&mut b, 0, false);
                elapse(&mut b, 20, chunk);
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x76543210));
                assert_eq!(b.read32(SCU + 0xa4), Ok(0x800));
            }
        }
    }
    #[test]
    fn scu_a_bus_uses_each_selects_physical_width_waits_and_external_address_comparisons() {
        // ST-097 §3.2 tables3.10–24: NW=3 per physical beat. The
        // standard board's unpopulated cartridge/dummy selects read high.
        // H §6.3.4: an external comparison of A+3 distinguishes8-bit
        // A,A+1,A+2,A+3 from16-bit A,A+2 regardless of copied data.
        for (source, register, shift) in [
            (0x02000000, 0xb0, 16),
            (0x04000000, 0xb0, 0),
            (0x05000000, 0xb4, 0),
        ] {
            for (narrow, completion, flag) in [(0, 34, 0), (1, 56, 0x4000)] {
                for chunk in [1, 3, 8, 64] {
                    let mut b = Bus::default();
                    b.scu
                        .write32(SCU + register, (0x30 | narrow) << shift)
                        .unwrap();
                    b.onchip[0].write32(0xffffff40, source + 3).unwrap();
                    b.onchip[0].write16(0xffffff48, 0xa4).unwrap();
                    b.onchip[0].write16(0xffffff78, 0x3000).unwrap();
                    b.onchip[0].read16(0xffffff78).unwrap();
                    for (o, v) in [
                        (0, source),
                        (4, HIGH_WRAM),
                        (8, 4),
                        (12, 0x102),
                        (20, 7),
                        (16, 0x101),
                    ] {
                        b.scu.write32(SCU + o, v).unwrap();
                    }
                    elapse(&mut b, completion - 1, chunk);
                    assert_eq!(b.read32(HIGH_WRAM), Ok(0));
                    assert_eq!(b.read32(SCU + 0xa4), Ok(0));
                    elapse(&mut b, 1, chunk);
                    assert_eq!(b.read32(HIGH_WRAM), Ok(u32::MAX));
                    assert_eq!(b.read32(SCU + 0xa4), Ok(0x800));
                    assert_eq!(b.onchip[0].read16(0xffffff78).unwrap() & 0x4000, flag);
                }
            }
        }
    }
    fn sound_channel(b: &mut Bus, index: u32, read_sound: bool) {
        let (source, destination) = if read_sound {
            (crate::sound::SOUND_RAM + 0x100, HIGH_WRAM + 0x200)
        } else {
            (HIGH_WRAM + 0x100, crate::sound::SOUND_RAM + 0x200)
        };
        b.write32(source, 0x11223344).unwrap();
        for (offset, value) in [
            (0, source),
            (4, destination),
            (8, 4),
            (12, if read_sound { 0x102 } else { 0x101 }),
            (20, 7),
            (16, 0x101),
        ] {
            b.write32(SCU + index * 32 + offset, value).unwrap();
        }
    }
    fn elapse(b: &mut Bus, clocks: u32, chunk: u32) {
        let mut left = clocks;
        while left != 0 {
            let n = left.min(chunk);
            b.advance_clocks(n).unwrap();
            left -= n;
        }
    }
    #[test]
    fn scu_b_bus_reads_sample_each_halfword_at_its_deadline_and_hold_the_other_half() {
        // ST-097 §2.1 / figs.3.6–3.8 and ST-077 §3.2:
        // setup2, main DRAM5..10 and15..20, destination20..28.
        // Memory waits already satisfy the intercycle idle requirement.
        for index in 0..3 {
            for chunk in [1, 3, 8, 64] {
                let mut b = Bus::default();
                sound_channel(&mut b, index, true);
                elapse(&mut b, 9, chunk);
                b.write16(crate::sound::SOUND_RAM + 0x100, 0x5566).unwrap();
                elapse(&mut b, 1, chunk); // high word sampled now
                b.write32(crate::sound::SOUND_RAM + 0x100, 0xaabbccdd)
                    .unwrap();
                elapse(&mut b, 9, chunk);
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
                b.write16(crate::sound::SOUND_RAM + 0x102, 0x7788).unwrap();
                elapse(&mut b, 1, chunk); // low word sampled now
                b.write32(crate::sound::SOUND_RAM + 0x100, 0xdeadbeef)
                    .unwrap();
                elapse(&mut b, 7, chunk);
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
                assert_eq!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - index)), 0);
                elapse(&mut b, 1, chunk);
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x55667788));
                assert_ne!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - index)), 0);
            }
        }
    }
    #[test]
    fn scu_b_bus_writes_publish_separate_halfwords_and_irq_only_after_the_last_one() {
        for index in 0..3 {
            for chunk in [1, 3, 8, 64] {
                let mut b = Bus::default();
                sound_channel(&mut b, index, false);
                let destination = crate::sound::SOUND_RAM + 0x200;
                elapse(&mut b, 19, chunk);
                assert_eq!(b.read32(destination), Ok(0));
                elapse(&mut b, 1, chunk);
                assert_eq!(b.read32(destination), Ok(0x11220000));
                // The accepted buffered longword, not the changed source,
                // supplies the second physical write.
                b.write32(HIGH_WRAM + 0x100, 0xdeadbeef).unwrap();
                elapse(&mut b, 8, chunk);
                assert_eq!(b.read32(destination), Ok(0x11220000));
                assert_eq!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - index)), 0);
                elapse(&mut b, 1, chunk);
                assert_eq!(b.read32(destination), Ok(0x11223344));
                assert_ne!(b.read32(SCU + 0xa4).unwrap() & (1 << (11 - index)), 0);
            }
        }
    }
    #[test]
    fn scu_partial_b_bus_units_survive_priority_preemption_and_are_canceled_by_system_reset() {
        for chunk in [1, 3, 8, 64] {
            let mut b = Bus::default();
            sound_channel(&mut b, 0, true);
            elapse(&mut b, 10, chunk); // low-priority high word retained
            b.write32(crate::sound::SOUND_RAM + 0x100, 0xaabbccdd)
                .unwrap();
            b.write32(HIGH_WRAM + 0x300, 0x76543210).unwrap();
            for (o, v) in [
                (0, HIGH_WRAM + 0x300),
                (4, HIGH_WRAM + 0x400),
                (8, 4),
                (12, 0x102),
                (20, 7),
                (16, 0x101),
            ] {
                b.write32(SCU + 0x40 + o, v).unwrap();
            }
            elapse(&mut b, 30, chunk); // already granted low halfword drains before priority switch
            assert_eq!(b.read32(HIGH_WRAM + 0x400), Ok(0x76543210));
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
            assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0xa00, 0x200);
            elapse(&mut b, 40, chunk);
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x1122ccdd));
            assert_eq!(b.read32(SCU + 0xa4).unwrap() & 0xa00, 0xa00);

            for read_sound in [false, true] {
                for reset_at in [9, 10, 17, 18, 19, 20, 27] {
                    let mut b = Bus::default();
                    sound_channel(&mut b, 0, read_sound);
                    elapse(&mut b, reset_at, chunk);
                    b.system_reset();
                    let destination = if read_sound {
                        HIGH_WRAM + 0x200
                    } else {
                        crate::sound::SOUND_RAM + 0x200
                    };
                    b.write32(destination, 0xaabbccdd).unwrap();
                    elapse(&mut b, 100, chunk);
                    assert_eq!(b.read32(destination), Ok(0xaabbccdd));
                    assert_eq!(b.read32(SCU + 0xa4), Ok(0));
                    sound_channel(&mut b, 0, read_sound);
                    elapse(&mut b, if read_sound { 30 } else { 29 }, chunk);
                    assert_eq!(b.read32(destination), Ok(0x11223344));
                    assert_eq!(b.read32(SCU + 0xa4), Ok(0x800));
                }
            }
        }
    }
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
