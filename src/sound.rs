//! Shared SCSP/sound-CPU bus and independent audio clock domain.
//! Primary references: ST-077 §§1.2, 3, 4.2; M68000PRM §§2–4.
//! PCM, slot modulation/LFO/noise and the effects processor execute here.
//! Unimplemented envelope rates still fail explicitly rather than acting as no-ops.
mod dsp;
mod synthesis;
use crate::bus::{BusError, Memory};
use crate::m68k::M68k;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

pub const SOUND_RAM: u32 = 0x05A0_0000;
pub const SCSP: u32 = 0x05B0_0000;
pub const SOUND_RAM_SIZE: usize = 512 * 1024;
pub const SAMPLE_RATE: u64 = 44_100;
const REFERENCE_HZ: u64 = 26_874_100;
const SOUND_CPU_HZ: u64 = 11_289_600;
const BUFFER_FRAMES: usize = 8192;

fn mix_sample(mix: &mut [i64; 2], sample: i16, send: u16, pan: u16) {
    const PAN: [i64; 16] = [
        32768, 23170, 16384, 11585, 8192, 5793, 4096, 2896, 2048, 1448, 1024, 724, 512, 362, 256, 0,
    ];
    if send == 0 {
        return;
    }
    let level = (i64::from(sample) << (send - 1)) / 64;
    mix[0] += level
        * if pan < 16 {
            PAN[usize::from(pan)]
        } else {
            32768
        }
        / 32768;
    mix[1] += level
        * if pan >= 16 {
            PAN[usize::from(pan - 16)]
        } else {
            32768
        }
        / 32768;
}

