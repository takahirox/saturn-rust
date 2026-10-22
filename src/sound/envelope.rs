//! Slot EG state machine, ST-077 §4.2 pp.40–44, figs.4.12–4.17.
//! The published register manual gives stage/rate ordering but no numeric rate
//! table. The integer, octave-scaled slopes below are a functional timing policy,
//! not a claim of measured chip envelope times. Keep this deviation explicit.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay1,
    Decay2,
    Release,
}

const MAX: u32 = 1023 << 16;

#[derive(Clone, Copy)]
pub(super) struct Envelope {
    stage: Stage,
    attenuation: u32,
    linked: bool,
    // A forced loop-start transition below DL must never enter Decay2 later
    // (fig.4.17), even if a large per-sample step crosses a band boundary.
    decay_target: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            stage: Stage::Attack,
            attenuation: MAX,
            linked: false,
            decay_target: true,
        }
    }
}

impl Envelope {
    pub fn releasing(&self) -> bool {
        self.stage == Stage::Release
    }

    pub fn release(&mut self, hold: bool) {
        if self.stage == Stage::Attack && hold {
            self.attenuation = 0;
        }
        self.stage = Stage::Release;
    }

    fn step(rate: u16, key: u16, pitch: u16, attack: bool) -> u32 {
        if rate == 0 {
            return 0;
        }
        let oct = ((pitch >> 11) & 15) as i32;
        let oct = if oct >= 8 { oct - 16 } else { oct };
        let krs = (key >> 10) & 15;
        let scaling = if krs == 15 {
            0
        } else {
            oct + 2 * i32::from(krs) + i32::from((pitch >> 9) & 1)
        };
        let index = (i32::from(rate) * 2 + scaling).clamp(0, 63) as u32;
        if attack && index >= 62 {
            return MAX;
        }
        // Q16 attenuation units/sample. Every four effective rate steps doubles
        // the slope; attack traverses the range eight times faster than decay.
        let slope = (4 + (index & 3)) << (index / 4);
        if attack { slope * 8 } else { slope }
    }

    fn decay1(&mut self, key: u16) {
        self.stage = Stage::Decay1;
        self.decay_target = self.attenuation >> 21 <= u32::from((key >> 5) & 31);
    }

