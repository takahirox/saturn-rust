//! Slot EG state machine, ST-077 §4.2 pp.40–44, figs.4.12–4.17.
//! The published register manual gives stage/rate ordering but no numeric
//! rate table. The clocking below follows the chip behavior characterized by
//! existing SCSP emulation research (Mednafen's hardware-tested EG), written
//! independently here: an effective rate E = R + KS, where KS = KRS + OCT is
//! clamped to 0..15 and KRS=15 disables scaling. Below E=24 the EG is clocked
//! every 2^((24-E)/2 + 1) samples, an odd rate 1.5 times as often; from E=24
//! it runs every other sample with a larger step. A clock adds
//! 16>>shift to a decaying level and removes (level+1)>>shift from an
//! attacking one, so attacks approach full level exponentially. Key-on takes
//! effect at the next sample, starting at level 0x280, or at full level when
//! the AR+KS then in the registers reaches 32.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Decay1,
    Decay2,
    Release,
}

const MAX_LEVEL: u32 = 0x3FF;
const MAX: u32 = MAX_LEVEL << 16;
const KEY_ON_LEVEL: u32 = 0x280;

#[derive(Clone, Copy)]
#[cfg_attr(test, derive(PartialEq, Eq))]
#[cfg_attr(any(test, feature = "kernel-experiment"), derive(Debug))]
pub(super) struct Envelope {
    stage: Stage,
    /// Ten-bit EG level in Q16 attenuation units (0 is full volume).
    attenuation: u32,
    /// Samples since key-on, selecting which samples clock the EG.
    counter: u32,
    /// The first EG calculation after KYONEX picks the starting level.
    started: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            stage: Stage::Release,
            attenuation: MAX,
            counter: 0,
            started: true,
        }
    }
}

/// Key rate scaling from KRS and the signed octave (0..15).
fn scale(key: u16, pitch: u16) -> u32 {
    let krs = i32::from((key >> 10) & 15);
    if krs == 15 {
        return 0;
    }
    let oct = i32::from((pitch >> 11) & 15);
    let oct = if oct >= 8 { oct - 16 } else { oct };
    (krs + oct).clamp(0, 15) as u32
}

/// The step shift when `rate` clocks the EG at this sample, otherwise None.
/// An even rate below 24 clocks when the counter's lowest set bit is at
/// (24-E)/2; an odd rate also at the next bit, so it runs 1.5 times as often
/// as the even rate below it. From E=24 the EG runs on even samples.
fn clock(rate: u16, scaling: u32, counter: u32) -> Option<u32> {
    if rate == 0 {
        return None;
    }
    let effective = (u32::from(rate) + scaling).min(31);
    let wait = (24 - effective.min(24)) >> 1;
    let odd = u32::from(effective < 24 && effective & 1 != 0);
    // Bit 0 marks even counters; bit n (n > 0) marks the lowest set bit.
    let edges = (counter ^ counter.wrapping_sub(1)) & (counter ^ 1);
    if ((edges | (edges >> odd)) >> (wait + odd)) & 1 == 0 {
        return None;
    }
    let ceiling = effective.clamp(24, 30);
    Some(((32 - ceiling) >> 1) + (ceiling & 1 & (counter >> (wait + 1))))
}

impl Envelope {
    /// KYONEX restarts the attack from the key-on level.
    pub fn key_on() -> Self {
        Self {
            stage: Stage::Attack,
            attenuation: KEY_ON_LEVEL << 16,
            counter: 0,
            started: false,
        }
    }

    /// A keyed-on attack held at maximum attenuation (AR=0).
    #[cfg(test)]
    pub fn silent_attack() -> Self {
        Self {
            stage: Stage::Attack,
            attenuation: MAX,
            counter: 0,
            started: true,
        }
    }

    /// Current ten-bit EG level.
    pub fn level(&self) -> u32 {
        self.attenuation >> 16
    }

    pub fn releasing(&self) -> bool {
        self.stage == Stage::Release
    }

