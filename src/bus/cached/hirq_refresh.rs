//! Plan read-only HIRQ across up to two real, IRQ-disabled Master refreshes.
use super::{Bus, PassiveSequence, Processor, ScuError};
use crate::onchip::{CachedRefreshWindow, ReadSequence};
use crate::scu::Abus;

#[derive(Clone)]
struct RefreshEdge {
    compare: u64,
    delay: u64,
    before_abus: Option<Abus>,
    first_read: Option<u64>,
    waited: bool,
}

impl RefreshEdge {
    fn new(compare: u64) -> Self {
        Self {
            compare,
            delay: 0,
            before_abus: None,
            first_read: None,
            waited: false,
        }
    }

    fn start(&self) -> u64 {
        self.compare + self.delay
    }
}

#[derive(Clone)]
pub(super) struct PassiveRefresh {
    edges: [RefreshEdge; 2],
    length: u64,
    edge_count: usize,
    visited: usize,
    previous_beats: u64,
    allow_overlap: bool,
    allow_wait: bool,
}

impl PassiveRefresh {
    pub(super) fn new(
        window: CachedRefreshWindow,
        overlap: bool,
        wait: bool,
        second: bool,
    ) -> Self {
        Self {
            edges: [
                RefreshEdge::new(window.start),
                RefreshEdge::new(window.next),
            ],
            length: window.complete - window.start,
            edge_count: 1 + usize::from(second),
            visited: 0,
            previous_beats: 0,
            allow_overlap: overlap,
            allow_wait: wait,
        }
    }

    pub(super) fn start(&self) -> u64 {
        self.edges[0].start()
    }

    fn record_read(&mut self, granted: u64) {
        if let Some(edge) = self.edges[..self.visited].last_mut() {
            edge.first_read.get_or_insert(granted);
        }
    }

    pub(super) fn read(
        &mut self,
        sequence: &mut ReadSequence,
        abus: &mut Abus,
        address: u32,
        width: usize,
        requested: u64,
    ) -> Option<u64> {
        let mut granted = requested;
        while self.visited < self.edge_count {
            let next_compare = if self.visited + 1 < self.edge_count {
                self.edges[self.visited + 1].compare
            } else {
                u64::MAX
            };
            let edge = &mut self.edges[self.visited];
            if granted < edge.compare {
                let done = sequence.read(address, width, granted, Some(abus));
                if !self.allow_overlap && done >= edge.compare {
                    return None;
                }
                edge.delay = done.saturating_sub(edge.compare);
                if edge.start() + self.length >= next_compare {
                    // A later compare could occur within this delayed grant.
                    return None;
                }
                self.record_read(granted);
                return Some(done);
            }

            let complete = edge.start() + self.length;
            if granted < complete {
                if !self.allow_wait {
                    return None;
                }
                edge.waited = true;
                granted = complete;
            }
            debug_assert!(sequence.at() <= edge.start());
            sequence.advance_with_abus(edge.start(), Some(abus));
            if sequence.beats != self.previous_beats {
                edge.before_abus = Some(abus.clone());
            }
            self.previous_beats = sequence.beats;
            sequence.reset_cached_bus_for_refresh();
            abus.request_refresh();
            self.visited += 1;
        }

        self.record_read(granted);
        Some(sequence.read(address, width, granted, Some(abus)))
    }
}

impl Bus {
    pub(super) fn commit_hirq_refresh(
        &mut self,
        read: PassiveSequence,
        retired_at: u64,
        sound_advanced: bool,
    ) -> Result<u64, ScuError> {
        let PassiveSequence { bsc, abus, refresh } = read;
        let refresh = refresh.expect("accepted HIRQ refresh");
        let mut trailing = Some((bsc, abus));
        let mut reference = 0;

        for (index, edge) in refresh
            .edges
            .into_iter()
            .take(refresh.edge_count)
            .enumerate()
        {
            let start = edge.start();
            if retired_at < start {
                break;
            }
            let before = (start - self.main_clocks) as u32;
            reference += if edge.delay != 0 {
                // The raw compare occurred during an already granted HIRQ.
                self.advance_isolated_segment_mode::<false, true>(before, sound_advanced, false)?
            } else {
                self.advance_isolated_segment::<false>(before, sound_advanced, false)?
            };

            if index < refresh.visited {
                // A real grant resets BSC history. Only the pre-grant SCU
                // A-bus phase and Master arbitration predecessor survive.
                if let Some(before) = edge.before_abus {
                    debug_assert!(retired_at > start);
                    self.scu.commit_cached_abus(&before);
                    self.commit_cached_read_arbitration();
                }
            } else if let Some((sequence, before)) = trailing.take()
                && sequence.beats != refresh.previous_beats
            {
                // There is no planned read after this edge. Publish the full
                // protocol before returning at a pending but ungranted REF.
                self.commit_cached_read_sequence(sequence, before.as_ref());
            }

            if retired_at == start {
                return Ok(reference);
            }
            self.grant_bus_transaction();
            assert_eq!(
                self.bus_main_deadline(),
                refresh.length as u32,
                "publish each actual refresh grant at its planned boundary",
            );
            let complete = start + refresh.length;
            let during = (retired_at.min(complete) - start) as u32;
            reference += self.advance_isolated_segment::<false>(during, sound_advanced, false)?;
            self.advance_bus_transaction(during)?;
            if edge.waited {
                debug_assert_eq!(
                    edge.first_read,
                    Some(self.main_clocks),
                    "the waiting HIRQ is granted at the real refresh completion",
                );
            }
            if let Some(at) = edge.first_read {
                reference += self.advance_isolated_segment::<false>(
                    (at - self.main_clocks) as u32,
                    sound_advanced,
                    false,
                )?;
                self.claim_external_bus(Processor::Master);
            }

            #[cfg(test)]
            {
                if index == 0 {
                    self.hirq_refresh_plan_counts[0] += 1;
                    self.hirq_refresh_plan_counts[1] += u64::from(edge.first_read.is_some());
                    self.hirq_refresh_plan_counts[2] += retired_at - start;
                } else {
                    self.hirq_second_grants += 1;
                }
                self.hirq_sound_refresh_grants += u64::from(sound_advanced);
                self.hirq_second_grants_with_sound += u64::from(sound_advanced && index == 1);
                self.hirq_refresh_delayed_grants += u64::from(edge.delay != 0);
                self.hirq_refresh_waits += u64::from(edge.waited);
            }
        }

        reference += self.advance_isolated_segment::<false>(
            (retired_at - self.main_clocks) as u32,
            sound_advanced,
            false,
        )?;
        if let Some((sequence, after)) = trailing
            && sequence.beats != refresh.previous_beats
        {
            self.commit_cached_read_sequence(sequence, after.as_ref());
        }
        Ok(reference)
    }
}
