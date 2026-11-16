//! Owned Sound forecasting with bounded local state/RAM history.
//! Main consumes transition-free prefixes and returns ownership at inputs.
use super::*;
use checkpoint::SoundCheckpoint;
const CHUNK: u64 = 8192;
const POINTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Window,
    Sound(BusError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Window => f.write_str("sound forecast window"),
            Self::Sound(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for Error {}

struct Timeline {
    origin: u64,
    initial: bool,
    changes: Vec<(u64, bool)>,
}
impl Timeline {
    fn level(&self, at: u64) -> bool {
        let n = self.changes.partition_point(|&(time, _)| time <= at);
        if n == 0 {
            self.initial
        } else {
            self.changes[n - 1].1
        }
    }
    fn record(&mut self, at: u64, level: bool) {
        assert!(at >= self.origin);
        assert!(
            self.changes.last().is_none_or(|&(time, _)| time <= at),
            "IRQ observations must remain chronological"
        );
        if self.changes.last().is_some_and(|&(time, _)| time == at) {
            self.changes.pop();
        }
        if level
            != self
                .changes
                .last()
                .map_or(self.initial, |&(_, level)| level)
        {
            self.changes.push((at, level));
        }
    }
    fn truncate(&mut self, at: u64, level: bool) {
        self.changes
            .truncate(self.changes.partition_point(|&(time, _)| time < at));
        self.record(at, level);
    }
    fn discard_before(&mut self, at: u64) {
        let level = self.level(at);
        self.changes
            .drain(..self.changes.partition_point(|&(time, _)| time <= at));
        self.origin = at;
        self.initial = level;
    }
}

pub struct Forecast {
    sound: Sound,
    origin: u64,
    points: VecDeque<SoundCheckpoint>,
    irqs: Timeline,
}
impl Forecast {
    pub fn new(sound: Sound) -> Self {
        let origin = sound.time;
        let initial = sound.scsp.main_interrupt();
        Self {
            sound,
            origin,
            points: VecDeque::new(),
            irqs: Timeline {
                origin,
                initial,
                changes: Vec::new(),
            },
        }
    }
    pub fn origin(&self) -> u64 {
        self.origin
    }
    pub fn through(&self) -> u64 {
        self.sound.time
    }
    pub fn stable_frontier(&self) -> u64 {
        self.irqs
            .changes
            .first()
            .map_or(self.sound.time, |&(at, _)| at.saturating_sub(1))
    }
    pub fn initial_irq(&self) -> bool {
        self.irqs.initial
    }
    pub fn snapshot_at(&mut self, at: u64) -> Result<Vec<u8>, String> {
        self.rewind(at).map_err(|error| error.to_string())?;
        self.sound.domain_snapshot().map_err(str::to_owned)
    }
    pub fn irq_timeline(&self) -> Vec<(u64, bool)> {
        std::iter::once((self.irqs.origin, self.irqs.initial))
            .chain(self.irqs.changes.iter().copied())
            .collect()
    }
    pub fn extend(&mut self, to: u64) -> Result<(), Error> {
        if to < self.sound.time
            || to
                .checked_sub(self.origin)
                .is_none_or(|span| span > CHUNK * POINTS as u64)
        {
            return Err(Error::Window);
        }
        while self.sound.time < to {
            if self.points.len() == POINTS {
                return Err(Error::Window);
            }
            let n = (to - self.sound.time).min(CHUNK) as u32;
            let irqs = &mut self.irqs;
            match self
                .sound
                .forecast_segment(n, |at, level| irqs.record(at, level))
            {
                Ok(point) => self.points.push_back(point),
                Err(error) => {
                    self.irqs
                        .truncate(self.sound.time, self.sound.scsp.main_interrupt());
                    return Err(Error::Sound(error));
                }
            }
        }
        Ok(())
    }
    /// Discard only history that no longer precedes an uncommitted main input.
    pub fn discard_before(&mut self, at: u64) -> Result<(), Error> {
        if at < self.origin || at > self.sound.time {
            return Err(Error::Window);
        }
        while self.points.len() > 1 && self.points[1].reference_time() <= at {
            self.points.pop_front();
        }
        self.origin = at;
        self.irqs.discard_before(at);
        Ok(())
    }
    pub fn rewind(&mut self, at: u64) -> Result<(), Error> {
        if at < self.origin || at > self.sound.time {
            return Err(Error::Window);
        }
        while self.sound.time > at {
            let point = self.points.pop_back().ok_or(Error::Window)?;
            self.sound.restore_cached_lookahead(point);
        }
        // Observations replay from the retained checkpoint, so discard the
        // later calendar before appending any of them again.
        if self.sound.time < self.irqs.origin {
            self.irqs.changes.clear();
        } else {
            self.irqs
                .truncate(self.sound.time, self.sound.scsp.main_interrupt());
        }
        if self.sound.time < at {
            let irqs = &mut self.irqs;
            let point = self
                .sound
                .forecast_segment((at - self.sound.time) as u32, |time, level| {
                    if time >= irqs.origin {
                        irqs.record(time, level)
                    }
                })
                .map_err(Error::Sound)?;
            self.points.push_back(point);
        }
        self.irqs.truncate(at, self.sound.scsp.main_interrupt());
        Ok(())
    }
    pub fn into_sound(self) -> Sound {
        self.sound
    }
}

/// A transition-free prefix already computed by the exclusive Sound owner.
/// Only reference time moves on the main board; hardware remains with Forecast.
pub(super) struct Projection {
    pub origin: u64,
    pub through: u64,
    pub irq: bool,
}
impl Sound {
    pub(crate) fn recover_remote_projection(&mut self) -> Result<(), BusError> {
        let p = self.projection.take().expect("remote projection exists");
        let through = self.time;
        self.time = p.origin;
        self.advance((through - p.origin) as u32)
    }
    pub(crate) fn projected(&self) -> bool {
        self.projection.is_some()
    }
    pub(crate) fn extend_projection(&mut self, through: u64) -> bool {
        let Some(p) = &mut self.projection else {
            return false;
        };
        if through <= p.through || through < self.time {
            return false;
        }
        p.through = through;
        true
    }
    pub(crate) fn projection_at_frontier(&self) -> bool {
        self.projection
            .as_ref()
            .is_some_and(|p| self.time == p.through)
    }
    pub(crate) fn install_projection(
        &mut self,
        origin: u64,
        through: u64,
        irq: bool,
        enabled: bool,
    ) {
        self.time = origin;
        self.enabled = enabled;
        self.projection = Some(Projection {
            origin,
            through,
            irq,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(reset: bool) -> Sound {
        let mut sound = crate::sound::wire::tests::fixture(reset, 1);
        for (at, value) in [
            (0x700, 0x1230),
            (0x702, 0x5678),
            (0x704, 0x9AB8),
            (0x706, 0xDEF0),
            (0x412, 0x3000),
            (0x414, 0x700),
            (0x416, 0x3008),
        ] {
            sound.scsp.write16(at, value).unwrap();
        }
        sound
    }
    fn advance(sound: &mut Sound, to: u64) {
        while sound.time < to {
            sound.advance(1).unwrap();
        }
    }
    #[test]
    fn rewind_keeps_dma_ram_writes_pcm_and_all_private_hardware() {
        for reset in [false, true] {
            for at in [
                0, 1, 4, 5, 9, 10, 14, 15, 19, 20, 49, 129, 997, 8191, 8192, 8193, 15000, 20000,
            ] {
                let mut reference = fixture(reset);
                let mut forecast = Forecast::new(fixture(reset));
                forecast.extend(20000).unwrap();
                assert_eq!(
                    &forecast.sound.ram[0x3000..0x3008],
                    &[0x12, 0x30, 0x56, 0x78, 0x9A, 0xB8, 0xDE, 0xF0]
                );
                forecast.rewind(at).unwrap();
                advance(&mut reference, at);
                let mut moved = forecast.into_sound();
                assert_eq!(
                    moved.domain_digest(),
                    reference.domain_digest(),
                    "reset={reset},at={at}"
                );
                advance(&mut reference, at + 10000);
                advance(&mut moved, at + 10000);
                assert_eq!(moved.domain_digest(), reference.domain_digest());
                assert_eq!(moved.scsp.drain(8192), reference.scsp.drain(8192));
            }
        }
    }
    #[test]
    fn irq_calendar_matches_each_reference_clock_and_window_discard() {
        let make = || {
            let mut sound = crate::sound::wire::tests::fixture(false, 0);
            for (i, word) in [
                0x33FCu16, 0x0020, 0x0010, 0x042A, 0x33FC, 0x0020, 0x0010, 0x042C, 0x33FC, 0x0020,
                0x0010, 0x042E, 0x60EE,
            ]
            .into_iter()
            .enumerate()
            {
                sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
            }
            sound
        };
        let mut reference = make();
        let mut expected = vec![(0, false)];
        while reference.time < 20000 {
            reference.advance(1).unwrap();
            let level = reference.scsp.main_interrupt();
            if expected.last().unwrap().1 != level {
                expected.push((reference.time, level));
            }
        }
        assert!(expected.len() > 10, "must contain IRQ rises and clears");
        let mut forecast = Forecast::new(make());
        forecast.extend(20000).unwrap();
        assert_eq!(forecast.irq_timeline(), expected);
        forecast.discard_before(9000).unwrap();
        assert!(forecast.rewind(8999).is_err());
        let rewind_at = expected.last().unwrap().0 - 1;
        forecast.rewind(rewind_at).unwrap();
        let mut reference = make();
        advance(&mut reference, rewind_at);
        assert_eq!(forecast.sound.domain_digest(), reference.domain_digest());
        forecast.extend(25000).unwrap();
        assert_eq!(forecast.through(), 25000);
        let mut clocked = make();
        advance(&mut clocked, 9000);
        let mut expected = vec![(9000, clocked.scsp.main_interrupt())];
        while clocked.time < 25000 {
            clocked.advance(1).unwrap();
            let level = clocked.scsp.main_interrupt();
            if expected.last().unwrap().1 != level {
                expected.push((clocked.time, level));
            }
        }
        assert_eq!(forecast.irq_timeline(), expected);
    }
    #[test]
    fn forecast_error_rolls_back_the_failed_interval() {
        let mut sound = fixture(false);
        sound.scsp.write16(0x414, 0x412).unwrap(); // Reserved DMA register endpoint.
        let before = sound.domain_digest();
        let mut forecast = Forecast::new(sound);
        assert!(matches!(forecast.extend(10000), Err(Error::Sound(_))));
        assert_eq!(forecast.through(), 0);
        assert_eq!(forecast.sound.domain_digest(), before);
        assert_eq!(forecast.irq_timeline(), vec![(0, false)]);
    }
    #[test]
    fn rewind_retains_pcm_fifo_overflow_and_rejects_unretained_time() {
        let (mut reference, mut predicted) = (fixture(false), fixture(false));
        reference.advance(5_000_000).unwrap();
        predicted.advance(5_000_000).unwrap();
        assert!(reference.scsp.dropped > 0);
        let origin = reference.time;
        let mut forecast = Forecast::new(predicted);
        forecast.extend(origin + 40000).unwrap();
        forecast.rewind(origin + 12345).unwrap();
        reference.advance(12345).unwrap();
        assert_eq!(forecast.sound.domain_digest(), reference.domain_digest());
        forecast.discard_before(origin + 12345).unwrap();
        assert!(forecast.rewind(origin + 12344).is_err());
        let before = forecast.sound.domain_digest();
        assert!(
            forecast
                .extend(origin + 12345 + CHUNK * POINTS as u64 + 1)
                .is_err()
        );
        assert_eq!(forecast.sound.domain_digest(), before);
        assert_eq!(
            forecast.into_sound().scsp.drain(8192),
            reference.scsp.drain(8192)
        );
    }
    #[test]
    fn equal_edge_irq_changes_coalesce_to_the_observable_level() {
        let mut timeline = Timeline {
            origin: 0,
            initial: false,
            changes: Vec::new(),
        };
        timeline.record(5, true);
        timeline.record(5, false);
        assert!(timeline.changes.is_empty());
        timeline.record(7, true);
        timeline.record(7, true);
        timeline.record(9, false);
        timeline.record(9, true);
        assert_eq!(timeline.changes, vec![(7, true)]);
        timeline.record(12, false);
        timeline.discard_before(8);
        assert!(timeline.initial);
        assert_eq!(timeline.changes, vec![(12, false)]);
    }
}
