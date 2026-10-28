//! Independent MC68EC000 interpreter, derived from Motorola M68000PRM §§1–4,
//! appendix B (68000 entries only). No later-model opcodes or emulator core.
//! External address pins are 24-bit; words/longs require only word alignment.
use crate::bus::{BusError, Memory};

const C: u16 = 1;
const V: u16 = 2;
const Z: u16 = 4;
const N: u16 = 8;
const X: u16 = 16;
const S: u16 = 0x2000;
const T: u16 = 0x8000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub d: [u32; 8],
    pub a: [u32; 8],
    pub pc: u32,
    pub sr: u16,
    pub usp: u32,
    pub ssp: u32,
    pub stopped: bool,
    pub halted: bool,
    pub instructions: u64,
    pub exceptions: u64,
    pub last_vector: Option<u8>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            d: [0; 8],
            a: [0; 8],
            pc: 0,
            sr: 0x2700,
            usp: 0,
            ssp: 0,
            stopped: false,
            halted: false,
            instructions: 0,
            exceptions: 0,
            last_vector: None,
        }
    }
}
#[derive(Clone, Default)]
pub struct M68k {
    pub state: State,
    opcode: u16,
    start_pc: u32,
    irq: u8,
    level7_edge: bool,
    pub reset_output: bool,
    // Instruction register plus a prefetched word. Writes to the sound bus do
    // not invalidate either; only a control-flow change flushes the queue.
    prefetch: std::collections::VecDeque<(u32, u16)>,
    bus_pc: u32,
    exception_activity: bool,
    // MOVEM memory-to-register consumes its EA extensions before the operand
    // burst, then performs a terminal word read before its next program fetch.
    defer_prefetch: bool,
    predecrement_waited: bool,
    // A traced instruction has already retired while its trace frame/vector
    // cycles are in flight. Reset must retain that completed register state.
    retired_state: Option<State>,
}
#[derive(Clone, Copy)]
enum Size {
    Byte,
    Word,
    Long,
}
impl Size {
    fn bytes(self) -> u32 {
        match self {
            Self::Byte => 1,
            Self::Word => 2,
            Self::Long => 4,
        }
    }
    fn mask(self) -> u32 {
        match self {
            Self::Byte => 255,
            Self::Word => 65535,
            Self::Long => u32::MAX,
        }
    }
    fn sign(self) -> u32 {
        (self.mask() >> 1) + 1
    }
    fn extend(self, v: u32) -> u32 {
        match self {
            Self::Byte => v as i8 as i32 as u32,
            Self::Word => v as i16 as i32 as u32,
            Self::Long => v,
        }
    }
}
#[derive(Clone, Copy)]
enum Ea {
    Data(usize),
    Address(usize),
    Memory(u32),
    Predecrement(u32),
    Immediate(u32),
}
#[derive(Debug)]
enum Fault {
    Suspended,
    Vector(u8, bool), // true: stack the following PC (trap), false: instruction PC.
    Access {
        address: u32,
        write: bool,
        instruction: bool,
        address_error: bool,
        pc: u32,
        not_instruction: bool,
    },
}
type Result<T> = std::result::Result<T, Fault>;

