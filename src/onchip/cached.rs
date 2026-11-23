//! Private BSC protocol planning for bounded passive reads and isolated WRAM writes.
use super::*;

#[derive(Clone)]
pub(crate) struct ReadSequence {
    bsc: bsc::Bsc,
    at: u64,
    pub beats: u64,
}
#[derive(Clone, Copy)]
pub(crate) struct CachedRefreshWindow {
    pub start: u64,
    pub complete: u64,
    pub next: u64,
}

impl ReadSequence {
    pub(crate) fn reset_cached_bus_for_refresh(&mut self) {
        self.bsc.reset_bus();
    }
    pub(crate) fn at(&self) -> u64 {
        self.at
    }
    fn advance_bus(&mut self, clocks: u32, abus: Option<&crate::scu::Abus>) {
        self.bsc.advance_bus(clocks);
        if let Some(abus) = abus {
            abus.advance(clocks);
        }
        self.at += u64::from(clocks);
    }
    fn access_clocks(
        &mut self,
        address: u32,
        width: usize,
        write: bool,
        first: bool,
        last: bool,
        abus: Option<&crate::scu::Abus>,
    ) -> u32 {
        let (base, waits) = self.bsc.access_timing(address, width, write, first, last);
        base + waits + abus.map_or(0, |abus| abus.access(address, width, write))
    }
    pub fn advance(&mut self, at: u64) {
        self.advance_with_abus(at, None);
    }
    pub fn advance_with_abus(&mut self, at: u64, abus: Option<&crate::scu::Abus>) {
        assert!(at >= self.at);
        self.advance_bus(
            (at - self.at).try_into().expect("bounded bus interval"),
            abus,
        );
    }
    pub fn read(
        &mut self,
        address: u32,
        width: usize,
        at: u64,
        abus: Option<&crate::scu::Abus>,
    ) -> u64 {
        self.transfer(address, width, width, at, false, abus)
    }
    pub fn write(&mut self, address: u32, width: usize, at: u64) -> u64 {
        self.transfer(address, width, width, at, true, None)
    }
    pub fn write_video(&mut self, address: u32, width: usize, at: u64) -> u64 {
        // The video bridge transfers at most one 16-bit word per phase even
        // when the CPU BSC selects a 32-bit external interface.
        self.transfer(address, width, 2, at, true, None)
    }
    fn transfer(
        &mut self,
        address: u32,
        width: usize,
        lanes: usize,
        at: u64,
        write: bool,
        abus: Option<&crate::scu::Abus>,
    ) -> u64 {
        self.advance_with_abus(at, abus);
        let beat = self.bsc.bus_width(address).min(width).min(lanes);
        for offset in (0..width).step_by(beat) {
            let clocks = self.access_clocks(
                address + offset as u32,
                beat,
                write,
                offset == 0,
                offset + beat == width,
                abus,
            );
            self.advance_bus(clocks, abus);
            self.beats += 1;
        }
        self.at
    }
    /// Cache IF completes four longwords, wrapping to the requested one last.
    /// The first/last signals enclose the entire refill, including lane splits.
    pub fn read_cache_line(&mut self, requested: u32, at: u64) -> u64 {
        self.advance(at);
        let first = (requested & !15) | ((requested.wrapping_add(4)) & 12);
        if let Some((clocks, beats)) = self.bsc.cached_refill(first) {
            self.at += u64::from(clocks);
            self.beats += beats;
            return self.at;
        }
        for word in 1..=4 {
            let address = (requested & !15) | (((requested & 12) + word * 4) & 15);
            let width = self.bsc.bus_width(address).min(4);
            for offset in (0..4).step_by(width) {
                let clocks = self.access_clocks(
                    address + offset as u32,
                    width,
                    false,
                    word == 1 && offset == 0,
                    word == 4 && offset + width == 4,
                    None,
                );
                self.advance_bus(clocks, None);
                self.beats += 1;
            }
        }
        self.at
    }
    pub fn write_phase(
        &mut self,
        address: u32,
        width: usize,
        at: u64,
        first: bool,
        last: bool,
    ) -> u64 {
        self.advance(at);
        let clocks = self
            .access_clocks(address, width, true, first, last, None)
            .max(1);
        self.advance_bus(clocks, None);
        self.beats += 1;
        self.at
    }
    pub fn repeats(&self, before: &Self, period: u64) -> bool {
        self.bsc.same_read_protocol(&before.bsc)
            && if self.beats == before.beats {
                self.at == before.at
            } else {
                self.at.checked_sub(before.at) == Some(period)
            }
    }
    pub fn repeat(&mut self, before: &Self, period: u64, copies: u64) {
        if self.beats != before.beats {
            self.at += period * copies;
            self.beats += (self.beats - before.beats) * copies;
        }
    }
}

impl OnChip {
    pub(crate) fn plan_hirq_refresh(&self, at: u64) -> Option<CachedRefreshWindow> {
        self.readonly_refresh_deadline()?;
        let mut probe = self.bsc.clone();
        let delay = probe.clocks_to_event();
        if delay == u32::MAX {
            return None;
        }
        probe.advance(delay);
        let length = probe.grant_refresh()?;
        let start = at + u64::from(delay);
        Some(CachedRefreshWindow {
            start,
            complete: start + u64::from(length),
            next: start + u64::from(probe.clocks_to_event()),
        })
    }
    pub(crate) fn plan_cached_reads(&self, at: u64) -> ReadSequence {
        ReadSequence {
            bsc: self.bsc.clone(),
            at,
            beats: 0,
        }
    }
    pub(crate) fn commit_cached_reads(&self, mut sequence: ReadSequence, at: u64) {
        sequence.advance(at);
        // Real board catch-up already advanced refresh counters and deadlines.
        // Only the independently planned external protocol is published here.
        self.bsc.commit_read_protocol(&sequence.bsc);
    }
    #[cfg(test)]
    pub(crate) fn assert_cached_read_bus_matches(&self, other: &Self) {
        assert_eq!(self.bsc, other.bsc);
        assert_eq!(self.wait_clocks, other.wait_clocks);
    }
}
