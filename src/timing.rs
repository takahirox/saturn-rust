//! Deterministic TV raster clock domain (VDP2 ST-058-R2 §§2.1–2.5).
//! ST-210 #30/31 defines 263 lines, 224 effective lines and timer dot counts.
//! This functional model uses 427/455 dots × 4 main clocks per line.
//! Bus-cycle contention and half-line interlace phase remain incomplete.
pub const CLOCKS_PER_LINE: u32 = 427 * 4;
pub const CLOCKS_PER_FIELD: u64 = CLOCKS_PER_LINE as u64 * 263;
const HBLANK_CLOCK: u32 = 320 * 4;

/// SMPC ST-169 §§1.1/2.3.9–10: the PLL clocks the SH-2s and video,
/// independently of the fixed sound/CD/RTC domains. Public machine time stays
/// in 26.8741 MHz reference ticks, avoiding rounding drift when switching PLLs.
#[derive(Default)]
pub(crate) struct SystemClock {
    fast: bool,
    pal: bool,
    phase: u64,
}
impl SystemClock {
    pub fn is_reference(&self) -> bool {
        self.frequency() == Self::REFERENCE
    }
    const REFERENCE: u64 = 26_874_100;
    fn frequency(&self) -> u64 {
        match (self.pal, self.fast) {
            (false, false) => Self::REFERENCE,
            (false, true) => 28_636_400,
            (true, false) => 26_687_500,
            (true, true) => 28_437_500,
        }
    }
    pub fn set_pal(&mut self, pal: bool) {
        self.pal = pal;
        self.phase = 0;
    }
    pub fn select(&mut self, fast: bool) {
        self.fast = fast;
        self.phase = 0;
    }
    pub fn deadline(&self, main_clocks: u32) -> u32 {
        if main_clocks == u32::MAX {
            return u32::MAX;
        }
        (u64::from(main_clocks) * Self::REFERENCE)
            .saturating_sub(self.phase)
            .div_ceil(self.frequency()) as u32
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
    pub(crate) pal: bool,
    mode: u16,
}
impl Raster {
    pub(crate) fn select_clock(&mut self, fast: bool) {
        self.fast = fast;
        self.phase = 0;
        self.line = 0;
    }
    pub(crate) fn configure(&mut self, mode: u16) {
        self.mode = mode;
        // TVMD can shorten the raster period. Normalize the running beam
        // before publishing another deadline, including mid-line writes.
        self.phase %= self.line_clocks();
        self.line %= self.field_lines();
    }
    pub(crate) fn odd(&self) -> bool {
        self.mode & 0xC0 == 0 || self.fields.is_multiple_of(2)
    }
    pub(crate) fn display_lines(&self) -> u16 {
        if self.mode & 4 != 0 {
            480
        } else {
            [224, 240, 256, 256][(self.mode >> 4 & 3) as usize]
        }
    }
    fn field_lines(&self) -> u16 {
        if self.mode & 4 != 0 {
            return if self.mode & 1 == 0 { 525 } else { 562 };
        }
        let base = if self.pal { 313 } else { 263 };
        base - u16::from(self.mode & 0xC0 != 0 && !self.odd())
    }
    fn hblank_clock(&self) -> u32 {
        if self.mode & 4 != 0 {
            if self.mode & 1 == 0 { 640 } else { 704 }
        } else if self.fast {
            352 * 4
        } else {
            HBLANK_CLOCK
        }
    }
    /// Completed output dots in this line. Exclusive low-resolution modes
    /// multiplex two source samples into one output dot (ST-058 §2.1).
    pub(crate) fn visible_pixels(&self, width: usize) -> usize {
        (u64::from(self.phase.min(self.hblank_clock())) * width as u64
            / u64::from(self.hblank_clock())) as usize
    }
    pub(crate) fn active_clocks(&self) -> u32 {
        u32::from(self.display_lines()) * self.line_clocks()
    }
    pub(crate) fn line_clocks(&self) -> u32 {
        if self.mode & 4 != 0 {
            if self.mode & 1 == 0 { 852 } else { 848 }
        } else if self.fast {
            455 * 4
        } else {
            CLOCKS_PER_LINE
        }
    }
    // ST-058 §2.5 tables2.3/2.4: only normal-mode HCNT leaves bit0
    // invalid. VCNT inserts field parity only in double-density interlace.
    pub(crate) fn counters(&self) -> (u16, u16) {
        let horizontal = if self.mode & 4 != 0 || self.mode & 2 != 0 {
            self.phase / 2
        } else {
            self.phase / 4 * 2
        };
        let vertical = if self.mode & 4 == 0 && self.mode & 0xC0 == 0xC0 {
            self.line * 2 + u16::from(!self.odd())
        } else {
            self.line
        };
        (horizontal as u16, vertical)
    }
    pub fn clocks_to_event(&self) -> u32 {
        if self.phase < self.hblank_clock() {
            self.hblank_clock() - self.phase
        } else {
            self.line_clocks() - self.phase
        }
    }
    // A two-cycle horizontal lead is an emulator phase convention. §3.3
    // establishes slot order/bandwidth, not a measured display-pipeline delay.
    fn display_lead(&self) -> u32 {
        2 * if self.mode & 6 == 0 { 8 } else { 4 } * if self.mode & 4 == 0 { 4 } else { 2 }
    }
    fn next_display_line(&self) -> Option<(u64, u16)> {
        let next = self.line + 1;
        if next >= self.field_lines() {
            Some((self.fields + 1, 0))
        } else if next < self.display_lines() {
            Some((self.fields, next))
        } else {
            None
        }
    }
    /// The last two cycles of horizontal blank prime the following line.
    /// Subsequent accesses share the line-relative CPU VRAM slot phase.
    pub(crate) fn display_slot(&self) -> Option<(u64, u16, u32, usize)> {
        let clocks = if self.mode & 4 == 0 { 4 } else { 2 };
        let lead = self.display_lead();
        let start = self.line_clocks() - lead;
        let (field, line, phase) = if self.phase >= start {
            let (field, line) = self.next_display_line()?;
            (field, line, self.phase - start)
        } else if self.line < self.display_lines() && self.phase < self.hblank_clock() {
            (self.fields, self.line, self.phase + lead)
        } else {
            return None;
        };
        if !phase.is_multiple_of(clocks) {
            return None;
        }
        let count = if self.mode & 6 == 0 { 8 } else { 4 };
        Some((field, line, phase, ((phase / clocks) % count) as usize))
    }
    pub(crate) fn clocks_to_display_slot(&self) -> u32 {
        let clocks = if self.mode & 4 == 0 { 4 } else { 2 };
        let start = self.line_clocks() - self.display_lead();
        if self.line < self.display_lines() && self.phase < self.hblank_clock()
            || self.phase >= start && self.next_display_line().is_some()
        {
            clocks - self.phase % clocks
        } else if self.next_display_line().is_some() {
            start - self.phase
        } else {
            u32::MAX
        }
    }
    /// Rotation phase convention: acquire at the start of each output dot,
    /// after that line's parameter latch, without normal-screen prefetch.
    pub(crate) fn rotation_display_slot(&self) -> Option<(u64, u16, u32, usize)> {
        let clocks = if self.mode & 2 == 0 { 4 } else { 2 };
        (self.mode & 4 == 0
            && self.line < self.display_lines()
            && self.phase < self.hblank_clock()
            && self.phase.is_multiple_of(clocks))
        .then_some((
            self.fields,
            self.line,
            self.phase,
            (self.phase / clocks) as usize,
        ))
    }
    pub(crate) fn clocks_to_rotation_slot(&self) -> u32 {
        if self.mode & 4 != 0
            || self.line >= self.display_lines()
            || self.phase >= self.hblank_clock()
        {
            return u32::MAX;
        }
        let clocks = if self.mode & 2 == 0 { 4 } else { 2 };
        clocks - self.phase % clocks
    }
    /// Wait until a CPU VRAM grant, or horizontal blank if the display owns
    /// every slot. Normal/hi-res cycles use main-clock groups of four;
    /// exclusive monitor cycles run at twice that rate (ST-058 §3.3).
    pub(crate) fn vram_wait_clocks(&self, slots: u8, queued: u32) -> u32 {
        let position = u64::from(self.phase) + u64::from(queued);
        let phase = (position % u64::from(self.line_clocks())) as u32;
        let line = (u64::from(self.line) + position / u64::from(self.line_clocks()))
            % u64::from(self.field_lines());
        if line >= u64::from(self.display_lines()) || phase >= self.hblank_clock() {
            return 0;
        }
        let count = if self.mode & 6 == 0 { 8 } else { 4 };
        let clocks = if self.mode & 4 == 0 { 4 } else { 2 };
        let slot = (phase / clocks) % count;
        if slots & (1 << slot) != 0 {
            return 0;
        }
        let blank = self.hblank_clock() - phase;
        for distance in 1..=count {
            if slots & (1 << ((slot + distance) % count)) != 0 {
                return blank.min(distance * clocks - phase % clocks);
            }
        }
        blank
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
        if self.line == self.display_lines() {
            Some(RasterEvent::VBlankIn)
        } else if self.line >= self.field_lines() {
            self.line = 0;
            self.fields += 1;
            Some(RasterEvent::VBlankOut)
        } else {
            Some(RasterEvent::LineStart)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_dot_deadlines_have_no_normal_prefetch_lead_and_stop_at_blank() {
        for (mode, fast, width, clocks, line_clocks) in [
            (0, false, 320, 4, 1708),
            (1, true, 352, 4, 1820),
            (2, false, 640, 2, 1708),
            (3, true, 704, 2, 1820),
        ] {
            let mut r = Raster {
                mode,
                fast,
                fields: 2,
                line: 1,
                ..Raster::default()
            };
            assert_eq!(r.rotation_display_slot(), Some((2, 1, 0, 0)));
            assert_eq!(r.clocks_to_rotation_slot(), clocks);
            r.phase = clocks - 1;
            assert_eq!(r.rotation_display_slot(), None);
            assert_eq!(r.clocks_to_rotation_slot(), 1);
            r.phase = clocks;
            assert_eq!(r.rotation_display_slot(), Some((2, 1, clocks, 1)));
            r.phase = width * clocks - clocks;
            assert_eq!(
                r.rotation_display_slot(),
                Some((2, 1, width * clocks - clocks, (width - 1) as usize))
            );
            for phase in [width * clocks, line_clocks - 64, line_clocks - 1] {
                r.phase = phase;
                assert_eq!(r.rotation_display_slot(), None);
                assert_eq!(r.clocks_to_rotation_slot(), u32::MAX);
            }
            r.line = 224;
            r.phase = 0;
            assert_eq!(r.rotation_display_slot(), None);
        }
        for mode in 4..8 {
            let r = Raster {
                mode,
                ..Raster::default()
            };
            assert_eq!(r.rotation_display_slot(), None);
            assert_eq!(r.clocks_to_rotation_slot(), u32::MAX);
        }
    }
    #[test]
    fn vertical_counter_uses_v0_except_for_double_density_field_parity() {
        // ST-058 §2.5 table2.4: all ten V bits in non/single interlace
        // and exclusive modes; V8..V0 plus parity in double interlace.
        for horizontal in 0..8 {
            for interlace in [0, 0x80, 0xC0] {
                let mut r = Raster::default();
                r.configure(0x8000 | horizontal | interlace);
                for field in 0..4 {
                    r.fields = field;
                    for (line, plain, double_odd, double_even) in [
                        (0, 0, 0, 1),
                        (1, 1, 2, 3),
                        (2, 2, 4, 5),
                        (223, 223, 446, 447),
                        (255, 255, 510, 511),
                        (256, 256, 512, 513),
                        (511, 511, 1022, 1023),
                    ] {
                        r.line = line;
                        let expected = if horizontal < 4 && interlace == 0xC0 {
                            if field % 2 == 0 {
                                double_odd
                            } else {
                                double_even
                            }
                        } else {
                            plain
                        };
                        assert_eq!(
                            r.counters().1,
                            expected,
                            "mode={:X}, field={field}, line={line}",
                            r.mode
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn exclusive_monitors_have_480_active_lines_and_distinct_counter_bit_layouts() {
        // ST-058 tables2.1/2.3/2.4; ST-013 tables4.4/4.5.
        for (mode, line_clocks, total, hblank) in [
            (0x8004, 852, 525, 640),
            (0x8005, 848, 562, 704),
            (0x8006, 852, 525, 640),
            (0x8007, 848, 562, 704),
        ] {
            for chunk in [1, 3, 17, 10000] {
                let mut r = Raster::default();
                r.configure(mode);
                let mut time = 0;
                let mut events = Vec::new();
                while r.fields == 0 {
                    let n = chunk.min(r.clocks_to_event());
                    time += n;
                    if let Some(event) = r.advance(n) {
                        if time == hblank {
                            assert_eq!(r.counters(), (hblank as u16 / 2, 0));
                        }
                        if matches!(event, RasterEvent::VBlankIn | RasterEvent::VBlankOut) {
                            events.push((time, event, r.counters().1));
                        }
                    }
                }
                assert_eq!(
                    events,
                    [
                        (480 * line_clocks, RasterEvent::VBlankIn, 480),
                        (total * line_clocks, RasterEvent::VBlankOut, 0)
                    ]
                );
            }
        }
    }
    #[test]
    fn vram_grant_waits_use_live_slot_phase_and_release_at_horizontal_blank() {
        let mut r = Raster::default();
        assert_eq!(r.vram_wait_clocks(1, 0), 0);
        assert_eq!(r.vram_wait_clocks(1 << 3, 0), 12);
        assert_eq!(r.vram_wait_clocks(0, 0), 1280);
        r.advance(7);
        assert_eq!(r.vram_wait_clocks(1 << 3, 0), 5);
        assert_eq!(r.vram_wait_clocks(1, 0), 25);
        r.advance(1270);
        assert_eq!(r.vram_wait_clocks(1, 0), 3); // blank beats the next cycle
        r.advance(3);
        assert_eq!(r.vram_wait_clocks(0, 0), 0);
        r = Raster::default();
        r.configure(0x8006);
        r.advance(3);
        assert_eq!(r.vram_wait_clocks(1 << 3, 0), 3);
        assert_eq!(r.vram_wait_clocks(0, 0), 637);
    }
    #[test]
    fn tv_modes_have_independent_blank_field_and_counter_vectors() {
        // ST-058 tables2.1/2.3/2.4; compare event times in main clocks.
        for (mode, pal, active, total, width_clock) in [
            (0x8010, false, 240, 263, 1708),
            (0x8021, true, 256, 313, 1820),
            (0x80D2, false, 240, 263, 1708),
            (0x80E3, true, 256, 313, 1820),
        ] {
            for chunk in [1, 7, 127, 10000] {
                let mut r = Raster::default();
                r.select_clock(mode & 1 != 0);
                r.pal = pal;
                r.configure(mode);
                let mut clock = 0;
                let mut blanks = Vec::new();
                while r.fields < 2 {
                    let n = chunk.min(r.clocks_to_event());
                    clock += n;
                    if let Some(event) = r.advance(n)
                        && matches!(event, RasterEvent::VBlankIn | RasterEvent::VBlankOut)
                    {
                        blanks.push((clock, event, r.counters().1, r.odd()));
                    }
                }
                let even_length = total - u32::from(mode & 0xC0 != 0);
                assert_eq!(
                    blanks[0],
                    (
                        active * width_clock,
                        RasterEvent::VBlankIn,
                        active as u16 * if mode & 0xC0 == 0xC0 { 2 } else { 1 },
                        true
                    )
                );
                assert_eq!(blanks[1].0, total * width_clock);
                assert_eq!(blanks[2].0, (total + active) * width_clock);
                assert_eq!(blanks[3].0, (total + even_length) * width_clock);
                if mode & 0xC0 == 0xC0 {
                    assert_eq!(blanks[2].2, active as u16 * 2 + 1);
                }
                assert_eq!(blanks[1].3, mode & 0xC0 == 0);
            }
        }
    }
    #[test]
    fn pal_pll_keeps_fixed_domains_and_carries_fractional_main_clocks() {
        for fast in [false, true] {
            for chunk in [1, 3, 73, 1708, 10000] {
                let mut c = SystemClock::default();
                c.set_pal(true);
                c.select(fast);
                let frequency = if fast { 28_437_500 } else { 26_687_500 };
                let mut reference = 0;
                let mut main = 0u64;
                while reference < 100_000 {
                    let n = chunk.min(100_000 - reference);
                    reference += n;
                    main += u64::from(c.advance(n));
                    assert_eq!(main, u64::from(reference) * frequency / 26_874_100);
                }
            }
        }
    }
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
