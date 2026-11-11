//! Private BSC protocol planning for bounded passive reads and isolated WRAM writes.
use super::*;

#[derive(Clone)]
pub(crate) struct ReadSequence {
    bsc: bsc::Bsc,
    abus: Option<crate::scu::Abus>,
    at: u64,
    pub beats: u64,
}
impl ReadSequence {
    pub fn with_abus(mut self, abus: crate::scu::Abus) -> Self {
        self.abus = Some(abus);
        self
    }
    pub fn abus(&self) -> Option<&crate::scu::Abus> {
        self.abus.as_ref()
    }
    fn advance_bus(&mut self, clocks: u32) {
        self.bsc.advance_bus(clocks);
        if let Some(abus) = &self.abus {
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
    ) -> u32 {
        let (base, waits) = self.bsc.access_timing(address, width, write, first, last);
        base + waits
            + self
                .abus
                .as_ref()
                .map_or(0, |abus| abus.access(address, width, write))
    }
    pub fn advance(&mut self, at: u64) {
        assert!(at >= self.at);
        self.advance_bus((at - self.at).try_into().expect("bounded bus interval"));
    }
    pub fn read(&mut self, address: u32, width: usize, at: u64) -> u64 {
        self.transfer(address, width, width, at, false)
    }
    pub fn write(&mut self, address: u32, width: usize, at: u64) -> u64 {
        self.transfer(address, width, width, at, true)
    }
    pub fn write_video(&mut self, address: u32, width: usize, at: u64) -> u64 {
        // The video bridge transfers at most one 16-bit word per phase even
        // when the CPU BSC selects a 32-bit external interface.
        self.transfer(address, width, 2, at, true)
    }
    fn transfer(&mut self, address: u32, width: usize, lanes: usize, at: u64, write: bool) -> u64 {
        self.advance(at);
        let beat = self.bsc.bus_width(address).min(width).min(lanes);
        for offset in (0..width).step_by(beat) {
            let clocks = self.access_clocks(
                address + offset as u32,
                beat,
                write,
                offset == 0,
                offset + beat == width,
            );
            self.advance_bus(clocks);
            self.beats += 1;
        }
        self.at
    }
    /// Cache IF completes four longwords, wrapping to the requested one last.
    /// The first/last signals enclose the entire refill, including lane splits.
    pub fn read_cache_line(&mut self, requested: u32, at: u64) -> u64 {
        self.advance(at);
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
                );
                self.advance_bus(clocks);
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
        let clocks = self.access_clocks(address, width, true, first, last).max(1);
        self.advance_bus(clocks);
        self.beats += 1;
        self.at
    }
    pub fn repeats(&self, before: &Self, period: u64) -> bool {
        self.bsc.same_read_protocol(&before.bsc)
            && self.abus == before.abus
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
    pub(crate) fn plan_cached_reads(&self, at: u64) -> ReadSequence {
        ReadSequence {
            bsc: self.bsc.clone(),
            abus: None,
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
