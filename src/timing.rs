//! Deterministic NTSC 320-dot, noninterlaced raster clock domain.
//! ST-210 #30/31 defines 263 lines, 224 effective lines and timer dot counts.
//! This functional model uses 427 dots × 4 reference clocks per line. Other
//! television/clock/interlace modes and bus-cycle contention remain unsupported.
pub const CLOCKS_PER_LINE: u32 = 427 * 4;
pub const CLOCKS_PER_FIELD: u64 = CLOCKS_PER_LINE as u64 * 263;
const HBLANK_CLOCK: u32 = 320 * 4;

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
}
impl Raster {
    // ST-058 §2.5 tables2.3/2.4: normal-mode counters leave bit0 invalid.
    pub(crate) fn counters(&self) -> (u16, u16) {
        ((self.phase / 4) as u16 * 2, self.line * 2)
    }
    pub fn clocks_to_event(&self) -> u32 {
        if self.phase < HBLANK_CLOCK {
            HBLANK_CLOCK - self.phase
        } else {
            CLOCKS_PER_LINE - self.phase
        }
    }
    /// The scheduler splits device advancement at each event boundary.
    pub fn advance(&mut self, clocks: u32) -> Option<RasterEvent> {
        assert!(clocks <= self.clocks_to_event());
        self.phase += clocks;
        if self.phase == HBLANK_CLOCK {
            return Some(RasterEvent::HBlankIn);
        }
        if self.phase != CLOCKS_PER_LINE {
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
