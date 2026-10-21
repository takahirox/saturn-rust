//! ST-097 §4: four parallel operation fields, signed 48-bit MAC, control flow
//! with one prefetched successor, four 64-word data banks and external DMA.
use super::{ScuError, next_data_address};
use crate::bus::{BusError, Memory};
use std::cell::Cell;

#[cfg(test)]
mod tests;

const MASK48: u64 = 0xFFFF_FFFF_FFFF;
const SIGN: u32 = 1 << 22;
const ZERO: u32 = 1 << 21;
const CARRY: u32 = 1 << 20;
const OVERFLOW: u32 = 1 << 19;
const END: u32 = 1 << 18;

#[derive(Clone)]
pub(super) struct Dsp {
    program: [u32; 256],
    data: [[u32; 64]; 4],
    ct: [u8; 4],
    pc: u8,
    load_address: u8,
    data_address: Cell<u8>,
    executing: bool,
    paused: bool,
    single_step: bool,
    flags: Cell<u32>,
    rx: u32,
    ry: u32,
    p: u64,
    a: u64,
    top: u8,
    lop: u16,
    ra0: u32,
    wa0: u32,
    branch: Option<u8>,
    prefetched: Option<(u8, u32)>,
    repeat: Option<u8>,
    dma: Option<Dma>,
}
#[derive(Clone)]
struct Dma {
    bank: u8,
    write: bool,
    remaining: u32,
    address: u32,
    add: u32,
    hold: bool,
    // ST-097 §4.5: B-bus transfers use two external 16-bit cycles.
    // Retain the sampled longword between grants; a DSP RAM write between
    // its halves must not replace the low half of the in-flight word.
    halfword: Option<u32>,
    // Address/data sampled when the arbiter grants a word, retained through
    // both B-bus halves even if parallel DSP instructions change CT/RAM.
    latch: Option<(usize, u32)>,
}
impl Default for Dsp {
    fn default() -> Self {
        Self {
            program: [0; 256],
            data: [[0; 64]; 4],
            ct: [0; 4],
            pc: 0,
            load_address: 0,
            data_address: Cell::new(0),
            executing: false,
            paused: false,
            single_step: false,
            flags: Cell::new(0),
            rx: 0,
            ry: 0,
            p: 0,
            a: 0,
            top: 0,
            lop: 0,
            ra0: 0,
            wa0: 0,
            branch: None,
            prefetched: None,
            repeat: None,
            dma: None,
        }
    }
}
fn sx(value: u32, bits: u32) -> u32 {
    ((value << (32 - bits)) as i32 >> (32 - bits)) as u32
}
fn sx48(value: u32) -> u64 {
    value as i32 as i64 as u64 & MASK48
}
impl Dsp {
    pub(super) fn instruction_pending(&self) -> bool {
        (self.executing && !self.paused) || self.single_step
    }
    pub(super) fn dma_pending(&self) -> bool {
        self.dma.is_some()
    }
    pub(super) fn running(&self) -> bool {
        self.executing || self.single_step || self.dma.is_some()
    }
    pub(super) fn control(&self) -> u32 {
        let result = self.flags.get()
            | u32::from(self.dma.is_some()) << 23
            | u32::from(self.executing) << 16
            | u32::from(self.pc);
        self.flags.set(self.flags.get() & !(OVERFLOW | END));
        result
    }
    pub(super) fn set_control(&mut self, value: u32) {
        let was_executing = self.executing;
        if !was_executing {
            self.load_address = value as u8;
            if value & 0x8000 != 0 {
                self.pc = value as u8;
                self.branch = None;
                self.prefetched = None;
                self.repeat = None;
            }
            if value & 0x20000 != 0 {
                self.single_step = true;
            }
        }
        self.executing = value & 0x10000 != 0;
        if self.executing {
            if value & (1 << 25) != 0 {
                self.paused = true;
            }
            if value & (1 << 26) != 0 {
                self.paused = false;
            }
        } else {
            self.paused = false;
        }
    }
    pub(super) fn write_program(&mut self, value: u32, address: u32) -> Result<(), BusError> {
        if self.executing {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        self.program[self.load_address as usize] = value;
        self.prefetched = None;
        self.load_address = self.load_address.wrapping_add(1);
        // The host port and stopped program counter share the visible P register.
        self.pc = self.load_address;
        Ok(())
    }
    pub(super) fn set_data_address(&mut self, value: u32, address: u32) -> Result<(), BusError> {
        if self.executing {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        self.data_address.set(value as u8);
        Ok(())
    }
    pub(super) fn read_data(&self, address: u32) -> Result<u32, BusError> {
        if self.executing {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        let a = self.data_address.get();
        let value = self.data[(a >> 6) as usize][(a & 63) as usize];
        next_data_address(&self.data_address);
        Ok(value)
    }
    pub(super) fn write_data(&mut self, value: u32, address: u32) -> Result<(), BusError> {
        if self.executing {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        let a = self.data_address.get();
        self.data[(a >> 6) as usize][(a & 63) as usize] = value;
        next_data_address(&self.data_address);
        Ok(())
    }
    fn condition(&self, code: u8) -> Option<bool> {
        if code == 0 {
            return Some(true);
        }
        let flags = self.flags.get();
        let condition = match code & 31 {
            1 => flags & ZERO != 0,
            2 => flags & SIGN != 0,
            3 => flags & (ZERO | SIGN) != 0,
            4 => flags & CARRY != 0,
            8 => self.dma.is_some(),
            _ => return None,
        };
        if code & 0x40 == 0 {
            return None;
        }
        Some(condition == (code & 0x20 != 0))
    }
    fn source(&self, code: u8, alu: u64, increments: &mut [bool; 4]) -> Option<u32> {
        match code {
            0..=7 => {
                let bank = (code & 3) as usize;
                increments[bank] |= code & 4 != 0;
                Some(self.data[bank][self.ct[bank] as usize])
            }
            9 => Some(alu as u32),
            10 => Some((alu >> 16) as u32),
            _ => None,
        }
    }
    fn destination(&mut self, code: u8, value: u32, increments: &mut [bool; 4]) -> bool {
        match code {
            0..=3 => {
                self.data[code as usize][self.ct[code as usize] as usize] = value;
                increments[code as usize] = true;
            }
            4 => self.rx = value,
            5 => self.p = sx48(value),
            6 => self.ra0 = value,
            7 => self.wa0 = value,
            10 => self.lop = value as u16 & 0xFFF,
            11 => self.top = value as u8,
            12..=15 => {
                self.ct[code as usize - 12] = value as u8 & 63;
                increments[code as usize - 12] = false;
            }
            _ => return false,
        }
        true
    }
    fn alu(&mut self, op: u8) -> Option<u64> {
        let a = self.a as u32;
        let p = self.p as u32;
        let (result, carry, overflow, wide) = match op {
            0 => return Some(self.a),
            1 => (u64::from(a & p), false, false, false),
            2 => (u64::from(a | p), false, false, false),
            3 => (u64::from(a ^ p), false, false, false),
            4 => {
                let (r, c) = a.overflowing_add(p);
                (u64::from(r), c, (!(a ^ p) & (a ^ r)) >> 31 != 0, false)
            }
            5 => {
                let (r, c) = a.overflowing_sub(p);
                (u64::from(r), c, ((a ^ p) & (a ^ r)) >> 31 != 0, false)
            }
            6 => {
                let sum = self.a + self.p;
                let overflow = (!(self.a ^ self.p) & (self.a ^ sum)) & (1 << 47) != 0;
                (sum & MASK48, sum > MASK48, overflow, true)
            }
            8 => (u64::from((a as i32 >> 1) as u32), a & 1 != 0, false, false),
            9 => (u64::from(a.rotate_right(1)), a & 1 != 0, false, false),
            10 => (u64::from(a << 1), a >> 31 != 0, false, false),
            11 => (u64::from(a.rotate_left(1)), a >> 31 != 0, false, false),
            15 => (
                u64::from(a.rotate_left(8)),
                a & (1 << 24) != 0,
                false,
                false,
            ),
            _ => return None,
        };
        let sign_bit = if wide { 47 } else { 31 };
        let mut flags = self.flags.get() & !(SIGN | ZERO | CARRY);
        if result & (1 << sign_bit) != 0 {
            flags |= SIGN;
        }
        if result == 0 {
            flags |= ZERO;
        }
        if carry {
            flags |= CARRY;
        }
        // V latches until read, while logical/shift instructions leave it alone.
        if overflow {
            flags |= OVERFLOW;
        }
        self.flags.set(flags);
        Some(if wide {
            result
        } else {
            self.a & 0xFFFF_0000_0000 | result
        })
    }
    pub(super) fn step(&mut self) -> Result<bool, ScuError> {
        if (!self.executing || self.paused) && !self.single_step {
            return Ok(false);
        }
        let pc = self.pc;
        // ST-097 §4.3/fig.4.2: a successor is already fetched when the
        // preceding operation executes. Program-RAM DMA must not replace
        // that instruction between execution clocks.
        let instruction = self
            .prefetched
            .filter(|(address, _)| *address == pc)
            .map_or(self.program[pc as usize], |(_, word)| word);
        let illegal = || ScuError::IllegalDsp { pc, instruction };
        // A second DMA instruction waits rather than discarding the running transfer.
        if instruction >> 28 == 12 && self.dma.is_some() {
            return Ok(false);
        }
        self.single_step = false;
        let old_branch = self.branch.take();
        let repeating = self.repeat == Some(pc);
        self.pc = pc.wrapping_add(1);
        let mut interrupt = false;
        match instruction >> 30 {
            0 => {
                let alu = self
                    .alu((instruction >> 26) as u8 & 15)
                    .ok_or_else(illegal)?;
                let multiplier =
                    (self.rx as i32 as i64).wrapping_mul(self.ry as i32 as i64) as u64 & MASK48;
                let mut increments = [false; 4];
                // All buses sample the old RAM addresses before any destination writes.
                let xcode = (instruction >> 20) as u8 & 7;
                let ycode = (instruction >> 14) as u8 & 7;
                let xdata = self.source(xcode, alu, &mut increments).unwrap();
                let ydata = self.source(ycode, alu, &mut increments).unwrap();
                // Sampling a non-selected RAM must not increment a counter.
                increments = [false; 4];
                let dmode = (instruction >> 12) & 3;
                let ddata = match dmode {
                    0 => None,
                    1 => Some(sx(instruction & 255, 8)),
                    3 => Some(
                        self.source(instruction as u8 & 15, alu, &mut increments)
                            .ok_or_else(illegal)?,
                    ),
                    _ => return Err(illegal()),
                };
                let xmode = (instruction >> 23) & 7;
                if xmode & 4 != 0 {
                    self.rx = xdata;
                    increments[(xcode & 3) as usize] |= xcode & 4 != 0;
                }
                match xmode & 3 {
                    0 => (),
                    2 => self.p = multiplier,
                    3 => {
                        self.p = sx48(xdata);
                        increments[(xcode & 3) as usize] |= xcode & 4 != 0;
                    }
                    _ => return Err(illegal()),
                }
                let ymode = (instruction >> 17) & 7;
                if ymode & 4 != 0 {
                    self.ry = ydata;
                    increments[(ycode & 3) as usize] |= ycode & 4 != 0;
                }
                match ymode & 3 {
                    0 => (),
                    1 => self.a = 0,
                    2 => self.a = alu,
                    3 => {
                        self.a = sx48(ydata);
                        increments[(ycode & 3) as usize] |= ycode & 4 != 0;
                    }
                    _ => unreachable!(),
                }
                if let Some(value) = ddata
                    && !self.destination((instruction >> 8) as u8 & 15, value, &mut increments)
                {
                    return Err(illegal());
                }
                for (bank, increment) in increments.into_iter().enumerate() {
                    if increment {
                        self.ct[bank] = self.ct[bank].wrapping_add(1) & 63;
                    }
                }
            }
            2 => {
                let conditional = instruction & (1 << 25) != 0;
                let condition = if conditional {
                    self.condition(((instruction >> 19) as u8 & 63) | 0x40)
                        .ok_or_else(illegal)?
                } else {
                    true
                };
                if condition {
                    let value = sx(instruction, if conditional { 19 } else { 25 });
                    let dest = (instruction >> 26) as u8 & 15;
                    if dest == 12 {
                        self.top = self.pc;
                        self.branch = Some(value as u8);
                    } else if dest == 11
                        || dest >= 13
                        || !self.destination(dest, value, &mut [false; 4])
                    {
                        return Err(illegal());
                    }
                    // MVI to data RAM performs its postincrement too.
                    if dest <= 3 {
                        self.ct[dest as usize] = self.ct[dest as usize].wrapping_add(1) & 63;
                    }
                }
            }
            3 => match (instruction >> 28) & 3 {
                0 => {
                    let bank = (instruction >> 8) as u8 & 7;
                    let write = instruction & 0x1000 != 0;
                    if bank > 4 || (write && bank == 4) {
                        return Err(illegal());
                    }
                    let count = if instruction & 0x2000 != 0 {
                        let mut increments = [false; 4];
                        let v = self
                            .source(instruction as u8 & 7, 0, &mut increments)
                            .unwrap()
                            & 255;
                        for (i, inc) in increments.into_iter().enumerate() {
                            if inc {
                                self.ct[i] = self.ct[i].wrapping_add(1) & 63;
                            }
                        }
                        v
                    } else {
                        instruction & 255
                    };
                    let add = match instruction >> 15 & 7 {
                        0 => 0,
                        n => 1 << (n - 1),
                    };
                    let address = if write { self.wa0 } else { self.ra0 };
                    self.dma = Some(Dma {
                        bank,
                        write,
                        remaining: if count == 0 { 256 } else { count },
                        address: address.wrapping_mul(4) & 0x07FF_FFFF,
                        add,
                        hold: instruction & 0x4000 != 0,
                        halfword: None,
                        latch: None,
                    });
                }
                1 => {
                    if self
                        .condition((instruction >> 19) as u8 & 0x7F)
                        .ok_or_else(illegal)?
                    {
                        self.branch = Some(instruction as u8);
                    }
                }
                2 => {
                    if instruction & (1 << 27) != 0 {
                        self.repeat = Some(self.pc);
                    } else if self.lop != 0 {
                        self.lop -= 1;
                        self.branch = Some(self.top);
                    }
                }
                3 => {
                    self.executing = false;
                    if instruction & (1 << 27) != 0 {
                        interrupt = self.flags.get() & END == 0;
                        self.flags.set(self.flags.get() | END);
                    }
                }
                _ => unreachable!(),
            },
            _ => return Err(illegal()),
        }
        if repeating {
            if self.lop != 0 {
                self.lop -= 1;
                self.pc = pc;
            } else {
                self.repeat = None;
            }
        }
        if let Some(target) = old_branch {
            self.pc = target;
        }
        self.prefetched = Some((self.pc, self.program[self.pc as usize]));
        Ok(interrupt)
    }
    pub(super) fn grant_dma_cycle(&mut self) {
        if let Some(dma) = self.dma.as_mut()
            && dma.latch.is_none()
        {
            let bank = dma.bank as usize;
            let ct = if bank < 4 {
                self.ct[bank] as usize
            } else {
                self.load_address as usize
            };
            dma.latch = Some((ct, if dma.write { self.data[bank][ct] } else { 0 }));
        }
    }
    pub(super) fn dma_beat(&mut self, memory: &mut impl Memory) -> Result<(), ScuError> {
        self.grant_dma_cycle();
        let Some(dma) = self.dma.as_mut() else {
            return Ok(());
        };
        let bank = dma.bank as usize;
        let address = dma.address;
        let (ct, outgoing) = dma.latch.unwrap();
        if dma.write {
            if !super::dma::valid_destination(address) {
                return Err(ScuError::DmaParameters { channel: 3 });
            }
            if super::dma::b_bus(address) {
                if let Some(value) = dma.halfword {
                    let low_address = address + dma.add * 2;
                    if !super::dma::valid_destination(low_address)
                        || !super::dma::valid_destination(low_address + 1)
                    {
                        return Err(ScuError::DmaParameters { channel: 3 });
                    }
                    memory.write16(low_address, value as u16)?;
                    dma.halfword = None;
                } else {
                    let value = outgoing;
                    memory.write16(address, (value >> 16) as u16)?;
                    dma.halfword = Some(value);
                    return Ok(());
                }
                dma.address = address.wrapping_add(dma.add * 4);
            } else {
                // ST-097 §4.5 pp.134/136/138/140: A-bus writes permit
                // both fixed address (0) and longword increment (1).
                if dma.add > 1 {
                    return Err(ScuError::DmaParameters { channel: 3 });
                }
                memory.write32(address, outgoing)?;
                dma.address = address.wrapping_add(dma.add * 4);
            }
        } else {
            if !super::dma::valid_source(address) {
                return Err(ScuError::DmaParameters { channel: 3 });
            }
            if super::dma::b_bus(address) && dma.add != 2 {
                return Err(ScuError::DmaParameters { channel: 3 });
            }
            let value = if super::dma::b_bus(address) {
                if let Some(high) = dma.halfword {
                    if !super::dma::valid_source(address + 3) {
                        return Err(ScuError::DmaParameters { channel: 3 });
                    }
                    let value = high | u32::from(memory.read16(address + 2)?);
                    dma.halfword = None;
                    value
                } else {
                    dma.halfword = Some(u32::from(memory.read16(address)?) << 16);
                    return Ok(());
                }
            } else {
                memory.read32(address)?
            };
            if bank == 4 {
                self.program[ct] = value;
                self.load_address = (ct as u8).wrapping_add(1);
            } else {
                self.data[bank][ct] = value;
            }
            dma.address = address.wrapping_add(dma.add * 4);
        }
        if bank < 4 {
            self.ct[bank] = (ct as u8).wrapping_add(1) & 63;
        }
        dma.latch = None;
        dma.remaining -= 1;
        if !dma.hold {
            if dma.write {
                self.wa0 = dma.address / 4;
            } else {
                self.ra0 = dma.address / 4;
            }
        }
        if dma.remaining == 0 {
            self.dma = None;
        }
        Ok(())
    }
}
