//! Capture a complete observed period through ordinary, unbatched CPU issue.
use super::*;
use crate::bus::{Bus, Processor};
mod compiled;
mod dependencies;
mod timeline;

pub(in crate::bus) struct Phase {
    pub(in crate::bus) at: u64,
    pub(in crate::bus) ready: u64,
    pub(in crate::bus) cpu: Cpu,
    pub(in crate::bus) cache: Vec<u8>,
    pub(in crate::bus) retained: Option<(u32, u16)>,
    pub(in crate::bus) epoch: u64,
    pub(in crate::bus) touch_end: usize,
}
pub(in crate::bus) struct Read {
    pub(in crate::bus) owner: u64,
    pub(in crate::bus) at: u64,
    pub(in crate::bus) address: u32,
    pub(in crate::bus) width: usize,
    pub(in crate::bus) value: u32,
    pub(in crate::bus) instruction: bool,
}
#[derive(Clone, Copy)]
struct StoreRequest {
    pub(in crate::bus) owner: u64,
    pub(in crate::bus) at: u64,
    pub(in crate::bus) pc: u32,
    pub(in crate::bus) address: u32,
    pub(in crate::bus) width: usize,
    pub(in crate::bus) value: u32,
}
pub(in crate::bus) struct Store {
    pub(in crate::bus) owner: u64,
    pub(in crate::bus) requested: Option<u64>,
    pub(in crate::bus) accepted: u64,
    pub(in crate::bus) pc: u32,
    pub(in crate::bus) address: u32,
    pub(in crate::bus) width: usize,
    pub(in crate::bus) value: u32,
}
pub(in crate::bus) struct CapturedLoop {
    pub(in crate::bus) phases: Vec<Phase>,
    pub(in crate::bus) reads: Vec<Read>,
    pub(in crate::bus) stores: Vec<Store>,
    pub(in crate::bus) external_reads: u64,
    pub(in crate::bus) unsupported: u64,
    pub(in crate::bus) complete: bool,
    pub(in crate::bus) header_checks: Vec<[u32; 9]>,
    pub(in crate::bus) touches: Vec<crate::cache::CacheTouch>,
    compact_touches: Vec<Vec<(usize, crate::cache::CacheTouch)>>,
    pub(in crate::bus) by_pc: BTreeMap<u32, Vec<usize>>,
    pub(in crate::bus) multiplier_constant: bool,
    pub(in crate::bus) compiled: Vec<Vec<compiled::Guard>>,
    pub(in crate::bus) timeline: Option<timeline::Timeline>,
}
impl CapturedLoop {
    pub(in crate::bus) fn replay_touches(
        &self,
        first: usize,
        last: usize,
    ) -> &[(usize, crate::cache::CacheTouch)] {
        let touches = &self.compact_touches[last];
        &touches[touches.partition_point(|&(at, _)| at < self.phases[first].touch_end)..]
    }
}
pub(super) struct Capture {
    data: CapturedLoop,
    pending: Option<StoreRequest>,
    executing: u64,
}
fn work_ram(address: u32) -> bool {
    let p = Bus::physical(address);
    address < 0x40000000
        && ((crate::bus::LOW_WRAM..crate::bus::LOW_WRAM + crate::bus::WRAM_SIZE as u32)
            .contains(&p)
            || (crate::bus::HIGH_WRAM..crate::bus::HIGH_WRAM + crate::bus::WRAM_SIZE as u32)
                .contains(&p))
}
impl Bus {
    pub(in crate::bus) fn loop_trace_active(&self) -> bool {
        self.loop_probe
            .as_ref()
            .is_some_and(|probe| probe.tracing.get())
    }
    pub(in crate::bus) fn loop_trace_access(
        &self,
        processor: Processor,
        address: u32,
        width: usize,
        write: bool,
        instruction: bool,
        value: u32,
    ) {
        if processor != Processor::Master {
            return;
        }
        let Some(probe) = &self.loop_probe else {
            return;
        };
        if !probe.tracing.get() {
            return;
        }
        let mut capture = probe.capture.borrow_mut();
        let Some(capture) = capture.as_mut() else {
            return;
        };
        if write {
            if !work_ram(address) {
                capture.data.unsupported += 1;
            }
        } else {
            if !crate::bus::cached::ordinary_memory(address) {
                capture.data.unsupported += 1;
            }
            capture.data.reads.push(Read {
                owner: capture.executing,
                at: self.main_clocks,
                address,
                width,
                value,
                instruction,
            });
        }
    }
    pub(in crate::bus) fn loop_trace_store_request(
        &self,
        pc: u32,
        address: u32,
        width: usize,
        value: u32,
    ) {
        let Some(probe) = &self.loop_probe else {
            return;
        };
        if !probe.tracing.get() {
            return;
        }
        let mut capture = probe.capture.borrow_mut();
        let Some(capture) = capture.as_mut() else {
            return;
        };
        if capture.pending.is_some() {
            capture.data.unsupported += 1;
        }
        capture.pending = Some(StoreRequest {
            owner: capture.executing,
            at: self.main_clocks,
            pc,
            address,
            width,
            value,
        });
    }
    pub(in crate::bus) fn loop_trace_store_accepted(
        &self,
        pc: u32,
        address: u32,
        width: usize,
        value: u32,
    ) {
        let Some(probe) = &self.loop_probe else {
            return;
        };
        if !probe.tracing.get() {
            return;
        }
        let mut capture = probe.capture.borrow_mut();
        let Some(capture) = capture.as_mut() else {
            return;
        };
        let request = capture
            .pending
            .take()
            .filter(|r| r.pc == pc && r.address == address && r.width == width && r.value == value);
        if !work_ram(address) {
            capture.data.unsupported += 1;
        }
        capture.data.stores.push(Store {
            owner: request.map_or(capture.executing, |r| r.owner),
            requested: request.map(|r| r.at),
            accepted: self.main_clocks,
            pc,
            address,
            width,
            value,
        });
    }
    pub(in crate::bus) fn loop_trace_external_read(&self, processor: Processor, dmac: bool) {
        if processor != Processor::Master || dmac {
            return;
        }
        let Some(probe) = &self.loop_probe else {
            return;
        };
        if !probe.tracing.get() {
            return;
        }
        if let Some(capture) = probe.capture.borrow_mut().as_mut() {
            capture.data.external_reads += 1;
        }
    }
}
impl Machine {
    fn loop_trace_phase(&self) -> Option<Phase> {
        let cache = self.bus.cache[0].borrow();
        Some(Phase {
            at: self.bus.main_clocks,
            ready: self.cpu_ready[0],
            cpu: self.cpu.clone(),
            cache: cache.loop_probe_snapshot(cache.replacement_state(), |a| {
                cache.peek_cached_touch::<16>(a)
            })?,
            retained: self.bus.instruction_word[0].get(),
            epoch: self.bus.onchip[0].private_poll_epoch(),
            touch_end: cache.loop_touch_count(),
        })
    }
    fn loop_trace_turn_inner(&mut self, start: Option<u32>) -> Result<(), MachineError> {
        let before = self.cpu.state().instructions;
        if let Some(probe) = &self.bus.loop_probe
            && let Some(capture) = probe.capture.borrow_mut().as_mut()
        {
            let initial = capture.data.phases[0].cpu.state().instructions;
            if before < initial {
                capture.data.unsupported += 1;
            }
            capture.executing = before.saturating_sub(initial) + 1;
        }
        let mut committed = self.issue_cpu(Processor::Master)?;
        if self.cpu.state().instructions != before {
            let phase = self.loop_trace_phase();
            if let Some(probe) = &self.bus.loop_probe {
                let mut slot = probe.capture.borrow_mut();
                if let Some(capture) = slot.as_mut() {
                    if self.cpu.state().instructions != before + 1 || phase.is_none() {
                        capture.data.unsupported += 1;
                    }
                    if let Some(phase) = phase {
                        capture.data.phases.push(phase);
                    }
                } else if let Some(phase) = phase
                    && start == Some(phase.cpu.state().pc)
                {
                    self.bus.cache[0].borrow_mut().begin_loop_touches();
                    probe.tracing.set(true);
                    *slot = Some(Capture {
                        data: CapturedLoop {
                            phases: vec![phase],
                            reads: Vec::new(),
                            stores: Vec::new(),
                            external_reads: 0,
                            unsupported: 0,
                            complete: false,
                            header_checks: Vec::new(),
                            touches: Vec::new(),
                            compact_touches: Vec::new(),
                            by_pc: BTreeMap::new(),
                            multiplier_constant: false,
                            compiled: Vec::new(),
                            timeline: None,
                        },
                        pending: None,
                        executing: 0,
                    });
                }
            }
        }
        if self.bus.slave_enabled {
            committed |= self.issue_cpu(Processor::Slave)?;
        }
        self.finish_hardware_boundary_with_idle(committed, false)?;
        Ok(())
    }
    pub(super) fn loop_trace_turn(&mut self, start: Option<u32>) -> Result<(), MachineError> {
        let result = self.loop_trace_turn_inner(start);
        if let Err(error) = &result {
            if let Some(probe) = &self.bus.loop_probe {
                probe.capture.borrow_mut().take();
                probe.tracing.set(false);
            }
            self.bus.cache[0].borrow_mut().end_loop_touches();
            self.fault = Some(error.clone());
        }
        result
    }
    pub fn capture_loop_trace(&mut self) -> Result<Vec<u32>, MachineError> {
        self.capture_loop_trace_bounded(200_000)
    }
    pub(super) fn capture_loop_trace_bounded(
        &mut self,
        turns: u32,
    ) -> Result<Vec<u32>, MachineError> {
        self.capture_loop_trace_with_overlap(turns, false)
    }
    pub(super) fn capture_loop_trace_with_overlap(
        &mut self,
        turns: u32,
        overlap: bool,
    ) -> Result<Vec<u32>, MachineError> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        let Some((pc, expected)) = self.bus.loop_probe.as_ref().and_then(|p| p.best) else {
            return Ok(vec![0]);
        };
        for _ in 0..turns {
            self.loop_trace_turn(Some(pc))?;
            if self
                .bus
                .loop_probe
                .as_ref()
                .unwrap()
                .capture
                .borrow()
                .is_some()
            {
                break;
            }
        }
        let Some(exceptions) = self
            .bus
            .loop_probe
            .as_ref()
            .unwrap()
            .capture
            .borrow()
            .as_ref()
            .map(|c| c.data.phases[0].cpu.state().exceptions)
        else {
            return Ok(vec![0]);
        };
        let mut first_complete = None;
        for _ in 0..turns {
            self.loop_trace_turn(None)?;
            let probe = self.bus.loop_probe.as_ref().unwrap();
            let mut capture = probe.capture.borrow_mut();
            let data = &mut capture.as_mut().unwrap().data;
            let first = &data.phases[0];
            let last = data.phases.last().unwrap();
            if data.phases.len() > 1 && last.cpu.state().pc == pc {
                let period = last.at - first.at;
                if data
                    .header_checks
                    .last()
                    .is_none_or(|row| row[0] as usize != data.phases.len() - 1)
                {
                    let [registers, architecture, private] =
                        last.cpu.cached_cycle_differences(&first.cpu, period);
                    let mut cache = u32::from(last.cache.first() != first.cache.first());
                    if last.cache[1..65] != first.cache[1..65] {
                        cache |= 2;
                    }
                    if last.cache[65..] != first.cache[65..] {
                        cache |= 4;
                    }
                    let ready = (i128::from(last.ready)
                        - i128::from(first.ready)
                        - i128::from(period)) as i32 as u32;
                    data.header_checks.push([
                        (data.phases.len() - 1) as u32,
                        period as u32,
                        registers,
                        architecture,
                        private,
                        ready,
                        cache,
                        u32::from(last.retained == first.retained),
                        last.epoch.saturating_sub(first.epoch) as u32,
                    ]);
                }
                if period > 0
                    && last.cpu.same_cached_cycle_origin(&first.cpu, period)
                    && last.ready == first.ready + period
                    && last.retained == first.retained
                    && data.external_reads == 0
                    && data.unsupported == 0
                    && dependencies::repeats(data)
                {
                    let instructions = data.phases.len() - 1;
                    let first_length = first_complete.map(|(n, _, _, _, _)| n);
                    if !overlap
                        || data.phases.len() <= 32
                        || (first_length.is_none()
                            && (instructions as u64).saturating_mul(2)
                                > expected.saturating_mul(3).min(4096))
                        || first_length.is_some_and(|n| data.phases.len() > n)
                    {
                        data.complete = true;
                        break;
                    }
                    // Keep the first fully validated observation as a fallback.
                    // Continue only private training, recording real effects.
                    first_complete.get_or_insert((
                        data.phases.len(),
                        data.reads.len(),
                        data.stores.len(),
                        data.header_checks.len(),
                        self.bus.cache[0].borrow().loop_touch_count(),
                    ));
                }
            }
            if last.cpu.state().exceptions != exceptions
                || data.phases.len() as u64 > expected.saturating_mul(3).min(4096) + 1
            {
                break;
            }
        }
        let capture = self
            .bus
            .loop_probe
            .as_ref()
            .unwrap()
            .capture
            .borrow_mut()
            .take()
            .unwrap();
        let mut data = capture.data;
        let mut truncated_touches = None;
        if !data.complete
            && let Some((phases, reads, stores, headers, touches)) = first_complete
        {
            data.phases.truncate(phases);
            data.reads.truncate(reads);
            data.stores.truncate(stores);
            data.header_checks.truncate(headers);
            data.external_reads = 0;
            data.unsupported = 0;
            data.complete = true;
            truncated_touches = Some(touches);
        }
        self.bus.loop_probe.as_ref().unwrap().tracing.set(false);
        data.multiplier_constant = data
            .phases
            .iter()
            .all(|phase| phase.cpu.loop_multiplier_unchanged(&data.phases[0].cpu));
        for (index, phase) in data.phases.iter().enumerate().take(data.phases.len() - 1) {
            data.by_pc
                .entry(phase.cpu.state().pc)
                .or_default()
                .push(index);
        }
        data.touches = self.bus.cache[0].borrow_mut().end_loop_touches();
        if let Some(touches) = truncated_touches {
            data.touches.truncate(touches);
        }
        let mut lru: [u8; 64] = data.phases[0].cache[1..65].try_into().unwrap();
        let mut cursor = data.phases[0].touch_end;
        for phase in data.phases.iter().skip(1) {
            for touch in &data.touches[cursor..phase.touch_end] {
                touch.apply(&mut lru);
            }
            if lru.as_slice() != &phase.cache[1..65] {
                data.unsupported += 1;
                data.complete = false;
            }
            cursor = phase.touch_end;
        }
        data.compiled = data.compile().unwrap_or_default();
        if !data.compiled.is_empty() {
            data.compact_touches = crate::cache::compact_touch_prefixes(
                &data.touches,
                data.phases.iter().map(|phase| phase.touch_end),
            );
            data.timeline = timeline::Timeline::new(
                data.phases
                    .iter()
                    .map(|phase| (phase.at, phase.cpu.state().sr)),
            );
        }
        let first = &data.phases[0];
        let last = data.phases.last().unwrap();
        let missing = data.stores.iter().filter(|s| s.requested.is_none()).count();
        let stalls = data
            .stores
            .iter()
            .filter(|s| s.requested.is_some_and(|at| at < s.accepted))
            .count();
        // Consume complete read/store metadata in the diagnostic digest. Guest
        // addresses and values remain inside Wasm, never in its public summary.
        let mut hash = 2166136261u32;
        for read in &data.reads {
            for n in [
                read.owner as u32,
                read.at as u32,
                read.address,
                read.width as u32,
                read.value,
                u32::from(read.instruction),
            ] {
                hash = (hash ^ n).wrapping_mul(16777619);
            }
        }
        for store in &data.stores {
            for n in [
                store.owner as u32,
                store.requested.unwrap_or(0) as u32,
                store.accepted as u32,
                store.pc,
                store.address,
                store.width as u32,
                store.value,
            ] {
                hash = (hash ^ n).wrapping_mul(16777619);
            }
        }
        let mut summary = vec![
            u32::from(data.complete),
            (data.phases.len() - 1) as u32,
            (last.at - first.at) as u32,
            data.stores.len() as u32,
            data.reads.len() as u32,
            data.external_reads as u32,
            data.unsupported as u32,
            missing as u32,
            last.epoch.saturating_sub(first.epoch) as u32,
            stalls as u32,
            hash,
            data.stores.first().map_or(0, |s| s.owner as u32),
            data.stores.last().map_or(0, |s| s.owner as u32),
        ];
        summary.push(data.header_checks.len() as u32);
        summary.extend(
            data.header_checks
                .iter()
                .flat_map(|row| row.iter().copied()),
        );
        self.bus.loop_probe.as_mut().unwrap().captured = Some(data);
        Ok(summary)
    }
}
