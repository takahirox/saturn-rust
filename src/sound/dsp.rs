//! Original SCSP effects processor. ST-077 §4.1 figs.4.5–4.7, §4.3 and
//! §5 fig.5.1 define the buses, RAM widths and circular addressing. ST-228
//! Reference 1/2 describes multiply/add, delayed TEMP and sound-memory routing.
//! This is a sample-clock functional interpreter, not a DRAM contention model.
use super::{BusError, SOUND_RAM_SIZE, Scsp};

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
            0x800..=0xBFE => {
                self.program[(index - 0x800) / 8][(index / 2) & 3] = value;
                self.programmed = true;
            }
            0xC00..=0xDFE => part(&mut self.temp[(index - 0xC00) / 4], 8),
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
    pub fn sample(
        &mut self,
        ram: &mut [u8],
        control: u16,
        external: [i16; 2],
    ) -> Result<(), BusError> {
        self.effects.fill(0);
        if self.programmed {
            for step in 0..128 {
                let [a, b, c, d] = self.program[step];
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
                    self.temp[index] = shifted;
                }
                if c & 0x1000 != 0 {
                    let index = usize::from((c >> 8) & 15);
                    self.effects[index] = self.effects[index].wrapping_add((shifted >> 8) as i16);
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
                    let index = (((address & mask) + (u32::from(control & 127) << 12)) as usize
                        * 2)
                        & (SOUND_RAM_SIZE - 1);
                    if c & 0x2000 != 0 {
                        let word = u16::from_be_bytes([ram[index], ram[index + 1]]);
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
                        ram[index..index + 2].copy_from_slice(&word.to_be_bytes());
                    }
                }
                // Reads requested on odd step N are available to IWT at N+2.
                if let Some(value) = self.reads[(step + 1) & 1].take() {
                    self.memory = value;
                }
            }
        }
        self.decrement = self.decrement.wrapping_sub(1);
        self.mix.fill(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
