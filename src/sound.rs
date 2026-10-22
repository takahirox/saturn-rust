//! Shared SCSP/sound-CPU bus and independent audio clock domain.
//! Primary references: ST-077 §§1.2, 3, 4.2; M68000PRM §§2–4.
//! PCM, slot modulation/LFO/noise and the effects processor execute here.
mod dma;
mod dsp;
mod envelope;
mod pcm;
mod synthesis;
mod transactions;
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
const DSP_HZ: u64 = SAMPLE_RATE * 128;
const BUFFER_FRAMES: usize = 8192;

/// ST-077 §4.2, table 4.32 / fig.4.58: MEM4MB changes the DRAM
/// geometry, including the chip-select bit. The standard board installs only
/// the first chip. Unconnected expansion-chip data pins read high.
#[derive(Clone, Copy)]
struct RamConfig {
    four_mbit: bool,
}
impl RamConfig {
    fn offset(self, address: usize) -> Option<usize> {
        let capacity = if self.four_mbit { 0x80000 } else { 0x20000 };
        let pins = address & (capacity * 2 - 1);
        (pins < capacity).then_some(pins)
    }
    fn read8(self, ram: &[u8], address: usize) -> u8 {
        self.offset(address).map_or(255, |index| ram[index])
    }
    fn read16(self, ram: &[u8], address: usize) -> u16 {
        u16::from_be_bytes([self.read8(ram, address), self.read8(ram, address + 1)])
    }
    fn write8(self, ram: &mut [u8], address: usize, value: u8) {
        if let Some(index) = self.offset(address) {
            ram[index] = value;
        }
    }
    fn write16(self, ram: &mut [u8], address: usize, value: u16) {
        let [high, low] = value.to_be_bytes();
        self.write8(ram, address, high);
        self.write8(ram, address + 1, low);
    }
}

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
    /// ST-077 fig.4.3 / §4.2 p.89: current output address, in 4K samples
    /// relative to SA. Keep the sampled address, not the next phase accumulator.
    call_address: u16,
    reverse: bool,
    envelope: envelope::Envelope,
}
pub struct Scsp {
    reset_held: bool,
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
    dma_word: Option<dma::Word>,
    pcm_slot: Option<pcm::Slot>,
    pcm_word: Option<pcm::Word>,
    pcm_clocked: bool,
    refresh_pending: u32,
    external: [i16; 2],
    frame_mix: [i64; 2],
    frame_active: bool,
    pub generated: u64,
    pub dropped: u64,
    output: VecDeque<i16>,
}
impl Default for Scsp {
    fn default() -> Self {
        Self {
            reset_held: false,
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
            dma_word: None,
            pcm_slot: None,
            pcm_word: None,
            pcm_clocked: false,
            refresh_pending: 0,
            external: [0; 2],
            frame_mix: [0; 2],
            frame_active: false,
            generated: 0,
            dropped: 0,
            output: VecDeque::with_capacity(BUFFER_FRAMES * 2),
        }
    }
}
impl Scsp {
    fn set_reset(&mut self, held: bool) {
        // Retain elapsed time and already-produced host audio. RESET affects
        // the device, not sound RAM or samples from before the signal edge.
        let output = std::mem::take(&mut self.output);
        *self = Self {
            reset_held: held,
            generated: self.generated,
            dropped: self.dropped,
            output,
            ..Self::default()
        };
    }
    fn ram_config(&self) -> RamConfig {
        RamConfig {
            four_mbit: self.common[0] & 0x200 != 0,
        }
    }
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
                Ok(self.voices[slot].call_address << 7)
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
        if self.reset_held {
            return Ok(());
        }
        match offset {
            0..=0x3FF if offset & 31 < 24 => {
                let slot = (offset / 32) as usize;
                let index = (offset as usize & 31) / 2;
                if index == 0 && (value >> 7) & 3 == 3 {
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
                            if !self.voices[i].active || self.voices[i].envelope.releasing() {
                                self.voices[i] = Voice {
                                    active: true,
                                    phase: 0,
                                    call_address: 0,
                                    reverse: false,
                                    envelope: envelope::Envelope::default(),
                                };
                            }
                        } else {
                            self.voices[i]
                                .envelope
                                .release(self.slots[i][4] & 0x20 != 0);
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
                // ST-077 §4.2, table4.40: writing DEXE=0 has no effect
                // on execution. Only completion clears a running transfer.
                let executing = self.common[11] & 0x1000;
                self.common[11] = (value & 0x6FFE) | executing;
                if value & 0x1000 != 0 {
                    self.common[11] |= 0x1000;
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
        if self.reset_held {
            return false;
        }
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
    /// Frame edges reset output/effects; PCM operands are captured only by
    /// their individually granted DRAM reads (ST-077 §3.2/fig.3.2).
    fn begin_sample(&mut self) {
        self.frame_active = !self.reset_held;
        self.frame_mix = [0; 2];
        self.pcm_clocked = self.frame_active && self.voices.iter().any(|v| v.active);
        if !self.frame_active {
            return;
        }
        // An idle generator still clocks its noise/LFO/stack. It need not
        // wake the scheduler for each empty slot. KEYON becomes visible at
        // the next frame when the whole generator was idle at this edge.
        if !self.pcm_clocked {
            for i in 0..32 {
                self.begin_slot(i);
            }
        }
        self.dsp.begin_sample();
    }
    fn dsp_step(&mut self, step: usize, ram: &mut [u8]) -> Result<(), BusError> {
        if self.frame_active {
            self.dsp
                .step(step, ram, self.ram_config(), self.common[1], self.external)?;
        }
        Ok(())
    }
    fn end_sample(&mut self) {
        self.stack.end_frame();
        if !self.frame_active {
            self.push_output([0; 2]);
            return;
        }
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
        let mut mix = self.frame_mix;
        self.dsp.end_sample();
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
        self.push_output(mix);
        self.frame_active = false;
    }
    fn push_output(&mut self, mix: [i64; 2]) {
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
    }
    #[cfg(test)]
    fn sample(&mut self, ram: &mut [u8]) -> Result<(), BusError> {
        for step in 0..128 {
            self.sample_cycle(step, ram)?;
        }
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
    ram_busy_until: u64,
    main_ram_requested: bool,
    scsp_tick: u64,
    reset_output_start: Option<u64>,
    reset_output_end: Option<u64>,
    transactions: transactions::Transactions,
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
            ram_busy_until: 0,
            main_ram_requested: false,
            scsp_tick: 0,
            reset_output_start: None,
            reset_output_end: None,
            transactions: transactions::Transactions::default(),
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
            return Ok(self.scsp.ram_config().read8(self.ram, address as usize));
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
            self.scsp
                .ram_config()
                .write8(self.ram, address as usize, value);
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
    pub(crate) fn main_ram_request(&mut self, pending: bool) {
        self.main_ram_requested = pending;
    }
    /// Main CPU/SCU and sound CPU use the same SCSP DRAM controller.
    pub fn read_ram8(&self, address: usize) -> u8 {
        self.scsp.ram_config().read8(&self.ram, address)
    }
    pub fn write_ram8(&mut self, address: usize, value: u8) {
        self.scsp.ram_config().write8(&mut self.ram, address, value);
    }
    /// ST-077 §3.2/fig3.2: PCM, DSP, refresh and internal DMA precede the
    /// main and sound CPU. Each owns one of the 128 DRAM cycles/Fs.
    pub(crate) fn ram_busy(&self) -> bool {
        self.time < self.ram_busy_until
    }
    fn scsp_deadline(&self, now: u64) -> u64 {
        let current = now * DSP_HZ / REFERENCE_HZ;
        // A higher-priority reset edge may share the current DSP deadline.
        // Retain that unprocessed edge until its frame work has also run.
        let mut tick = current
            + u64::from(
                current <= self.scsp_tick || (current * REFERENCE_HZ).div_ceil(DSP_HZ) != now,
            );
        // An untouched DSP executes no instructions. Keep source-capture and
        // output edges, skipping only empty intervening execution slots. A live
        // MPRO write immediately exposes the next slot at the current phase.
        if !self.scsp.dsp.programmed()
            && !self.scsp.dma_active()
            && !self.scsp.pcm_clocked
            && self.scsp.refresh_pending == 0
        {
            // Frame edges, two refresh requests and output remain clocked
            // even when no effects program or internal DMA is running.
            let frame = (tick - 1) & !127;
            let phase = tick - frame;
            tick = frame
                + [1, 125, 127, 128]
                    .into_iter()
                    .find(|&t| t >= phase)
                    .unwrap();
        }
        (tick * REFERENCE_HZ).div_ceil(DSP_HZ)
    }
    pub(crate) fn clocks_to_event(&self) -> u32 {
        (self
            .scsp_deadline(self.time)
            .min(self.next_cpu)
            .min(self.reset_output_start.unwrap_or(u64::MAX))
            .min(self.reset_output_end.unwrap_or(u64::MAX))
            - self.time)
            .min(u64::from(self.scsp.midi_clocks_to_event())) as u32
    }
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), BusError> {
        if enabled && !self.enabled {
            self.cpu.assert_reset();
            self.transactions = transactions::Transactions::reset();
            // MC68000UM §5.5 fig.5-30/table8-14: sixteen internal clocks,
            // then six distinct four-clock reads (SSP, PC, two prefetches).
            // No vector is sampled until its bus cycle completes.
            self.cpu_ticks = 20;
            self.cpu_origin = self.time;
            self.next_cpu = self.time + (20 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
        } else if !enabled {
            self.cpu.assert_reset();
            self.transactions = transactions::Transactions::default();
            self.next_cpu = u64::MAX;
            // An already asserted RESET output finishes its external pulse;
            // canceling the CPU must not strand the devices in reset.
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
            let next_scsp = self.scsp_deadline(now);
            let deadline = self
                .next_cpu
                .min(next_scsp)
                .min(next_midi)
                .min(self.reset_output_start.unwrap_or(u64::MAX))
                .min(self.reset_output_end.unwrap_or(u64::MAX));
            if deadline > target {
                break;
            }
            self.scsp.advance_midi((deadline - now) as u32);
            if deadline < next_scsp {
                // Empty slots elapsed before this CPU/MIDI/reset event. A
                // first MPRO write at the edge must not execute them again.
                self.scsp_tick = deadline * DSP_HZ / REFERENCE_HZ;
            }
            now = deadline;
            if Some(deadline) == self.reset_output_start {
                self.scsp.set_reset(true);
                self.ram_busy_until = 0;
                self.reset_output_start = None;
            } else if Some(deadline) == self.reset_output_end {
                self.scsp.set_reset(false);
                self.reset_output_end = None;
            } else if deadline == next_scsp {
                let tick = deadline * DSP_HZ / REFERENCE_HZ;
                self.scsp_tick = tick;
                let step = ((tick - 1) & 127) as usize;
                if step == 0 {
                    self.scsp.external = cd.as_mut().map_or([0; 2], |cd| cd.audio_sample());
                }
                if self.scsp.sample_cycle(step, &mut self.ram)? {
                    self.ram_busy_until = ((tick + 1) * REFERENCE_HZ).div_ceil(DSP_HZ);
                }
            } else if deadline == self.next_cpu {
                if self.transactions.ram_pending(self.cpu.state.pc)
                    && (deadline < self.ram_busy_until || self.main_ram_requested)
                {
                    // Translate wait states into the independently clocked CPU
                    // domain without retiring, repeating or publishing a cycle.
                    let release = self.ram_busy_until.max(deadline + 1);
                    let end = ((release - self.cpu_origin) * SOUND_CPU_HZ).div_ceil(REFERENCE_HZ);
                    self.transactions.stall(end - self.cpu_ticks);
                    self.cpu_ticks = end;
                } else {
                    self.advance_cpu_cycle();
                    self.cpu.reset_output = false;
                }
                self.next_cpu =
                    self.cpu_origin + (self.cpu_ticks * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
            }
        }
        self.scsp.advance_midi((target - now) as u32);
        self.scsp_tick = target * DSP_HZ / REFERENCE_HZ;
        self.time = target;
        Ok(())
    }
}
#[cfg(test)]
mod reset_tests;
#[cfg(test)]
mod tests;
