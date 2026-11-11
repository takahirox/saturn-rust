//! Independently written SH-2 reference interpreter. See docs/hardware-conformance.md.
use crate::bus::{BusError, Memory};
use std::fmt;

mod fetch;
#[cfg(test)]
mod interrupt_tests;
mod memory;
mod multiplier;
mod scalar;
#[cfg(test)]
mod scalar_step_tests;
mod timing;
use memory::Operands;

const T: u32 = 1;
const S: u32 = 2;
const Q: u32 = 0x100;
const M: u32 = 0x200;
const SR_MASK: u32 = 0x3F3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    NotRunning,
    Bus {
        pc: u32,
        source: BusError,
    },
    ExceptionEntry {
        pc: u32,
        vector: u8,
        source: BusError,
    },
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "CPU is stopped; reset before stepping"),
            Self::Bus { pc, source } => write!(f, "Memory access at PC 0x{pc:08X}: {source}"),
            Self::ExceptionEntry { pc, vector, source } => write!(
                f,
                "Exception vector {vector} entry at PC 0x{pc:08X}: {source}"
            ),
        }
    }
}
impl std::error::Error for CpuError {}

impl CpuError {
    fn with_instruction(mut self, opcode: u16) -> Self {
        if let Self::Bus {
            source: BusError::Smpc(fault),
            ..
        }
        | Self::ExceptionEntry {
            source: BusError::Smpc(fault),
            ..
        } = &mut self
        {
            fault.instruction = Some(opcode);
        }
        self
    }
}

// Ordinary steps retain complete diagnostics. A rollback-owned cache attempt
// needs only a rejection marker; the ordinary path reports the real fault.
trait ExecutionErrors {
    type Error;
    fn convert(error: CpuError, instruction: Option<u16>) -> Self::Error;
}
struct DetailedErrors;
impl ExecutionErrors for DetailedErrors {
    type Error = CpuError;
    fn convert(error: CpuError, instruction: Option<u16>) -> CpuError {
        match instruction {
            Some(opcode) => error.with_instruction(opcode),
            None => error,
        }
    }
}
struct DiscardedErrors;
impl ExecutionErrors for DiscardedErrors {
    type Error = ();
    fn convert(_error: CpuError, _instruction: Option<u16>) -> Self::Error {}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuStatus {
    Reset,
    Running,
    Sleeping,
    Halted,
    Faulted(CpuError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub vector: u8,
    pub saved_pc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuState {
    pub r: [u32; 16],
    /// Next fetch address; while sleeping, the instruction after SLEEP.
    pub pc: u32,
    pub sr: u32,
    pub pr: u32,
    pub gbr: u32,
    pub vbr: u32,
    pub mach: u32,
    pub macl: u32,
    /// Retired instructions; illegal instructions and interrupt entry do not retire.
    pub instructions: u64,
    pub delay_target: Option<u32>,
    pub exceptions: u64,
    pub last_exception: Option<Exception>,
    /// Latched CPU address error, accepted before the next eligible fetch.
    pub address_error_pending: bool,
    pub status: CpuStatus,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            r: [0; 16],
            pc: 0,
            sr: 0xF0,
            pr: 0,
            gbr: 0,
            vbr: 0,
            mach: 0,
            macl: 0,
            instructions: 0,
            delay_target: None,
            exceptions: 0,
            last_exception: None,
            address_error_pending: false,
            status: CpuStatus::Reset,
        }
    }
}

impl CpuState {
    fn set_t(&mut self, value: bool) {
        self.sr = (self.sr & !T) | u32::from(value);
    }

    fn accumulator(&self) -> u64 {
        ((self.mach as u64) << 32) | self.macl as u64
    }