#[derive(Default, Clone, Copy)]
struct Voice {
    active: bool,
    phase: i64,
    reverse: bool,
}
pub struct Scsp {
    dsp: dsp::Dsp,
    slots: [[u16; 12]; 32],
    voices: [Voice; 32],
    common: [u16; 24],
    timer_phase: [u16; 3],
    pending: Cell<u16>,
    main_pending: Cell<u16>,
    midi_in: RefCell<VecDeque<u8>>,
    midi_overflow: Cell<bool>,
    midi_out: RefCell<VecDeque<u8>>,
    midi_sent: RefCell<VecDeque<u8>>,
    midi_phase: u64,
    stack: synthesis::Stack,
    lfo: [synthesis::Lfo; 32],
    dma_remaining: u16,
    external: [i16; 2],
    pub generated: u64,
    pub dropped: u64,
    output: VecDeque<i16>,
}
impl Default for Scsp {
    fn default() -> Self {
        Self {
            dsp: dsp::Dsp::default(),
            slots: [[0; 12]; 32],
            voices: [Voice::default(); 32],
            common: [0; 24],
            timer_phase: [0; 3],
            pending: Cell::new(0),
            main_pending: Cell::new(0),
            midi_in: RefCell::new(VecDeque::new()),
            midi_overflow: Cell::new(false),
            midi_out: RefCell::new(VecDeque::new()),
            midi_sent: RefCell::new(VecDeque::new()),
            midi_phase: 0,
            stack: synthesis::Stack::default(),
            lfo: std::array::from_fn(synthesis::Lfo::new),
            dma_remaining: 0,
            external: [0; 2],
            generated: 0,
            dropped: 0,
            output: VecDeque::with_capacity(BUFFER_FRAMES * 2),
        }
    }
}
impl Scsp {
    fn error(address: u32) -> BusError {
        BusError::UnsupportedDevice {
            address: SCSP + address,
            width: 2,
        }
    }
    pub fn read16(&self, offset: u32) -> Result<u16, BusError> {
        if offset & 1 != 0 {
            return Err(BusError::Unaligned {
                address: SCSP + offset,
                width: 2,
            });
        }
        match offset {
            0..=0x3FF if offset & 31 < 24 => {
                Ok(self.slots[(offset / 32) as usize][(offset as usize & 31) / 2])
            }
            0x400 => Ok(0), // MEM4MB/MVOL/DAC18B are write-only (ST-077 §4.2).
            0x402 => Ok(0), // RBL/RBP are write-only, ST-077 §4.3.
            0x404 => {
                let flags = self.midi_status();
                let mut fifo = self.midi_in.borrow_mut();
                let byte = fifo.pop_front().unwrap_or(0);
                if fifo.is_empty() {
                    self.pending.set(self.pending.get() & !8);
                    self.main_pending.set(self.main_pending.get() & !8);
                }
                Ok(flags | u16::from(byte))
            }
            0x406 => Ok(0), // MIDI output is write-only.
            0x408 => {
                let slot = ((self.common[4] >> 11) & 31) as usize;
                Ok((self.voices[slot].phase.max(0) as u64 >> 28) as u16 & 15)
            }
            0x412..=0x416 => Ok(if offset == 0x416 {
                self.common[11] & 0x7000
            } else {
                0
            }),
            0x418..=0x41C => Ok(self.common[((offset - 0x400) / 2) as usize] & 255),
            0x41E | 0x42A => Ok(self.common[((offset - 0x400) / 2) as usize]),
            0x420 => Ok(self.pending.get()),
            0x42C => Ok(self.main_pending.get()),
            0x422 | 0x424 | 0x426 | 0x428 | 0x42E => Ok(0),
            0x600..=0x67E => Ok(self.stack.data[((offset - 0x600) / 2) as usize] as u16),
            0x700..=0xEDE => self.dsp.read(offset),
            _ => Err(Self::error(offset)),
        }
    }
    pub fn write16(&mut self, offset: u32, value: u16) -> Result<(), BusError> {
        if offset & 1 != 0 {
            return Err(BusError::Unaligned {
                address: SCSP + offset,
                width: 2,
            });
        }
        match offset {
            0..=0x3FF if offset & 31 < 24 => {
                let slot = (offset / 32) as usize;
                let index = (offset as usize & 31) / 2;
                // Zero-rate attack and EGHOLD are defined; timed AR/D1R/D2R/
                // RR/KRS/LPSLNK still require independent rate evidence.
                if (index == 4 && value & !0x20 != 0)
                    || (index == 5 && value != 0)
                    || (index == 0 && (value >> 7) & 3 == 3)
                {
                    return Err(Self::error(offset));
                }
                let mask = match index {
                    0 => 0x0FFF,
                    5 => 0x7FFF,
                    6 => 0x03FF,
                    8 => 0x7BFF,
                    10 => 0x007F,
                    11 if slot >= 18 => 0xFF00, // No EFREG/EXTS output on these slots.
                    _ => u16::MAX,
                };
                self.slots[slot][index] = value & mask;
                if index == 9 && value & 0x8000 != 0 {
                    self.lfo[slot].reset();
                }
                if index == 0 && value & 0x1000 != 0 {
                    for i in 0..32 {
                        if self.slots[i][0] & 0x800 != 0 {
                            if !self.voices[i].active {
                                self.voices[i] = Voice {
                                    active: true,
                                    phase: 0,
                                    reverse: false,
                                };
                            }
                        } else {
                            self.voices[i].active = false;
                        }
                    }
                }
            }
            0x400 => {
                if value & !0x20F != 0 {
                    return Err(Self::error(offset));
                }
                self.common[0] = value;
            }
            0x402 => self.common[1] = value & 0x1FF,
            0x406 => {
                let fifo = self.midi_out.get_mut();
                if fifo.len() == 4 {
                    return Err(Self::error(offset));
                }
                fifo.push_back(value as u8);
                self.pending.set(self.pending.get() & !0x200);
                self.main_pending.set(self.main_pending.get() & !0x200);
            }
            0x408 => self.common[4] = value & 0xF800,
            0x412 => self.common[9] = value & 0xFFFE,
            0x414 => self.common[10] = value & 0xFFFE,
            0x416 => {
                self.common[11] = value & 0x7FFE;
                if value & 0x1000 != 0 {
                    self.dma_remaining = value & 0xFFE;
                }
            }
            0x418..=0x41C => {
                let timer = ((offset - 0x418) / 2) as usize;
                self.common[12 + timer] = value & 0x7FF;
                self.timer_phase[timer] = 0;
            }
            0x41E | 0x42A => self.common[((offset - 0x400) / 2) as usize] = value & 0x7FF,
            0x420 => self.pending.set(self.pending.get() | (value & 0x20)),
            0x42C => self
                .main_pending
                .set(self.main_pending.get() | (value & 0x20)),
            0x422 => self.pending.set(self.pending.get() & !value),
            0x42E => self.main_pending.set(self.main_pending.get() & !value),
            0x424 | 0x426 | 0x428 => self.common[((offset - 0x400) / 2) as usize] = value & 255,
            0x600..=0x67E => self.stack.data[((offset - 0x600) / 2) as usize] = value as i16,
            0x700..=0xEDE => self.dsp.write(offset, value)?,
            _ => return Err(Self::error(offset)),
        }
        Ok(())
    }
    fn request(&self, bit: u16) {
        self.pending.set(self.pending.get() | bit);
        self.main_pending.set(self.main_pending.get() | bit);
    }
    pub fn interrupt_level(&self) -> u8 {
        let sources = self.pending.get() & self.common[15];
        (0..11)
            .filter(|b| sources & (1 << b) != 0)
            .map(|b| {
                let bit = 1 << b.min(7);
                u8::from(self.common[18] & bit != 0)
                    | (u8::from(self.common[19] & bit != 0) << 1)
                    | (u8::from(self.common[20] & bit != 0) << 2)
            })
            .max()
            .unwrap_or(0)
    }
    pub fn main_interrupt(&self) -> bool {
        self.main_pending.get() & self.common[21] != 0
    }
    pub fn receive_midi(&self, byte: u8) -> bool {
        let mut fifo = self.midi_in.borrow_mut();
        if fifo.len() == 4 {
            self.midi_overflow.set(true);
            return false;
        }
        let empty = fifo.is_empty();
        fifo.push_back(byte);
        if empty {
            self.request(8);
        }
        true
    }
    /// Completed serial characters, never a host-driven FIFO drain. The
    /// standard console leaves these pins unconnected (ST-077 fig.4.59).
    pub fn take_midi_output(&self) -> Option<u8> {
        self.midi_sent.borrow_mut().pop_front()
    }
    fn midi_clocks_to_event(&self) -> u32 {
        if self.midi_out.borrow().is_empty() {
            u32::MAX
        } else {
            (REFERENCE_HZ * 10 - self.midi_phase).div_ceil(31_250) as u32
        }
    }
    fn advance_midi(&mut self, clocks: u32) {
        if self.midi_out.get_mut().is_empty() {
            return;
        }
        // MIDI 8-N-1: one byte occupies ten serial bits at 31.25 kbit/s.
        // Carry the rational phase across characters, including native/Wasm.
        self.midi_phase += u64::from(clocks) * 31_250;
        while self.midi_phase >= REFERENCE_HZ * 10 {
            self.midi_phase -= REFERENCE_HZ * 10;
            if let Some(byte) = self.midi_out.get_mut().pop_front() {
                let sent = self.midi_sent.get_mut();
                if sent.len() == 256 {
                    sent.pop_front();
                }
                sent.push_back(byte);
            }
            if self.midi_out.get_mut().is_empty() {
                self.midi_phase = 0;
                self.request(0x200);
                break;
            }
        }
    }
    fn dma(&mut self, ram: &mut [u8]) -> Result<(), BusError> {
        if self.dma_remaining == 0 {
            self.common[11] &= !0x1000;
            return Ok(());
        }
        let address = ((usize::from(self.common[10] & 0xF000) << 4) | usize::from(self.common[9]))
            & (SOUND_RAM_SIZE - 1);
        let reg = u32::from(self.common[10] & 0xFFE);
        if (0x412..=0x416).contains(&reg) {
            return Err(Self::error(reg));
        }
        let gate = self.common[11] & 0x4000 != 0;
        if self.common[11] & 0x2000 != 0 {
            let value = if gate { 0 } else { self.read16(reg)? };
            ram[address..address + 2].copy_from_slice(&value.to_be_bytes());
        } else {
            let value = if gate {
                0
            } else {
                u16::from_be_bytes([ram[address], ram[address + 1]])
            };
            self.write16(reg, value)?;
        }
        let next = (address + 2) & (SOUND_RAM_SIZE - 1);
        self.common[9] = next as u16;
        self.common[10] = ((next >> 4) as u16 & 0xF000) | ((reg as u16 + 2) & 0xFFE);
        self.dma_remaining -= 2;
        if self.dma_remaining == 0 {
            self.common[11] &= !0x1000;
            self.request(16);
        }
        Ok(())
    }
    /// One stereo 44.1 kHz frame. Integer Q16 phase and fixed gain constants make
    /// native and Wasm output identical. Direct mode bypasses envelope/TL/LFO.
    fn sample(&mut self, ram: &mut [u8]) -> Result<(), BusError> {
        self.dma(ram)?;
        for timer in 0..3 {
            let index = 12 + timer;
            let prescale = 1 << ((self.common[index] >> 8) & 7);
            self.timer_phase[timer] += 1;
            if self.timer_phase[timer] >= prescale {
                self.timer_phase[timer] = 0;
                let count = self.common[index] & 255;
                if count != 255 {
                    self.common[index] = (self.common[index] & !255) | (count + 1);
                }
                if count == 254 {
                    self.request(1 << (6 + timer));
                }
            }
        }
        self.request(0x400);
        let mut mix = [0i64; 2];
        for i in 0..32 {
            let regs = self.slots[i];
            self.stack.begin_slot(i);
            let lfo = self.lfo[i];
            self.lfo[i].advance(regs[9]);
            self.stack.write(i, 0, regs[6] & 0x200 != 0);
            if !self.voices[i].active {
                continue;
            }
            let voice = &mut self.voices[i];
            let start = (usize::from(regs[0] & 15) << 16) | usize::from(regs[1]);
            let lsa = i64::from(regs[2]) << 16;
            let lea = i64::from(regs[3]) << 16;
            if lea == 0 {
                voice.active = false;
                continue;
            }
            let looping = (regs[0] >> 5) & 3;
            let span = lea - lsa;
            if looping != 0 && span <= 0 {
                voice.active = false;
                continue;
            }
            // ST-077 figure 4.10: reverse mode skips from the attack's LSA
            // crossing to the loop end; alternative mode first traverses forward.
            if looping == 2 && !voice.reverse && voice.phase >= lsa {
                voice.phase = lea - 1 - (voice.phase - lsa);
                voice.reverse = true;
            }
            if voice.phase >= lea {
                match looping {
                    0 => {
                        voice.active = false;
                        self.slots[i][0] &= !0x800;
                        continue;
                    }
                    1 => voice.phase = lsa + (voice.phase - lsa).rem_euclid(span),
                    3 => {
                        let n = (voice.phase - lea).rem_euclid(2 * span);
                        voice.reverse = n < span;
                        voice.phase = if voice.reverse {
                            lea - 1 - n
                        } else {
                            lsa + n - span
                        };
                    }
                    _ => unreachable!(),
                }
            }
            if voice.reverse && voice.phase < lsa {
                if looping == 2 {
                    voice.phase = lea - 1 - (lsa - 1 - voice.phase).rem_euclid(span);
                } else {
                    let n = (lsa - 1 - voice.phase).rem_euclid(2 * span);
                    voice.reverse = n >= span;
                    voice.phase = if voice.reverse {
                        lea - 1 - (n - span)
                    } else {
                        lsa + n
                    };
                }
            }
            let pcm8 = regs[0] & 16 != 0;
            // ST-077 §§2.2/3.1, fig.4.30 OP4: first-order interpolation
            // follows waveform reads. Interpolate decoded signed samples, not
            // addresses or mixed output. Q16 arithmetic rounds towards -infinity.
            let shift = self.stack.displacement(i, regs[7]);
            let position = voice.phase + shift;
            let index = position >> 16;
            // A reverse phase names the upper edge of its sample interval.
            // Measure interpolation distance from that edge toward the previous
            // sample, wrapping only reverse loops and holding ping-pong ends.
            let fraction = if voice.reverse {
                0xFFFF - (position & 0xFFFF)
            } else {
                position & 0xFFFF
            };
            let next = if shift != 0 {
                index + if voice.reverse { -1 } else { 1 }
            } else if voice.reverse {
                if index <= i64::from(regs[2]) {
                    if looping == 2 {
                        i64::from(regs[3]) - 1
                    } else {
                        index
                    }
                } else {
                    index - 1
                }
            } else if index + 1 >= i64::from(regs[3]) {
                if looping == 1 {
                    i64::from(regs[2])
                } else {
                    index
                }
            } else {
                index + 1
            };
            let decode = |index: i64| {
                let offset = (start as i64 + index * if pcm8 { 1 } else { 2 })
                    .rem_euclid(SOUND_RAM_SIZE as i64) as usize;
                let mut value = if pcm8 {
                    i16::from(ram[offset] as i8) << 8
                } else {
                    i16::from_be_bytes([ram[offset & !1], ram[(offset & !1) + 1]])
                };
                if regs[0] & 0x200 != 0 {
                    value ^= 0x7FFF;
                }
                if regs[0] & 0x400 != 0 {
                    value ^= i16::MIN;
                }
                i64::from(value)
            };
            let mut value = match (regs[0] >> 7) & 3 {
                0 => {
                    let a = decode(index);
                    ((a * (65536 - fraction) + decode(next) * fraction) >> 16) as i16
                }
                source @ (1 | 2) => {
                    let mut value = if source == 1 { lfo.noise() } else { 0 };
                    if regs[0] & 0x200 != 0 {
                        value ^= 0x7FFF;
                    }
                    if regs[0] & 0x400 != 0 {
                        value ^= i16::MIN;
                    }
                    value
                }
                _ => unreachable!("reserved SSCTL is rejected on write"),
            };
            if regs[6] & 0x100 == 0 {
                value = if regs[4] & 0x20 != 0 {
                    synthesis::attenuate(value, regs[6] as u8, lfo.attenuation(regs[9]))
                } else {
                    0 // AR=0 never leaves the initial, fully attenuated attack.
                };
            }
            self.stack.write(i, value, regs[6] & 0x200 != 0);
            self.dsp.add_slot(regs[10], value);
            let send = (regs[11] >> 13) & 7;
            let pan = ((regs[11] >> 8) & 31) as usize;
            let level = if send == 0 {
                0
            } else {
                i64::from(value) << (send - 1)
            } / 64;
            // ST-077 table 4.28: 3 dB steps, ±infinity at extreme pan.
            const PAN: [i64; 16] = [
                32768, 23170, 16384, 11585, 8192, 5793, 4096, 2896, 2048, 1448, 1024, 724, 512,
                362, 256, 0,
            ];
            mix[0] += level * if pan < 16 { PAN[pan] } else { 32768 } / 32768;
            mix[1] += level * if pan >= 16 { PAN[pan - 16] } else { 32768 } / 32768;
            let oct = ((regs[8] >> 11) & 15) as i8;
            let oct = if oct >= 8 { oct - 16 } else { oct };
            let mut step = i64::from(1024 + (regs[8] & 1023)) << 6;
            if oct >= 0 {
                step <<= oct;
            } else {
                step >>= -oct;
            }
            step = lfo.step(regs[9], step);
            voice.phase += if voice.reverse { -step } else { step };
        }
        self.stack.end_frame();
        self.dsp.sample(ram, self.common[1], self.external)?;
        for (channel, sample) in self.dsp.effects.into_iter().enumerate() {
            let routing = self.slots[channel][11];
            let send = (routing >> 5) & 7;
            let pan = routing & 31;
            mix_sample(&mut mix, sample, send, pan);
        }
        // ST-077 table 4.31: EXTS0/1 use slot 16/17 EFSDL/EFPAN.
        for (channel, sample) in self.external.into_iter().enumerate() {
            let routing = self.slots[16 + channel][11];
            let send = (routing >> 5) & 7;
            let pan = routing & 31;
            let level = if send == 0 {
                0
            } else {
                i64::from(sample) << (send - 1)
            } / 64;
            const PAN: [i64; 16] = [
                32768, 23170, 16384, 11585, 8192, 5793, 4096, 2896, 2048, 1448, 1024, 724, 512,
                362, 256, 0,
            ];
            mix[0] += level * if pan < 16 { PAN[pan as usize] } else { 32768 } / 32768;
            mix[1] += level
                * if pan >= 16 {
                    PAN[(pan - 16) as usize]
                } else {
                    32768
                }
                / 32768;
        }
        if self.output.len() >= BUFFER_FRAMES * 2 {
            self.output.pop_front();
            self.output.pop_front();
            self.dropped += 1;
        }
        // MVOL is a four-bit linear DAC output scale; clipping precedes MVOL.
        let volume = i64::from(self.common[0] & 15);
        for value in mix {
            self.output
                .push_back((value.clamp(-32768, 32767) * volume / 15) as i16);
        }
        self.generated += 1;
        Ok(())
    }
    pub fn drain(&mut self, frames: usize) -> Vec<i16> {
        let count = frames
            .min(BUFFER_FRAMES)
            .saturating_mul(2)
            .min(self.output.len());
        self.output.drain(..count).collect()
    }
}

