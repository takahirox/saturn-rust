//! Original SCSP effects processor. ST-077 §4.1 figs.4.5–4.7, §4.3 and
//! §5 fig.5.1 define the buses, RAM widths and circular addressing. ST-228
//! Reference 1/2 describes multiply/add, delayed TEMP and sound-memory routing.
//! One instruction executes at each 1/(128*44100) second deadline.
//! Program, coefficient and RAM accesses use their execution-time values.
use super::{BusError, RamConfig, Scsp};

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(super) struct Dsp {
    coefficients: [u16; 64],
    addresses: [u16; 32],
    program: [[u16; 4]; 128],
    temp: [i32; 128],
    mems: [i32; 32],
    pub mix: [i32; 16],
    pub effects: [i16; 16],
    decrement: u16,
    accumulator: i32,
    y: i32,
    fraction: i32,
    address: u16,
    memory: i32,
    reads: [Option<i32>; 2],
    programmed: bool,
    nonzero_program: u16,
    live_steps: [u64; 2],
    nonzero_temp: u16,
    validated_program: std::cell::Cell<Option<bool>>,
}
impl Default for Dsp {
    fn default() -> Self {
        Self {
            coefficients: [0; 64],
            addresses: [0; 32],
            program: [[0; 4]; 128],
            temp: [0; 128],
            mems: [0; 32],
            mix: [0; 16],
            effects: [0; 16],
            decrement: 0,
            accumulator: 0,
            y: 0,
            fraction: 0,
            address: 0,
            memory: 0,
            reads: [None; 2],
            programmed: false,
            nonzero_program: 0,
            live_steps: [0; 2],
            nonzero_temp: 0,
            validated_program: std::cell::Cell::new(None),
        }
    }
}
fn signed(value: i64, bits: u32) -> i32 {
    ((value as i32) << (32 - bits)) >> (32 - bits)
}
// Yamaha's 16-bit sound-memory float: sign, four-bit normalization count,
// eleven explicit mantissa bits; the implicit bit is the inverse of the sign.
fn unpack(word: u16) -> i32 {
    let exponent = (word >> 11) & 15;
    let sign = i32::from(word >> 15);
    let mut value = (i32::from(word & 0x7FF) << 11) | (sign << 22);
    if exponent < 12 {
        value ^= 1 << 22;
    }
    value |= sign << 23;
    signed(i64::from(value), 24) >> exponent.min(11)
}
fn pack(value: i32) -> u16 {
    let value = signed(i64::from(value), 24);
    let mut transitions = ((value ^ (value << 1)) as u32) & 0xFFFFFF;
    let mut exponent = 0;
    while exponent < 12 && transitions & 0x800000 == 0 {
        exponent += 1;
        transitions <<= 1;
    }
    let mantissa = if exponent < 12 {
        value.wrapping_shl(exponent)
    } else {
        value << 11
    };
    (((value >> 23) as u16 & 1) << 15)
        | ((exponent as u16) << 11)
        | ((mantissa >> 11) as u16 & 0x7FF)
}
impl Dsp {
    pub fn read(&self, offset: u32) -> Result<u16, BusError> {
        let index = offset as usize;
        let part = |v: i32, low_bits: u32| {
            if offset & 2 == 0 {
                (v as u16) & ((1 << low_bits) - 1)
            } else {
                (v >> low_bits) as u16
            }
        };
        Ok(match offset {
            0x700..=0x77E => self.coefficients[(index - 0x700) / 2],
            0x780..=0x7BE => self.addresses[(index - 0x780) / 2],
            // ST-077 §4.1: unused gap between MADRS and MPRO. Deterministic
            // compatibility read, not a measured bus value.
            0x7C0..=0x7FE => 0,
            0x800..=0xBFE => self.program[(index - 0x800) / 8][(index / 2) & 3],
            0xC00..=0xDFE => part(self.temp[(index - 0xC00) / 4], 8),
            0xE00..=0xE7E => part(self.mems[(index - 0xE00) / 4], 8),
            0xE80..=0xEBE => part(self.mix[(index - 0xE80) / 4], 4),
            0xEC0..=0xEDE => self.effects[(index - 0xEC0) / 2] as u16,
            _ => return Err(Scsp::error(offset)),
        })
    }
    pub fn write(&mut self, offset: u32, value: u16) -> Result<(), BusError> {
        let index = offset as usize;
        let part = |v: &mut i32, low_bits: u32| {
            let mask = (1 << low_bits) - 1;
            *v = if offset & 2 == 0 {
                (*v & !mask) | (i32::from(value) & mask)
            } else {
                (*v & mask) | (i32::from(value) << low_bits)
            };
            *v = signed(i64::from(*v), low_bits + 16);
        };
        match offset {
            0x700..=0x77E => self.coefficients[(index - 0x700) / 2] = value & 0xFFF8,
            0x780..=0x7BE => self.addresses[(index - 0x780) / 2] = value,
            // Initialization sweeps may cross this unused block; no live
            // coefficient/address/program state is selected by these words.
            0x7C0..=0x7FE => (),
            0x800..=0xBFE => {
                self.validated_program.set(None);
                let word = &mut self.program[(index - 0x800) / 8][(index / 2) & 3];
                self.nonzero_program =
                    self.nonzero_program - u16::from(*word != 0) + u16::from(value != 0);
                *word = value;
                let step = (index - 0x800) / 8;
                let bit = 1u64 << (step & 63);
                if self.program[step] == [0; 4] {
                    self.live_steps[step / 64] &= !bit;
                } else {
                    self.live_steps[step / 64] |= bit;
                }
                self.programmed = true;
            }
            0xC00..=0xDFE => {
                let word = &mut self.temp[(index - 0xC00) / 4];
                let before = u16::from(*word != 0);
                part(word, 8);
                self.nonzero_temp = self.nonzero_temp - before + u16::from(*word != 0);
            }
            0xE00..=0xE7E => part(&mut self.mems[(index - 0xE00) / 4], 8),
            0xE80..=0xEBE => part(&mut self.mix[(index - 0xE80) / 4], 4),
            0xEC0..=0xEDE => self.effects[(index - 0xEC0) / 2] = value as i16,
            _ => return Err(Scsp::error(offset)),
        }
        Ok(())
    }
    pub fn add_slot(&mut self, routing: u16, value: i16) {
        let level = routing & 7;
        if level != 0 {
            let index = usize::from((routing >> 3) & 15);
            // A full-level signed PCM16 occupies all 20 MIXS bits. Overflow
            // wraps; ST-077 §4.2 explicitly specifies no overflow protection.
            self.mix[index] = signed(
                i64::from(self.mix[index]) + ((i64::from(value) << (level - 1)) >> 2),
                20,
            );
        }
    }
    /// Zero MPRO is inert only when its TEMP inputs and accumulator are zero
    /// and no delayed memory input remains. Programming zero still starts DSP.
    pub fn quiescent(&self) -> bool {
        !self.programmed
            || (self.nonzero_program == 0
                && self.nonzero_temp == 0
                && self.accumulator == 0
                && self.reads.iter().all(Option::is_none))
    }
    fn zero_accumulator(&self) -> i32 {
        let temp = self.temp[usize::from(self.decrement) & 127];
        signed(
            ((i64::from(temp) * i64::from(self.fraction)) >> 12) + i64::from(temp),
            26,
        )
    }
    /// Zero MPRO still updates ACC and delayed memory input. Only an already
    /// fixed ACC with empty input stages permits omitting intervening zero
    /// words; the next nonzero word must keep its individual deadline.
    pub fn idle_zero_steps(&self, step: usize) -> u32 {
        if !self.programmed {
            return 128;
        }
        if self.reads.iter().any(Option::is_some) || self.accumulator != self.zero_accumulator() {
            return 0;
        }
        if step < 64 {
            let low = self.live_steps[0] >> step;
            if low != 0 {
                return low.trailing_zeros();
            }
            if self.live_steps[1] != 0 {
                return 64 - step as u32 + self.live_steps[1].trailing_zeros();
            }
        } else {
            let high = self.live_steps[1] >> (step - 64);
            if high != 0 {
                return high.trailing_zeros();
            }
        }
        128
    }
    /// With no DMA/program write, these are all possible DSP execution errors.
    /// Validation only widens an independent cache-only CPU horizon: every DSP
    /// instruction, memory grant and PCM sample still executes normally.
    pub fn program_cannot_fault(&self) -> bool {
        if !self.programmed {
            return true;
        }
        if let Some(valid) = self.validated_program.get() {
            return valid;
        }
        let valid = self
            .program
            .iter()
            .enumerate()
            .all(|(step, &[a, b, c, d])| {
                a & 0x8000 == 0
                    && b & 0x1000 == 0
                    && d & 0x180 == 0
                    && (b >> 6) & 63 <= 49
                    && (c & 0x6000 == 0 || step & 1 != 0)
            });
        self.validated_program.set(Some(valid));
        valid
    }
    pub fn memory_access(&self, step: usize) -> bool {
        self.programmed && step & 1 != 0 && self.program[step][2] & 0x6000 != 0
    }
    pub fn begin_sample(&mut self) {
        self.effects.fill(0);
    }
    pub fn step(
        &mut self,
        step: usize,
        ram: &mut [u8],
        config: RamConfig,
        control: u16,
        external: [i16; 2],
    ) -> Result<(), BusError> {
        self.step_recorded(step, ram, config, control, external, None)
    }
    pub fn step_recorded(
        &mut self,
        step: usize,
        ram: &mut [u8],
        config: RamConfig,
        control: u16,
        external: [i16; 2],
        undo: Option<&mut super::checkpoint::RamUndo>,
    ) -> Result<(), BusError> {
        self.step_core::<true>(step, ram, config, control, external, undo)
    }
    fn step_core<const FAST_ZERO: bool>(
        &mut self,
        step: usize,
        ram: &mut [u8],
        config: RamConfig,
        control: u16,
        external: [i16; 2],
        undo: Option<&mut super::checkpoint::RamUndo>,
    ) -> Result<(), BusError> {
        if !self.programmed {
            return Ok(());
        }
        let [a, b, c, d] = self.program[step];
        if FAST_ZERO && [a, b, c, d] == [0; 4] {
            self.accumulator = self.zero_accumulator();
            if let Some(value) = self.reads[(step + 1) & 1].take() {
                self.memory = value;
            }
            return Ok(());
        }
        // MPRO63, MPRO44 and MPRO8:7 are reserved on SCSP.
        if a & 0x8000 != 0 || b & 0x1000 != 0 || d & 0x180 != 0 {
            return Err(Scsp::error(0x800 + step as u32 * 8));
        }
        let tra = usize::from((a >> 8) & 127);
        let temp = self.temp[(tra + usize::from(self.decrement)) & 127];
        let ira = usize::from((b >> 6) & 63);
        let input = match ira {
            0..=31 => self.mems[ira],
            32..=47 => self.mix[ira - 32] << 4,
            48..=49 => i32::from(external[ira - 48]) << 8,
            _ => return Err(Scsp::error(0x802 + step as u32 * 8)),
        };
        let x = if b & 0x8000 != 0 { input } else { temp };
        let y = match (b >> 13) & 3 {
            0 => self.fraction,
            1 => i32::from(self.coefficients[usize::from((d >> 9) & 63)] as i16) >> 3,
            2 => self.y >> 11,
            3 => (self.y >> 4) & 0xFFF,
            _ => unreachable!(),
        };
        let addend = if c & 2 != 0 {
            0
        } else if c & 1 != 0 {
            self.accumulator
        } else {
            temp
        };
        let addend = if c & 4 != 0 {
            -i64::from(addend)
        } else {
            i64::from(addend)
        };
        let memory_address = self.address;
        let shift = (c >> 4) & 3;
        let shifted = i64::from(self.accumulator) << u32::from(matches!(shift, 1 | 2));
        let shifted = if shift < 2 {
            shifted.clamp(-0x800000, 0x7FFFFF) as i32
        } else {
            signed(shifted, 24)
        };
        // The shifter consumes the previous accumulator. Parallel reads
        // precede writes, including TEMP, INPUTS and YRL feedback.
        self.accumulator = signed(((i64::from(x) * i64::from(y)) >> 12) + addend, 26);
        if b & 0x20 != 0 {
            self.mems[usize::from(b & 31)] = self.memory;
        }
        if a & 0x80 != 0 {
            let index = (usize::from(a & 127) + usize::from(self.decrement)) & 127;
            self.nonzero_temp =
                self.nonzero_temp - u16::from(self.temp[index] != 0) + u16::from(shifted != 0);
            self.temp[index] = shifted;
        }
        if c & 0x1000 != 0 {
            let index = usize::from((c >> 8) & 15);
            // ST-228 p.31, "Multiple STR to EFREGxx": only the last
            // store is valid. EFREG is the shifter's output register, not
            // an accumulator; sums must be computed by the DSP datapath.
            self.effects[index] = (shifted >> 8) as i16;
        }
        if c & 8 != 0 {
            self.y = input;
        }
        if c & 0x40 != 0 {
            self.fraction = if shift == 3 {
                shifted & 0xFFF
            } else {
                shifted >> 11
            };
        }
        if c & 0x80 != 0 {
            self.address = if shift == 3 {
                (shifted >> 12) as u16
            } else {
                (input >> 16) as u16
            } & 0xFFF;
        }
        if c & 0x6000 != 0 {
            if step & 1 == 0 {
                return Err(Scsp::error(0x804 + step as u32 * 8));
            }
            let table = c & 0x8000 != 0;
            let mut address = u32::from(self.addresses[usize::from((d >> 2) & 31)]);
            if !table {
                address += u32::from(self.decrement);
            }
            if d & 2 != 0 {
                address += u32::from(memory_address);
            }
            address += u32::from(d & 1);
            let mask = if table {
                0xFFFF
            } else {
                (0x2000 << ((control >> 7) & 3)) - 1
            };
            let index = ((address & mask) + (u32::from(control & 127) << 12)) as usize * 2;
            if c & 0x2000 != 0 {
                let word = config.read16(ram, index);
                self.reads[step & 1] = Some(if d & 0x8000 != 0 {
                    i32::from(word as i16) << 8
                } else {
                    unpack(word)
                });
            }
            if c & 0x4000 != 0 {
                let word = if d & 0x8000 != 0 {
                    (shifted >> 8) as u16
                } else {
                    pack(shifted)
                };
                if let Some(undo) = undo {
                    undo.record(ram, config, index, 2);
                }
                config.write16(ram, index, word);
            }
        }
        // Reads requested on odd step N are available to IWT at N+2.
        if let Some(value) = self.reads[(step + 1) & 1].take() {
            self.memory = value;
        }
        Ok(())
    }
    pub fn end_sample(&mut self) {
        self.decrement = self.decrement.wrapping_sub(1);
        self.mix.fill(0);
    }
    #[cfg(test)]
    pub fn sample(
        &mut self,
        ram: &mut [u8],
        config: RamConfig,
        control: u16,
        external: [i16; 2],
    ) -> Result<(), BusError> {
        self.begin_sample();
        for step in 0..128 {
            self.step(step, ram, config, control, external)?;
        }
        self.end_sample();
        Ok(())
    }
}

#[cfg(test)]
mod clock_tests;
#[cfg(test)]
mod tests;