    /// One 44.1 kHz EG calculation. Return Q16 attenuation, or None at the end
    /// of release. A zero release rate holds the key-off level indefinitely.
    pub fn advance(&mut self, rates: u16, key: u16, pitch: u16, loop_start: bool) -> Option<u32> {
        if self.stage == Stage::Attack && key & 0x4000 != 0 && loop_start && !self.linked {
            self.linked = true;
            self.decay1(key);
        }
        match self.stage {
            Stage::Attack => {
                self.attenuation =
                    self.attenuation
                        .saturating_sub(Self::step(rates & 31, key, pitch, true));
                let output = if rates & 0x20 != 0 {
                    0
                } else {
                    self.attenuation
                };
                if self.attenuation == 0 && key & 0x4000 == 0 {
                    self.decay1(key);
                }
                Some(output)
            }
            Stage::Decay1 => {
                let target = u32::from((key >> 5) & 31) << 21;
                if self.decay_target && self.attenuation >= target {
                    self.stage = Stage::Decay2;
                    return Some(self.attenuation);
                }
                self.attenuation = self
                    .attenuation
                    .saturating_add(Self::step((rates >> 6) & 31, key, pitch, false))
                    .min(MAX);
                if self.decay_target && self.attenuation >= target {
                    self.stage = Stage::Decay2;
                }
                Some(self.attenuation)
            }
            Stage::Decay2 | Stage::Release => {
                let rate = if self.releasing() {
                    key & 31
                } else {
                    rates >> 11
                };
                self.attenuation = self
                    .attenuation
                    .saturating_add(Self::step(rate, key, pitch, false))
                    .min(MAX);
                if self.releasing() && self.attenuation == MAX {
                    None
                } else {
                    Some(self.attenuation)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_rates_hold_and_key_off_uses_current_level() {
        let mut e = Envelope::default();
        for _ in 0..1000 {
            assert_eq!(e.advance(0, 0x3C00, 0, false), Some(MAX));
            assert_eq!(e.advance(0x20, 0x3C00, 0, false), Some(0));
        }
        assert_eq!(e.advance(31, 0x3C00, 0, false), Some(0));
        for _ in 0..1000 {
            assert_eq!(e.advance(31, 0x3C00, 0, false), Some(0));
        }
        e.release(false);
        assert_eq!(e.advance(31, 0x3C00, 0, false), Some(0));
        assert!(e.advance(31, 0x3C1F, 0, false).unwrap() > 0);
        let mut ended = false;
        for _ in 0..1024 {
            if e.advance(31, 0x3C1F, 0, false).is_none() {
                ended = true;
                break;
            }
        }
        assert!(ended);
    }

    #[test]
    fn key_off_of_held_attack_releases_from_audible_level_and_dl_zero_skips_decay_one() {
        let mut held = Envelope::default();
        assert_eq!(held.advance(0x20, 0x3C1F, 0, false), Some(0));
        held.release(true);
        let first = held.advance(0x20, 0x3C1F, 0, false).unwrap();
        assert!(first > 0 && first < MAX);
        let mut skip = Envelope::default();
        assert_eq!(skip.advance(31 | 31 << 6, 0x3C00, 0, false), Some(0));
        assert_eq!(skip.advance(31 | 31 << 6, 0x3C00, 0, false), Some(0));
        assert_eq!(skip.stage, Stage::Decay2);
    }

    #[test]
    fn every_decay_level_transitions_and_decay_two_zero_holds() {
        for dl in 0..32 {
            let mut e = Envelope::default();
            let key = 0x3C00 | dl << 5;
            e.advance(31, key, 0, false);
            for _ in 0..1024 {
                e.advance(31 | 31 << 6, key, 0, false);
                if e.stage == Stage::Decay2 {
                    break;
                }
            }
            assert_eq!(e.stage, Stage::Decay2, "DL={dl}");
            let held = e.attenuation;
            for _ in 0..100 {
                assert_eq!(e.advance(31, key, 0, false), Some(held));
            }
            assert!(e.advance(31 << 11, key, 0, false).unwrap() >= held);
        }
    }

    #[test]
    fn loop_start_can_end_attack_early_or_hold_completed_attack() {
        let key = 0x7C00 | 16 << 5;
        let mut e = Envelope::default();
        e.advance(31, key, 0, false);
        assert_eq!(e.stage, Stage::Attack);
        assert_eq!(e.attenuation, 0);
        e.advance(31 | 31 << 6, key, 0, true);
        assert_eq!(e.stage, Stage::Decay1);
        assert!(e.attenuation > 0);
        let mut early = Envelope::default();
        early.advance(1 | 31 << 6, key, 0, true);
        assert_eq!(early.stage, Stage::Decay1);
        for _ in 0..1024 {
            early.advance(31 << 6, key, 0, true);
        }
        assert_eq!(early.stage, Stage::Decay1); // fig.4.17, never reaches DL
        assert_eq!(early.attenuation, MAX);
    }

    #[test]
    fn all_rates_are_monotonic_and_key_scaling_responds_to_pitch() {
        for attack in [false, true] {
            let mut last = 0;
            for rate in 0..32 {
                let step = Envelope::step(rate, 0x3C00, 0, attack);
                assert!(step >= last);
                last = step;
            }
            let base = Envelope::step(8, 0, 0, attack);
            assert!(Envelope::step(8, 0, 2 << 11, attack) > base);
            assert!(Envelope::step(8, 2 << 10, 0, attack) > base);
            assert_eq!(
                Envelope::step(8, 0x3C00, 7 << 11, attack),
                Envelope::step(8, 0x3C00, 8 << 11, attack)
            );
            assert!(Envelope::step(8, 0, 0x200, attack) > base);
        }
    }
}