    /// Slot monitor SGC (0 attack .. 3 release) and the 10-bit EG level.
    /// A held attack reports full level, as it sounds.
    pub fn monitor(&self, hold: bool) -> (u16, u16) {
        let level = if self.stage == Stage::Attack && hold {
            0
        } else {
            (self.attenuation >> 16).min(MAX_LEVEL) as u16
        };
        (self.stage as u16, level)
    }

    pub fn release(&mut self, hold: bool) {
        if self.stage == Stage::Attack && hold {
            self.attenuation = 0;
        }
        self.stage = Stage::Release;
    }

    /// One 44.1 kHz EG calculation. Return Q16 attenuation, or None at the end
    /// of release. A zero rate holds the current level indefinitely.
    pub fn advance(&mut self, rates: u16, key: u16, pitch: u16, loop_start: bool) -> Option<u32> {
        let scaling = scale(key, pitch);
        if !self.started {
            self.started = true;
            if u32::from(rates & 31) + scaling >= 32 {
                self.attenuation = 0;
            }
        }
        let counter = self.counter;
        self.counter = self.counter.wrapping_add(1);
        let level = self.attenuation >> 16;
        let rate = match self.stage {
            Stage::Attack => rates & 31,
            Stage::Decay1 => (rates >> 6) & 31,
            Stage::Decay2 => rates >> 11,
            Stage::Release => key & 31,
        };
        let instant = self.stage == Stage::Attack && u32::from(rate) + scaling >= 32;
        if let Some(shift) = clock(rate, scaling, counter).filter(|_| !instant) {
            let next = if self.stage == Stage::Attack {
                // Remove ceil((level + 1) / 2^shift), stopping at full level.
                level - ((level + (1 << shift)) >> shift).min(level)
            } else {
                (level + (16 >> shift)).min(MAX_LEVEL)
            };
            self.attenuation = next << 16;
        }
        // Transitions observe the level before this sample's step.
        match self.stage {
            Stage::Attack
                if (key & 0x4000 != 0 && loop_start) || (key & 0x4000 == 0 && level == 0) =>
            {
                self.stage = Stage::Decay1;
            }
            Stage::Decay1 if level >> 5 == u32::from((key >> 5) & 31) => {
                self.stage = Stage::Decay2;
            }
            Stage::Release if self.attenuation >> 16 == MAX_LEVEL => return None,
            _ => (),
        }
        Some(if self.stage == Stage::Attack && rates & 0x20 != 0 {
            0
        } else {
            self.attenuation
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_SCALING: u16 = 0x3C00; // KRS=15

    fn level(e: &Envelope) -> u32 {
        e.attenuation >> 16
    }

    /// Key on, then run the first EG calculation with these registers.
    fn start(rates: u16, key: u16, pitch: u16) -> Envelope {
        let mut e = Envelope::key_on();
        e.advance(rates, key, pitch, false);
        e
    }

    #[test]
    fn key_on_starts_attack_at_the_key_on_level_or_full_level_for_instant_attack() {
        assert_eq!(level(&Envelope::key_on()), KEY_ON_LEVEL);
        // The first calculation sees the rates written after KYONEX.
        assert_eq!(level(&start(31, 0, 1 << 11)), 0); // AR 31 + KS 1
        // KRS 7 + OCT 7 = KS 14; AR 18 + 14 reaches 32.
        assert_eq!(level(&start(18, 7 << 10, 7 << 11)), 0);
        let below = level(&start(17, 7 << 10, 7 << 11));
        assert!(below > 0 && below < KEY_ON_LEVEL);
        assert_eq!(level(&start(0, NO_SCALING, 0)), KEY_ON_LEVEL);
        // An instant rate reached only later in the attack holds the level.
        let mut e = start(0, NO_SCALING, 0);
        e.advance(31, 0, 1 << 11, false);
        assert_eq!(level(&e), KEY_ON_LEVEL);
        // A negative octave reduces scaling, clamped at zero.
        assert_eq!(scale(2 << 10, 15 << 11), 1);
        assert_eq!(scale(0, 8 << 11), 0);
        assert_eq!(scale(15 << 10, 7 << 11), 0);
    }

    #[test]
    fn effective_rates_set_clock_spacing_and_step_size() {
        // Every other sample from E=24; one level per clock at 24, and odd
        // rates alternate between the neighboring step sizes.
        for (rate, per_two) in [(24, 1), (26, 2), (28, 4), (30, 8), (31, 8)] {
            let mut e = Envelope::key_on();
            e.stage = Stage::Release;
            e.attenuation = 0;
            for _ in 0..8 {
                e.advance(0, NO_SCALING | rate, 0, false);
            }
            assert_eq!(level(&e), 4 * per_two, "rate {rate}");
        }
        // Below 24, each two rate steps halve the clock frequency; an odd
        // rate clocks 1.5 times as often as the even rate below it.
        for (rate, samples, steps) in [
            (22, 256, 64),
            (20, 256, 32),
            (12, 4096, 32),
            (23, 256, 96),
            (13, 4096, 48),
        ] {
            let mut e = Envelope::key_on();
            e.stage = Stage::Release;
            e.attenuation = 0;
            for _ in 0..samples {
                e.advance(0, NO_SCALING | rate, 0, false);
            }
            assert_eq!(level(&e), steps, "rate {rate}");
        }
    }

    #[test]
    fn attack_is_exponential_and_reaches_full_level() {
        let mut e = Envelope::key_on();
        let mut previous = level(&e);
        let mut steps = Vec::new();
        for _ in 0..4000 {
            e.advance(20, NO_SCALING, 0, false);
            if level(&e) != previous {
                steps.push(previous - level(&e));
                previous = level(&e);
            }
            if e.stage != Stage::Attack {
                break;
            }
        }
        assert_eq!(e.stage, Stage::Decay1);
        assert_eq!(level(&e), 0);
        // Larger levels fall faster: ceil((level + 1) / 16).
        assert_eq!(steps[0], (KEY_ON_LEVEL + 16) >> 4);
        assert!(steps.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn zero_rates_hold_and_held_attack_releases_from_full_level() {
        let mut e = Envelope::key_on();
        for _ in 0..1000 {
            assert_eq!(e.advance(0, NO_SCALING, 0, false), Some(KEY_ON_LEVEL << 16));
        }
        assert_eq!(e.advance(0x20, NO_SCALING, 0, false), Some(0));
        e.release(true);
        assert_eq!(level(&e), 0);
        let mut ended = false;
        for _ in 0..2048 {
            if e.advance(0x20, NO_SCALING | 31, 0, false).is_none() {
                ended = true;
                break;
            }
        }
        assert!(ended);
    }

    #[test]
    fn decay_level_moves_to_decay_two_and_loop_link_ends_attack() {
        for dl in [0u16, 1, 16, 31] {
            let key = NO_SCALING | dl << 5;
            let mut e = Envelope::key_on();
            for _ in 0..8192 {
                e.advance(31 | 24 << 6, key, 0, false);
                if e.stage == Stage::Decay2 {
                    break;
                }
            }
            assert_eq!(e.stage, Stage::Decay2, "DL={dl}");
            let held = e.attenuation;
            for _ in 0..100 {
                assert_eq!(e.advance(31 | 24 << 6, key, 0, false), Some(held));
            }
        }
        let key = 0x7C00 | 16 << 5;
        let mut linked = Envelope::key_on();
        linked.advance(1 | 31 << 6, key, 0, true);
        assert_eq!(linked.stage, Stage::Decay1);
        assert!(level(&linked) > 0);
    }
}

#[cfg(any(test, feature = "sound-worker"))]
crate::sound::wire::structure!(Envelope {
    stage,
    attenuation,
    counter,
    started
});

#[cfg(any(test, feature = "sound-worker"))]
crate::sound::wire::enumeration!(Stage { Attack=0, Decay1=1, Decay2=2, Release=3 });