    fn set_accumulator(&mut self, value: u64) {
        self.mach = (value >> 32) as u32;
        self.macl = value as u32;
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cpu {
    state: CpuState,
    /// System/control transfers and delayed conditional branches inhibit interrupts
    /// for one successor, including when the conditional branch is untaken.
    interrupt_inhibited: bool,
    /// Unlike control transfers, delayed branches also defer address errors.
    address_error_inhibited: bool,
    /// Main-clock execution states for the last issued boundary, excluding
    /// external bus waits. Sleeping polls issue no instruction and report zero.
    last_clocks: u32,
    load_pipeline: timing::LoadPipeline,
    multiplier: multiplier::Multiplier,
    fetch: fetch::Fetch,
    execution_clock: Option<u64>,
}

enum CachedEffect {
    Scalar(scalar::Kind),
    Load,
    Store(u32),
}
#[derive(Clone, Copy)]
struct Branch {
    target: Option<u32>,
    delayed: bool,
    link: bool,
    inhibit: bool,
}
impl Branch {
    fn recognized(opcode: u16) -> bool {
        matches!(opcode >> 12, 0xA | 0xB)
            || matches!(opcode >> 8, 0x89 | 0x8B | 0x8D | 0x8F)
            || matches!(opcode & 0xF0FF, 0x400B | 0x402B | 0x0003 | 0x0023)
            || opcode == 0x000B
    }
    fn decode(opcode: u16, s: &CpuState) -> Option<Self> {
        let relative = |offset: u32| s.pc.wrapping_add(4).wrapping_add(offset);
        if matches!(opcode >> 8, 0x89 | 0x8B | 0x8D | 0x8F) {
            let taken = (s.sr & T != 0) == (opcode & 0x200 == 0);
            let delayed = opcode & 0x400 != 0;
            let displacement = i32::from(opcode as i8) * 2;
            return Some(Self {
                target: taken.then(|| relative(displacement as u32)),
                delayed,
                link: false,
                // H table 4.10: a conditional delay slot suppresses IRQ/NMI
                // entry even when its branch is not taken.
                inhibit: delayed,
            });
        }
        let (target, link) = match opcode {
            0x000B => (s.pr, false),
            _ if matches!(opcode & 0xF0FF, 0x400B | 0x402B | 0x0003 | 0x0023) => {
                let r = s.r[((opcode >> 8) & 15) as usize];
                (
                    if opcode & 0x4000 != 0 { r } else { relative(r) },
                    opcode & 0x20 == 0,
                )
            }
            _ if matches!(opcode >> 12, 0xA | 0xB) => {
                let displacement = i32::from((opcode << 4) as i16 >> 4) * 2;
                (relative(displacement as u32), opcode >> 12 == 0xB)
            }
            _ => return None,
        };
        Some(Self {
            target: Some(target),
            delayed: true,
            link,
            inhibit: false,
        })
    }
}

pub(crate) struct PreparedBranch {
    branch: Branch,
    fetch: fetch::Fetch,
    pipeline: timing::LoadPipeline,
    states: u32,
    ex: u64,
}
pub(crate) struct CachedInstruction {
    pub at: u64,
    pub operand: Option<(u32, u32)>,
    pub future: u32,
    next_pc: u32,
    issue_at: u64,
    decode_ready: u64,
    ex: u64,
    pipeline: timing::LoadPipeline,
    opcode: u16,
    words: (u16, u16),
    effect: CachedEffect,
    interlock: u32,
}
impl CachedInstruction {
    pub(crate) fn store_value(&self) -> Option<u32> {
        match self.effect {
            CachedEffect::Store(value) => Some(value),
            _ => None,
        }
    }
    pub(crate) fn issue_at(&self) -> u64 {
        self.issue_at
    }
    pub(crate) fn load_wait(&self, now: u64, scheduled: bool) -> u32 {
        self.at
            .saturating_sub(u64::from(scheduled))
            .saturating_sub(now) as u32
    }
}

impl Cpu {
    pub fn state(&self) -> &CpuState {
        &self.state
    }

    pub(crate) fn decoded_opcode(&self) -> Option<u16> {
        self.fetch.decoded_opcode(self.state.pc)
    }
    /// Execute an uninterrupted sequence of one-state register operations.
    /// The caller bounds clocks against all external observers. Real IFs still
    /// run before each EX, preserving cache replacement and retained words.
    pub(crate) fn cached_scalar_block(&mut self, bus: &mut impl Memory, limit: u32) -> u32 {
        self.cached_scalar_block_with_fetch(
            bus.sh2_clock().expect("clocked cache block"),
            limit,
            |pc, address| {
                let word = bus.fetch16(address).ok()?;
                bus.instruction_break(pc, false);
                Some(word)
            },
        )
    }

    pub(crate) fn cached_scalar_block_with_fetch(
        &mut self,
        start: u64,
        limit: u32,
        mut fetch: impl FnMut(u32, u32) -> Option<u16>,
    ) -> u32 {
        if self.state.status != CpuStatus::Running
            || self.state.address_error_pending
            || self.state.delay_target.is_some()
            || !self.load_pipeline.scalar_ready()
        {
            return 0;
        }
        let Some(mut words) = self.fetch.cached_pair(self.state.pc, start) else {
            return 0;
        };
        let mut count = 0;
        while count < limit {
            let Some(kind) = scalar::decode(words.0) else {
                break;
            };
            let Some(future) = fetch(self.state.pc, self.state.pc.wrapping_add(4)) else {
                break;
            };
            scalar::execute(kind, words.0, &mut self.state);
            self.state.pc = self.state.pc.wrapping_add(2);
            words = (words.1, future);
            count += 1;
        }
        if count != 0 {
            self.load_pipeline = timing::LoadPipeline::default();
            self.state.instructions += u64::from(count);
            self.multiplier.clock += u64::from(count);
            self.last_clocks = 1;
            self.execution_clock = Some(start + u64::from(count) - 1);
            self.interrupt_inhibited = false;
            self.address_error_inhibited = false;
            self.fetch
                .finish_cached_pair(self.state.pc, words, start + u64::from(count));
        }
        count
    }
    /// Prepare register operations or MOV loads, including legal delay slots.
    /// The caller proves operand/future IF hits and retains real WB/EX state
    /// while retirement stays within every observer horizon.
    pub(crate) fn prepare_cached_instruction(
        &self,
        opcode: u16,
        now: u64,
    ) -> Option<CachedInstruction> {
        let instruction = timing::instruction(opcode);
        let operand = instruction.prepared_load(opcode, &self.state);
        let effect = if let Some(kind) = instruction.prepared_scalar() {
            CachedEffect::Scalar(kind)
        } else if operand.is_some() {
            CachedEffect::Load
        } else {
            return None;
        };
        self.prepare_cached_operation(opcode, now, instruction, operand, effect)
    }
    pub(crate) fn prepare_cached_store(&self, opcode: u16, now: u64) -> Option<CachedInstruction> {
        // System-register stores retain their dedicated MDU/control path.
        if opcode >> 12 == 4 {
            return None;
        }
        let address = timing::mov_store_address(opcode, &self.state)?;
        let width = match opcode >> 12 {
            0 | 2 => 1 << (opcode & 3),
            1 => 4,
            8 => 1 << (opcode >> 8 & 1),
            0xC => 1 << (opcode >> 8 & 3),
            _ => return None,
        };
        let source = if matches!(opcode >> 12, 0..=2) {
            (opcode >> 4 & 15) as usize
        } else {
            0
        };
        self.prepare_cached_operation(
            opcode,
            now,
            timing::instruction(opcode),
            Some((address, width)),
            CachedEffect::Store(self.state.r[source]),
        )
    }
    fn prepare_cached_operation(
        &self,
        opcode: u16,
        now: u64,
        instruction: timing::Instruction,
        operand: Option<(u32, u32)>,
        effect: CachedEffect,
    ) -> Option<CachedInstruction> {
        if self.state.status != CpuStatus::Running
            || self.state.address_error_pending
            || self.state.pc & 1 != 0
        {
            return None;
        }
        let next_pc = self
            .state
            .delay_target
            .unwrap_or(self.state.pc.wrapping_add(2));
        let words = self.fetch.cached_pair_for(self.state.pc, next_pc, now)?;
        if words.0 != opcode {
            return None;
        }
        let mut pipeline = self.load_pipeline.clone();
        let ex = pipeline.issue_clock(now);
        let interlock = pipeline.issue_instruction(instruction);
        let wait = interlock.saturating_sub(pipeline.issue_overlap);
        let issue_at = now + u64::from(wait);
        if operand.is_some_and(|(a, width)| !a.is_multiple_of(width)) {
            return None;
        }
        let at = if operand.is_some() {
            (ex + u64::from(interlock) + 2).max(issue_at)
        } else {
            issue_at
        };
        Some(CachedInstruction {
            at,
            operand,
            future: next_pc.wrapping_add(2),
            next_pc,
            issue_at,
            decode_ready: (ex + u64::from(interlock) + 1).max(issue_at),
            ex,
            pipeline,
            opcode: words.0,
            words,
            effect,
            interlock,
        })
    }
    pub(crate) fn execute_cached_instruction(
        &mut self,
        mut plan: CachedInstruction,
        bus: &mut impl Memory,
    ) {
        let pc = self.state.pc;
        bus.sh2_write_context(pc, None);
        bus.sh2_pipeline_wait(0).expect("ready IF/ID pair");
        bus.instruction_break(pc, false);
        let now = bus.sh2_clock().expect("clocked cache plan");
        if plan.issue_at > now {
            bus.sh2_pipeline_wait((plan.issue_at - now) as u32)
                .expect("bounded WB interlock");
        }
        let future = bus.fetch16(plan.future).expect("proven scalar IF hit");
        let split = match plan.effect {
            CachedEffect::Scalar(kind) => {
                scalar::execute(kind, plan.opcode, &mut self.state);
                0
            }
            CachedEffect::Load => {
                let (address, width) = plan.operand.unwrap();
                let access = timing::MovAccess::start(
                    bus,
                    address,
                    false,
                    plan.next_pc,
                    plan.ex + u64::from(plan.interlock),
                )
                .expect("proven cached MA");
                let split = access.split();
                let value = read_operand(bus, address, width).expect("proven cached load");
                commit_mov_load(&mut self.state, plan.opcode, value);
                access
                    .finish(&mut plan.pipeline, bus)
                    .expect("bounded cached MA finish");
                split
            }
            CachedEffect::Store(value) => {
                let (address, width) = plan.operand.unwrap();
                let access = timing::MovAccess::start(
                    bus,
                    address,
                    true,
                    plan.next_pc,
                    plan.ex + u64::from(plan.interlock),
                )
                .expect("proven store MA");
                let split = access.split();
                write_operand(bus, address, width, value).expect("proven cached store");
                self.commit_prepared_store_address(&plan);
                access
                    .finish(&mut plan.pipeline, bus)
                    .expect("bounded cached store finish");
                split
            }
        };
        self.retire_cached_instruction(plan, future, split);
        bus.instruction_break(pc, true);
    }
    pub(crate) fn retire_prepared_load(
        &mut self,
        plan: CachedInstruction,
        future: u16,
        value: u32,
        now: u64,
    ) {
        assert!(now >= plan.at);
        commit_mov_load(&mut self.state, plan.opcode, value);
        self.retire_prepared_memory(plan, future, now);
    }
    pub(crate) fn retire_prepared_store(&mut self, plan: CachedInstruction, future: u16, now: u64) {
        assert!(now >= plan.at);
        self.commit_prepared_store_address(&plan);
        self.retire_prepared_memory(plan, future, now);
    }
    fn commit_prepared_store_address(&mut self, plan: &CachedInstruction) {
        if plan.opcode >> 12 == 2 && plan.opcode & 4 != 0 {
            self.state.r[(plan.opcode >> 8 & 15) as usize] = plan.operand.unwrap().0;
        }
    }
    fn retire_prepared_memory(&mut self, mut plan: CachedInstruction, future: u16, now: u64) {
        let split = plan.next_pc & 2 == 0;
        plan.pipeline.load_split(split);
        if !split {
            plan.pipeline.retain_ex(now - 1);
        }
        self.retire_cached_instruction(plan, future, u32::from(split));
    }
    fn retire_cached_instruction(&mut self, plan: CachedInstruction, future: u16, split: u32) {
        self.state.pc = plan.next_pc;
        self.state.delay_target = None;
        self.state.instructions += 1;
        self.load_pipeline = plan.pipeline;
        self.last_clocks = 1 + plan.interlock + split;
        self.execution_clock = Some(plan.ex);
        self.interrupt_inhibited = false;
        self.address_error_inhibited = false;
        self.multiplier.clock += u64::from(self.last_clocks);
        self.fetch
            .finish_cached_pair(self.state.pc, (plan.words.1, future), plan.decode_ready);
    }

    pub(crate) fn prepare_cached_control(&self, bus: &mut impl Memory) -> Option<PreparedBranch> {
        if self.state.status != CpuStatus::Running
            || self.state.address_error_pending
            || self.state.delay_target.is_some()
        {
            return None;
        }
        let opcode = self.decoded_opcode()?;
        let branch = Branch::decode(opcode, &self.state)?;
        let instruction = timing::instruction(opcode);
        let mut fetch = self.fetch.clone();
        let pc = self.state.pc;
        if fetch.select(bus, pc).ok()? != opcode {
            return None;
        }
        bus.instruction_break(pc, false);
        let mut pipeline = self.load_pipeline.clone();
        let ex = pipeline.issue_clock(bus.sh2_clock()?);
        let interlock = pipeline.issue_instruction(instruction);
        let wait = interlock.saturating_sub(pipeline.issue_overlap);
        bus.sh2_pipeline_wait(wait).ok()?;
        let fetch_start = bus.sh2_clock()?;
        let issue = ex + u64::from(interlock);
        if instruction.control_flags() & timing::UNCONDITIONAL != 0 {
            fetch.hold_delay_slot(bus, issue);
        } else {
            fetch.advance(bus, pc.wrapping_add(2), issue).ok()?;
        }
        let stretch = bus
            .sh2_clock()?
            .saturating_sub(fetch_start)
            .saturating_sub(1);
        if let Some(target) = branch.target {
            bus.sh2_discard_prefetch();
            fetch
                .redirect(bus, target, branch.delayed, issue + stretch + 1)
                .ok()?;
        }
        let states = instruction.states(self.state.sr & T != 0) + interlock;
        Some(PreparedBranch {
            branch,
            fetch,
            pipeline,
            states,
            ex: ex + stretch,
        })
    }
    pub(crate) fn has_cached_control(&self, opcode: u16) -> bool {
        Branch::recognized(opcode)
            && self.state.status == CpuStatus::Running
            && !self.state.address_error_pending
            && self.state.delay_target.is_none()
            && self.decoded_opcode() == Some(opcode)
    }
    pub(crate) fn is_scalar(opcode: u16) -> bool {
        scalar::decode(opcode).is_some()
    }
    pub(crate) fn retire_cached_control(&mut self, plan: PreparedBranch) {
        let branch = plan.branch;
        let pc = self.state.pc;
        if branch.link {
            self.state.pr = pc.wrapping_add(4);
        }
        self.state.pc = branch
            .target
            .filter(|_| !branch.delayed)
            .unwrap_or_else(|| pc.wrapping_add(2));
        self.state.delay_target = branch.target.filter(|_| branch.delayed);
        self.state.instructions += 1;
        self.fetch = plan.fetch;
        self.load_pipeline = plan.pipeline;
        self.last_clocks = plan.states;
        self.execution_clock = Some(plan.ex);
        self.interrupt_inhibited = branch.inhibit;
        self.address_error_inhibited = branch.inhibit || self.state.delay_target.is_some();
        self.multiplier.clock += u64::from(plan.states);
    }

    /// Repeat a proven cache-only cycle. Compare the complete pipeline state
    /// after translating its clocks/counters; DT/BF may additionally decrease
    /// one nonzero counter. Every other architectural/private field must match.
    pub(crate) fn repeat_cached_cycle(
        &mut self,
        before: &Self,
        clocks: u64,
        repetitions: u64,
        countdown: Option<usize>,
    ) -> bool {
        let instructions = self.state.instructions - before.state.instructions;
        let multiplier = self.multiplier.clock - before.multiplier.clock;
        let mut expected = before.clone();
        expected.state.instructions += instructions;
        expected.multiplier.clock += multiplier;
        if let Some(register) = countdown {
            expected.state.r[register] = expected.state.r[register].wrapping_sub(1);
        }
        expected.shift_cached_clock(clocks);
        if expected != *self {
            return false;
        }
        if let Some(register) = countdown {
            self.state.r[register] -= repetitions as u32;
        }
        self.state.instructions += instructions * repetitions;
        self.multiplier.clock += multiplier * repetitions;
        self.shift_cached_clock(clocks * repetitions);
        true
    }
    fn shift_cached_clock(&mut self, clocks: u64) {
        self.fetch.shift_cached_clock(clocks);
        self.load_pipeline.shift_cached_clock(clocks);
        self.execution_clock = self.execution_clock.map(|at| at + clocks);
    }
    pub(crate) fn same_cached_cycle_origin(&self, prior: &Self, elapsed: u64) -> bool {
        let mut expected = prior.clone();
        expected.state.instructions = self.state.instructions;
        expected.multiplier.clock = self.multiplier.clock;
        expected.shift_cached_clock(elapsed);
        expected == *self
    }
    pub(crate) fn cached_local_clock(&self) -> u64 {
        self.multiplier.clock
    }
    pub(crate) fn publish_poll_periods(
        &mut self,
        period: u64,
        instructions: u64,
        local: u64,
        copies: u64,
    ) {
        self.state.instructions += instructions * copies;
        self.multiplier.clock += local * copies;
        self.shift_cached_clock(period * copies);
    }
    pub(crate) fn rebase_poll_prefix(&mut self, prior: &Self, current: &Self, elapsed: u64) {
        self.state.instructions =
            current.state.instructions + (self.state.instructions - prior.state.instructions);
        self.multiplier.clock =
            current.multiplier.clock + (self.multiplier.clock - prior.multiplier.clock);
        self.shift_cached_clock(elapsed);
    }
    pub(crate) fn cached_load_address(&self, opcode: u16) -> Option<u32> {
        timing::mov_load_address(opcode, &self.state).or_else(|| {
            matches!(
                opcode & 0xF0FF,
                0x4006 | 0x4016 | 0x4026 | 0x4007 | 0x4017 | 0x4027
            )
            .then(|| self.state.r[((opcode >> 8) & 15) as usize])
        })
    }
    pub(crate) fn cached_store_address(&self, opcode: u16) -> Option<u32> {
        timing::mov_store_address(opcode, &self.state).or_else(|| {
            matches!(
                opcode & 0xF0FF,
                0x4002 | 0x4012 | 0x4022 | 0x4003 | 0x4013 | 0x4023
            )
            .then(|| self.state.r[((opcode >> 8) & 15) as usize].wrapping_sub(4))
        })
    }
    pub fn last_clocks(&self) -> u32 {
        self.last_clocks
    }

    pub(crate) fn execution_clock(&self) -> Option<u64> {
        self.execution_clock
    }

    #[cfg(test)]
    pub(crate) fn prime_fetch(&mut self, bus: &impl Memory) {
        self.fetch.prime(bus, self.state.pc);
    }

    pub fn start(pc: u32, sp: u32) -> Self {
        let mut state = CpuState {
            pc,
            status: CpuStatus::Running,
            ..CpuState::default()
        };
        state.r[15] = sp;
        Self {
            state,
            interrupt_inhibited: false,
            address_error_inhibited: false,
            last_clocks: 0,
            load_pipeline: timing::LoadPipeline::default(),
            multiplier: multiplier::Multiplier::default(),
            fetch: fetch::Fetch::default(),
            execution_clock: None,
        }
    }

    /// Power-on vectors are physical 0 and 4, not VBR-relative.
    pub fn power_on(bus: &impl Memory) -> Result<Self, BusError> {
        Ok(Self::start(bus.read32(0)?, bus.read32(4)?))
    }

    /// SH7604 table 4.3: manual reset vectors are physical 8/12.
    pub fn manual_reset(bus: &impl Memory) -> Result<Self, BusError> {
        Ok(Self::start(bus.read32(8)?, bus.read32(12)?))
    }

    /// Debugger stop, separate from the architectural SLEEP state.
    pub fn halt(&mut self) {
        self.state.status = CpuStatus::Halted;
    }
    pub(crate) fn deferred_write_fault(&mut self, error: CpuError) {
        self.state.status = CpuStatus::Faulted(error);
    }

    /// The caller supplies the winning hardware interrupt and retries if deferred.
    /// This is CPU entry only; device requests/acknowledgment are not synthesized here.
    pub fn accept_interrupt(
        &mut self,
        bus: &mut impl Memory,
        level: u8,
        vector: u8,
    ) -> Result<bool, CpuError> {
        if !(1..=15).contains(&level) || level as u32 <= (self.state.sr >> 4) & 15 {
            return Ok(false);
        }
        self.interrupt(bus, vector, level, false)
    }

    /// DMA address errors use vector 10 and cannot be masked by SR.I.
    pub fn accept_dma_address_error(&mut self, bus: &mut impl Memory) -> Result<bool, CpuError> {
        if !matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            || self.state.delay_target.is_some()
            || self.address_error_inhibited
            || self.state.address_error_pending
        {
            return Ok(false);
        }
        let result = self.enter_exception(bus, 10, self.state.pc);
        if let Err(error) = result {
            self.state.status = CpuStatus::Faulted(error.clone());
            return Err(error);
        }
        Ok(true)
    }

    /// NMI has vector 11 and raises the interrupt mask to 15.
    pub fn accept_nmi(&mut self, bus: &mut impl Memory) -> Result<bool, CpuError> {
        self.interrupt(bus, 11, 15, true)
    }

    fn interrupt(
        &mut self,
        bus: &mut impl Memory,
        vector: u8,
        level: u8,
        nmi: bool,
    ) -> Result<bool, CpuError> {
        if !matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            || self.state.delay_target.is_some()
            || self.interrupt_inhibited
            || self.state.address_error_pending
        {
            return Ok(false);
        }
        let pc = self.state.pc;
        let result = self.enter_exception_sequence(bus, vector, pc, Some((level, nmi)));
        if let Err(error) = result {
            self.state.status = CpuStatus::Faulted(error.clone());
            return Err(error);
        }
        self.state.sr = (self.state.sr & !0xF0) | (level as u32) << 4;
        Ok(true)
    }

    /// Bounded: sleeping calls return without fetching or retiring an instruction.
    /// Faults caused by unavailable host bus/device behavior stay sticky until reset.
    pub fn step(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        self.step_mode::<true>(bus)
    }
    /// Attempt an instruction in a private cache/bus snapshot. The caller
    /// verifies retirement and discards CPU/memory state on rejection; ordinary
    /// fallback supplies fault status and full diagnostics at the real boundary.
    pub(crate) fn step_cached(&mut self, bus: &mut impl Memory) -> bool {
        if self.state.status != CpuStatus::Running || self.state.address_error_pending {
            return false;
        }
        debug_assert!(!bus.break_pending());
        self.last_clocks = 0;
        self.execution_clock = None;
        self.load_pipeline.issue_overlap = 0;
        let pc = self.state.pc;
        let retired = self.state.instructions;
        let completed = self.execute::<true, DiscardedErrors>(bus).is_ok();
        if self.state.instructions != retired {
            bus.instruction_break(pc, true);
        }
        completed
    }
    fn step_mode<const SCALAR_RETIRE: bool>(
        &mut self,
        bus: &mut impl Memory,
    ) -> Result<(), CpuError> {
        self.last_clocks = 0;
        self.execution_clock = None;
        self.load_pipeline.issue_overlap = 0;
        let pc = self.state.pc;
        if matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            && !self.state.address_error_pending
            && bus.break_pending()
            && 15 > (self.state.sr >> 4) & 15
            && self.interrupt(bus, 12, 15, false)?
        {
            bus.acknowledge_break();
            return Ok(());
        }
        match &self.state.status {
            CpuStatus::Running => (),
            CpuStatus::Sleeping if !self.state.address_error_pending => return Ok(()),
            CpuStatus::Sleeping => (),
            CpuStatus::Faulted(error) => return Err(error.clone()),
            _ => return Err(CpuError::NotRunning),
        }
        let retired = self.state.instructions;
        let result = if self.state.address_error_pending && !self.address_error_inhibited {
            self.address_error(bus)
        } else {
            self.execute::<SCALAR_RETIRE, DetailedErrors>(bus)
        };
        if let Err(error) = &result {
            self.state.status = CpuStatus::Faulted(error.clone());
        }
        if self.state.instructions != retired {
            bus.instruction_break(pc, true);
        }
        result
    }

    fn enter_exception(
        &mut self,
        bus: &mut impl Memory,
        vector: u8,
        saved_pc: u32,
    ) -> Result<(), CpuError> {
        self.enter_exception_sequence(bus, vector, saved_pc, None)
    }

    fn enter_exception_sequence(
        &mut self,
        bus: &mut impl Memory,
        mut vector: u8,
        saved_pc: u32,
        interrupt: Option<(u8, bool)>,
    ) -> Result<(), CpuError> {
        // H Rev4 §5.5 table5.8: acceptance to handler fetch is 5+m1+m2+m3;
        // the three single-state memory operations are included here. The
        // bus supplies additional waits independently to the machine scheduler.
        bus.sh2_write_context(self.state.pc, Some(vector));
        self.last_clocks = 8;
        self.execution_clock = bus.sh2_clock();
        self.fetch.clear();
        self.multiplier.clock += 8;
        self.load_pipeline = timing::LoadPipeline::default();
        let s = &mut self.state;
        let access = |source| CpuError::ExceptionEntry {
            pc: s.pc,
            vector,
            source,
        };
        if let Some((_, nmi)) = interrupt {
            // H figs5.7/5.10: D, then E,E before the first stack MA.
            bus.sh2_pipeline_wait(1).map_err(access)?;
            if nmi {
                bus.sh2_clear_nmi();
            }
            bus.sh2_pipeline_wait(2).map_err(access)?;
        }
        // Bus operations are ordered SR push, PC push, vector fetch (SH7604 §4.1.2).
        // A failed entry is a bounded diagnostic, not recursive host exception dispatch.
        let sp = s.r[15].wrapping_sub(8);
        let start = bus.sh2_clock();
        bus.exception_write32(sp.wrapping_add(4), s.sr)
            .map_err(access)?;
        if interrupt.is_some()
            && let Some(start) = start
        {
            bus.sh2_pipeline_wait((start + 1).saturating_sub(bus.sh2_clock().unwrap()) as u32)
                .map_err(access)?;
        }
        let start = bus.sh2_clock();
        bus.exception_write32(sp, saved_pc).map_err(access)?;
        if interrupt.is_some()
            && let Some(start) = start
        {
            bus.sh2_pipeline_wait((start + 1).saturating_sub(bus.sh2_clock().unwrap()) as u32)
                .map_err(access)?;
        }
        if let Some((level, nmi)) = interrupt {
            s.sr = (s.sr & !0xF0) | u32::from(level) << 4;
            bus.sh2_pipeline_wait(1).map_err(access)?;
            if !nmi {
                vector = bus.sh2_interrupt_vector(level, vector).map_err(access)?;
            }
        }
        let entry_pc = s.pc;
        let access = |source| CpuError::ExceptionEntry {
            pc: entry_pc,
            vector,
            source,
        };
        let start = bus.sh2_clock();
        let target = bus
            .exception_read32(s.vbr.wrapping_add(vector as u32 * 4))
            .map_err(access)?;
        if interrupt.is_some() {
            if let Some(start) = start {
                bus.sh2_pipeline_wait((start + 1).saturating_sub(bus.sh2_clock().unwrap()) as u32)
                    .map_err(access)?;
            }
            bus.sh2_pipeline_wait(1).map_err(access)?;
        }
        // Accept a stacking/vector alignment error after this entry completes.
        // Vector 9 must not recursively re-enter forever (§§4.8.2–3).
        if (sp | s.vbr) & 3 != 0 && vector != 9 {
            s.address_error_pending = true;
        }
        s.r[15] = sp;
        s.pc = target;
        s.delay_target = None;
        s.exceptions += 1;
        s.last_exception = Some(Exception { vector, saved_pc });
        s.status = CpuStatus::Running;
        self.interrupt_inhibited = false;
        self.address_error_inhibited = false;
        bus.sh2_discard_prefetch();
        if interrupt.is_some() {
            // Begin the handler IF at its specified boundary, retaining the
            // fetched instruction for the ordinary IF/ID pipeline.
            self.fetch.handler(bus, target).map_err(access)?;
        }
        Ok(())
    }

    fn address_error(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        // SH7604 §§4.3.2/4.7: save the next instruction, without changing I.
        self.enter_exception(bus, 9, self.state.pc)?;
        self.state.address_error_pending = false;
        Ok(())
    }

    fn illegal(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        // SH7604 §4.5.3: a slot exception saves the pending jump destination.
        let (vector, saved_pc) = self
            .state
            .delay_target
            .map_or((4, self.state.pc), |target| (6, target));
        self.enter_exception(bus, vector, saved_pc)
    }

    fn execute<const SCALAR_RETIRE: bool, E: ExecutionErrors>(
        &mut self,
        bus: &mut impl Memory,
    ) -> Result<(), E::Error> {
        let pc = self.state.pc;
        bus.sh2_write_context(pc, None);
        let access = |source| E::convert(CpuError::Bus { pc, source }, None);
        // SH7604 Table 4.6: no instruction fetches from odd addresses or
        // on-chip peripheral modules. Do not send such reads to a device.
        if pc & 1 != 0 || pc >= 0xFFFF_FE00 {
            self.state.address_error_pending = true;
            if !self.address_error_inhibited {
                return self
                    .address_error(bus)
                    .map_err(|error| E::convert(error, None));
            }
            // An unfetchable successor cannot decode/retire. Finish the branch
            // boundary before accepting the latched error (Table 4.10).
            self.state.pc = self.state.delay_target.take().unwrap_or(pc.wrapping_add(2));
            bus.sh2_discard_prefetch();
            self.address_error_inhibited = false;
            self.interrupt_inhibited = false;
            return Ok(());
        }
        let opcode = self.fetch.select(bus, pc).map_err(access)?;
        // Retain only the already decoded instruction on a rejected SMPC
        // operand or synchronous exception entry. Never re-read firmware or
        // devices for diagnostics. Buffered writes may fault after retirement,
        // when this opcode is unavailable.
        let instruction = |error: CpuError| E::convert(error, Some(opcode));
        let access = |source| instruction(CpuError::Bus { pc, source });
        // H §6.3.2: a before-execution break requires a successfully fetched
        // instruction. A fetch failure or speculative fill cannot cause it.
        if bus.instruction_break(pc, false)
            && 15 > (self.state.sr >> 4) & 15
            && self
                .interrupt(bus, 12, 15, false)
                .map_err(|error| E::convert(error, None))?
        {
            bus.acknowledge_break();
            return Ok(());
        }
        let properties = SCALAR_RETIRE.then(|| timing::instruction(opcode));
        let control =
            properties.map_or_else(|| timing::control_flags(opcode), |p| p.control_flags());
        let unconditional_branch = control & timing::UNCONDITIONAL != 0;
        let rewrites_pc = control & timing::REWRITES_PC != 0;
        if rewrites_pc && self.state.delay_target.is_some() {
            return self.illegal(bus).map_err(instruction);
        }
        let clock = bus.sh2_clock().unwrap_or(self.multiplier.clock);
        let ex = self.load_pipeline.issue_clock(clock);
        let (interlock, states) = if let Some(instruction) = properties {
            (
                self.load_pipeline.issue_instruction(instruction),
                instruction.states(self.state.sr & T != 0),
            )
        } else {
            (
                self.load_pipeline.issue(opcode),
                timing::execution_states(opcode, self.state.sr & T != 0),
            )
        };
        self.last_clocks = states + interlock;
        // A dependency consumes a retained MA/EX clock before adding a wait.
        let wait = interlock.saturating_sub(self.load_pipeline.issue_overlap);
        if wait != 0 {
            bus.sh2_pipeline_wait(wait).map_err(access)?;
        }
        // IF of the second successor and ID of the successor share this EX
        // slot. The real fetch runs through the replay journal and arbiter;
        // its actual completion stretches the slot, without retiring EX early.
        let fetch_start = bus.sh2_clock();
        let successor = self.state.delay_target.unwrap_or(pc.wrapping_add(2));
        let deferred_if = control & timing::DEFERRED_IF != 0;
        let ldc_long = control & timing::LDC_LONG != 0;
        let stc_long = control & timing::STC_LONG != 0;
        let byte_memory = control & timing::BYTE_MEMORY != 0;
        // P fig7.94: SLEEP permits IF of its immediate successor (already
        // selected above), but no IF of the second successor in its EX slot.
        if unconditional_branch {
            self.fetch.hold_delay_slot(bus, ex + u64::from(interlock));
        } else if !deferred_if && !ldc_long && !stc_long && !byte_memory && opcode != 0x001B {
            self.fetch
                .advance(bus, successor, ex + u64::from(interlock))
                .map_err(access)?;
        }
        let stretch = fetch_start.map_or(0, |start| {
            bus.sh2_clock()
                .unwrap()
                .saturating_sub(start)
                .saturating_sub(1)
        });
        self.execution_clock = fetch_start.map(|_| ex + stretch);
        let now = ex + u64::from(interlock) + stretch;
        if opcode & 0xFF00 == 0xC300 {
            // Rev.5 fig7.93 fetches/discards both successors. Complete its
            // arbiter-backed IF above before exception entry clears the stream.
            self.enter_exception(bus, opcode as u8, pc.wrapping_add(2))
                .map_err(instruction)?;
            self.execution_clock = fetch_start.map(|_| ex + stretch);
            self.state.instructions += 1;
            return Ok(());
        }
        // Register ALU operations have no operand or multiplier phases. Their
        // actual IF, WB interlock, break checks and EX timing already completed
        // above; retire here instead of entering memory/control dispatch.
        if SCALAR_RETIRE && let Some(kind) = scalar::decode(opcode) {
            scalar::execute(kind, opcode, &mut self.state);
            if self.state.delay_target.is_some() && bus.sh2_clock().is_none() {
                bus.sh2_discard_prefetch();
            }
            self.state.pc = self.state.delay_target.take().unwrap_or(pc.wrapping_add(2));
            if self.state.status == CpuStatus::Sleeping {
                self.fetch.clear();
                bus.sh2_discard_prefetch();
            }
            self.state.instructions += 1;
            self.interrupt_inhibited = false;
            self.address_error_inhibited = false;
            self.multiplier.clock += u64::from(self.last_clocks);
            return Ok(());
        }
        let mov_access = timing::mov_load_address(opcode, &self.state)
            .map(|address| (address, false))
            .or_else(|| {
                timing::mov_store_address(opcode, &self.state).map(|address| (address, true))
            })
            .map(|(address, write)| timing::MovAccess::start(bus, address, write, successor, now))
            .transpose()
            .map_err(access)?;
        if let Some(mov) = &mov_access {
            self.last_clocks += mov.split();
        }
        let mut multiplier_access = self
            .multiplier
            .access(opcode, successor, now, bus.sh2_fetch_slots())
            .map(|(access, split)| {
                self.last_clocks += split;
                access
            });
        if let Some(multiplier_access) = &mut multiplier_access
            && !multiplier_access.memory_mac
        {
            let address = self.state.r[((opcode >> 8) & 15) as usize];
            let memory = match opcode & 0xF0FF {
                0x4002 | 0x4012 => Some((address.wrapping_sub(4), true)),
                0x4006 | 0x4016 => Some((address, false)),
                _ => None,
            };
            self.multiplier
                .wait(multiplier_access, bus, memory)
                .map_err(access)?;
        }
        if deferred_if && !multiplier_access.as_ref().unwrap().memory_mac {
            // The long multiplier's successor ID stalls once. Its IF is
            // beside MA, with MA first when this is a real longword IF.
            if let Some(clock) = bus.sh2_clock() {
                let first_split = u64::from(successor & 2 != 0);
                bus.sh2_pipeline_wait((now + 1 + first_split).saturating_sub(clock) as u32)
                    .map_err(access)?;
                let start = bus.sh2_clock().unwrap();
                self.fetch
                    .advance(bus, successor, now + 1)
                    .map_err(access)?;
                let extra = bus
                    .sh2_clock()
                    .unwrap()
                    .saturating_sub(start)
                    .saturating_sub(1);
                self.last_clocks += extra as u32;
                multiplier_access.as_mut().unwrap().stretch_if(extra);
            }
        }

        // Invalid operand cycles latch vector 9, allowing the instruction to
        // complete before exception acceptance, including auto-update effects.
        let mut operands = Operands::new(bus);
        let bus = &mut operands;

        let n = ((opcode >> 8) & 15) as usize;
        let m = ((opcode >> 4) & 15) as usize;
        let imm = opcode as u8 as u32;
        let s = &mut self.state;
        let mut next_branch = None;
        let mut sequential_pc = pc.wrapping_add(2);
        let mut direct_branch = false;
        let mut inhibit = false;
        let mut target_if = now + 1;
        let operand_pc = s
            .delay_target
            .map_or(pc.wrapping_add(4), |target| target.wrapping_add(2));

        if !SCALAR_RETIRE && let Some(kind) = scalar::decode_literal(opcode) {
            scalar::execute(kind, opcode, s);
        } else {
            match opcode {
                0x0028 => {
                    s.mach = 0;
                    s.macl = 0;
                } // CLRMAC
                0x001B => {
                    s.status = CpuStatus::Sleeping;
                    bus.inner.enter_sleep();
                }
                0x002B => {
                    // P fig7.92: EX, MA, MA, then target IF. Each MA occupies
                    // at least one slot, including synchronous cache-array reads;
                    // a real bus access stretches that slot until completion.
                    let mut restored = [0; 2];
                    let mut ma_start = now + 1;
                    for (i, value) in restored.iter_mut().enumerate() {
                        let address = s.r[15].wrapping_add(i as u32 * 4);
                        let end = ma_start + 1;
                        if let Some(clock) = bus.inner.sh2_clock() {
                            let scheduled =
                                bus.inner.sh2_schedule_memory_access(address, false, end);
                            let boundary = end - u64::from(scheduled);
                            bus.inner
                                .sh2_pipeline_wait(boundary.saturating_sub(clock) as u32)
                                .map_err(access)?;
                        }
                        *value = bus.read32(address).map_err(access)?;
                        ma_start = if let Some(clock) = bus.inner.sh2_clock() {
                            bus.inner
                                .sh2_pipeline_wait(end.saturating_sub(clock) as u32)
                                .map_err(access)?;
                            clock.max(end)
                        } else {
                            end
                        };
                    }
                    target_if = ma_start;
                    // Restore SR before executing the preserved delay slot.
                    let [target, sr] = restored;
                    s.r[15] = s.r[15].wrapping_add(8);
                    s.sr = sr & SR_MASK;
                    next_branch = Some(target);
                }
                _ if matches!(opcode & 0xF00F, 0x6000..=0x6002 | 0x6004..=0x6006) => {
                    let width = 1 << (opcode & 3);
                    let value = read_operand(bus, s.r[m], width).map_err(access)?;
                    commit_mov_load(s, opcode, value);
                }
                // Predecrement stores latch the source before changing the address register.
                _ if matches!(opcode & 0xF00F, 0x2000..=0x2002 | 0x2004..=0x2006) => {
                    let width = 1 << (opcode & 3);
                    let address = s.r[n].wrapping_sub(if opcode & 4 != 0 { width } else { 0 });
                    write_operand(bus, address, width, s.r[m]).map_err(access)?;
                    s.r[n] = address;
                }
                // R0-indexed loads/stores.
                _ if matches!(opcode & 0xF00F, 0x0004..=0x0006 | 0x000C..=0x000E) => {
                    let width = 1 << (opcode & 3);
                    if opcode & 8 == 0 {
                        write_operand(bus, s.r[n].wrapping_add(s.r[0]), width, s.r[m])
                            .map_err(access)?;
                    } else {
                        s.r[n] = read_operand(bus, s.r[m].wrapping_add(s.r[0]), width)
                            .map_err(access)?;
                    }
                }
                _ if matches!(opcode & 0xF000, 0x1000 | 0x5000) => {
                    let disp = (opcode as u32 & 15) * 4;
                    if opcode & 0xF000 == 0x1000 {
                        bus.write32(s.r[n].wrapping_add(disp), s.r[m])
                            .map_err(access)?;
                    } else {
                        s.r[n] = bus.read32(s.r[m].wrapping_add(disp)).map_err(access)?;
                    }
                }
                _ if matches!(opcode & 0xFF00, 0x8000 | 0x8100 | 0x8400 | 0x8500) => {
                    // Here bits 7..4 select the base, not the usual n field.
                    let width = 1 << ((opcode >> 8) & 1);
                    let address = s.r[m].wrapping_add((opcode as u32 & 15) * width);
                    if opcode & 0x400 == 0 {
                        write_operand(bus, address, width, s.r[0]).map_err(access)?;
                    } else {
                        s.r[0] = read_operand(bus, address, width).map_err(access)?;
                    }
                }
                _ if matches!(opcode & 0xFF00, 0xC000..=0xC200 | 0xC400..=0xC600) => {
                    let width = 1 << ((opcode >> 8) & 3);
                    let address = s.gbr.wrapping_add(imm * width);
                    if opcode & 0x400 == 0 {
                        write_operand(bus, address, width, s.r[0]).map_err(access)?;
                    } else {
                        s.r[0] = read_operand(bus, address, width).map_err(access)?;
                    }
                }
                _ if opcode & 0xF000 == 0x9000 => {
                    s.r[n] = bus
                        .pc_relative16(operand_pc.wrapping_add(imm * 2))
                        .map_err(access)? as i16 as i32 as u32;
                }
                _ if opcode & 0xF000 == 0xD000 => {
                    s.r[n] = bus
                        .pc_relative32((operand_pc & !3).wrapping_add(imm * 4))
                        .map_err(access)?;
                }
                _ if opcode & 0xFF00 == 0xC700 => s.r[0] = (operand_pc & !3).wrapping_add(imm * 4), // MOVA
                _ if opcode & 0xF00F == 0x0007 => s.macl = s.r[n].wrapping_mul(s.r[m]),
                _ if opcode & 0xF00F == 0x200E => s.macl = (s.r[n] & 0xFFFF) * (s.r[m] & 0xFFFF),
                _ if opcode & 0xF00F == 0x200F => {
                    s.macl = (s.r[n] as i16 as i32).wrapping_mul(s.r[m] as i16 as i32) as u32
                }
                _ if opcode & 0xF00F == 0x3005 => s.set_accumulator(s.r[n] as u64 * s.r[m] as u64),
                _ if opcode & 0xF00F == 0x300D => {
                    s.set_accumulator((s.r[n] as i32 as i64 * s.r[m] as i32 as i64) as u64)
                }
                _ if matches!(opcode & 0xF00F, 0x000F | 0x400F) => {
                    let width = if opcode & 0x4000 == 0 { 4 } else { 2 };
                    // Read/increment Rn first, then Rm, including when n == m.
                    self.multiplier
                        .first_operand(multiplier_access.as_ref().unwrap(), bus.inner, s.r[n])
                        .map_err(access)?;
                    let a = read_operand(bus, s.r[n], width).map_err(access)? as i32 as i64;
                    if let Some(start) = bus.inner.sh2_clock() {
                        self.fetch
                            .advance(bus.inner, successor, start)
                            .map_err(access)?;
                        let duration = bus.inner.sh2_clock().unwrap() - start;
                        multiplier_access.as_mut().unwrap().first_if(duration);
                    }
                    let m_address = s.r[m].wrapping_add(if n == m { width } else { 0 });
                    self.multiplier
                        .wait(
                            multiplier_access.as_mut().unwrap(),
                            bus.inner,
                            Some((m_address, false)),
                        )
                        .map_err(access)?;
                    let b = read_operand(bus, m_address, width).map_err(access)? as i32 as i64;
                    s.r[n] = s.r[n].wrapping_add(width);
                    s.r[m] = s.r[m].wrapping_add(width);
                    let product = a * b;
                    if s.sr & S == 0 {
                        s.set_accumulator(s.accumulator().wrapping_add(product as u64));
                    } else if width == 2 {
                        // SH-2 (unlike SH-1) leaves MACH unchanged for saturated MAC.W.
                        s.macl = (s.macl as i32 as i64 + product)
                            .clamp(i32::MIN as i64, i32::MAX as i64)
                            as u32;
                    } else {
                        let acc = ((s.accumulator() << 16) as i64) >> 16;
                        let sum = (acc + product).clamp(-(1_i64 << 47), (1_i64 << 47) - 1);
                        s.set_accumulator(sum as u64);
                    }
                }
                _ if byte_memory => {
                    // P figs7.75/76: hold younger ID/IF through initial EX
                    // and read MA. TAS also holds them through second EX.
                    let tas = opcode & 0xF0FF == 0x401B;
                    let address = if tas {
                        s.r[n]
                    } else {
                        s.gbr.wrapping_add(s.r[0])
                    };
                    let read_end = now + 2;
                    if let Some(clock) = bus.inner.sh2_clock() {
                        // TAS bypasses cache even for a resident operand. Start
                        // its locked external read at MA, not at cache completion.
                        let scheduled = tas
                            || bus
                                .inner
                                .sh2_schedule_memory_access(address, false, read_end);
                        let boundary = read_end - u64::from(scheduled);
                        bus.inner
                            .sh2_pipeline_wait(boundary.saturating_sub(clock) as u32)
                            .map_err(access)?;
                    }
                    let value = if tas {
                        bus.tas_read(address)
                    } else {
                        bus.read8(address)
                    }
                    .map_err(access)?;
                    if let Some(clock) = bus.inner.sh2_clock() {
                        bus.inner
                            .sh2_pipeline_wait(read_end.saturating_sub(clock) as u32)
                            .map_err(access)?;
                    }
                    let second_ex = bus.inner.sh2_clock().unwrap_or(read_end).max(read_end);
                    if !tas {
                        self.fetch
                            .advance(bus.inner, successor, second_ex)
                            .map_err(access)?;
                    }
                    let second_end =
                        (second_ex + 1).max(bus.inner.sh2_clock().unwrap_or(second_ex));
                    if let Some(clock) = bus.inner.sh2_clock() {
                        bus.inner
                            .sh2_pipeline_wait(second_end.saturating_sub(clock) as u32)
                            .map_err(access)?;
                    }
                    let write = if tas {
                        s.set_t(value == 0);
                        Some(value | 0x80)
                    } else {
                        match (opcode >> 8) & 3 {
                            0 => {
                                s.set_t(value & imm as u8 == 0);
                                None
                            }
                            1 => Some(value & imm as u8),
                            2 => Some(value ^ imm as u8),
                            _ => Some(value | imm as u8),
                        }
                    };
                    let mut write_stretch = 0;
                    if let Some(value) = write {
                        let write_end = second_end + 1;
                        if let Some(clock) = bus.inner.sh2_clock() {
                            let scheduled = bus
                                .inner
                                .sh2_schedule_memory_access(address, true, write_end);
                            let boundary = write_end - u64::from(scheduled);
                            bus.inner
                                .sh2_pipeline_wait(boundary.saturating_sub(clock) as u32)
                                .map_err(access)?;
                        }
                        if tas {
                            bus.tas_write(address, value)
                        } else {
                            bus.write8(address, value)
                        }
                        .map_err(access)?;
                        if let Some(clock) = bus.inner.sh2_clock() {
                            bus.inner
                                .sh2_pipeline_wait(write_end.saturating_sub(clock) as u32)
                                .map_err(access)?;
                            let completion = clock.max(write_end);
                            write_stretch = completion - write_end;
                            if tas {
                                // Final MA wins against real IF; retained if shares
                                // the same slot. Count the contention only once.
                                self.fetch
                                    .advance(bus.inner, successor, second_end)
                                    .map_err(access)?;
                                self.last_clocks +=
                                    u32::from(bus.inner.sh2_fetch_slots() && successor & 2 != 0);
                                write_stretch += bus
                                    .inner
                                    .sh2_clock()
                                    .unwrap()
                                    .saturating_sub(completion)
                                    .saturating_sub(1);
                            } else {
                                // Final MA shares the successor's EX (fig7.75).
                                self.load_pipeline.retain_ex(completion - 1);
                            }
                        }
                    }
                    self.execution_clock = fetch_start.map(|_| {
                        ex + (second_ex - read_end) + (second_end - second_ex - 1) + write_stretch
                    });
                }
                _ if matches!(
                    opcode & 0xF0FF,
                    0x0002 | 0x0012 | 0x0022 | 0x000A | 0x001A | 0x002A
                ) =>
                {
                    s.r[n] = special(s, opcode & 15 == 2, (opcode >> 4) & 3);
                    inhibit = true;
                }
                _ if matches!(
                    opcode & 0xF0FF,
                    0x4003 | 0x4013 | 0x4023 | 0x4002 | 0x4012 | 0x4022
                ) =>
                {
                    let address = s.r[n].wrapping_sub(4);
                    // P fig7.85 / Rev.5 p.491: hold successor ID in EX.
                    // Resume younger IF beside MA, with the operand first when
                    // IF needs the bus. Actual MA/IF waits stretch this slot once.
                    let ma_end = now + 2;
                    if stc_long && let Some(clock) = bus.inner.sh2_clock() {
                        let scheduled = bus.inner.sh2_schedule_memory_access(address, true, ma_end);
                        let boundary = ma_end - u64::from(scheduled);
                        bus.inner
                            .sh2_pipeline_wait(boundary.saturating_sub(clock) as u32)
                            .map_err(access)?;
                    }
                    bus.write32(address, special(s, opcode & 1 != 0, (opcode >> 4) & 3))
                        .map_err(access)?;
                    if stc_long && let Some(clock) = bus.inner.sh2_clock() {
                        bus.inner
                            .sh2_pipeline_wait(ma_end.saturating_sub(clock) as u32)
                            .map_err(access)?;
                        let ma_completion = clock.max(ma_end);
                        self.fetch
                            .advance(bus.inner, successor, now + 1)
                            .map_err(access)?;
                        let if_stretch = bus
                            .inner
                            .sh2_clock()
                            .unwrap()
                            .saturating_sub(ma_completion)
                            .saturating_sub(1);
                        self.last_clocks +=
                            u32::from(bus.inner.sh2_fetch_slots() && successor & 2 != 0);
                        self.execution_clock =
                            fetch_start.map(|_| ex + (ma_completion - ma_end) + if_stretch);
                    }
                    s.r[n] = address;
                    inhibit = true;
                }
                _ if matches!(
                    opcode & 0xF0FF,
                    0x400E | 0x401E | 0x402E | 0x400A | 0x401A | 0x402A
                ) =>
                {
                    set_special(s, opcode & 15 == 14, (opcode >> 4) & 3, s.r[n]);
                    inhibit = true;
                }
                _ if matches!(
                    opcode & 0xF0FF,
                    0x4007 | 0x4017 | 0x4027 | 0x4006 | 0x4016 | 0x4026
                ) =>
                {
                    // P fig7.84 / Rev.5 p.491: successor ID stalls through
                    // initial EX and MA. Younger IF resumes beside final EX,
                    // after the real operand's completion, including its waits.
                    let ma_end = now + 2;
                    if ldc_long && let Some(clock) = bus.inner.sh2_clock() {
                        let scheduled = bus.inner.sh2_schedule_memory_access(s.r[n], false, ma_end);
                        let boundary = ma_end - u64::from(scheduled);
                        bus.inner
                            .sh2_pipeline_wait(boundary.saturating_sub(clock) as u32)
                            .map_err(access)?;
                    }
                    let value = bus.read32(s.r[n]).map_err(access)?;
                    if ldc_long && let Some(clock) = bus.inner.sh2_clock() {
                        bus.inner
                            .sh2_pipeline_wait(ma_end.saturating_sub(clock) as u32)
                            .map_err(access)?;
                        let final_ex = clock.max(ma_end);
                        self.fetch
                            .advance(bus.inner, successor, final_ex)
                            .map_err(access)?;
                        let if_stretch = bus
                            .inner
                            .sh2_clock()
                            .unwrap()
                            .saturating_sub(final_ex)
                            .saturating_sub(1);
                        // MA and final EX are distinct slots: stretch each once.
                        self.execution_clock =
                            fetch_start.map(|_| ex + (final_ex - ma_end) + if_stretch);
                    }
                    s.r[n] = s.r[n].wrapping_add(4);
                    set_special(s, opcode & 1 != 0, (opcode >> 4) & 3, value);
                    inhibit = true;
                }
                _ if let Some(branch) = Branch::decode(opcode, s) => {
                    if branch.link {
                        s.pr = pc.wrapping_add(4);
                    }
                    inhibit = branch.inhibit;
                    if let Some(target) = branch.target {
                        if branch.delayed {
                            next_branch = Some(target);
                        } else {
                            sequential_pc = target;
                            direct_branch = true;
                        }
                    }
                }
                _ => return self.illegal(bus.inner).map_err(instruction),
            }
        }
        if let Some(mov) = mov_access {
            mov.finish(&mut self.load_pipeline, bus.inner)
                .map_err(access)?;
        }
        if direct_branch || next_branch.is_some() {
            bus.inner.sh2_discard_prefetch();
            self.fetch
                .redirect(
                    bus.inner,
                    next_branch.unwrap_or(sequential_pc),
                    next_branch.is_some(),
                    target_if,
                )
                .map_err(access)?;
        } else if s.delay_target.is_some() && bus.inner.sh2_clock().is_none() {
            bus.inner.sh2_discard_prefetch();
        }
        s.pc = s.delay_target.take().unwrap_or(sequential_pc);
        s.delay_target = next_branch;
        if s.status == CpuStatus::Sleeping {
            self.fetch.clear();
            bus.inner.sh2_discard_prefetch();
        }
        s.instructions += 1;
        s.address_error_pending |= operands.address_error.get();
        self.interrupt_inhibited = inhibit;
        self.address_error_inhibited =
            next_branch.is_some() || matches!(opcode & 0xFF00, 0x8D00 | 0x8F00);
        if let Some(access) = multiplier_access {
            self.last_clocks +=
                self.multiplier
                    .finish(access, operands.inner, &mut self.load_pipeline);
        }
        self.multiplier.clock += u64::from(self.last_clocks);
        Ok(())
    }
}

fn commit_mov_load(s: &mut CpuState, opcode: u16, value: u32) {
    let n = (opcode >> 8 & 15) as usize;
    let m = (opcode >> 4 & 15) as usize;
    if opcode & 0xF000 == 0x6000 && opcode & 4 != 0 && n != m {
        s.r[m] = s.r[m].wrapping_add(1 << (opcode & 3));
    }
    let destination = if matches!(opcode >> 12, 8 | 0xC) {
        0
    } else {
        n
    };
    s.r[destination] = value;
}

pub(crate) fn read_operand(bus: &impl Memory, address: u32, width: u32) -> Result<u32, BusError> {
    match width {
        1 => Ok(bus.read8(address)? as i8 as i32 as u32),
        2 => Ok(bus.read16(address)? as i16 as i32 as u32),
        _ => bus.read32(address),
    }
}

pub(crate) fn write_operand(
    bus: &mut impl Memory,
    address: u32,
    width: u32,
    value: u32,
) -> Result<(), BusError> {
    match width {
        1 => bus.write8(address, value as u8),
        2 => bus.write16(address, value as u16),
        _ => bus.write32(address, value),
    }
}

fn special(s: &CpuState, control: bool, index: u16) -> u32 {
    match (control, index) {
        (true, 0) => s.sr,
        (true, 1) => s.gbr,
        (true, _) => s.vbr,
        (false, 0) => s.mach,
        (false, 1) => s.macl,
        (false, _) => s.pr,
    }
}

fn set_special(s: &mut CpuState, control: bool, index: u16, value: u32) {
    match (control, index) {
        (true, 0) => s.sr = value & SR_MASK,
        (true, 1) => s.gbr = value,
        (true, _) => s.vbr = value,
        (false, 0) => s.mach = value,
        (false, 1) => s.macl = value,
        (false, _) => s.pr = value,
    }
}

#[cfg(test)]
mod address_tests;
#[cfg(test)]
mod load_tests;
#[cfg(test)]
mod multiplier_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod tests;
