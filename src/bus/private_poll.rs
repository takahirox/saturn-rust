//! Guarded memoization of private slave polling microstates.
use super::cached::PeerSchedule;
use super::transactions::PrivateTransaction;
use super::*;
use crate::cache::CacheTouch;
use crate::onchip;
use crate::{CachedIssue, Cpu, CpuBoundary, Machine, MachineError};

#[derive(Default)]
pub(crate) struct Context {
    templates: Vec<Template>,
    recent: Option<usize>,
    scratch: Option<Box<Machine>>,
    pub(crate) retry: u64,
}
#[derive(Clone)]
struct State {
    at: u64,
    cpu: Cpu,
    ready: u64,
    boundary: Option<CpuBoundary>,
    issue: Option<CachedIssue>,
    transaction: PrivateTransaction,
    observed: u8,
    epoch: u64,
    replacement: [u8; 64],
}
impl CachedIssue {
    fn shift_private_clock(&mut self, elapsed: u64) {
        match self {
            Self::Branch(branch) => branch.shift_private_clock(elapsed),
            Self::Load(load) => load.plan.shift_private_clock(elapsed),
            Self::Store(store) => store.plan.shift_private_clock(elapsed),
        }
    }
}
fn shift_ready(ready: u64, elapsed: u64) -> u64 {
    if ready == u64::MAX {
        ready
    } else {
        ready + elapsed
    }
}
impl State {
    fn capture(machine: &Machine) -> Option<Self> {
        Some(Self {
            at: machine.bus.main_clocks,
            cpu: machine.slave.clone(),
            ready: machine.cpu_ready[1],
            boundary: machine.cpu_boundary[1],
            issue: machine.cached_issue[1].clone(),
            transaction: machine.bus.private_transaction()?,
            observed: machine.bus.onchip[1].private_poll_observed(),
            epoch: machine.bus.onchip[1].private_poll_epoch(),
            replacement: machine.bus.cache[1].borrow().replacement_state(),
        })
    }
    fn same_phase(&self, prior: &Self) -> bool {
        let Some(elapsed) = self.at.checked_sub(prior.at) else {
            return false;
        };
        if self.cpu.state().pc != prior.cpu.state().pc
            || self.cpu.execution_clock() != prior.cpu.execution_clock().map(|at| at + elapsed)
            || self.cpu.last_clocks() != prior.cpu.last_clocks()
            || self.observed != prior.observed
            || self.ready != shift_ready(prior.ready, elapsed)
            || !self.cpu.same_cached_cycle_origin(&prior.cpu, elapsed)
        {
            return false;
        }
        let mut issue = prior.issue.clone();
        if let Some(issue) = &mut issue {
            issue.shift_private_clock(elapsed);
        }
        if self.issue != issue {
            return false;
        }
        let mut boundary = prior.boundary;
        if let Some(boundary) = &mut boundary {
            boundary.started += elapsed;
        }
        if self.boundary != boundary {
            return false;
        }
        let mut transaction = prior.transaction.clone();
        transaction.shift_clock(elapsed);
        self.transaction == transaction
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Code {
    address: u32,
    bytes: [u8; 4],
    touch: CacheTouch,
}
struct Phase {
    state: State,
    keep: [u8; 64],
    value: [u8; 64],
}
struct Template {
    initial: State,
    control: u8,
    status: u8,
    code: Vec<Code>,
    phases: Vec<Phase>,
    prefix: usize,
    period: usize,
}
impl Template {
    fn matches(&self, machine: &Machine) -> bool {
        let prior = &self.initial;
        let Some(elapsed) = machine.bus.main_clocks.checked_sub(prior.at) else {
            return false;
        };
        if self.control != machine.bus.cache[1].borrow().control()
            || self.status != machine.bus.onchip[1].ftcsr()
            || machine.slave.state().pc != prior.cpu.state().pc
            || machine.slave.execution_clock() != prior.cpu.execution_clock().map(|at| at + elapsed)
            || machine.slave.last_clocks() != prior.cpu.last_clocks()
            || machine.bus.onchip[1].private_poll_observed() != prior.observed
            || machine.cpu_ready[1] != shift_ready(prior.ready, elapsed)
            || !machine.slave.same_cached_cycle_origin(&prior.cpu, elapsed)
        {
            return false;
        }
        let mut issue = prior.issue.clone();
        if let Some(issue) = &mut issue {
            issue.shift_private_clock(elapsed);
        }
        let mut boundary = prior.boundary;
        if let Some(boundary) = &mut boundary {
            boundary.started += elapsed;
        }
        machine.cached_issue[1] == issue
            && machine.cpu_boundary[1] == boundary
            && machine
                .bus
                .private_transaction_matches(&prior.transaction, elapsed)
            && self.code.iter().all(|code| {
                machine.bus.private_code(code.address) == Some((code.bytes, code.touch))
            })
    }
    fn publish(&self, machine: &mut Machine, elapsed: u64) {
        let elapsed = elapsed as usize;
        let (index, copies) = if elapsed < self.prefix + self.period {
            (elapsed, 0)
        } else {
            let relative = elapsed - self.prefix;
            (
                self.prefix + self.period + relative % self.period,
                relative / self.period - 1,
            )
        };
        let phase = &self.phases[index];
        let first = &self.phases[self.prefix].state;
        let second = &self.phases[self.prefix + self.period].state;
        let extra = self.period as u64 * copies as u64;
        let shift = machine.bus.main_clocks - elapsed as u64 - self.initial.at;
        let mut cpu = phase.state.cpu.clone();
        cpu.rebase_poll_prefix(&self.initial.cpu, &machine.slave, shift);
        cpu.publish_poll_periods(
            self.period as u64,
            second.cpu.state().instructions - first.cpu.state().instructions,
            second.cpu.cached_local_clock() - first.cpu.cached_local_clock(),
            copies as u64,
        );
        machine.slave = cpu;
        machine.cpu_ready[1] = shift_ready(phase.state.ready, shift + extra);
        machine.cpu_boundary[1] = phase.state.boundary;
        if let Some(boundary) = &mut machine.cpu_boundary[1] {
            boundary.started += shift + extra;
        }
        machine.cached_issue[1] = phase.state.issue.clone();
        if let Some(issue) = &mut machine.cached_issue[1] {
            issue.shift_private_clock(shift + extra);
        }
        let mut transaction = phase.state.transaction.clone();
        transaction.shift_clock(shift + extra);
        machine.bus.restore_private_transaction(transaction);
        let epoch = machine.bus.onchip[1].private_poll_epoch() + phase.state.epoch
            - self.initial.epoch
            + (second.epoch - first.epoch) * copies as u64;
        machine.bus.onchip[1].publish_private_poll(phase.state.observed, epoch);
        machine
            .bus
            .publish_private_replacement(&phase.keep, &phase.value);
    }
}
fn remember_code(machine: &Machine, code: &mut Vec<Code>) -> Option<()> {
    let pc = machine.slave.state().pc;
    let mut addresses = vec![
        pc,
        pc.wrapping_add(2),
        pc.wrapping_add(4),
        pc.wrapping_add(6),
    ];
    addresses.extend(machine.slave.private_code_addresses().into_iter().flatten());
    match &machine.cached_issue[1] {
        Some(CachedIssue::Load(load)) => addresses.push(load.plan.future),
        Some(CachedIssue::Branch(branch)) => {
            addresses.extend(branch.private_code_addresses().into_iter().flatten())
        }
        Some(CachedIssue::Store(_)) => return None,
        None => (),
    }
    for address in addresses {
        let address = address & !3;
        let (bytes, touch) = machine.bus.private_code(address)?;
        if let Some(prior) = code.iter().find(|prior| prior.address == address) {
            if prior.bytes != bytes || prior.touch != touch {
                return None;
            }
        } else {
            if code.len() == 32 {
                return None;
            }
            code.push(Code {
                address,
                bytes,
                touch,
            });
        }
    }
    Some(())
}
fn private_instruction(cpu: &Cpu) -> bool {
    cpu.decoded_opcode().is_some_and(|op| {
        Cpu::is_scalar(op)
            || cpu.has_cached_control(op)
            || (op & 0xF00F == 0x6000 && cpu.cached_load_address(op) == Some(onchip::FRT + 1))
    })
}
impl Context {
    pub(crate) fn has_templates(&self) -> bool {
        !self.templates.is_empty()
    }
    fn matching(&mut self, machine: &Machine) -> Option<usize> {
        if let Some(index) = self.recent
            && self
                .templates
                .get(index)
                .is_some_and(|template| template.matches(machine))
        {
            return Some(index);
        }
        if let Some(index) = self
            .templates
            .iter()
            .position(|template| template.matches(machine))
        {
            self.recent = Some(index);
            return Some(index);
        }
        None
    }
    fn prepare(&mut self, machine: &Machine, span: u32) -> Option<usize> {
        if let Some(index) = self.matching(machine) {
            return Some(index);
        }
        let initial = State::capture(machine)?;
        let span = span.min(128);
        let scratch = self.scratch.get_or_insert_with(|| Box::new(Machine::new()));
        let mut first = Vec::new();
        let mut code = Vec::new();
        let mut repetition = None;
        let mut ones = Vec::new();
        for seed in [0, 0x3F] {
            scratch.bus.setup_private_probe(&machine.bus, seed)?;
            scratch.slave = machine.slave.clone();
            scratch.cpu_ready[1] = machine.cpu_ready[1];
            scratch.cpu_boundary[1] = machine.cpu_boundary[1];
            scratch.cached_issue[1] = machine.cached_issue[1].clone();
            scratch.cpu_reset_vectors[1] = None;
            scratch.dma_errors = [false; 2];
            scratch.fault = None;
            for time in 0..=span as usize {
                remember_code(scratch, &mut code)?;
                for _ in 0..8 {
                    if scratch.cpu_ready[1] > scratch.bus.main_clocks {
                        break;
                    }
                    if !private_instruction(&scratch.slave) {
                        return None;
                    }
                    scratch.issue_cpu_with_inline(Processor::Slave, true).ok()?;
                    remember_code(scratch, &mut code)?;
                }
                if scratch.cpu_ready[1] <= scratch.bus.main_clocks {
                    return None;
                }
                scratch.bus.grant_bus_transaction();
                let state = State::capture(scratch)?;
                if seed == 0 {
                    if repetition.is_none() {
                        repetition = first
                            .iter()
                            .enumerate()
                            .find(|(_, prior): &(usize, &State)| {
                                state.cpu.state().instructions > prior.cpu.state().instructions
                                    && state.same_phase(prior)
                            })
                            .map(|(index, _)| (index, time - index));
                    }
                    first.push(state);
                } else {
                    let phase = first.get(time)?;
                    if state.cpu.state().instructions != phase.cpu.state().instructions
                        || !state.same_phase(phase)
                        || state.epoch != phase.epoch
                    {
                        return None;
                    }
                    ones.push(state.replacement);
                }
                if repetition.is_some_and(|(prefix, period)| time == prefix + period * 2) {
                    break;
                }
                let clock = scratch.bus.main_clocks;
                scratch.cpu_ready[0] = clock + 1;
                while scratch.bus.main_clocks == clock {
                    scratch
                        .finish_hardware_boundary_with_idle(false, false)
                        .ok()?;
                }
            }
            repetition?;
            if seed == 0x3F {
                break;
            }
        }
        let (prefix, period) = repetition?;
        if first.len() != prefix + period * 2 + 1 {
            return None;
        }
        let phases = first
            .into_iter()
            .zip(ones)
            .map(|(state, ones)| {
                let keep = std::array::from_fn(|i| state.replacement[i] ^ ones[i]);
                let value = state.replacement;
                Phase { state, keep, value }
            })
            .collect();
        if self.templates.len() == 32 {
            self.templates.remove(0);
        }
        let index = self.templates.len();
        self.templates.push(Template {
            initial,
            control: machine.bus.cache[1].borrow().control(),
            status: machine.bus.onchip[1].ftcsr(),
            code,
            phases,
            prefix,
            period,
        });
        self.recent = Some(index);
        Some(index)
    }
}
impl Machine {
    pub(crate) fn try_private_poll_batch(&mut self, limit: u32) -> Result<u32, MachineError> {
        let now = self.bus.main_clocks;
        if !self.bus.slave_enabled
            || self.fault.is_some()
            || self.nmi_pending
            || self.dma_errors.iter().any(|&error| error)
            || self.cpu_boundary[0].is_some()
            || self.cached_issue[0].is_some()
            || self.cpu_reset_vectors.iter().any(Option::is_some)
            || self.cpu_ready[0].saturating_sub(now) > 8
            || !self.bus.cached_private_master_candidate(&self.cpu)
        {
            return Ok(0);
        }
        let span = self.bus.private_probe_span();
        if span < 2 {
            return Ok(0);
        }
        let mut context = std::mem::take(&mut self.private_poll);
        // The retry deadline bounds new proof construction. Retained proofs
        // recheck the complete private CPU, transaction, status and code state.
        let can_prepare = now >= context.retry;
        let matched = if can_prepare {
            context.prepare(self, span)
        } else {
            context.matching(self)
        };
        let Some(index) = matched else {
            if can_prepare {
                context.retry = now + 64;
            }
            self.private_poll = context;
            return Ok(0);
        };
        let mask = (self.slave.state().sr >> 4) & 15;
        let poll = PeerSchedule::Pending { mask, span };
        let saved = self.bus.take_private_transaction();
        let result =
            self.bus
                .cached_batch(&self.cpu, limit, Some(&poll), now.max(self.cpu_ready[0]));
        let batch = match result {
            Ok(Some(batch)) => batch,
            other => {
                self.bus.restore_private_transaction(saved);
                context.retry = now + 64;
                self.private_poll = context;
                if let Err(error) = other {
                    self.fault = Some(error.clone());
                    return Err(error);
                }
                return Ok(0);
            }
        };
        drop(saved);
        self.cpu = batch.cpu;
        context.templates[index].publish(self, self.bus.main_clocks - now);
        self.cpu_ready[0] = batch.ready;
        self.clocks += batch.reference;
        self.private_poll = context;
        #[cfg(test)]
        {
            self.cached_private_phases += 1;
            self.cached_retired += u64::from(batch.retired);
        }
        if let Err(error) = self.finish_hardware_boundary(true) {
            self.fault = Some(error.clone());
            return Err(error);
        }
        Ok(batch.retired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_poll_templates_recheck_ftcsr_and_resident_code() {
        for change in [0, 1, 2] {
            let mut m = crate::scheduling_tests::cached_pair(
                &[0x7001, 0x7101, 0x7201, 0xAFFB, 9],
                &[0x60F0, 0xC880, 0x89FC, 0xAFFB, 9],
            );
            m.slave = Cpu::start(HIGH_WRAM + 0x100, onchip::FRT + 1);
            m.slave.prime_fetch(&m.bus.for_cpu(Processor::Slave));
            let mut context = Context::default();
            let index = context.prepare(&m, m.bus.private_probe_span()).unwrap();
            assert!(context.templates[index].matches(&m));
            match change {
                0 => m.bus.write16(0x21000000, 0).unwrap(),
                1 => m
                    .bus
                    .for_cpu(Processor::Slave)
                    .write16(HIGH_WRAM + 0x106, 0xE011)
                    .unwrap(),
                2 => m
                    .bus
                    .for_cpu(Processor::Slave)
                    .write8(crate::cache::CCR, 9)
                    .unwrap(),
                _ => unreachable!(),
            }
            assert!(!context.templates[index].matches(&m), "change={change}");
        }
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    #[test]
    fn a_rejected_external_probe_cannot_poison_the_next_private_proof() {
        let mut source = crate::scheduling_tests::cached_pair(
            &[0x7001, 0x7101, 0x7201, 0xAFFB, 9],
            &[0x60F0, 0xC880, 0x89FC, 0xAFFB, 9],
        );
        source.slave = Cpu::start(HIGH_WRAM + 0x100, onchip::FRT + 1);
        source
            .slave
            .prime_fetch(&source.bus.for_cpu(Processor::Slave));
        let mut scratch = Box::new(Machine::new());
        scratch.bus.set_slave_enabled(true);
        assert_eq!(
            scratch
                .bus
                .replay_cpu(Processor::Slave)
                .read32(HIGH_WRAM + 0x800),
            Err(BusError::Pending)
        );
        scratch.bus.grant_bus_transaction();
        assert!(scratch.bus.private_transaction().is_none());
        let mut context = Context {
            scratch: Some(scratch),
            ..Context::default()
        };
        assert!(
            context
                .prepare(&source, source.bus.private_probe_span())
                .is_some()
        );
    }
}

impl Bus {
    fn setup_private_probe(&mut self, source: &Self, replacement: u8) -> Option<()> {
        // A rejected cache miss may leave a private probe's external grant.
        // Every attempt starts with fresh transaction ownership and journals.
        self.transactions = Default::default();
        self.external_owner.set(source.external_owner.get());
        self.restore_private_transaction(source.private_transaction()?);
        self.slave_enabled = true;
        self.main_clocks = source.main_clocks;
        self.system_clock = source.system_clock.clone();
        self.pending_main_clocks = source.pending_main_clocks;
        self.raster = source.raster.clone();
        self.vdp2.hblank = source.vdp2.hblank;
        self.vdp2.vblank = source.vdp2.vblank;
        self.irq_observed[1] = source.irq_observed[1];
        self.onchip[1] = source.onchip[1].clone();
        self.cache[1]
            .get_mut()
            .copy_private(&source.cache[1].borrow(), replacement);
        Some(())
    }
    fn private_probe_span(&self) -> u32 {
        if self.smpc.clocks_to_cpu_reset().is_some()
            || !self.onchip[1].cached_batch_allowed()
            || self.onchip[1].monitors_external_bus()
        {
            return 0;
        }
        self.onchip[1]
            .clocks_to_event()
            .saturating_sub(9)
            .min(self.raster.clocks_to_event().saturating_sub(1))
            .min(1024)
    }
    fn private_code(&self, address: u32) -> Option<([u8; 4], crate::cache::CacheTouch)> {
        self.cache[1].borrow().peek_cached_touch(address & !3)
    }
    fn publish_private_replacement(&mut self, keep: &[u8; 64], value: &[u8; 64]) {
        let mut lru = self.cache[1].borrow().replacement_state();
        for i in 0..64 {
            lru[i] = (lru[i] & keep[i]) | value[i];
        }
        self.cache[1].get_mut().commit_replacement_state(lru);
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    fn polling_machine() -> Machine {
        let mut machine = crate::scheduling_tests::cached_pair(
            &[0x7001, 0x7101, 0x7201, 0xAFFB, 9],
            &[0x60F0, 0xC880, 0x89FC, 0xAFFB, 9],
        );
        machine.slave = Cpu::start(HIGH_WRAM + 0x100, onchip::FRT + 1);
        machine
            .slave
            .prime_fetch(&machine.bus.for_cpu(Processor::Slave));
        let mut context = Context::default();
        context
            .prepare(&machine, machine.bus.private_probe_span())
            .unwrap();
        context.retry = machine.bus.main_clocks + 64;
        machine.private_poll = context;
        machine
    }
    #[test]
    fn known_private_proof_runs_while_new_probes_are_throttled() {
        let mut optimized = polling_machine();
        let mut reference = polling_machine();
        let templates = optimized.private_poll.templates.len();
        assert!(optimized.try_private_poll_batch(32).unwrap() > 0);
        assert_eq!(optimized.private_poll.templates.len(), templates);
        for _ in 0..2 {
            optimized.advance_frame().unwrap();
            while reference.frames() < optimized.frames() {
                reference.step().unwrap();
            }
            assert_eq!(optimized.cpu, reference.cpu);
            assert_eq!(optimized.slave, reference.slave);
            assert_eq!(optimized.cpu_ready, reference.cpu_ready);
            assert_eq!(optimized.bus.main_clocks, reference.bus.main_clocks);
            assert_eq!(optimized.clocks, reference.clocks);
            for index in 0..2 {
                optimized.bus.cache[index]
                    .borrow()
                    .assert_same_contents(&reference.bus.cache[index].borrow());
                optimized.bus.onchip[index]
                    .assert_cached_read_bus_matches(&reference.bus.onchip[index]);
            }
        }
    }
    #[test]
    fn repeated_misses_keep_the_scheduled_retry_time() {
        for change in [0, 1, 2] {
            let mut machine = polling_machine();
            let retry = machine.private_poll.retry;
            match change {
                0 => machine.bus.write16(0x21000000, 0).unwrap(),
                1 => machine
                    .bus
                    .for_cpu(Processor::Slave)
                    .write16(HIGH_WRAM + 0x106, 0xE011)
                    .unwrap(),
                2 => machine
                    .bus
                    .for_cpu(Processor::Slave)
                    .write8(crate::cache::CCR, 9)
                    .unwrap(),
                _ => unreachable!(),
            }
            let cpu = machine.cpu.clone();
            let slave = machine.slave.clone();
            let clock = machine.bus.main_clocks;
            for _ in 0..3 {
                assert_eq!(
                    machine.try_private_poll_batch(32).unwrap(),
                    0,
                    "change={change}"
                );
                assert_eq!(machine.private_poll.retry, retry);
                assert_eq!(machine.cpu, cpu);
                assert_eq!(machine.slave, slave);
                assert_eq!(machine.bus.main_clocks, clock);
            }
        }
    }
}
