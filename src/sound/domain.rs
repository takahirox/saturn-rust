//! Diagnostic external-input replay for the original sound domain.
//! This does not enable asynchronous production execution or relax any deadline.
use super::*;
use std::fmt::Write;

#[derive(Default)]
pub(super) struct Trace {
    events: Vec<[u32; 8]>,
}

struct Digest(u64);
impl Digest {
    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x100000001B3);
        }
    }
    fn words(&self) -> [u32; 2] {
        [self.0 as u32, (self.0 >> 32) as u32]
    }
}
impl Write for Digest {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.bytes(text.as_bytes());
        Ok(())
    }
}
fn pcm_digest(samples: &[i16]) -> [u32; 2] {
    let mut hash = Digest(0xCBF29CE484222325);
    for sample in samples {
        hash.bytes(&sample.to_le_bytes());
    }
    hash.words()
}

impl Sound {
    pub(crate) fn domain_reference_time(&self) -> u64 {
        self.time
    }
    /// Full private sound state, excluding only derived lookup memos and the
    /// test memory trace. In-flight operands, CPU replay, PCM and RAM remain.
    pub fn domain_digest(&self) -> [u32; 2] {
        let mut scsp = self.scsp.clone();
        scsp.pcm_quiescent.set(None);
        scsp.dsp.normalize_domain_memo();
        let mut hash = Digest(0xCBF29CE484222325);
        write!(
            hash,
            "{:?}{:?}{:?}{:?}",
            self.cpu,
            scsp,
            self.memory.checkpoint_state(),
            self.transactions.domain_diagnostic_state()
        )
        .unwrap();
        write!(
            hash,
            "{:?}",
            (
                self.enabled,
                self.time,
                self.cpu_ticks,
                self.cpu_origin,
                self.next_cpu,
                self.first_cpu_request,
                self.ram_busy_until,
                self.scsp_tick,
                self.reset_output_start,
                self.reset_output_end
            )
        )
        .unwrap();
        hash.bytes(&self.ram);
        hash.words()
    }
    pub fn begin_domain_trace(&mut self) -> Result<(), String> {
        if self.domain_digest() != Self::default().domain_digest() {
            return Err("sound trace requires an untouched cold sound domain".into());
        }
        self.domain_trace = Some(std::rc::Rc::default());
        Ok(())
    }
    pub(super) fn record_domain_event(&self, kind: u32, [a, b, c, d]: [u32; 4]) {
        if let Some(trace) = &self.domain_trace {
            let mut trace = trace.borrow_mut();
            assert!(trace.events.len() < 1_048_576, "bounded sound domain trace");
            trace.events.push([
                self.time as u32,
                (self.time >> 32) as u32,
                kind,
                a,
                b,
                c,
                d,
                0,
            ]);
        }
    }
    pub(crate) fn record_domain_scsp(
        &self,
        write: bool,
        address: u32,
        width: u32,
        result: &Result<u32, BusError>,
    ) {
        match result {
            Ok(value) => {
                self.record_domain_event(if write { 4 } else { 3 }, [address, width, *value, 0])
            }
            Err(_) => self.record_domain_event(0, [1, 0, 0, 0]),
        }
    }
    /// A marker validates every state field at this timestamp before draining.
    pub fn take_domain_trace(&mut self) -> Vec<u32> {
        if self.domain_trace.is_none() {
            return Vec::new();
        }
        let [low, high] = self.domain_digest();
        self.record_domain_event(10, [0, 0, low, high]);
        let events = std::mem::take(&mut self.domain_trace.as_ref().unwrap().borrow_mut().events);
        events.into_iter().flatten().collect()
    }
    pub fn drain_domain_audio(&mut self, frames: usize) -> Vec<i16> {
        let samples = self.scsp.drain(frames);
        if self.domain_trace.is_some() {
            let [low, high] = pcm_digest(&samples);
            self.record_domain_event(9, [frames as u32, samples.len() as u32, low, high]);
        }
        samples
    }
    /// Apply captured accepted external operations to a separate cold domain.
    /// Instrumented throughput is diagnostic and cannot represent game FPS.
    pub fn replay_domain_trace(&mut self, words: &[u32]) -> Result<(), String> {
        self.replay_domain_trace_mode::<false>(words).map(|_| ())
    }
    pub fn replay_domain_trace_forecast(&mut self, words: &[u32]) -> Result<u32, String> {
        self.replay_domain_trace_mode::<true>(words)
    }
    fn replay_domain_trace_mode<const FORECAST: bool>(
        &mut self,
        words: &[u32],
    ) -> Result<u32, String> {
        let mut forecast_gaps = 0;
        if !words.len().is_multiple_of(8) || self.domain_trace.is_some() {
            return Err("invalid sound trace framing or replay owner".into());
        }
        for (index, event) in words.as_chunks::<8>().0.iter().enumerate() {
            let &[low, high, kind, a, b, c, d, reserved] = event;
            let at = u64::from(low) | u64::from(high) << 32;
            if at < self.time || reserved != 0 {
                return Err(format!("invalid sound trace time at event {index}"));
            }
            if FORECAST && at.saturating_sub(self.time) >= 512 {
                self.advance_trace_forecast(at)?;
                forecast_gaps += 1;
            } else {
                while self.time < at {
                    self.advance((at - self.time).min(u64::from(u32::MAX)) as u32)
                        .map_err(|error| error.to_string())?;
                }
            }
            let valid = match kind {
                1 => u32::from(self.read_ram8(a as usize)) == c,
                2 => {
                    self.write_ram8(a as usize, c as u8);
                    true
                }
                3 => {
                    match b {
                        1 => self.scsp.read8(a).map(u32::from),
                        2 => self.scsp.read16(a).map(u32::from),
                        _ => return Err("invalid SCSP read width".into()),
                    }
                    .map_err(|error| error.to_string())?
                        == c
                }
                4 => {
                    match b {
                        1 => self.scsp.write8(a, c as u8),
                        2 => self.scsp.write16(a, c as u16),
                        _ => return Err("invalid SCSP write width".into()),
                    }
                    .map_err(|error| error.to_string())?;
                    true
                }
                5 if matches!(b, 1 | 2 | 4) && d <= 1 => {
                    self.main_ram_request(Some(memory::Request {
                        address: a as usize,
                        width: b as usize,
                        write: (d != 0).then_some(c),
                    }));
                    true
                }
                6 => {
                    self.main_ram_request(None);
                    true
                }
                7 if self.main_ram_ready() => {
                    self.complete_main_ram()
                        .map_err(|error| error.to_string())?
                        == c
                }
                8 if a <= 1 => {
                    self.set_enabled(a != 0)
                        .map_err(|error| error.to_string())?;
                    true
                }
                9 => {
                    let samples = self.scsp.drain(a as usize);
                    samples.len() == b as usize && pcm_digest(&samples) == [c, d]
                }
                10 => self.domain_digest() == [c, d],
                _ => return Err(format!("unsupported sound trace event {index}: {kind}")),
            };
            if !valid {
                return Err(format!("sound trace mismatch at event {index}: {kind}"));
            }
        }
        Ok(forecast_gaps)
    }
    fn advance_trace_forecast(&mut self, to: u64) -> Result<(), String> {
        let mut predicted = crate::sound::forecast::Forecast::new(std::mem::take(self));
        let result = (|| {
            let target = to.saturating_add(8192);
            while predicted.through() < target {
                predicted.discard_before(predicted.through().min(to))?;
                let end = target.min(predicted.through().saturating_add(524288));
                if let Err(error) = predicted.extend(end) {
                    // A fault beyond the next real input is speculative. Keep
                    // only the known prefix and compute through the input itself.
                    if predicted.through() < to {
                        predicted.extend(to)?;
                    }
                    if predicted.through() < to {
                        return Err(error);
                    }
                    break;
                }
            }
            predicted.rewind(to)
        })();
        *self = predicted.into_sound();
        result.map_err(|error| error.to_string())
    }
}

