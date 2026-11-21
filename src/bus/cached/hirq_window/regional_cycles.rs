//! Recognize exact periodic SH-2 state inside each physical HIRQ bus region.

use super::super::*;
use super::{Window, affine::Trace};

#[derive(Default)]
pub(super) struct RegionCycles {
    trace: Trace,
    anchor: Option<Anchor>,
    region: usize,
    used: bool,
    failed: u8,
}

struct Anchor {
    cpu: Cpu,
    sequence: PassiveSequence,
    replacement: [u8; 64],
    retained: Option<(u32, u16)>,
    epoch: u64,
    clock: u64,
    at: u64,
    ready: u64,
    retired: u32,
}

impl RegionCycles {
    pub(super) fn record(&mut self, retired: u32, pc: u32, op: u16, hirq: bool) {
        if self.anchor.is_some() {
            self.trace.record(retired, pc, op, hirq);
        }
    }

    pub(super) fn after_branch(
        &mut self,
        memory: &mut Cached<'_>,
        cpu: &mut Cpu,
        window: &mut Window,
        limit: u32,
        points: Option<&mut Vec<Retirement>>,
        learning: Option<&mut super::super::super::loop_probe::LoopProbe>,
    ) {
        let Some(sequence) = memory.passive.as_mut().map(|read| read.sequence.get_mut()) else {
            return;
        };
        let Some(refresh) = &sequence.refresh else {
            return;
        };
        let region = refresh.region();
        if region != self.region {
            self.region = region;
            self.anchor = None;
            self.used = false;
            self.failed = 0;
        }
        let clock = memory.clock.get();
        let end = memory.end.get().min(refresh.repeat_before());
        if self.used || self.failed >= 2 || end.saturating_sub(clock) < 4 {
            return;
        }

        if let Some(prior) = &self.anchor
            && prior.cpu.state().pc == cpu.state().pc
        {
            let instructions = window.retired - prior.retired;
            let period = clock - prior.clock;
            let stable = period != 0
                && instructions > 0
                && instructions <= 16
                && window.ready.checked_sub(prior.ready) == Some(period)
                && window.at.checked_sub(prior.at) == Some(period)
                && prior.replacement == *memory.lru.get_mut()
                && prior.retained == memory.retained.get()
                && prior.sequence.refresh.as_ref() == Some(refresh)
                && prior.sequence.abus == sequence.abus
                && sequence.bsc.repeats(&prior.sequence.bsc, period)
                && sequence.bsc.beats != prior.sequence.bsc.beats;
            if stable {
                let count = u64::from((limit - window.retired) / instructions);
                let copies = (end.saturating_sub(clock) / period).min(count);
                let copies = self.trace.repeat(
                    &prior.cpu,
                    cpu,
                    prior.retired,
                    window.retired,
                    period,
                    copies,
                );
                if copies != 0 {
                    if let Some(probe) = learning {
                        self.trace
                            .discover_repeated_branches(probe, prior.retired, window.retired);
                    }
                    let skipped = instructions * copies as u32;
                    if let Some(points) = points {
                        let last = points.len();
                        debug_assert_eq!(last, window.retired as usize);
                        let first = last - instructions as usize;
                        for copy in 1..=copies {
                            let appended = points.len();
                            points.extend_from_within(first..last);
                            for retirement in &mut points[appended..] {
                                retirement.at += period * copy;
                            }
                        }
                    }
                    sequence.bsc.repeat(&prior.sequence.bsc, period, copies);
                    let epochs = memory.epochs.get();
                    memory.epochs.set(epochs + (epochs - prior.epoch) * copies);
                    let clocks = period * copies;
                    memory.clock.set(clock + clocks);
                    #[cfg(test)]
                    {
                        window.folded_regions[region] =
                            Some((window.retired, window.retired + skipped));
                    }
                    window.retired += skipped;
                    window.ready += clocks;
                    window.at += clocks;
                    self.used = true;
                    self.anchor = None;
                    return;
                }
            }
            self.failed += 1;
            if self.failed >= 2 {
                self.anchor = None;
                return;
            }
        }
        self.anchor = Some(Anchor {
            cpu: cpu.clone(),
            sequence: sequence.clone(),
            replacement: *memory.lru.get_mut(),
            retained: memory.retained.get(),
            epoch: memory.epochs.get(),
            clock,
            at: window.at,
            ready: window.ready,
            retired: window.retired,
        });
    }
}
