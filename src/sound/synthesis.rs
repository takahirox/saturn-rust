//! Independent slot modulation, ST-077 §4.2, figs.4.18/4.30/4.31,
//! tables 4.16–4.18 and 4.21–4.24. No external synthesizer/core.
//! The public manual specifies white noise, but not its polynomial or seed;
//! use a reproducible maximal 16-bit LFSR, without claiming its exact chip sequence.

/// 2^x in Q30, with a signed Q16 exponent. Constants are rounded mathematical
/// 2^(2^-n), n=1..16, not a sampled/canned audio waveform. All runtime arithmetic
/// is integer, including on Wasm. The synthesis callers use -20 <= x <= 1.
fn exp2(exponent: i32) -> i64 {
    const ROOTS: [i64; 16] = [
        1518500250, 1276901417, 1170923762, 1121280436, 1097253708, 1085434106, 1079572136,
        1076653033, 1075196443, 1074468888, 1074105294, 1073923544, 1073832680, 1073787251,
        1073764537, 1073753181,
    ];
    let integer = exponent.div_euclid(65536);
    let fraction = exponent.rem_euclid(65536);
    let mut gain = 1i64 << 30;
    for (bit, root) in ROOTS.into_iter().enumerate() {
        if fraction & (0x8000 >> bit) != 0 {
            gain = (gain * root) >> 30;
        }
    }
    if integer < 0 {
        gain >> -integer
    } else {
        gain << integer
    }
}

/// TL's binary attenuation: each 16 units halves the waveform (table 4.18).
/// ALFO adds attenuation in Q8 TL units; it never amplifies the source.
pub(super) fn attenuate(sample: i16, tl: u8, alfo: u32) -> i16 {
    let exponent = -((i32::from(tl) * 256 + alfo as i32) * 16);
    ((i64::from(sample) * exp2(exponent)) >> 30) as i16
}

#[derive(Clone, Copy)]
pub(super) struct Lfo {
    // Rational cycles: one cycle is 44100*100; frequencies in table 4.21
    // are stated to .01 Hz. Carry the remainder, rather than rounding a period.
    phase: u32,
    noise: u16,
}

impl Lfo {
    pub fn new(slot: usize) -> Self {
        Self {
            phase: 0,
            noise: 0xACE1 ^ (slot as u16 * 0x123),
        }
    }

    pub fn reset(&mut self) {
        // LFORE has no effect on the noise oscillator (ST-077 p.37).
        self.phase = 0;
    }

    pub fn advance(&mut self, control: u16) {
        const HUNDREDTHS_HZ: [u32; 32] = [
            17, 19, 23, 27, 34, 39, 45, 55, 68, 78, 92, 110, 139, 160, 187, 227, 287, 331, 392,
            479, 615, 718, 860, 1080, 1440, 1720, 2150, 2870, 4310, 5740, 8610, 17230,
        ];
        self.phase = if control & 0x8000 != 0 {
            0
        } else {
            (self.phase + HUNDREDTHS_HZ[usize::from((control >> 10) & 31)]) % 4_410_000
        };
        // x^16+x^14+x^13+x^11+1, a fixed functional noise policy. Clocked
        // per slot/sample even when keyed off; neither pitch nor LFORE gates it.
        self.noise = (self.noise >> 1) ^ if self.noise & 1 != 0 { 0xB400 } else { 0 };
    }

    pub fn noise(&self) -> i16 {
        self.noise as i16
    }

    fn amplitude(&self, shape: u16) -> i32 {
        let phase = self.phase * 256 / 4_410_000;
        match shape {
            0 => phase as i32,
            1 => {
                if phase < 128 {
                    0
                } else {
                    255
                }
            }
            2 => {
                (if phase < 128 {
                    phase * 2
                } else {
                    (255 - phase) * 2
                }) as i32
            }
            _ => i32::from(self.noise >> 8),
        }
    }

    fn pitch(&self, shape: u16) -> i32 {
        let phase = (self.phase * 256 / 4_410_000) as i32;
        match shape {
            0 => phase as u8 as i8 as i32,
            1 => {
                if phase < 128 {
                    127
                } else {
                    -128
                }
            }
            2 => match phase {
                0..=63 => phase * 2,
                64..=191 => 255 - phase * 2,
                _ => phase * 2 - 511,
            },
            _ => i32::from((self.noise >> 8) as u8 as i8),
        }
    }

    pub fn attenuation(&self, control: u16) -> u32 {
        let sensitivity = control & 7;
        if sensitivity == 0 {
            return 0;
        }
        // 0.4/0.8/1.5/3/6/12/24 dB, binary TL depth 1/2/4/8/16/32/64.
        (self.amplitude((control >> 3) & 3) << (sensitivity - 1)) as u32
    }

    pub fn step(&self, control: u16, step: i64) -> i64 {
        // Table 4.24: documented cents, expressed in half-cent units.
        const HALF_CENTS: [i32; 8] = [0, 14, 27, 54, 110, 224, 460, 988];
        let depth = HALF_CENTS[usize::from((control >> 5) & 7)];
        if depth == 0 {
            return step;
        }
        let exponent = self.pitch((control >> 8) & 3) * depth * 256 / 1200;
        (step * exp2(exponent)) >> 30
    }
}

/// Sound-stack data are output five slot clocks before OP2 can consume them
/// (fig.4.31). Keep two committed generations and the pending OP pipeline.
pub(super) struct Stack {
    pub data: [i16; 64],
    bank: [usize; 32],
    pending: [Option<i16>; 32],
    frame_bank: usize,
}

impl Default for Stack {
    fn default() -> Self {
        Self {
            data: [0; 64],
            bank: [0; 32],
            pending: [None; 32],
            frame_bank: 0,
        }
    }
}

impl Stack {
    pub fn begin_slot(&mut self, slot: usize) {
        let source = (slot + 27) & 31;
        if let Some(value) = self.pending[source].take() {
            let bank = self.frame_bank ^ if slot < 5 { 32 } else { 0 };
            self.data[bank + source] = value;
            self.bank[source] = bank;
        }
    }

    pub fn write(&mut self, slot: usize, value: i16, inhibit: bool) {
        self.pending[slot] = (!inhibit).then_some(value);
    }

    pub fn end_frame(&mut self) {
        self.frame_bank ^= 32;
    }

    fn read(&self, slot: usize, select: u16) -> i64 {
        let source = (slot + usize::from(select)) & 31;
        // Explicit latest/past columns in table 4.16; do not treat MDXSL
        // as an absolute slot number or ignore its sixth bit.
        let past = if (0x1C..=0x3B).contains(&select) {
            0
        } else {
            32
        };
        i64::from(self.data[(self.bank[source] ^ past) + source])
    }

    pub fn displacement(&self, slot: usize, control: u16) -> i64 {
        let level = control >> 12;
        if level <= 4 {
            return 0;
        }
        let x = self.read(slot, (control >> 6) & 63);
        let y = self.read(slot, control & 63);
        let average = (x + y) >> 1;
        // Table 4.17: MDL5 => ±32 samples, MDLF => ±32768.
        // Fig.4.41: address-shift overflow returns to zero each 1K samples.
        // Preserve the signed fractional phase and guard waveforms outside LSA/LEA.
        (average << (level + 1)) % (1024 << 16)
    }
}

#[cfg(test)]
mod tests;