impl crate::Machine {
    pub fn sound_domain_snapshot(&self) -> Result<Vec<u8>, &'static str> {
        self.bus.sound.domain_snapshot()
    }
    pub fn begin_sound_domain_trace(&mut self) -> Result<(), String> {
        self.bus.sound.begin_domain_trace()
    }
    pub fn take_sound_domain_trace(&mut self) -> Vec<u32> {
        self.bus.sound.take_domain_trace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::Memory;

    #[test]
    fn separate_domain_replays_original_reset_irq_dram_and_pcm_at_arbitrary_splits() {
        for chunk in [1, 3, 17, 10000] {
            let mut machine = crate::Machine::new();
            machine.begin_sound_domain_trace().unwrap();
            machine.bus.write16(SCSP + 0x400, 0x20F).unwrap();
            // Original reset vectors and a control-write/RESET/branch program.
            let s = &mut machine.bus.sound;
            for (at, value) in [(0, 0x00007000u32), (4, 0x00000100)] {
                for (i, byte) in value.to_be_bytes().into_iter().enumerate() {
                    s.write_ram8(at + i, byte);
                }
            }
            for (i, word) in [0x33FCu16, 0x0020, 0x0010, 0x042A, 0x4E70, 0x60FE]
                .into_iter()
                .enumerate()
            {
                for (j, byte) in word.to_be_bytes().into_iter().enumerate() {
                    s.write_ram8(0x100 + i * 2 + j, byte);
                }
            }
            s.set_enabled(true).unwrap();
            let mut replay = Sound::default();
            let mut predicted = Sound::default();
            let words = machine.take_sound_domain_trace();
            replay.replay_domain_trace(&words).unwrap();
            let mut forecast_gaps = predicted.replay_domain_trace_forecast(&words).unwrap();
            for target in [1, 14, 15, 129, 997, 10000, 20000] {
                let s = &mut machine.bus.sound;
                while s.time < target {
                    s.advance((target - s.time).min(chunk) as u32).unwrap();
                }
                if target == 129 {
                    s.main_ram_request(Some(memory::Request {
                        address: 0x100,
                        width: 2,
                        write: None,
                    }));
                }
                if target == 997 {
                    assert_eq!(s.complete_main_ram(), Ok(0x33FC));
                }
                s.drain_domain_audio(8192);
                let words = machine.take_sound_domain_trace();
                replay.replay_domain_trace(&words).unwrap();
                forecast_gaps += predicted.replay_domain_trace_forecast(&words).unwrap();
                assert_eq!(machine.bus.sound.domain_digest(), replay.domain_digest());
                assert_eq!(predicted.domain_digest(), replay.domain_digest());
            }
            assert!(forecast_gaps > 0);
            assert!(machine.bus.sound.cpu.state.instructions > 0);
            assert!(machine.bus.sound.scsp.generated > 0);
            assert_eq!(machine.bus.sound.scsp.common[21], 0, "RESET executed");
        }
    }

    #[test]
    fn speculative_fault_after_the_next_real_input_does_not_escape() {
        let make = || {
            let mut sound = Sound::default();
            sound.scsp.write16(0x412, 0x2000).unwrap();
            sound.scsp.write16(0x414, 0x412).unwrap();
            sound.scsp.write16(0x416, 0x1002).unwrap();
            sound
        };
        let (mut normal, mut predicted) = (make(), make());
        normal.advance(4).unwrap();
        predicted.advance_trace_forecast(4).unwrap();
        assert_eq!(predicted.domain_digest(), normal.domain_digest());
        for sound in [&mut normal, &mut predicted] {
            // The actual input fixes the endpoint before the first DMA tick.
            sound.scsp.write16(0x414, 0x700).unwrap();
            sound.advance(1000).unwrap();
        }
        assert_eq!(predicted.domain_digest(), normal.domain_digest());
    }

    #[test]
    fn trace_rejects_missing_effects_changed_reads_bad_framing_and_time_reversal() {
        let mut sound = Sound::default();
        sound.begin_domain_trace().unwrap();
        sound.write_ram8(0x101, 0x37);
        assert_eq!(sound.read_ram8(0x101), 0x37);
        sound.advance(1000).unwrap();
        let words = sound.take_domain_trace();
        Sound::default().replay_domain_trace(&words).unwrap();
        let mut missing_write = words[8..].to_vec();
        assert!(
            Sound::default()
                .replay_domain_trace(&missing_write)
                .is_err()
        );
        missing_write.truncate(missing_write.len() - 1);
        assert!(
            Sound::default()
                .replay_domain_trace(&missing_write)
                .is_err()
        );
        let mut reversed = words.clone();
        reversed[0] = 2000;
        assert!(Sound::default().replay_domain_trace(&reversed).is_err());
        let mut read = words.clone();
        read[8 + 5] ^= 1;
        assert!(Sound::default().replay_domain_trace(&read).is_err());
    }
}