impl M68k {
    /// MC68000UM table8-1, 16-bit bus. Includes extension fetch and operand
    /// read cycles; callers add the instruction's documented base time.
    fn ea_clocks(bits: u16, size: Size) -> u32 {
        let long = u32::from(matches!(size, Size::Long)) * 4;
        match (bits >> 3) & 7 {
            0 | 1 => 0,
            2 | 3 => 4 + long,
            4 => 6 + long,
            5 => 8 + long,
            6 => 10 + long,
            _ => match bits & 7 {
                0 | 2 => 8 + long,
                1 => 12 + long,
                3 => 10 + long,
                4 => 4 + long,
                _ => 0,
            },
        }
    }
    fn jump_clocks(bits: u16) -> u32 {
        // Table8-10: JMP base8; JSR adds eight clocks.
        match bits & 0x3F {
            0x10..=0x17 => 8,
            0x28..=0x2F | 0x38 | 0x3A => 10,
            0x39 => 12,
            _ => 14,
        }
    }
    fn lea_clocks(bits: u16) -> u32 {
        Self::ea_clocks(bits, Size::Word) + u32::from(bits & 0x38 == 0x30 || bits == 0x3B) * 2
    }
    /// MC68000UM §§5.5/6.3.1: reset changes S/T/I and reloads SSP/PC;
    /// it does not clear D0-D7, A0-A6 or the user stack. Clear internal work
    /// (including a stale prefetch/IRQ) without inventing register writes.
    pub(crate) fn assert_reset(&mut self) {
        let old = std::mem::take(self);
        self.state.d = old.state.d;
        self.state.a = old.state.a;
        self.state.usp = if old.state.sr & S == 0 {
            old.state.a[7]
        } else {
            old.state.usp
        };
        self.state.ssp = old.state.ssp;
        self.state.sr = (old.state.sr & 31) | 0x2700;
    }
    pub fn reset(&mut self, memory: &impl Memory) -> std::result::Result<(), BusError> {
        self.assert_reset();
        let vectors = (|| {
            self.state.a[7] = Self::reset_long(memory, 0)?;
            self.state.ssp = self.state.a[7];
            self.state.pc = Self::reset_long(memory, 4)?;
            Ok(())
        })();
        // UM §6.3.9.1: reset-vector BERR halts; it cannot recursively dispatch.
        self.state.halted = vectors.is_err();
        vectors
    }
    fn reset_long(memory: &impl Memory, address: u32) -> std::result::Result<u32, BusError> {
        Ok(u32::from(memory.read16(address)?) << 16 | u32::from(memory.read16(address + 2)?))
    }
    pub(crate) fn reset_prefetch(&mut self, address: u32, value: u16) {
        self.prefetch.push_back((address, value));
    }
    /// Autovectored IRQ pins. A lower-to-seven transition also qualifies at I=7;
    /// a held seven still qualifies by level comparison after software lowers I.
    pub fn set_interrupt(&mut self, level: u8) {
        let level = level.min(7);
        self.level7_edge |= level == 7 && self.irq != 7;
        self.irq = level;
    }
    /// Freeze boundary input for deterministic bus replay. Subsequent input
    /// transitions belong to the live CPU, not this speculative execution.
    pub(crate) fn begin_replay(&mut self) -> Self {
        let snapshot = self.clone();
        self.level7_edge = false;
        snapshot
    }
    /// Keep input received during replay, plus any boundary request that the
    /// execution did not consume (for example a halted CPU). No event queue.
    pub(crate) fn finish_replay(&mut self, live: &Self) {
        self.irq = live.irq;
        self.level7_edge |= live.level7_edge;
    }
    fn sr(&mut self, value: u16) {
        let value = value & 0xA71F;
        if (self.state.sr ^ value) & S != 0 {
            if value & S != 0 {
                self.state.usp = self.state.a[7];
                self.state.a[7] = self.state.ssp;
            } else {
                self.state.ssp = self.state.a[7];
                self.state.a[7] = self.state.usp;
            }
        }
        self.state.sr = value;
    }
    fn flag(&mut self, flag: u16, value: bool) {
        self.state.sr = (self.state.sr & !flag) | if value { flag } else { 0 };
    }
    fn supervisor(&self) -> Result<()> {
        if self.state.sr & S == 0 {
            Err(Fault::Vector(8, false))
        } else {
            Ok(())
        }
    }
    fn read(
        &self,
        memory: &impl Memory,
        address: u32,
        size: Size,
        instruction: bool,
    ) -> Result<u32> {
        let fault = |e: BusError| Fault::Access {
            address,
            write: false,
            instruction,
            address_error: matches!(e, BusError::Unaligned { .. }),
            pc: self.bus_pc,
            not_instruction: self.exception_activity,
        };
        let a = address & 0xFF_FFFF;
        if size.bytes() > 1 && a & 1 != 0 {
            return Err(fault(BusError::Unaligned {
                address,
                width: size.bytes() as usize,
            }));
        }
        Ok(match size {
            Size::Byte => u32::from(memory.read8(a).map_err(fault)?),
            Size::Word => u32::from(memory.read16(a).map_err(fault)?),
            Size::Long => {
                u32::from(memory.read16(a).map_err(fault)?) << 16
                    | u32::from(memory.read16(a.wrapping_add(2) & 0xFF_FFFF).map_err(|e| {
                        Fault::Access {
                            address: address.wrapping_add(2),
                            write: false,
                            instruction,
                            address_error: matches!(e, BusError::Unaligned { .. }),
                            pc: self.bus_pc,
                            not_instruction: self.exception_activity,
                        }
                    })?)
            }
        })
    }
    fn write(&self, memory: &mut impl Memory, address: u32, size: Size, value: u32) -> Result<()> {
        let fault = |e: BusError| Fault::Access {
            address,
            write: true,
            instruction: false,
            address_error: matches!(e, BusError::Unaligned { .. }),
            pc: self.bus_pc,
            not_instruction: self.exception_activity,
        };
        let a = address & 0xFF_FFFF;
        if size.bytes() > 1 && a & 1 != 0 {
            return Err(fault(BusError::Unaligned {
                address,
                width: size.bytes() as usize,
            }));
        }
        match size {
            Size::Byte => memory.write8(a, value as u8).map_err(fault)?,
            Size::Word => memory.write16(a, value as u16).map_err(fault)?,
            Size::Long => {
                memory.write16(a, (value >> 16) as u16).map_err(fault)?;
                memory
                    .write16(a.wrapping_add(2) & 0xFF_FFFF, value as u16)
                    .map_err(|e| Fault::Access {
                        address: address.wrapping_add(2),
                        write: true,
                        instruction: false,
                        address_error: matches!(e, BusError::Unaligned { .. }),
                        pc: self.bus_pc,
                        not_instruction: self.exception_activity,
                    })?;
            }
        }
        Ok(())
    }
    // MC68000 predecrement bus sequencing: long writes issue the low word
    // first, then the high word. A fault preserves the completed bus cycle.
    fn write_predecrement(
        &self,
        memory: &mut impl Memory,
        address: u32,
        size: Size,
        value: u32,
    ) -> Result<()> {
        if matches!(size, Size::Long) {
            if address & 1 != 0 {
                return self.write(memory, address, size, value);
            }
            self.write(memory, address.wrapping_add(2), Size::Word, value)?;
            self.write(memory, address, Size::Word, value >> 16)
        } else {
            self.write(memory, address, size, value)
        }
    }
    fn refill(&mut self, memory: &impl Memory, count: usize, redirect: bool) -> Result<()> {
        let old_pc = self.bus_pc;
        while self.prefetch.len() < count {
            let address = self
                .prefetch
                .back()
                .map_or(self.state.pc, |(a, _)| a.wrapping_add(2));
            // UM §6.3.9.1: an aborted sequential prefetch saves the internal
            // fetch PC; a failed branch-target fetch retains the branch context.
            self.bus_pc = if redirect {
                old_pc
            } else {
                address.wrapping_add(2)
            };
            let word = self.read(memory, address, Size::Word, true)? as u16;
            self.prefetch.push_back((address, word));
        }
        self.bus_pc = self
            .prefetch
            .back()
            .map_or(self.bus_pc, |(a, _)| a.wrapping_add(2));
        Ok(())
    }
    fn next_word(&mut self, memory: &impl Memory, opcode: bool) -> Result<u16> {
        if self
            .prefetch
            .front()
            .is_some_and(|(a, _)| *a != self.state.pc)
        {
            self.prefetch.clear();
        }
        self.refill(memory, 1, false)?;
        let (_, value) = self.prefetch.pop_front().unwrap();
        self.state.pc = self.state.pc.wrapping_add(2);
        if opcode {
            self.opcode = value;
        }
        // A five-word instruction has used the final EA extension. Do not
        // eagerly issue a sixth word before its operand cycle: UM §6.3.9.1
        // bounds the saved internal PC at ten bytes beyond the instruction.
        if !self.defer_prefetch && self.state.pc.wrapping_sub(self.start_pc) != 10 {
            self.refill(memory, 1, false)?;
        }
        Ok(value)
    }
    fn next(&mut self, memory: &impl Memory) -> Result<u16> {
        self.next_word(memory, false)
    }
    fn redirect(&mut self, memory: &impl Memory, pc: u32) -> Result<()> {
        self.state.pc = pc;
        self.prefetch.clear();
        self.refill(memory, 2, true)
    }
    fn immediate(&mut self, memory: &impl Memory, size: Size) -> Result<u32> {
        let value = u32::from(self.next(memory)?);
        Ok(if matches!(size, Size::Long) {
            value << 16 | u32::from(self.next(memory)?)
        } else {
            value & size.mask()
        })
    }
    fn size(op: u16) -> Result<Size> {
        match (op >> 6) & 3 {
            0 => Ok(Size::Byte),
            1 => Ok(Size::Word),
            2 => Ok(Size::Long),
            _ => Err(Fault::Vector(4, false)),
        }
    }
    fn index(&mut self, memory: &impl Memory, base: u32) -> Result<u32> {
        let e = self.next(memory)?;
        // UM table8-1: the brief-index sum needs two internal clocks after
        // its extension fetch, before any operand cycle can sample memory.
        Self::internal_wait(memory, 2)?;
        // MC68000 brief extension: scale/full-format bits have no function.
        let r = ((e >> 12) & 7) as usize;
        let index = if e & 0x8000 != 0 {
            self.state.a[r]
        } else {
            self.state.d[r]
        };
        let index = if e & 0x800 == 0 {
            Size::Word.extend(index)
        } else {
            index
        };
        Ok(base.wrapping_add(index).wrapping_add(e as i8 as i32 as u32))
    }
    fn ea(&mut self, memory: &impl Memory, bits: u16, size: Size) -> Result<Ea> {
        let r = (bits & 7) as usize;
        let delta = if r == 7 && matches!(size, Size::Byte) {
            2
        } else {
            size.bytes()
        };
        Ok(match (bits >> 3) & 7 {
            0 => Ea::Data(r),
            1 => Ea::Address(r),
            2 => Ea::Memory(self.state.a[r]),
            3 => {
                let a = self.state.a[r];
                self.state.a[r] = a.wrapping_add(delta);
                Ea::Memory(a)
            }
            4 => {
                self.state.a[r] = self.state.a[r].wrapping_sub(delta);
                Ea::Predecrement(self.state.a[r])
            }
            5 => {
                let d = self.next(memory)? as i16 as i32 as u32;
                Ea::Memory(self.state.a[r].wrapping_add(d))
            }
            6 => Ea::Memory(self.index(memory, self.state.a[r])?),
            _ => match r {
                0 => Ea::Memory(self.next(memory)? as i16 as i32 as u32),
                1 => Ea::Memory(self.immediate(memory, Size::Long)?),
                2 => {
                    let base = self.state.pc;
                    let d = self.next(memory)? as i16 as i32 as u32;
                    Ea::Memory(base.wrapping_add(d))
                }
                3 => {
                    let base = self.state.pc;
                    Ea::Memory(self.index(memory, base)?)
                }
                4 => Ea::Immediate(self.immediate(memory, size)?),
                _ => return Err(Fault::Vector(4, false)),
            },
        })
    }
    fn data(bits: u16, alterable: bool) -> bool {
        bits & 0x38 != 8 && (bits & 0x38 != 0x38 || bits & 7 <= if alterable { 1 } else { 4 })
    }
    fn memory_alterable(bits: u16) -> bool {
        bits & 0x38 >= 0x10 && Self::data(bits, true)
    }
    fn control(bits: u16) -> bool {
        matches!(bits & 0x38, 0x10 | 0x28 | 0x30) || (bits & 0x38 == 0x38 && bits & 7 <= 3)
    }
    fn require(value: bool) -> Result<()> {
        if value {
            Ok(())
        } else {
            Err(Fault::Vector(4, false))
        }
    }
    fn internal_wait(memory: &impl Memory, clocks: u32) -> Result<()> {
        memory
            .m68k_internal_wait(clocks)
            .map_err(|_| Fault::Suspended)
    }
    fn get(&mut self, memory: &impl Memory, ea: Ea, size: Size) -> Result<u32> {
        if matches!(ea, Ea::Predecrement(_)) && !self.predecrement_waited {
            // The source predecrement adds two clocks. MOVE destinations
            // do not use get; paired ADDX/SUBX/ABCD/SBCD operands share this
            // one internal interval (UM tables8-2/8-11).
            Self::internal_wait(memory, 2)?;
            self.predecrement_waited = true;
        }
        Ok(match ea {
            Ea::Data(r) => self.state.d[r],
            Ea::Address(r) => self.state.a[r],
            Ea::Memory(a) | Ea::Predecrement(a) => self.read(memory, a, size, false)?,
            Ea::Immediate(v) => v,
        } & size.mask())
    }
    fn put(&mut self, memory: &mut impl Memory, ea: Ea, size: Size, value: u32) -> Result<()> {
        match ea {
            Ea::Data(r) => {
                self.state.d[r] = (self.state.d[r] & !size.mask()) | (value & size.mask())
            }
            Ea::Address(r) => {
                self.state.a[r] = if matches!(size, Size::Word) {
                    Size::Word.extend(value)
                } else {
                    value
                }
            }
            Ea::Memory(a) => self.write(memory, a, size, value)?,
            Ea::Predecrement(a) => self.write_predecrement(memory, a, size, value)?,
            Ea::Immediate(_) => return Err(Fault::Vector(4, false)),
        }
        Ok(())
    }
    fn logic(&mut self, value: u32, size: Size) {
        self.state.sr &= !(N | Z | V | C);
        self.flag(Z, value & size.mask() == 0);
        self.flag(N, value & size.sign() != 0);
    }
    /// Extended-width arithmetic avoids host signed overflow, including ADDX/SUBX.
    fn arithmetic(
        &mut self,
        dst: u32,
        src: u32,
        size: Size,
        sub: bool,
        extend: bool,
        compare: bool,
    ) -> u32 {
        let x = u64::from(extend && self.state.sr & X != 0);
        let a = u64::from(dst & size.mask());
        let b = u64::from(src & size.mask());
        let wide = if sub {
            a.wrapping_sub(b + x)
        } else {
            a + b + x
        };
        let result = wide as u32 & size.mask();
        let carry = if sub {
            a < b + x
        } else {
            wide > u64::from(size.mask())
        };
        let overflow = if sub {
            (dst ^ src) & (dst ^ result)
        } else {
            !(dst ^ src) & (dst ^ result)
        } & size.sign()
            != 0;
        let old_z = self.state.sr & Z != 0;
        self.logic(result, size);
        self.flag(V, overflow);
        self.flag(C, carry);
        if !compare {
            self.flag(X, carry);
        }
        if extend {
            self.flag(Z, old_z && result == 0);
        }
        result
    }
    fn condition(&self, c: u16) -> bool {
        let sr = self.state.sr;
        let n = sr & N != 0;
        let v = sr & V != 0;
        let z = sr & Z != 0;
        let carry = sr & C != 0;
        match c & 15 {
            0 => true,
            1 => false,
            2 => !carry && !z,
            3 => carry || z,
            4 => !carry,
            5 => carry,
            6 => !z,
            7 => z,
            8 => !v,
            9 => v,
            10 => !n,
            11 => n,
            12 => n == v,
            13 => n != v,
            14 => !z && n == v,
            _ => z || n != v,
        }
    }
    fn push(&mut self, memory: &mut impl Memory, size: Size, value: u32) -> Result<()> {
        self.state.a[7] = self.state.a[7].wrapping_sub(size.bytes());
        self.write_predecrement(memory, self.state.a[7], size, value)
    }
    fn pop(&mut self, memory: &impl Memory, size: Size) -> Result<u32> {
        let v = self.read(memory, self.state.a[7], size, false)?;
        self.state.a[7] = self.state.a[7].wrapping_add(size.bytes());
        Ok(v)
    }
    fn exception(
        &mut self,
        memory: &mut impl Memory,
        vector: u8,
        pc: u32,
        level: Option<u8>,
        access: Option<(u32, u16)>,
    ) -> Result<()> {
        let old_sr = self.state.sr;
        // UM §§6.3.9.1/6.3.10: I/N describes processor activity, not the
        // function-code pins. Group 0/1 exception cycles assert I/N.
        // UM table6-3: only TRAP/TRAPV/CHK/zero-divide are group2.
        // Illegal/privilege/trace (including line-A/F emulation) are group1.
        self.exception_activity = level.is_some() || !matches!(vector, 5..=7 | 32..=47);
        self.prefetch.clear();
        self.sr((old_sr | S) & !T);
        self.state.stopped = false;
        if let Some(l) = level {
            self.state.sr = (self.state.sr & !0x700) | (u16::from(l) << 8);
        }
        self.push(memory, Size::Long, pc)?;
        self.push(memory, Size::Word, u32::from(old_sr))?;
        // Appendix B, figure B-2: seven words; SSW at SP, saved SR at SP+8.
        if let Some((address, ssw)) = access {
            self.push(memory, Size::Word, u32::from(self.opcode))?;
            self.push(memory, Size::Long, address)?;
            self.push(memory, Size::Word, u32::from(ssw))?;
        }
        // UM §6.1: vector lookup uses supervisor data space. A failure in this
        // final phase reports the vector address in the saved internal PC.
        self.bus_pc = u32::from(vector) * 4;
        self.state.pc = self.read(memory, self.bus_pc, Size::Long, false)?;
        self.refill(memory, 2, true)?;
        self.state.exceptions += 1;
        self.state.last_vector = Some(vector);
        self.exception_activity = false;
        Ok(())
    }
    fn fault_context(&mut self, fault: Fault) -> (u8, u32, Option<(u32, u16)>) {
        match fault {
            Fault::Suspended => unreachable!("a suspended control cycle has no exception frame"),
            Fault::Vector(v, next) => {
                self.state.instructions += u64::from(next);
                (v, if next { self.state.pc } else { self.start_pc }, None)
            }
            Fault::Access {
                address,
                write,
                instruction,
                address_error,
                pc,
                not_instruction,
            } => {
                let fc =
                    if self.state.sr & S != 0 { 4 } else { 0 } | if instruction { 2 } else { 1 };
                (
                    if address_error { 3 } else { 2 },
                    pc,
                    Some((
                        address,
                        fc | if write { 0 } else { 16 } | if not_instruction { 8 } else { 0 },
                    )),
                )
            }
        }
    }
    fn deliver_exception(
        &mut self,
        memory: &mut impl Memory,
        vector: u8,
        pc: u32,
        level: Option<u8>,
        access: Option<(u32, u16)>,
    ) -> u32 {
        if let Err(fault) = self.exception(memory, vector, pc, level, access) {
            // UM §6.3.9.1: a fault during group1/2 processing enters group0.
            // Only a fault while handling bus/address error (or reset) halts.
            if access.is_some() {
                self.state.halted = true;
                return 0;
            }
            let (v, pc, frame) = self.fault_context(fault);
            if self.exception(memory, v, pc, None, frame).is_err() {
                self.state.halted = true;
            }
            return 50;
        }
        0
    }
    /// Executes one architectural boundary, returning a bounded functional clock
    /// budget from MC68000UM §8. DIV and data-register bit modifications retain
    /// the documented maximum time; per-cycle bus arbitration remains separate.
    /// Fatal double faults halt the core until reset; no host panic or silent NOP.
    pub fn step(&mut self, memory: &mut impl Memory) -> u32 {
        self.retired_state = None;
        if self.state.halted {
            return 4;
        }
        let mask = ((self.state.sr >> 8) & 7) as u8;
        if self.level7_edge || (self.irq != 0 && self.irq > mask) {
            let level = if self.level7_edge { 7 } else { self.irq };
            let vector = match memory.m68k_interrupt_acknowledge(level) {
                Ok(vector) => vector,
                Err(BusError::Pending) => return 0,
                Err(_) => 24, // BERR during IACK is spurious, not vector2.
            };
            self.level7_edge = false;
            let clocks =
                44 + self.deliver_exception(memory, vector, self.state.pc, Some(level), None);
            return Self::finish_clocks(memory, clocks);
        }
        if self.state.stopped {
            return 4;
        }
        self.start_pc = self.state.pc;
        self.exception_activity = false;
        self.defer_prefetch = false;
        self.predecrement_waited = false;
        let trace = self.state.sr & T != 0;
        let result = self
            .next_word(memory, true)
            .and_then(|op| self.execute(memory, op));
        match result {
            Err(Fault::Suspended) => 0,
            Ok(clocks) => {
                if memory.m68k_finish_clocks(clocks).is_err() {
                    return 0;
                }
                self.state.instructions += 1;
                self.retired_state = Some(self.state.clone());
                let exception_clocks = if trace {
                    34 + self.deliver_exception(memory, 9, self.state.pc, None, None)
                } else {
                    0
                };
                if trace {
                    Self::finish_clocks(memory, clocks + exception_clocks)
                } else {
                    clocks
                }
            }
            Err(fault) => {
                // UM §6.3.8: a traced TRAP/CHK/divide exception precedes the
                // pending trace; group0 faults abort the instruction instead.
                let trace_after = trace && matches!(fault, Fault::Vector(_, true));
                let (vector, pc, access) = self.fault_context(fault);
                let mut extra = self.deliver_exception(memory, vector, pc, None, access);
                if trace_after && extra == 0 && !self.state.halted {
                    extra += 34 + self.deliver_exception(memory, 9, self.state.pc, None, None);
                }
                let clocks = extra
                    + if access.is_some() {
                        50
                    } else {
                        match vector {
                            5 => 38 + Self::ea_clocks(self.opcode & 63, Size::Word),
                            6 => 40 + Self::ea_clocks(self.opcode & 63, Size::Word),
                            _ => 34,
                        }
                    };
                Self::finish_clocks(memory, clocks)
            }
        }
    }
    fn finish_clocks(memory: &impl Memory, clocks: u32) -> u32 {
        if memory.m68k_finish_clocks(clocks).is_err() {
            0
        } else {
            clocks
        }
    }
    pub(crate) fn retired_state(&self) -> Option<&State> {
        self.retired_state.as_ref()
    }
    fn execute(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let bits = op & 63;
        let r = ((op >> 9) & 7) as usize;
        match op >> 12 {
            1..=3 => {
                // MOVE / MOVEA; source extension precedes destination.
                let size = match op >> 12 {
                    1 => Size::Byte,
                    2 => Size::Long,
                    _ => Size::Word,
                };
                Self::require(!matches!(size, Size::Byte) || bits & 0x38 != 8)?;
                let dst = ((op >> 3) & 0x38) | ((op >> 9) & 7);
                Self::require(if dst & 0x38 == 8 {
                    !matches!(size, Size::Byte)
                } else {
                    Self::data(dst, true)
                })?;
                let src = self.ea(memory, bits, size)?;
                let value = self.get(memory, src, size)?;
                let dest = self.ea(memory, dst, size)?;
                self.put(memory, dest, size, value)?;
                if dst & 0x38 != 8 {
                    self.logic(value, size);
                }
                Ok(4 + Self::ea_clocks(bits, size) + Self::ea_clocks(dst, size)
                    - if dst & 0x38 == 0x20 { 2 } else { 0 })
            }
            0 => self.group0(memory, op),
            4 => self.group4(memory, op),
            5 => {
                // quick arithmetic, Scc, DBcc
                if op & 0xC0 == 0xC0 {
                    if bits & 0x38 == 8 {
                        let base = self.state.pc;
                        let d = self.next(memory)? as i16 as i32 as u32;
                        if !self.condition(op >> 8) {
                            let reg = (bits & 7) as usize;
                            let count = (self.state.d[reg] as u16).wrapping_sub(1);
                            self.state.d[reg] =
                                (self.state.d[reg] & 0xFFFF_0000) | u32::from(count);
                            if count != 0xFFFF {
                                self.redirect(memory, base.wrapping_add(d))?;
                                return Ok(10);
                            }
                            return Ok(14);
                        }
                        return Ok(12);
                    }
                    Self::require(Self::data(bits, true))?;
                    let dst = self.ea(memory, bits, Size::Byte)?;
                    // M68000PRM Scc, p.4-173: MC68000 memory destinations
                    // are read before being written, even for ST/SF.
                    self.get(memory, dst, Size::Byte)?;
                    self.put(
                        memory,
                        dst,
                        Size::Byte,
                        if self.condition(op >> 8) { 255 } else { 0 },
                    )?;
                    return Ok(if bits & 0x38 == 0 {
                        if self.condition(op >> 8) { 6 } else { 4 }
                    } else {
                        8 + Self::ea_clocks(bits, Size::Byte)
                    });
                }
                let size = Self::size(op)?;
                let q = if r == 0 { 8 } else { r as u32 };
                let address = bits & 0x38 == 8;
                Self::require(if address {
                    !matches!(size, Size::Byte)
                } else {
                    Self::data(bits, true)
                })?;
                let size = if address { Size::Long } else { size };
                let dst = self.ea(memory, bits, size)?;
                let a = self.get(memory, dst, size)?;
                let result = if address {
                    if op & 0x100 != 0 {
                        a.wrapping_sub(q)
                    } else {
                        a.wrapping_add(q)
                    }
                } else {
                    self.arithmetic(a, q, size, op & 0x100 != 0, false, false)
                };
                self.put(memory, dst, size, result)?;
                Ok(if address {
                    8
                } else if bits & 0x38 == 0 {
                    if matches!(size, Size::Long) { 8 } else { 4 }
                } else {
                    (if matches!(size, Size::Long) { 12 } else { 8 }) + Self::ea_clocks(bits, size)
                })
            }
            6 => {
                // Bcc / BRA / BSR; displacement base is extension-word address.
                let base = self.state.pc;
                let d = if op & 255 == 0 {
                    self.next(memory)? as i16 as i32
                } else {
                    op as i8 as i32
                };
                let condition = (op >> 8) & 15;
                if condition == 1 {
                    self.push(memory, Size::Long, self.state.pc)?;
                }
                let taken = condition <= 1 || self.condition(condition);
                if taken {
                    self.redirect(memory, base.wrapping_add(d as u32))?;
                }
                Ok(if condition == 1 {
                    18
                } else if taken {
                    10
                } else if op & 255 == 0 {
                    12
                } else {
                    8
                })
            }
            7 => {
                Self::require(op & 0x100 == 0)?;
                self.state.d[r] = op as i8 as i32 as u32;
                self.logic(self.state.d[r], Size::Long);
                Ok(4)
            }
            8 | 9 | 11 | 12 | 13 => self.alu(memory, op),
            10 => Err(Fault::Vector(10, false)),
            14 => self.shift(memory, op),
            _ => Err(Fault::Vector(11, false)),
        }
    }
    fn group0(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let bits = op & 63;
        let r = ((op >> 9) & 7) as usize;
        if op & 0xF138 == 0x0108 {
            // MOVEP, spaced bytes, either alignment.
            let size = if op & 0x40 == 0 {
                Size::Word
            } else {
                Size::Long
            };
            let d = self.next(memory)? as i16 as i32 as u32;
            let a = self.state.a[(op & 7) as usize].wrapping_add(d);
            if op & 0x80 == 0 {
                let mut v = 0;
                for i in 0..size.bytes() {
                    v = v << 8 | self.read(memory, a.wrapping_add(i * 2), Size::Byte, false)?;
                }
                self.put(memory, Ea::Data(r), size, v)?;
            } else {
                for i in 0..size.bytes() {
                    self.write(
                        memory,
                        a.wrapping_add(i * 2),
                        Size::Byte,
                        self.state.d[r] >> ((size.bytes() - 1 - i) * 8),
                    )?;
                }
            }
            return Ok(if matches!(size, Size::Long) { 24 } else { 16 });
        }
        if op & 0x100 != 0 || op & 0xFF00 == 0x0800 {
            // bit operations
            let bit = if op & 0x100 != 0 {
                self.state.d[r]
            } else {
                u32::from(self.next(memory)?)
            };
            let kind = (op >> 6) & 3;
            Self::require(Self::data(bits, kind != 0) && (bits != 0x3C || op & 0x100 != 0))?;
            let size = if bits & 0x38 == 0 {
                Size::Long
            } else {
                Size::Byte
            };
            let dst = self.ea(memory, bits, size)?;
            let value = self.get(memory, dst, size)?;
            let mask = 1 << (bit % (size.bytes() * 8));
            self.flag(Z, value & mask == 0);
            if kind != 0 {
                self.put(
                    memory,
                    dst,
                    size,
                    match kind {
                        1 => value ^ mask,
                        2 => value & !mask,
                        _ => value | mask,
                    },
                )?;
            }
            let base = if bits & 0x38 == 0 {
                match kind {
                    0 => 6,
                    2 => 10,
                    _ => 8,
                }
            } else if kind == 0 {
                4
            } else {
                8
            };
            return Ok(base
                + if op & 0x100 == 0 { 4 } else { 0 }
                + if bits & 0x38 == 0 {
                    0
                } else {
                    Self::ea_clocks(bits, Size::Byte)
                });
        }
        let kind = (op >> 8) & 15;
        Self::require(matches!(kind, 0 | 2 | 4 | 6 | 10 | 12))?;
        if matches!(op, 0x003C | 0x007C | 0x023C | 0x027C | 0x0A3C | 0x0A7C) {
            let full = op & 0x40 != 0;
            if full {
                self.supervisor()?;
            }
            let v = self.next(memory)? & if full { 0xFFFF } else { 0xFF };
            let value = match kind {
                0 => self.state.sr | v,
                2 => self.state.sr & (v | if full { 0 } else { 0xFF00 }),
                _ => self.state.sr ^ v,
            };
            self.sr(value);
            return Ok(20);
        }
        let size = Self::size(op)?;
        Self::require(Self::data(op & 63, true))?;
        let src = self.immediate(memory, size)?;
        let dst = self.ea(memory, bits, size)?;
        let old = self.get(memory, dst, size)?;
        let result = match kind {
            0 => old | src,
            2 => old & src,
            4 | 6 | 12 => self.arithmetic(old, src, size, kind != 6, false, kind == 12),
            _ => old ^ src,
        };
        if matches!(kind, 0 | 2 | 10) {
            self.logic(result, size);
        }
        if kind != 12 {
            self.put(memory, dst, size, result)?;
        }
        let long = matches!(size, Size::Long);
        Ok(if bits & 0x38 == 0 {
            if long {
                if matches!(kind, 2 | 12) { 14 } else { 16 }
            } else {
                8
            }
        } else {
            (if kind == 12 {
                if long { 12 } else { 8 }
            } else if long {
                20
            } else {
                12
            }) + Self::ea_clocks(bits, size)
        })
    }
    fn group4(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let bits = op & 63;
        let r = (op & 7) as usize;
        match op {
            0x4E70 => {
                self.supervisor()?;
                self.reset_output = true;
                // Suspend before trace processing or the next instruction can
                // publish any bus cycle during the external reset interval.
                memory.m68k_reset_output().map_err(|_| Fault::Suspended)?;
                return Ok(132);
            } // RESET: device reset output, handled by sound board.
            0x4E71 => return Ok(4),
            0x4E72 => {
                self.supervisor()?;
                let sr = self.next(memory)?;
                self.sr(sr);
                self.state.stopped = true;
                return Ok(4);
            }
            0x4E73 => {
                self.supervisor()?;
                let sr = self.pop(memory, Size::Word)? as u16;
                let pc = self.pop(memory, Size::Long)?;
                self.sr(sr);
                self.redirect(memory, pc)?;
                return Ok(20);
            }
            0x4E75 => {
                let pc = self.pop(memory, Size::Long)?;
                self.redirect(memory, pc)?;
                return Ok(16);
            }
            0x4E76 => {
                if self.state.sr & V != 0 {
                    return Err(Fault::Vector(7, true));
                }
                return Ok(4);
            }
            0x4E77 => {
                let ccr = self.pop(memory, Size::Word)? as u16;
                self.state.sr = (self.state.sr & !31) | (ccr & 31);
                let pc = self.pop(memory, Size::Long)?;
                self.redirect(memory, pc)?;
                return Ok(20);
            }
            _ => (),
        }
        if op & 0xFFF0 == 0x4E40 {
            return Err(Fault::Vector(32 + (op & 15) as u8, true));
        }
        if op & 0xFFF8 == 0x4E50 {
            // LINK
            let d = self.next(memory)? as i16 as i32 as u32;
            let old = self.state.a[r];
            self.push(memory, Size::Long, old)?;
            self.state.a[r] = self.state.a[7];
            self.state.a[7] = self.state.a[7].wrapping_add(d);
            return Ok(16);
        }
        if op & 0xFFF8 == 0x4E58 {
            self.state.a[7] = self.state.a[r];
            // M68000PRM p.4-194: An -> SP; (SP) -> An; SP+4 -> SP.
            // In particular UNLK A7 increments the *loaded* SP. A generic
            // pop increments the old pointer before assigning An and loses
            // this final increment when the two registers alias. Do not
            // publish either the loaded register or increment on a failed
            // first/second operand word.
            self.state.a[r] = self.read(memory, self.state.a[7], Size::Long, false)?;
            self.state.a[7] = self.state.a[7].wrapping_add(4);
            return Ok(12);
        }
        if op & 0xFFF0 == 0x4E60 {
            self.supervisor()?;
            if op & 8 == 0 {
                self.state.usp = self.state.a[r];
            } else {
                self.state.a[r] = self.state.usp;
            }
            return Ok(4);
        }
        if op & 0xFFC0 == 0x4E80 || op & 0xFFC0 == 0x4EC0 {
            Self::require(Self::control(bits))?;
            let Ea::Memory(a) = self.ea(memory, bits, Size::Long)? else {
                unreachable!()
            };
            if op & 0x40 == 0 {
                self.push(memory, Size::Long, self.state.pc)?;
            }
            self.redirect(memory, a)?;
            return Ok(Self::jump_clocks(bits) + if op & 0x40 == 0 { 8 } else { 0 });
        }
        if op & 0xF1C0 == 0x41C0 {
            Self::require(Self::control(bits))?;
            let Ea::Memory(a) = self.ea(memory, bits, Size::Long)? else {
                unreachable!()
            };
            self.state.a[((op >> 9) & 7) as usize] = a;
            return Ok(Self::lea_clocks(bits));
        }
        if op & 0xF1C0 == 0x4180 {
            // CHK.W signed
            Self::require(Self::data(bits, false))?;
            let ea = self.ea(memory, bits, Size::Word)?;
            let bound = self.get(memory, ea, Size::Word)? as i16;
            let value = self.state.d[((op >> 9) & 7) as usize] as i16;
            if value < 0 || value > bound {
                self.flag(N, value < 0);
                return Err(Fault::Vector(6, true));
            }
            return Ok(10 + Self::ea_clocks(bits, Size::Word));
        }
        if op & 0xFFF8 == 0x4840 {
            self.state.d[r] = self.state.d[r].rotate_left(16);
            self.logic(self.state.d[r], Size::Long);
            return Ok(4);
        }
        if op & 0xFFC0 == 0x4840 {
            Self::require(Self::control(bits))?;
            let Ea::Memory(a) = self.ea(memory, bits, Size::Long)? else {
                unreachable!()
            };
            self.push(memory, Size::Long, a)?;
            return Ok(8 + Self::lea_clocks(bits));
        }
        if op & 0xFFF8 == 0x4880 || op & 0xFFF8 == 0x48C0 {
            // EXT
            let size = if op & 0x40 == 0 {
                Size::Word
            } else {
                Size::Long
            };
            let v = if op & 0x40 == 0 {
                Size::Byte.extend(self.state.d[r])
            } else {
                Size::Word.extend(self.state.d[r])
            };
            self.put(memory, Ea::Data(r), size, v)?;
            self.logic(v, size);
            return Ok(4);
        }
        if op & 0xFB80 == 0x4880 {
            return self.movem(memory, op);
        }
        if matches!(op & 0xFFC0, 0x40C0 | 0x44C0 | 0x46C0) {
            let from = op & 0x0600 == 0;
            Self::require(Self::data(bits, from))?;
            if op & 0x0600 == 0x0600 {
                self.supervisor()?;
            }
            let ea = self.ea(memory, bits, Size::Word)?;
            if from {
                // M68000PRM MOVE from SR, p.4-125: the otherwise unused
                // destination read has real MMIO and bus-error effects.
                self.get(memory, ea, Size::Word)?;
                self.put(memory, ea, Size::Word, u32::from(self.state.sr))?;
            } else {
                let v = self.get(memory, ea, Size::Word)? as u16;
                if op & 0x0200 == 0 {
                    self.state.sr = (self.state.sr & !31) | (v & 31);
                } else {
                    self.sr(v);
                }
            }
            return Ok(if from {
                if bits & 0x38 == 0 {
                    6
                } else {
                    8 + Self::ea_clocks(bits, Size::Word)
                }
            } else {
                12 + Self::ea_clocks(bits, Size::Word)
            });
        }
        if op & 0xFFC0 == 0x4800 {
            // NBCD
            Self::require(Self::data(bits, true))?;
            let ea = self.ea(memory, bits, Size::Byte)?;
            let value = self.get(memory, ea, Size::Byte)?;
            let v = self.bcd(0, value, true);
            self.put(memory, ea, Size::Byte, v)?;
            return Ok((if bits & 0x38 == 0 { 6 } else { 8 }) + Self::ea_clocks(bits, Size::Byte));
        }
        if op & 0xFFC0 == 0x4AC0 {
            // TAS
            Self::require(Self::data(bits, true))?;
            let ea = self.ea(memory, bits, Size::Byte)?;
            let value = self.get(memory, ea, Size::Byte)?;
            self.logic(value, Size::Byte);
            self.put(memory, ea, Size::Byte, value | 0x80)?;
            return Ok((if bits & 0x38 == 0 { 4 } else { 14 }) + Self::ea_clocks(bits, Size::Byte));
        }
        Self::require(matches!(
            op & 0xFF00,
            0x4000 | 0x4200 | 0x4400 | 0x4600 | 0x4A00
        ))?;
        let size = Self::size(op)?;
        Self::require(Self::data(bits, true))?;
        let ea = self.ea(memory, bits, size)?;
        // 68000 CLR performs a read-modify-write bus cycle, even though data are unused.
        let value = self.get(memory, ea, size)?;
        let result = match op & 0xFF00 {
            0x4000 => self.arithmetic(0, value, size, true, true, false),
            0x4400 => self.arithmetic(0, value, size, true, false, false),
            0x4600 => {
                let v = !value & size.mask();
                self.logic(v, size);
                v
            }
            0x4200 => {
                self.logic(0, size);
                0
            }
            _ => {
                self.logic(value, size);
                return Ok(4 + Self::ea_clocks(bits, size));
            }
        };
        self.put(memory, ea, size, result)?;
        Ok((if bits & 0x38 == 0 {
            if matches!(size, Size::Long) { 6 } else { 4 }
        } else if matches!(size, Size::Long) {
            12
        } else {
            8
        }) + Self::ea_clocks(bits, size))
    }
    fn movem(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let bits = op & 63;
        let mode = bits & 0x38;
        let reg = (bits & 7) as usize;
        let load = op & 0x400 != 0;
        Self::require(if load {
            Self::control(bits) || mode == 0x18
        } else {
            mode == 0x20 || Self::memory_alterable(bits) && mode != 0x18
        })?;
        self.defer_prefetch = load;
        let mask = self.next(memory)?;
        let size = if op & 0x40 != 0 {
            Size::Long
        } else {
            Size::Word
        };
        let mut address = if mode == 0x20 || mode == 0x18 {
            self.state.a[reg]
        } else {
            let Ea::Memory(a) = self.ea(memory, bits, size)? else {
                unreachable!()
            };
            a
        };
        let original_base = self.state.a[reg];
        // Predecrement mask is reversed; on 68000 a listed base register stores its original value.
        for bit in 0..16 {
            if mask & (1 << bit) == 0 {
                continue;
            }
            let n = if mode == 0x20 { 15 - bit } else { bit };
            if mode == 0x20 {
                address = address.wrapping_sub(size.bytes());
                self.state.a[reg] = address;
            }
            if load {
                let v = size.extend(self.read(memory, address, size, false)?);
                if n < 8 {
                    self.state.d[n] = v;
                } else {
                    self.state.a[n - 8] = v;
                }
            } else {
                let v = if n < 8 {
                    self.state.d[n]
                } else {
                    if mode == 0x20 && n - 8 == reg {
                        original_base
                    } else {
                        self.state.a[n - 8]
                    }
                };
                if mode == 0x20 {
                    self.write_predecrement(memory, address, size, v)?;
                } else {
                    self.write(memory, address, size, v)?;
                }
            }
            if mode != 0x20 {
                address = address.wrapping_add(size.bytes());
                if mode == 0x18 {
                    self.state.a[reg] = address;
                }
            }
        }
        if mode == 0x20 || mode == 0x18 {
            self.state.a[reg] = address;
        }
        if load {
            // MC68000UM table8-10: the load direction has one extra read
            // independent of the mask and operand size. It reads the word
            // following the operand burst, including for an empty list. The
            // data are discarded, but device effects and BERR are real. This
            // cycle never changes a listed register or advances the base.
            self.read(memory, address, Size::Word, false)?;
            self.defer_prefetch = false;
            self.refill(memory, 1, false)?;
        }
        let address_clocks = if mode == 0x20 {
            0
        } else {
            Self::ea_clocks(bits, Size::Word) - 4
        };
        Ok((if load { 12 } else { 8 })
            + address_clocks
            + mask.count_ones() * if matches!(size, Size::Long) { 8 } else { 4 })
    }
    fn bcd(&mut self, dst: u32, src: u32, sub: bool) -> u32 {
        let x = i32::from(self.state.sr & X != 0);
        let a = (dst & 255) as i32;
        let b = (src & 255) as i32;
        let binary = if sub { a - b - x } else { a + b + x };
        let mut result = binary;
        if sub {
            if (a & 15) - (b & 15) - x < 0 {
                result -= 6;
            }
            if binary < 0 {
                result -= 0x60;
            }
        } else {
            if (a & 15) + (b & 15) + x > 9 {
                result += 6;
            }
            if result > 0x99 {
                result += 0x60;
            }
        }
        let carry = if sub { binary < 0 } else { result > 255 };
        let z = self.state.sr & Z != 0;
        // N/V are architecturally undefined for BCD. Choose binary-to-adjusted flags.
        self.flag(N, result & 128 != 0);
        self.flag(V, (binary ^ result) & 128 != 0);
        self.flag(C, carry);
        self.flag(X, carry);
        self.flag(Z, z && result & 255 == 0);
        result as u32 & 255
    }
    fn alu(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let group = op >> 12;
        let bits = op & 63;
        let r = ((op >> 9) & 7) as usize;
        let src_r = (op & 7) as usize;
        let mode = (op >> 6) & 7;
        if matches!(group, 8 | 12) && op & 0x01F0 == 0x0100 {
            // SBCD/ABCD
            let src = if op & 8 == 0 {
                Ea::Data(src_r)
            } else {
                self.ea(memory, 0x20 | src_r as u16, Size::Byte)?
            };
            let b = self.get(memory, src, Size::Byte)?;
            let dst = if op & 8 == 0 {
                Ea::Data(r)
            } else {
                self.ea(memory, 0x20 | r as u16, Size::Byte)?
            };
            let a = self.get(memory, dst, Size::Byte)?;
            let v = self.bcd(a, b, group == 8);
            self.put(memory, dst, Size::Byte, v)?;
            return Ok(if op & 8 == 0 { 6 } else { 18 });
        }
        if group == 12 && matches!(op & 0xF1F8, 0xC140 | 0xC148 | 0xC188) {
            match op & 0xF1F8 {
                0xC140 => self.state.d.swap(r, src_r),
                0xC148 => self.state.a.swap(r, src_r),
                _ => std::mem::swap(&mut self.state.d[r], &mut self.state.a[src_r]),
            }
            return Ok(6);
        }
        if matches!(group, 9 | 13) && op & 0x0130 == 0x0100 && mode != 7 && mode != 3 {
            // SUBX/ADDX
            let size = Self::size(op)?;
            let src = if op & 8 == 0 {
                Ea::Data(src_r)
            } else {
                self.ea(memory, 0x20 | src_r as u16, size)?
            };
            let b = self.get(memory, src, size)?;
            let dst = if op & 8 == 0 {
                Ea::Data(r)
            } else {
                self.ea(memory, 0x20 | r as u16, size)?
            };
            let a = self.get(memory, dst, size)?;
            let v = self.arithmetic(a, b, size, group == 9, true, false);
            self.put(memory, dst, size, v)?;
            return Ok(if op & 8 == 0 {
                if matches!(size, Size::Long) { 8 } else { 4 }
            } else if matches!(size, Size::Long) {
                30
            } else {
                18
            });
        }
        if group == 11 && op & 0x0138 == 0x0108 && mode != 7 && mode != 3 {
            // CMPM
            let size = Self::size(op)?;
            let src = self.ea(memory, 0x18 | src_r as u16, size)?;
            let b = self.get(memory, src, size)?;
            let dst = self.ea(memory, 0x18 | r as u16, size)?;
            let a = self.get(memory, dst, size)?;
            self.arithmetic(a, b, size, true, false, true);
            return Ok(if matches!(size, Size::Long) { 20 } else { 12 });
        }
        if mode == 3 || mode == 7 {
            if matches!(group, 9 | 11 | 13) {
                // ADDA, SUBA, CMPA (allow An source)
                let size = if mode == 3 { Size::Word } else { Size::Long };
                let ea = self.ea(memory, bits, size)?;
                let v = size.extend(self.get(memory, ea, size)?);
                let old = self.state.a[r];
                if group == 11 {
                    self.arithmetic(old, v, Size::Long, true, false, true);
                } else {
                    self.state.a[r] = if group == 9 {
                        old.wrapping_sub(v)
                    } else {
                        old.wrapping_add(v)
                    };
                }
                let base = if group == 11 {
                    6
                } else if matches!(size, Size::Word) || bits & 0x38 <= 8 || bits == 0x3C {
                    8
                } else {
                    6
                };
                return Ok(base + Self::ea_clocks(bits, size));
            }
            Self::require(Self::data(bits, false))?;
            let ea = self.ea(memory, bits, Size::Word)?;
            let value = self.get(memory, ea, Size::Word)?;
            let dst = self.state.d[r];
            if group == 12 {
                // MULU/MULS
                let v = if mode == 3 {
                    (dst & 65535) * value
                } else {
                    (dst as i16 as i32).wrapping_mul(value as i16 as i32) as u32
                };
                self.state.d[r] = v;
                self.logic(v, Size::Long);
                // MC68000UM table8-4: MULU counts set source bits; MULS
                // counts adjacent 01/10 pairs in the signed source with a zero
                // appended at bit0. No operand is reread for timing.
                let n = if mode == 3 {
                    value.count_ones()
                } else {
                    ((value ^ (value << 1)) & 0xFFFF).count_ones()
                };
                return Ok(38 + 2 * n + Self::ea_clocks(bits, Size::Word));
            }
            if value == 0 {
                return Err(Fault::Vector(5, true));
            }
            let (q, rem, overflow) = if mode == 3 {
                (dst / value, dst % value, dst / value > 65535)
            } else {
                let dividend = i64::from(dst as i32);
                let divisor = i64::from(value as i16);
                let q = dividend / divisor;
                (
                    (q as i32) as u32,
                    (dividend % divisor) as u32,
                    !(-32768..=32767).contains(&q),
                )
            };
            self.flag(C, false);
            self.flag(V, overflow);
            if !overflow {
                self.state.d[r] = (rem << 16) | (q & 65535);
                self.logic(q, Size::Word);
            }
            // The primary timing table specifies a maximum, but not a
            // data-dependent DIV microsequence. Preserve that explicit bound.
            return Ok((if mode == 3 { 140 } else { 158 }) + Self::ea_clocks(bits, Size::Word));
        }
        let size = Self::size(op)?;
        let to_ea = mode & 4 != 0;
        Self::require(if to_ea {
            if group == 11 {
                Self::data(bits, true)
            } else {
                Self::memory_alterable(bits)
            }
        } else {
            Self::data(bits, false)
                || (!matches!(size, Size::Byte) && matches!(group, 9 | 11 | 13) && bits & 0x38 == 8)
        })?;
        let ea = self.ea(memory, bits, size)?;
        let ea_value = self.get(memory, ea, size)?;
        let (a, b) = if to_ea {
            (ea_value, self.state.d[r] & size.mask())
        } else {
            (self.state.d[r], ea_value)
        };
        let v = match group {
            8 => a | b,
            12 => a & b,
            11 if to_ea => a ^ b,
            _ => self.arithmetic(a, b, size, group != 13, false, group == 11),
        };
        if matches!(group, 8 | 12) || group == 11 && to_ea {
            self.logic(v, size);
        }
        if group != 11 || to_ea {
            self.put(memory, if to_ea { ea } else { Ea::Data(r) }, size, v)?;
        }
        let long = matches!(size, Size::Long);
        let base = if to_ea {
            if bits & 0x38 == 0 {
                if long { 8 } else { 4 }
            } else if long {
                12
            } else {
                8
            }
        } else if !long {
            4
        } else if group == 11 {
            6
        } else if bits & 0x38 <= 8 || bits == 0x3C {
            8
        } else {
            6
        };
        Ok(base + Self::ea_clocks(bits, size))
    }
    fn shift(&mut self, memory: &mut impl Memory, op: u16) -> Result<u32> {
        let memory_form = op & 0xC0 == 0xC0;
        let size = if memory_form {
            Size::Word
        } else {
            Self::size(op)?
        };
        let kind = if memory_form {
            (op >> 9) & 3
        } else {
            (op >> 3) & 3
        };
        let left = op & 0x100 != 0;
        let count = if memory_form {
            Self::require(op & 0x800 == 0 && Self::memory_alterable(op & 63))?;
            1
        } else if op & 0x20 != 0 {
            self.state.d[((op >> 9) & 7) as usize] & 63
        } else {
            let n = (op >> 9) & 7;
            u32::from(if n == 0 { 8 } else { n })
        };
        let ea = if memory_form {
            self.ea(memory, op & 63, size)?
        } else {
            Ea::Data((op & 7) as usize)
        };
        let mut v = self.get(memory, ea, size)?;
        let mut carry = false;
        let mut overflow = false;
        let mut x = self.state.sr & X != 0;
        for _ in 0..count {
            let old = v;
            carry = if left {
                v & size.sign() != 0
            } else {
                v & 1 != 0
            };
            v = if left { (v << 1) & size.mask() } else { v >> 1 };
            match kind {
                0 => {
                    if !left {
                        v |= old & size.sign();
                    }
                    overflow |= (old ^ v) & size.sign() != 0;
                }
                2 => {
                    if x {
                        v |= if left { 1 } else { size.sign() };
                    }
                }
                3 if carry => {
                    v |= if left { 1 } else { size.sign() };
                }
                _ => (),
            }
            if kind != 3 {
                x = carry;
            }
        }
        self.logic(v, size);
        self.flag(C, if count == 0 && kind == 2 { x } else { carry });
        self.flag(V, kind == 0 && left && overflow);
        if kind != 3 {
            self.flag(X, x);
        }
        self.put(memory, ea, size, v)?;
        Ok(if memory_form {
            8 + Self::ea_clocks(op & 63, Size::Word)
        } else {
            (if matches!(size, Size::Long) { 8 } else { 6 }) + count * 2
        })
    }
}

#[cfg(test)]
mod bus_tests;
#[cfg(test)]
mod tests;
