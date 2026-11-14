//! Actual-board IF/ID/EX/MA continuation slice. No CPU cloning or decode replay.
//! Scalar instructions, branches and ordinary WRAM MOV loads/stores are supported.
//! Interrupts and exceptions are not integrated.
use super::{Branch, CpuState, CpuStatus, commit_mov_load, scalar, timing};
use crate::bus::{Bus, BusError, Processor};
use crate::kernel::{Error, Read, ReadError, ReadPoll, ReadPort, ReadTransfer};

pub mod experiment;
pub use experiment::verify_active_wram_board;
mod fixture;
mod store;
pub use fixture::verify_board;
use store::Store;
pub use store::verify_board as verify_store_board;
#[cfg(test)]
mod tests;

enum Effect {
    Scalar(scalar::Kind),
    Load { address: u32, width: u32 },
    Store(Store),
    Branch(Branch),
}

struct Issued {
    opcode: u16,
    effect: Effect,
    ex: u64,
    interlock: u32,
    stretch: u64,
    states: u32,
}

enum Phase {
    Select,
    InitialFetch {
        start: u64,
    },
    InitialSlot {
        end: u64,
    },
    Lookahead {
        start: u64,
    },
    DecodeSlot {
        end: u64,
    },
    Issue,
    Interlock {
        instruction: Issued,
        end: u64,
    },
    Future {
        instruction: Issued,
        start: u64,
    },
    OperandStart {
        instruction: Issued,
        end: u64,
    },
    Operand {
        instruction: Issued,
        end: u64,
    },
    OperandEnd {
        instruction: Issued,
        end: u64,
        value: u32,
    },
    RedirectWait {
        instruction: Issued,
        end: u64,
    },
    RedirectFetch {
        instruction: Issued,
        start: u64,
    },
    RedirectSlot {
        instruction: Issued,
        end: u64,
    },
    RedirectLookahead {
        instruction: Issued,
        start: u64,
    },
    RedirectEnd {
        instruction: Issued,
        end: u64,
    },
}

/// Experimental CPU with real cache/BSC/arbiter timing. `poll` retires at most
/// one instruction; a board owner advances to the next CPU or device deadline.
/// No Clone implementation: partially completed instructions retain ownership.
pub struct TimedExecutor {
    owner: Processor,
    state: CpuState,
    port: ReadPort,
    transfer: Option<ReadTransfer>,
    phase: Phase,
    decoded: Option<u16>,
    fetched: Option<u16>,
    pipeline: timing::LoadPipeline,
    ready: u64,
    deadline: Option<u64>,
    failed: bool,
}

impl TimedExecutor {
    pub fn new(owner: Processor, state: CpuState) -> Self {
        Self {
            owner,
            state,
            port: ReadPort::new(owner),
            transfer: None,
            phase: Phase::Select,
            decoded: None,
            fetched: None,
            pipeline: timing::LoadPipeline::default(),
            ready: 0,
            deadline: None,
            failed: false,
        }
    }

    pub fn state(&self) -> &CpuState {
        &self.state
    }

    pub fn set_cache_control(&mut self, control: u8) -> Result<(), Error> {
        if self.failed || !matches!(self.phase, Phase::Select) {
            return Err(Error::Protocol);
        }
        self.port.set_control(control)
    }

    pub fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    fn wait(&mut self, now: u64, end: u64) -> bool {
        if end > now {
            self.deadline = Some(end);
            true
        } else {
            false
        }
    }

    fn read(&mut self, bus: &mut Bus, read: Read, private: bool) -> Result<Option<u32>, ReadError> {
        if private
            && (self.transfer.is_some()
                || !self.port.private_read(read).map_err(ReadError::State)?)
        {
            return Ok(None);
        }
        if self.transfer.is_none() {
            match self.port.request(read).map_err(ReadError::State)? {
                ReadPoll::Ready(value) => return Ok(Some(value)),
                ReadPoll::Pending => self.transfer = self.port.take_transfer(),
                ReadPoll::Discarded => unreachable!("new request"),
            }
        }
        loop {
            let transfer = self.transfer.unwrap();
            match self.port.resume(bus, transfer)? {
                ReadPoll::Ready(value) => {
                    self.transfer = None;
                    return Ok(Some(value));
                }
                ReadPoll::Pending => {
                    if let Some(next) = self.port.take_transfer() {
                        self.transfer = Some(next);
                    } else {
                        return Ok(None);
                    }
                }
                ReadPoll::Discarded => return Err(ReadError::State(Error::Protocol)),
            }
        }
    }