pub struct Sound {
    pub cpu: M68k,
    pub ram: Vec<u8>,
    pub scsp: Scsp,
    pub enabled: bool,
    time: u64,
    cpu_ticks: u64,
    cpu_origin: u64,
    next_cpu: u64,
    next_sample: u64,
}
impl Default for Sound {
    fn default() -> Self {
        Self {
            cpu: M68k::default(),
            ram: vec![0; SOUND_RAM_SIZE],
            scsp: Scsp::default(),
            enabled: false,
            time: 0,
            cpu_ticks: 0,
            cpu_origin: 0,
            next_cpu: u64::MAX,
            next_sample: REFERENCE_HZ.div_ceil(SAMPLE_RATE),
        }
    }
}
struct SoundBus<'a> {
    ram: &'a mut Vec<u8>,
    scsp: &'a mut Scsp,
}
impl Memory for SoundBus<'_> {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address < SOUND_RAM_SIZE as u32 {
            return Ok(self.ram[address as usize]);
        }
        if (0x100000..0x101000).contains(&address) {
            return self.scsp.read8(address - 0x100000);
        }
        Err(BusError::Unmapped { address, width: 1 })
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if (0x100000..0x101000).contains(&address) {
            return self.scsp.read16(address - 0x100000);
        }
        Ok(u16::from(self.read8(address)?) << 8 | u16::from(self.read8(address + 1)?))
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        Ok(u32::from(self.read16(address)?) << 16 | u32::from(self.read16(address + 2)?))
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address < SOUND_RAM_SIZE as u32 {
            self.ram[address as usize] = value;
            return Ok(());
        }
        if (0x100000..0x101000).contains(&address) {
            // Byte lane updates preserve the internal register latch, not a read
            // of write-only/status registers (which could pop MIDI or start DMA).
            return self.scsp.write8(address - 0x100000, value);
        }
        Err(BusError::Unmapped { address, width: 1 })
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if (0x100000..0x101000).contains(&address) {
            return self.scsp.write16(address - 0x100000, value);
        }
        self.write8(address, (value >> 8) as u8)?;
        self.write8(address + 1, value as u8)
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.write16(address, (value >> 16) as u16)?;
        self.write16(address + 2, value as u16)
    }
}
impl Scsp {
    fn midi_status(&self) -> u16 {
        let input = self.midi_in.borrow();
        let output = self.midi_out.borrow();
        u16::from(output.len() == 4) << 12
            | u16::from(output.is_empty()) << 11
            | u16::from(self.midi_overflow.get()) << 10
            | u16::from(input.len() == 4) << 9
            | u16::from(input.is_empty()) << 8
    }
    pub fn read8(&self, offset: u32) -> Result<u8, BusError> {
        if offset == 0x404 {
            return Ok((self.midi_status() >> 8) as u8);
        }
        let word = self.read16(offset & !1)?;
        Ok(if offset & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        })
    }
    pub fn write8(&mut self, offset: u32, value: u8) -> Result<(), BusError> {
        if offset == 0x406 {
            return Ok(());
        } // MIDI data is on the low byte lane.
        let aligned = offset & !1;
        let old = if offset < 0x400 && offset & 31 < 24 {
            self.slots[(offset / 32) as usize][(offset as usize & 31) / 2]
        } else if (0x400..0x430).contains(&offset) {
            self.common[((offset - 0x400) / 2) as usize]
        } else if (0x600..0xEE0).contains(&offset) {
            self.read16(aligned)?
        } else {
            return Err(Self::error(offset));
        };
        let word = if offset & 1 == 0 {
            (old & 255) | (u16::from(value) << 8)
        } else {
            (old & 0xFF00) | u16::from(value)
        };
        self.write16(aligned, word)
    }
}
impl Sound {
    pub(crate) fn clocks_to_sample(&self) -> u32 {
        (self.next_sample - self.time).min(u64::from(u32::MAX)) as u32
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), BusError> {
        if enabled && !self.enabled {
            let memory = SoundBus {
                ram: &mut self.ram,
                scsp: &mut self.scsp,
            };
            self.cpu.reset(&memory)?;
            self.cpu_ticks = 0;
            self.cpu_origin = self.time;
            self.next_cpu = self.time;
        } else if !enabled {
            self.cpu = M68k::default();
            self.next_cpu = u64::MAX;
        }
        self.enabled = enabled;
        Ok(())
    }
    pub fn advance(&mut self, clocks: u32) -> Result<(), BusError> {
        self.advance_domain(clocks, None)
    }
    pub fn advance_with_cd(
        &mut self,
        clocks: u32,
        cd: &mut crate::cd::CdBlock,
    ) -> Result<(), BusError> {
        self.advance_domain(clocks, Some(cd))
    }
    fn advance_domain(
        &mut self,
        clocks: u32,
        mut cd: Option<&mut crate::cd::CdBlock>,
    ) -> Result<(), BusError> {
        let target = self.time + u64::from(clocks);
        let mut now = self.time;
        loop {
            let next_midi = now + u64::from(self.scsp.midi_clocks_to_event());
            let deadline = self.next_cpu.min(self.next_sample).min(next_midi);
            if deadline > target {
                break;
            }
            self.scsp.advance_midi((deadline - now) as u32);
            now = deadline;
            if deadline == self.next_cpu {
                self.cpu.set_interrupt(self.scsp.interrupt_level());
                let mut memory = SoundBus {
                    ram: &mut self.ram,
                    scsp: &mut self.scsp,
                };
                let clocks = self.cpu.step(&mut memory);
                if std::mem::take(&mut self.cpu.reset_output) {
                    let generated = self.scsp.generated;
                    let dropped = self.scsp.dropped;
                    self.scsp = Scsp {
                        generated,
                        dropped,
                        ..Scsp::default()
                    };
                }
                self.cpu_ticks += u64::from(clocks);
                self.next_cpu =
                    self.cpu_origin + (self.cpu_ticks * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
            } else if deadline == self.next_sample {
                self.scsp.external = cd.as_mut().map_or([0; 2], |cd| cd.audio_sample());
                self.scsp.sample(&mut self.ram)?;
                self.next_sample = ((self.scsp.generated + 1) * REFERENCE_HZ).div_ceil(SAMPLE_RATE);
            }
        }
        self.scsp.advance_midi((target - now) as u32);
        self.time = target;
        Ok(())
    }
}
#[cfg(test)]
mod tests;
