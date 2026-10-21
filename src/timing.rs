//! Deterministic NTSC 320/352-dot, noninterlaced raster clock domain.
//! ST-210 #30/31 defines 263 lines, 224 effective lines and timer dot counts.
//! This functional model uses 427/455 dots × 4 main clocks per line. Other
//! television/interlace modes and bus-cycle contention remain unsupported.
pub const CLOCKS_PER_LINE: u32 = 427 * 4;
pub const CLOCKS_PER_FIELD: u64 = CLOCKS_PER_LINE as u64 * 263;
const HBLANK_CLOCK: u32 = 320 * 4;

/// SMPC ST-169 §§1.1/2.3.9–10: the PLL clocks the SH-2s and video,
/// independently of the fixed sound/CD/RTC domains. Public machine time stays
/// in 26.8741 MHz reference ticks, avoiding rounding drift when switching PLLs.
#[derive(Default)]
pub(crate) struct SystemClock {
    fast: bool,
    phase: u64,
}
impl SystemClock {
    const REFERENCE: u64 = 26_874_100;
    fn frequency(&self) -> u64 {
        if self.fast {
            28_636_400
        } else {
            Self::REFERENCE
        }
    }
    pub fn select(&mut self, fast: bool) {
        self.fast = fast;
        self.phase = 0;
    }
    pub fn deadline(&self, main_clocks: u32) -> u32 {
        if main_clocks == u32::MAX {
            return u32::MAX;
        }
        (u64::from(main_clocks) * Self::REFERENCE - self.phase).div_ceil(self.frequency()) as u32
    }
    pub fn advance(&mut self, reference_clocks: u32) -> u32 {
        let elapsed = self.phase + u64::from(reference_clocks) * self.frequency();
        self.phase = elapsed % Self::REFERENCE;
        (elapsed / Self::REFERENCE) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterEvent {
    HBlankIn,
    LineStart,
    VBlankIn,
    VBlankOut,
}
#[derive(Default)]
pub struct Raster {
    pub line: u16,
    pub fields: u64,
    phase: u32,
    fast: bool,
}
impl Raster {
    pub(crate) fn select_clock(&mut self, fast: bool) {
        self.fast = fast;
        self.phase = 0;
        self.line = 0;
    }
    fn hblank_clock(&self) -> u32 {
        if self.fast { 352 * 4 } else { HBLANK_CLOCK }
    }
    fn line_clocks(&self) -> u32 {
        if self.fast { 455 * 4 } else { CLOCKS_PER_LINE }
    }
    // ST-058 §2.5 tables2.3/2.4: normal-mode counters leave bit0 invalid.
    pub(crate) fn counters(&self) -> (u16, u16) {
        ((self.phase / 4) as u16 * 2, self.line * 2)
    }
    pub fn clocks_to_event(&self) -> u32 {
        if self.phase < self.hblank_clock() {
            self.hblank_clock() - self.phase
        } else {
            self.line_clocks() - self.phase
        }
    }
    /// The scheduler splits device advancement at each event boundary.
    pub fn advance(&mut self, clocks: u32) -> Option<RasterEvent> {
        assert!(clocks <= self.clocks_to_event());
        self.phase += clocks;
        if self.phase == self.hblank_clock() {
            return Some(RasterEvent::HBlankIn);
        }
        if self.phase != self.line_clocks() {
            return None;
        }
        self.phase = 0;
        self.line += 1;
        match self.line {
            224 => Some(RasterEvent::VBlankIn),
            263 => {
                self.line = 0;
                self.fields += 1;
                Some(RasterEvent::VBlankOut)
            }
            _ => Some(RasterEvent::LineStart),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pll_rational_phase_and_deadlines_do_not_drift_or_depend_on_chunks() {
        // SMPC NTSC clock frequencies, not output sampled from the model.
        for chunk in [1, 3, 8, 607, 1708, 2_687_410] {
            let mut clock = SystemClock::default();
            clock.select(true);
            let mut reference = 0;
            let mut main = 0u64;
            while reference < 2_687_410 {
                let n = chunk.min(2_687_410 - reference);
                reference += n;
                main += u64::from(clock.advance(n));
                assert_eq!(main, u64::from(reference) * 28_636_400 / 26_874_100);
                for deadline in [1, 2, 3, 455 * 4] {
                    let ticks = clock.deadline(deadline);
                    let frequency = 28_636_400;
                    assert!(
                        (clock.phase + u64::from(ticks) * frequency) / 26_874_100
                            >= u64::from(deadline)
                    );
                    assert!(
                        (clock.phase + u64::from(ticks - 1) * frequency) / 26_874_100
                            < u64::from(deadline)
                    );
                }
            }
            assert_eq!(main, 2_863_640);
            clock.select(false);
            assert_eq!(clock.advance(1708), 1708);
        }
    }
    #[test]
    fn raster_deadlines_are_independent_of_advance_chunk_size() {
        fn trace(chunk: u32) -> Vec<(u64, RasterEvent, u16)> {
            let mut raster = Raster::default();
            let mut time = 0;
            let mut trace = Vec::new();
            while time < CLOCKS_PER_FIELD * 2 {
                let n = chunk
                    .min(raster.clocks_to_event())
                    .min((CLOCKS_PER_FIELD * 2 - time) as u32);
                time += u64::from(n);
                if let Some(event) = raster.advance(n) {
                    trace.push((time, event, raster.line));
                }
            }
            trace
        }
        let expected = trace(1);
        for chunk in [3, 8, 127, 1708, 10000] {
            assert_eq!(trace(chunk), expected);
        }
        assert_eq!(expected[0], (1280, RasterEvent::HBlankIn, 0));
        assert!(expected.contains(&(224 * 1708, RasterEvent::VBlankIn, 224)));
        assert!(expected.contains(&(CLOCKS_PER_FIELD, RasterEvent::VBlankOut, 0)));
        assert_eq!(
            expected
                .iter()
                .filter(|(_, e, _)| *e == RasterEvent::HBlankIn)
                .count(),
            526
        );
    }
}