    pub fn poll(&mut self, bus: &mut Bus) -> Result<bool, ReadError> {
        if self.failed {
            return Err(ReadError::State(Error::Protocol));
        }
        self.failed = true;
        let result = self.poll_inner(bus, bus.main_clocks, false);
        if result.is_ok() {
            self.failed = false;
        }
        result
    }

    pub(super) fn poll_private(&mut self, bus: &mut Bus, now: u64) -> Result<bool, ReadError> {
        if self.failed {
            return Err(ReadError::State(Error::Protocol));
        }
        self.failed = true;
        let result = if self.private_scalar(now) {
            Ok(true)
        } else {
            self.poll_inner(bus, now, true)
        };
        if result.is_ok() {
            self.failed = false;
        }
        result
    }

    /// A resident scalar EX has no intermediate observation. Keep the common
    /// path in place instead of moving its latch through several phase tags.
    fn private_scalar(&mut self, now: u64) -> bool {
        if !matches!(self.phase, Phase::Select)
            || self.ready > now
            || self.state.status != CpuStatus::Running
            || self.state.address_error_pending
            || self.state.delay_target.is_some()
            || !self.pipeline.scalar_ready()
        {
            return false;
        }
        let (Some(opcode), Some(successor)) = (self.decoded, self.fetched) else {
            return false;
        };
        let Some(kind) = timing::instruction(opcode).prepared_scalar() else {
            return false;
        };
        let read = Read::Fetch16(self.state.pc.wrapping_add(4));
        if self.transfer.is_some() || self.port.private_read(read) != Ok(true) {
            return false;
        }
        let ReadPoll::Ready(future) = self.port.request(read).expect("resident private scalar IF")
        else {
            unreachable!("resident proof owns the same cache");
        };
        scalar::execute(kind, opcode, &mut self.state);
        self.pipeline = timing::LoadPipeline::default();
        self.state.pc = self.state.pc.wrapping_add(2);
        self.state.instructions += 1;
        self.decoded = Some(successor);
        self.fetched = Some(future as u16);
        self.ready = now + 1;
        self.deadline = Some(self.ready);
        true
    }

    fn poll_inner(&mut self, bus: &mut Bus, now: u64, private: bool) -> Result<bool, ReadError> {
        self.deadline = None;
        if self.state.status != CpuStatus::Running || self.state.address_error_pending {
            return Err(ReadError::State(Error::Protocol));
        }
        loop {
            let phase = std::mem::replace(&mut self.phase, Phase::Select);
            match phase {
                Phase::Select => {
                    self.phase = Phase::Select;
                    if self.wait(now, self.ready) {
                        return Ok(false);
                    }
                    self.phase = if self.decoded.is_none() {
                        Phase::InitialFetch { start: now }
                    } else if self.fetched.is_none() {
                        Phase::Lookahead { start: now }
                    } else {
                        Phase::Issue
                    };
                }
                Phase::InitialFetch { start } => {
                    self.phase = Phase::InitialFetch { start };
                    let Some(word) = self.read(bus, Read::Fetch16(self.state.pc), private)? else {
                        return Ok(false);
                    };
                    self.decoded = Some(word as u16);
                    self.phase = Phase::InitialSlot { end: start + 1 };
                }
                Phase::InitialSlot { end } => {
                    self.phase = Phase::InitialSlot { end };
                    if self.wait(now, end) {
                        return Ok(false);
                    }
                    self.phase = Phase::Lookahead { start: now };
                }
                Phase::Lookahead { start } => {
                    self.phase = Phase::Lookahead { start };
                    let Some(word) =
                        self.read(bus, Read::Fetch16(self.state.pc.wrapping_add(2)), private)?
                    else {
                        return Ok(false);
                    };
                    self.fetched = Some(word as u16);
                    self.phase = Phase::DecodeSlot { end: start + 1 };
                }
                Phase::DecodeSlot { end } => {
                    self.phase = Phase::DecodeSlot { end };
                    if self.wait(now, end) {
                        return Ok(false);
                    }
                    self.phase = Phase::Issue;
                }
                Phase::Issue => {
                    let opcode = self.decoded.unwrap();
                    let descriptor = timing::instruction(opcode);
                    let effect = if let Some(kind) = descriptor.prepared_scalar() {
                        Effect::Scalar(kind)
                    } else if let Some((address, width)) =
                        descriptor.prepared_load(opcode, &self.state)
                    {
                        Effect::Load { address, width }
                    } else if let Some(branch) = Branch::decode(opcode, &self.state) {
                        Effect::Branch(branch)
                    } else if let Some(store) = Store::decode(opcode, &self.state) {
                        ReadPort::validate_store(store.address, store.width)
                            .map_err(ReadError::State)?;
                        Effect::Store(store)
                    } else {
                        return Err(ReadError::State(Error::Unsupported {
                            pc: self.state.pc,
                            opcode,
                        }));
                    };
                    if self.state.delay_target.is_some() && matches!(effect, Effect::Branch(_)) {
                        return Err(ReadError::State(Error::Protocol));
                    }
                    let ex = self.pipeline.issue_clock(now);
                    let interlock = self.pipeline.issue_instruction(descriptor);
                    let end =
                        now + u64::from(interlock.saturating_sub(self.pipeline.issue_overlap));
                    self.phase = Phase::Interlock {
                        instruction: Issued {
                            opcode,
                            effect,
                            ex,
                            interlock,
                            stretch: 0,
                            states: descriptor.states(self.state.sr & super::T != 0),
                        },
                        end,
                    };
                }
                Phase::Interlock { instruction, end } => {
                    if end > now {
                        self.phase = Phase::Interlock { instruction, end };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    self.decoded = self.fetched.take();
                    if timing::instruction(instruction.opcode).control_flags()
                        & timing::UNCONDITIONAL
                        != 0
                    {
                        self.redirect(instruction, now);
                    } else {
                        self.phase = Phase::Future {
                            instruction,
                            start: now,
                        };
                    }
                }
                Phase::Future {
                    mut instruction,
                    start,
                } => {
                    // Keep the latched instruction while the next IF completes.
                    let successor = self
                        .state
                        .delay_target
                        .unwrap_or(self.state.pc.wrapping_add(2));
                    match self.read(bus, Read::Fetch16(successor.wrapping_add(2)), private)? {
                        None => {
                            self.phase = Phase::Future { instruction, start };
                            return Ok(false);
                        }
                        Some(word) => self.fetched = Some(word as u16),
                    }
                    instruction.stretch = now.saturating_sub(start).saturating_sub(1);
                    self.ready = (instruction.ex + u64::from(instruction.interlock) + 1).max(now);
                    if matches!(instruction.effect, Effect::Scalar(_)) {
                        return Ok(self.retire(bus, instruction, None, now, private));
                    }
                    if let Effect::Branch(branch) = &instruction.effect {
                        if branch.target.is_some() {
                            self.redirect(instruction, now);
                        } else {
                            return Ok(self.retire(bus, instruction, None, now, private));
                        }
                        continue;
                    }
                    let resident = match &instruction.effect {
                        Effect::Load { address, .. } => self.port.resident(*address),
                        Effect::Store(_) => false,
                        _ => unreachable!(),
                    };
                    let end =
                        instruction.ex + u64::from(instruction.interlock) + instruction.stretch + 2;
                    self.phase = Phase::OperandStart {
                        instruction,
                        end: end - u64::from(!resident),
                    };
                }
                Phase::OperandStart { instruction, end } => {
                    if end > now {
                        self.phase = Phase::OperandStart { instruction, end };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    let completion =
                        instruction.ex + u64::from(instruction.interlock) + instruction.stretch + 2;
                    self.phase = Phase::Operand {
                        instruction,
                        end: completion,
                    };
                }
                Phase::Operand { instruction, end } => {
                    if let Effect::Store(store) = &instruction.effect {
                        if private {
                            self.phase = Phase::Operand { instruction, end };
                            return Ok(false);
                        }
                        match bus.resume_kernel_store(
                            self.owner,
                            store.address,
                            store.width,
                            store.value,
                            self.state.pc,
                        ) {
                            Ok(()) => {
                                self.port
                                    .accepted_store(store.address, store.width, store.value);
                                self.phase = Phase::OperandEnd {
                                    instruction,
                                    end,
                                    value: 0,
                                };
                            }
                            Err(BusError::Pending) => {
                                self.phase = Phase::Operand { instruction, end };
                                return Ok(false);
                            }
                            Err(error) => return Err(ReadError::Bus(Box::new(error))),
                        }
                        continue;
                    }
                    let Effect::Load { address, width } = instruction.effect else {
                        unreachable!()
                    };
                    if !private && self.transfer.is_none() && !self.port.resident(address) {
                        bus.schedule_kernel_operand(self.owner, end);
                    }
                    match self.read(bus, Read::Data { address, width }, private)? {
                        None => {
                            self.phase = Phase::Operand { instruction, end };
                            return Ok(false);
                        }
                        Some(value) => {
                            self.phase = Phase::OperandEnd {
                                instruction,
                                end,
                                value,
                            }
                        }
                    }
                }
                Phase::OperandEnd {
                    instruction,
                    end,
                    value,
                } => {
                    if end > now {
                        self.phase = Phase::OperandEnd {
                            instruction,
                            end,
                            value,
                        };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    return Ok(self.retire(bus, instruction, Some(value), now, private));
                }
                Phase::RedirectWait { instruction, end } => {
                    if end > now {
                        self.phase = Phase::RedirectWait { instruction, end };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    self.phase = Phase::RedirectFetch {
                        instruction,
                        start: now,
                    };
                }
                Phase::RedirectFetch { instruction, start } => {
                    let Effect::Branch(branch) = &instruction.effect else {
                        unreachable!()
                    };
                    let target = branch.target.unwrap();
                    let delayed = branch.delayed;
                    let Some(word) = self.read(bus, Read::Fetch16(target), private)? else {
                        self.phase = Phase::RedirectFetch { instruction, start };
                        return Ok(false);
                    };
                    if delayed {
                        self.fetched = Some(word as u16);
                        return Ok(self.retire(bus, instruction, None, now, private));
                    }
                    self.decoded = Some(word as u16);
                    self.phase = Phase::RedirectSlot {
                        instruction,
                        end: start + 1,
                    };
                }
                Phase::RedirectSlot { instruction, end } => {
                    if end > now {
                        self.phase = Phase::RedirectSlot { instruction, end };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    self.phase = Phase::RedirectLookahead {
                        instruction,
                        start: now,
                    };
                }
                Phase::RedirectLookahead { instruction, start } => {
                    let Effect::Branch(branch) = &instruction.effect else {
                        unreachable!()
                    };
                    let Some(word) = self.read(
                        bus,
                        Read::Fetch16(branch.target.unwrap().wrapping_add(2)),
                        private,
                    )?
                    else {
                        self.phase = Phase::RedirectLookahead { instruction, start };
                        return Ok(false);
                    };
                    self.fetched = Some(word as u16);
                    self.phase = Phase::RedirectEnd {
                        instruction,
                        end: start + 1,
                    };
                }
                Phase::RedirectEnd { instruction, end } => {
                    if end > now {
                        self.phase = Phase::RedirectEnd { instruction, end };
                        self.deadline = Some(end);
                        return Ok(false);
                    }
                    return Ok(self.retire(bus, instruction, None, now, private));
                }
            }
        }
    }

    fn redirect(&mut self, instruction: Issued, now: u64) {
        self.port.discard_prefetch();
        let issue = instruction.ex + u64::from(instruction.interlock);
        self.ready = (issue + 1).max(now);
        let end = issue + instruction.stretch + 1;
        self.phase = Phase::RedirectWait { instruction, end };
    }

    fn retire(
        &mut self,
        bus: &mut Bus,
        instruction: Issued,
        value: Option<u32>,
        now: u64,
        private: bool,
    ) -> bool {
        let mut split = 0;
        let mut next_branch = None;
        let mut sequential = self.state.pc.wrapping_add(2);
        match instruction.effect {
            Effect::Scalar(kind) => scalar::execute(kind, instruction.opcode, &mut self.state),
            Effect::Load { width, .. } => {
                let raw = value.unwrap();
                let value = match width {
                    1 => raw as i8 as i32 as u32,
                    2 => raw as i16 as i32 as u32,
                    _ => raw,
                };
                commit_mov_load(&mut self.state, instruction.opcode, value);
                let successor = self.state.delay_target.unwrap_or(sequential);
                split = u32::from(successor & 2 == 0);
                self.pipeline.load_split(split != 0);
                if split == 0 {
                    self.pipeline.retain_ex(now - 1);
                }
            }
            Effect::Branch(branch) => {
                if branch.link {
                    self.state.pr = self.state.pc.wrapping_add(4);
                }
                if let Some(target) = branch.target {
                    if branch.delayed {
                        next_branch = Some(target);
                    } else {
                        sequential = target;
                    }
                }
            }
            Effect::Store(store) => {
                if let Some((register, address)) = store.update {
                    self.state.r[register] = address;
                }
                let successor = self.state.delay_target.unwrap_or(sequential);
                split = u32::from(successor & 2 == 0);
                self.pipeline.load_split(split != 0);
                if split == 0 {
                    self.pipeline.retain_ex(now - 1);
                }
            }
        }
        self.state.pc = self.state.delay_target.take().unwrap_or(sequential);
        self.state.delay_target = next_branch;
        self.state.instructions += 1;
        let clocks = instruction.states + instruction.interlock + split;
        self.ready = self
            .ready
            .max(instruction.ex + instruction.stretch + u64::from(clocks))
            .max(now);
        self.phase = Phase::Select;
        self.deadline = Some(self.ready);
        if !private {
            bus.kernel_retired(self.owner, 1);
            bus.finish_cpu_transaction(self.owner);
        }
        true
    }
}
