//! Independently written SH-2 reference interpreter. See docs/hardware-conformance.md.
use crate::bus::{BusError, Memory};
use std::fmt;

mod memory;
mod timing;
use memory::Operands;

const T: u32 = 1;
const S: u32 = 2;
const Q: u32 = 0x100;
const M: u32 = 0x200;
const SR_MASK: u32 = 0x3F3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    NotRunning,
    Bus {
        pc: u32,
        source: BusError,
    },
    ExceptionEntry {
        pc: u32,
        vector: u8,
        source: BusError,
    },
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "CPU is stopped; reset before stepping"),
            Self::Bus { pc, source } => write!(f, "Memory access at PC 0x{pc:08X}: {source}"),
            Self::ExceptionEntry { pc, vector, source } => write!(
                f,
                "Exception vector {vector} entry at PC 0x{pc:08X}: {source}"
            ),
        }
    }
}
impl std::error::Error for CpuError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuStatus {
    Reset,
    Running,
    Sleeping,
    Halted,
    Faulted(CpuError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exception {
    pub vector: u8,
    pub saved_pc: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuState {
    pub r: [u32; 16],
    /// Next fetch address; while sleeping, the instruction after SLEEP.
    pub pc: u32,
    pub sr: u32,
    pub pr: u32,
    pub gbr: u32,
    pub vbr: u32,
    pub mach: u32,
    pub macl: u32,
    /// Retired instructions; illegal instructions and interrupt entry do not retire.
    pub instructions: u64,
    pub delay_target: Option<u32>,
    pub exceptions: u64,
    pub last_exception: Option<Exception>,
    /// Latched CPU address error, accepted before the next eligible fetch.
    pub address_error_pending: bool,
    pub status: CpuStatus,
}

impl Default for CpuState {
    fn default() -> Self {
        Self {
            r: [0; 16],
            pc: 0,
            sr: 0xF0,
            pr: 0,
            gbr: 0,
            vbr: 0,
            mach: 0,
            macl: 0,
            instructions: 0,
            delay_target: None,
            exceptions: 0,
            last_exception: None,
            address_error_pending: false,
            status: CpuStatus::Reset,
        }
    }
}

impl CpuState {
    fn set_t(&mut self, value: bool) {
        self.sr = (self.sr & !T) | u32::from(value);
    }

    fn accumulator(&self) -> u64 {
        ((self.mach as u64) << 32) | self.macl as u64
    }

    fn set_accumulator(&mut self, value: u64) {
        self.mach = (value >> 32) as u32;
        self.macl = value as u32;
    }
}

#[derive(Debug, Default)]
pub struct Cpu {
    state: CpuState,
    /// System/control transfers and delayed conditional branches inhibit interrupts
    /// for one successor, including when the conditional branch is untaken.
    interrupt_inhibited: bool,
    /// Unlike control transfers, delayed branches also defer address errors.
    address_error_inhibited: bool,
    /// Main-clock execution states for the last issued boundary, excluding
    /// external bus waits. Sleeping polls issue no instruction and report zero.
    last_clocks: u32,
}

impl Cpu {
    pub fn state(&self) -> &CpuState {
        &self.state
    }

    pub fn last_clocks(&self) -> u32 {
        self.last_clocks
    }

    pub fn start(pc: u32, sp: u32) -> Self {
        let mut state = CpuState {
            pc,
            status: CpuStatus::Running,
            ..CpuState::default()
        };
        state.r[15] = sp;
        Self {
            state,
            interrupt_inhibited: false,
            address_error_inhibited: false,
            last_clocks: 0,
        }
    }

    /// Power-on vectors are physical 0 and 4, not VBR-relative.
    pub fn power_on(bus: &impl Memory) -> Result<Self, BusError> {
        Ok(Self::start(bus.read32(0)?, bus.read32(4)?))
    }

    /// SH7604 table 4.3: manual reset vectors are physical 8/12.
    pub fn manual_reset(bus: &impl Memory) -> Result<Self, BusError> {
        Ok(Self::start(bus.read32(8)?, bus.read32(12)?))
    }

    /// Debugger stop, separate from the architectural SLEEP state.
    pub fn halt(&mut self) {
        self.state.status = CpuStatus::Halted;
    }

    /// The caller supplies the winning hardware interrupt and retries if deferred.
    /// This is CPU entry only; device requests/acknowledgment are not synthesized here.
    pub fn accept_interrupt(
        &mut self,
        bus: &mut impl Memory,
        level: u8,
        vector: u8,
    ) -> Result<bool, CpuError> {
        if !(1..=15).contains(&level) || level as u32 <= (self.state.sr >> 4) & 15 {
            return Ok(false);
        }
        self.interrupt(bus, vector, level)
    }

    /// DMA address errors use vector 10 and cannot be masked by SR.I.
    pub fn accept_dma_address_error(&mut self, bus: &mut impl Memory) -> Result<bool, CpuError> {
        if !matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            || self.state.delay_target.is_some()
            || self.address_error_inhibited
            || self.state.address_error_pending
        {
            return Ok(false);
        }
        let result = self.enter_exception(bus, 10, self.state.pc);
        if let Err(error) = result {
            self.state.status = CpuStatus::Faulted(error.clone());
            return Err(error);
        }
        Ok(true)
    }

    /// NMI has vector 11 and raises the interrupt mask to 15.
    pub fn accept_nmi(&mut self, bus: &mut impl Memory) -> Result<bool, CpuError> {
        self.interrupt(bus, 11, 15)
    }

    fn interrupt(
        &mut self,
        bus: &mut impl Memory,
        vector: u8,
        level: u8,
    ) -> Result<bool, CpuError> {
        if !matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            || self.state.delay_target.is_some()
            || self.interrupt_inhibited
            || self.state.address_error_pending
        {
            return Ok(false);
        }
        let pc = self.state.pc;
        let result = self.enter_exception(bus, vector, pc);
        if let Err(error) = result {
            self.state.status = CpuStatus::Faulted(error.clone());
            return Err(error);
        }
        self.state.sr = (self.state.sr & !0xF0) | (level as u32) << 4;
        Ok(true)
    }

    /// Bounded: sleeping calls return without fetching or retiring an instruction.
    /// Faults caused by unavailable host bus/device behavior stay sticky until reset.
    pub fn step(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        self.last_clocks = 0;
        let pc = self.state.pc;
        if matches!(self.state.status, CpuStatus::Running | CpuStatus::Sleeping)
            && !self.state.address_error_pending
            && bus.break_pending()
            && 15 > (self.state.sr >> 4) & 15
            && self.interrupt(bus, 12, 15)?
        {
            bus.acknowledge_break();
            return Ok(());
        }
        match &self.state.status {
            CpuStatus::Running => (),
            CpuStatus::Sleeping if !self.state.address_error_pending => return Ok(()),
            CpuStatus::Sleeping => (),
            CpuStatus::Faulted(error) => return Err(error.clone()),
            _ => return Err(CpuError::NotRunning),
        }
        let retired = self.state.instructions;
        let result = if self.state.address_error_pending && !self.address_error_inhibited {
            self.address_error(bus)
        } else {
            self.execute(bus)
        };
        if let Err(error) = &result {
            self.state.status = CpuStatus::Faulted(error.clone());
        }
        if self.state.instructions != retired {
            bus.instruction_break(pc, true);
        }
        result
    }

    fn enter_exception(
        &mut self,
        bus: &mut impl Memory,
        vector: u8,
        saved_pc: u32,
    ) -> Result<(), CpuError> {
        // H §5.5 table5.9: acceptance to handler fetch is 5+m1+m2+m3;
        // the three single-state memory operations are included here. The
        // bus supplies additional waits independently to the machine scheduler.
        self.last_clocks = 8;
        let s = &mut self.state;
        let access = |source| CpuError::ExceptionEntry {
            pc: s.pc,
            vector,
            source,
        };
        // Bus operations are ordered SR push, PC push, vector fetch (SH7604 §4.1.2).
        // A failed entry is a bounded diagnostic, not recursive host exception dispatch.
        let sp = s.r[15].wrapping_sub(8);
        bus.exception_write32(sp.wrapping_add(4), s.sr)
            .map_err(access)?;
        bus.exception_write32(sp, saved_pc).map_err(access)?;
        let target = bus
            .read32(s.vbr.wrapping_add(vector as u32 * 4))
            .map_err(access)?;
        // Accept the first stacking error after completing this exception.
        // Vector 9 stacking must not recursively re-enter forever (§4.8.3).
        if sp & 3 != 0 && vector != 9 {
            s.address_error_pending = true;
        }
        s.r[15] = sp;
        s.pc = target;
        s.delay_target = None;
        s.exceptions += 1;
        s.last_exception = Some(Exception { vector, saved_pc });
        s.status = CpuStatus::Running;
        self.interrupt_inhibited = false;
        self.address_error_inhibited = false;
        Ok(())
    }

    fn address_error(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        // SH7604 §§4.3.2/4.7: save the next instruction, without changing I.
        self.enter_exception(bus, 9, self.state.pc)?;
        self.state.address_error_pending = false;
        Ok(())
    }

    fn illegal(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        // SH7604 §4.5.3: a slot exception saves the pending jump destination.
        let (vector, saved_pc) = self
            .state
            .delay_target
            .map_or((4, self.state.pc), |target| (6, target));
        self.enter_exception(bus, vector, saved_pc)
    }

    fn execute(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        let pc = self.state.pc;
        let access = |source| CpuError::Bus { pc, source };
        // SH7604 Table 4.6: no instruction fetches from odd addresses or
        // on-chip peripheral modules. Do not send such reads to a device.
        if pc & 1 != 0 || pc >= 0xFFFF_FE00 {
            self.state.address_error_pending = true;
            if !self.address_error_inhibited {
                return self.address_error(bus);
            }
            // An unfetchable successor cannot decode/retire. Finish the branch
            // boundary before accepting the latched error (Table 4.10).
            self.state.pc = self.state.delay_target.take().unwrap_or(pc.wrapping_add(2));
            self.address_error_inhibited = false;
            self.interrupt_inhibited = false;
            return Ok(());
        }
        let opcode = bus.fetch16(pc).map_err(access)?;
        self.last_clocks = timing::execution_states(opcode, self.state.sr & T != 0);
        // H §6.3.2: a before-execution break requires a successfully fetched
        // instruction. A fetch failure or speculative fill cannot cause it.
        if bus.instruction_break(pc, false)
            && 15 > (self.state.sr >> 4) & 15
            && self.interrupt(bus, 12, 15)?
        {
            bus.acknowledge_break();
            return Ok(());
        }
        let rewrites_pc = matches!(opcode & 0xF000, 0xA000 | 0xB000)
            || matches!(opcode & 0xFF00, 0x8900 | 0x8B00 | 0x8D00 | 0x8F00 | 0xC300)
            || matches!(opcode & 0xF0FF, 0x0003 | 0x0023 | 0x400B | 0x402B)
            || matches!(opcode, 0x000B | 0x002B);
        if rewrites_pc && self.state.delay_target.is_some() {
            return self.illegal(bus);
        }
        if opcode & 0xFF00 == 0xC300 {
            self.enter_exception(bus, opcode as u8, pc.wrapping_add(2))?;
            self.state.instructions += 1;
            return Ok(());
        }

        // Invalid operand cycles latch vector 9, allowing the instruction to
        // complete before exception acceptance, including auto-update effects.
        let mut operands = Operands::new(bus);
        let bus = &mut operands;

        let n = ((opcode >> 8) & 15) as usize;
        let m = ((opcode >> 4) & 15) as usize;
        let imm = opcode as u8 as u32;
        let signed_imm = opcode as u8 as i8 as i32 as u32;
        let s = &mut self.state;
        let mut next_branch = None;
        let mut sequential_pc = pc.wrapping_add(2);
        let mut inhibit = false;
        let operand_pc = s
            .delay_target
            .map_or(pc.wrapping_add(4), |target| target.wrapping_add(2));

        match opcode {
            0x0008 => s.sr &= !T, // CLRT
            0x0018 => s.sr |= T,  // SETT
            0x0028 => {
                s.mach = 0;
                s.macl = 0;
            } // CLRMAC
            0x0019 => s.sr &= !(M | Q | T), // DIV0U
            0x0009 => (),
            0x000B => next_branch = Some(s.pr),
            0x001B => {
                s.status = CpuStatus::Sleeping;
                bus.inner.enter_sleep();
            }
            0x002B => {
                // RTE: restore SR before executing the slot.
                let target = bus.read32(s.r[15]).map_err(access)?;
                let sr = bus.read32(s.r[15].wrapping_add(4)).map_err(access)?;
                s.r[15] = s.r[15].wrapping_add(8);
                s.sr = sr & SR_MASK;
                next_branch = Some(target);
            }
            _ if opcode & 0xF000 == 0xE000 => s.r[n] = signed_imm,
            _ if opcode & 0xF000 == 0x7000 => s.r[n] = s.r[n].wrapping_add(signed_imm),
            _ if opcode & 0xF00F == 0x6003 => s.r[n] = s.r[m],
            // Indirect and postincrement loads, with sign extension at byte/word widths.
            _ if matches!(opcode & 0xF00F, 0x6000..=0x6002 | 0x6004..=0x6006) => {
                let width = 1 << (opcode & 3);
                let value = read_operand(bus, s.r[m], width).map_err(access)?;
                if opcode & 4 != 0 && n != m {
                    s.r[m] = s.r[m].wrapping_add(width);
                }
                s.r[n] = value;
            }
            // Predecrement stores latch the source before changing the address register.
            _ if matches!(opcode & 0xF00F, 0x2000..=0x2002 | 0x2004..=0x2006) => {
                let width = 1 << (opcode & 3);
                let address = s.r[n].wrapping_sub(if opcode & 4 != 0 { width } else { 0 });
                write_operand(bus, address, width, s.r[m]).map_err(access)?;
                s.r[n] = address;
            }
            // R0-indexed loads/stores.
            _ if matches!(opcode & 0xF00F, 0x0004..=0x0006 | 0x000C..=0x000E) => {
                let width = 1 << (opcode & 3);
                if opcode & 8 == 0 {
                    write_operand(bus, s.r[n].wrapping_add(s.r[0]), width, s.r[m])
                        .map_err(access)?;
                } else {
                    s.r[n] =
                        read_operand(bus, s.r[m].wrapping_add(s.r[0]), width).map_err(access)?;
                }
            }
            _ if matches!(opcode & 0xF000, 0x1000 | 0x5000) => {
                let disp = (opcode as u32 & 15) * 4;
                if opcode & 0xF000 == 0x1000 {
                    bus.write32(s.r[n].wrapping_add(disp), s.r[m])
                        .map_err(access)?;
                } else {
                    s.r[n] = bus.read32(s.r[m].wrapping_add(disp)).map_err(access)?;
                }
            }
            _ if matches!(opcode & 0xFF00, 0x8000 | 0x8100 | 0x8400 | 0x8500) => {
                // Here bits 7..4 select the base, not the usual n field.
                let width = 1 << ((opcode >> 8) & 1);
                let address = s.r[m].wrapping_add((opcode as u32 & 15) * width);
                if opcode & 0x400 == 0 {
                    write_operand(bus, address, width, s.r[0]).map_err(access)?;
                } else {
                    s.r[0] = read_operand(bus, address, width).map_err(access)?;
                }
            }
            _ if matches!(opcode & 0xFF00, 0xC000..=0xC200 | 0xC400..=0xC600) => {
                let width = 1 << ((opcode >> 8) & 3);
                let address = s.gbr.wrapping_add(imm * width);
                if opcode & 0x400 == 0 {
                    write_operand(bus, address, width, s.r[0]).map_err(access)?;
                } else {
                    s.r[0] = read_operand(bus, address, width).map_err(access)?;
                }
            }
            _ if opcode & 0xF000 == 0x9000 => {
                s.r[n] = bus
                    .pc_relative16(operand_pc.wrapping_add(imm * 2))
                    .map_err(access)? as i16 as i32 as u32;
            }
            _ if opcode & 0xF000 == 0xD000 => {
                s.r[n] = bus
                    .pc_relative32((operand_pc & !3).wrapping_add(imm * 4))
                    .map_err(access)?;
            }
            _ if opcode & 0xFF00 == 0xC700 => s.r[0] = (operand_pc & !3).wrapping_add(imm * 4), // MOVA
            _ if opcode & 0xF0FF == 0x0029 => s.r[n] = s.sr & T, // MOVT
            _ if opcode & 0xF00F == 0x6008 => {
                s.r[n] = (s.r[m] & 0xFFFF0000) | (s.r[m] as u16).swap_bytes() as u32
            }
            _ if opcode & 0xF00F == 0x6009 => s.r[n] = s.r[m].rotate_left(16),
            _ if opcode & 0xF00F == 0x200D => s.r[n] = (s.r[m] << 16) | (s.r[n] >> 16), // XTRCT
            _ if opcode & 0xF00F == 0x600C => s.r[n] = s.r[m] & 0xFF,
            _ if opcode & 0xF00F == 0x600D => s.r[n] = s.r[m] & 0xFFFF,
            _ if opcode & 0xF00F == 0x600E => s.r[n] = s.r[m] as i8 as i32 as u32,
            _ if opcode & 0xF00F == 0x600F => s.r[n] = s.r[m] as i16 as i32 as u32,
            _ if opcode & 0xF00F == 0x300C => s.r[n] = s.r[n].wrapping_add(s.r[m]),
            _ if opcode & 0xF00F == 0x3008 => s.r[n] = s.r[n].wrapping_sub(s.r[m]),
            _ if matches!(opcode & 0xF00F, 0x300E | 0x300A | 0x600A) => {
                // ADDC/SUBC/NEGC
                let a = if opcode & 0xF000 == 0x6000 {
                    0
                } else {
                    s.r[n] as u64
                };
                let b = s.r[m] as u64 + (s.sr & T) as u64;
                if opcode & 15 == 14 {
                    let sum = a + b;
                    s.r[n] = sum as u32;
                    s.set_t(sum > u32::MAX as u64);
                } else {
                    s.r[n] = a.wrapping_sub(b) as u32;
                    s.set_t(a < b);
                }
            }
            _ if matches!(opcode & 0xF00F, 0x300F | 0x300B) => {
                let a = s.r[n] as i32;
                let b = s.r[m] as i32;
                let (value, overflow) = if opcode & 4 != 0 {
                    a.overflowing_add(b)
                } else {
                    a.overflowing_sub(b)
                };
                s.r[n] = value as u32;
                s.set_t(overflow);
            }
            _ if opcode & 0xF00F == 0x600B => s.r[n] = s.r[m].wrapping_neg(),
            _ if opcode & 0xF00F == 0x3000 => s.set_t(s.r[n] == s.r[m]),
            _ if opcode & 0xF00F == 0x3002 => s.set_t(s.r[n] >= s.r[m]),
            _ if opcode & 0xF00F == 0x3003 => s.set_t(s.r[n] as i32 >= s.r[m] as i32),
            _ if opcode & 0xF00F == 0x3006 => s.set_t(s.r[n] > s.r[m]),
            _ if opcode & 0xF00F == 0x3007 => s.set_t(s.r[n] as i32 > s.r[m] as i32),
            _ if opcode & 0xFF00 == 0x8800 => s.set_t(s.r[0] == signed_imm),
            _ if opcode & 0xF0FF == 0x4011 => s.set_t(s.r[n] as i32 >= 0),
            _ if opcode & 0xF0FF == 0x4015 => s.set_t(s.r[n] as i32 > 0),
            _ if opcode & 0xF00F == 0x200C => s.set_t((s.r[n] ^ s.r[m]).to_be_bytes().contains(&0)),
            _ if opcode & 0xF0FF == 0x4010 => {
                s.r[n] = s.r[n].wrapping_sub(1);
                s.set_t(s.r[n] == 0);
            }
            _ if opcode & 0xF00F == 0x2007 => {
                // DIV0S
                let q = s.r[n] >> 31;
                let m_bit = s.r[m] >> 31;
                s.sr = (s.sr & !(Q | M | T)) | (q << 8) | (m_bit << 9) | (q ^ m_bit);
            }
            _ if opcode & 0xF00F == 0x3004 => {
                // Non-restoring division: select +/- by old Q/M; carry/borrow and
                // the old dividend sign determine the next Q. No host division.
                let negative = s.r[n] >> 31 != 0;
                let old_q = s.sr & Q != 0;
                let m_bit = s.sr & M != 0;
                // P §6.19 updates Rn before reading Rm, which may alias Rn.
                s.r[n] = (s.r[n] << 1) | (s.sr & T);
                let (value, carry) = if old_q == m_bit {
                    s.r[n].overflowing_sub(s.r[m])
                } else {
                    s.r[n].overflowing_add(s.r[m])
                };
                let q = negative ^ carry ^ m_bit;
                s.r[n] = value;
                s.sr = (s.sr & !Q) | if q { Q } else { 0 };
                s.set_t(q == m_bit);
            }
            _ if opcode & 0xF00F == 0x0007 => s.macl = s.r[n].wrapping_mul(s.r[m]),
            _ if opcode & 0xF00F == 0x200E => s.macl = (s.r[n] & 0xFFFF) * (s.r[m] & 0xFFFF),
            _ if opcode & 0xF00F == 0x200F => {
                s.macl = (s.r[n] as i16 as i32).wrapping_mul(s.r[m] as i16 as i32) as u32
            }
            _ if opcode & 0xF00F == 0x3005 => s.set_accumulator(s.r[n] as u64 * s.r[m] as u64),
            _ if opcode & 0xF00F == 0x300D => {
                s.set_accumulator((s.r[n] as i32 as i64 * s.r[m] as i32 as i64) as u64)
            }
            _ if matches!(opcode & 0xF00F, 0x000F | 0x400F) => {
                let width = if opcode & 0x4000 == 0 { 4 } else { 2 };
                // Read/increment Rn first, then Rm, including when n == m.
                let a = read_operand(bus, s.r[n], width).map_err(access)? as i32 as i64;
                let m_address = s.r[m].wrapping_add(if n == m { width } else { 0 });
                let b = read_operand(bus, m_address, width).map_err(access)? as i32 as i64;
                s.r[n] = s.r[n].wrapping_add(width);
                s.r[m] = s.r[m].wrapping_add(width);
                let product = a * b;
                if s.sr & S == 0 {
                    s.set_accumulator(s.accumulator().wrapping_add(product as u64));
                } else if width == 2 {
                    // SH-2 (unlike SH-1) leaves MACH unchanged for saturated MAC.W.
                    s.macl = (s.macl as i32 as i64 + product)
                        .clamp(i32::MIN as i64, i32::MAX as i64)
                        as u32;
                } else {
                    let acc = ((s.accumulator() << 16) as i64) >> 16;
                    let sum = (acc + product).clamp(-(1_i64 << 47), (1_i64 << 47) - 1);
                    s.set_accumulator(sum as u64);
                }
            }
            _ if opcode & 0xF00F == 0x2008 => s.set_t(s.r[n] & s.r[m] == 0),
            _ if opcode & 0xF00F == 0x2009 => s.r[n] &= s.r[m],
            _ if opcode & 0xF00F == 0x200A => s.r[n] ^= s.r[m],
            _ if opcode & 0xF00F == 0x200B => s.r[n] |= s.r[m],
            _ if opcode & 0xF00F == 0x6007 => s.r[n] = !s.r[m],
            _ if matches!(opcode & 0xFF00, 0xC800..=0xCB00) => match (opcode >> 8) & 3 {
                0 => s.set_t(s.r[0] & imm == 0),
                1 => s.r[0] &= imm,
                2 => s.r[0] ^= imm,
                _ => s.r[0] |= imm,
            },
            _ if matches!(opcode & 0xFF00, 0xCC00..=0xCF00) => {
                let address = s.gbr.wrapping_add(s.r[0]);
                let value = bus.read8(address).map_err(access)?;
                match (opcode >> 8) & 3 {
                    0 => s.set_t(value & imm as u8 == 0),
                    1 => bus.write8(address, value & imm as u8).map_err(access)?,
                    2 => bus.write8(address, value ^ imm as u8).map_err(access)?,
                    _ => bus.write8(address, value | imm as u8).map_err(access)?,
                }
            }
            _ if opcode & 0xF0FF == 0x401B => {
                // TAS.B: one synchronous read/modify/write.
                let value = bus.tas_read(s.r[n]).map_err(access)?;
                bus.tas_write(s.r[n], value | 0x80).map_err(access)?;
                s.set_t(value == 0);
            }
            _ if matches!(
                opcode & 0xF0FF,
                0x4000 | 0x4020 | 0x4001 | 0x4021 | 0x4004 | 0x4005 | 0x4024 | 0x4025
            ) =>
            {
                let old = s.r[n];
                let right = opcode & 1 != 0;
                let outgoing = if right { old & 1 } else { old >> 31 };
                let incoming = if opcode & 4 == 0 {
                    0
                } else if opcode & 0x20 != 0 {
                    s.sr & T
                } else {
                    outgoing
                };
                s.r[n] = if right {
                    if opcode & 0x24 == 0x20 {
                        ((old as i32) >> 1) as u32
                    } else {
                        (old >> 1) | (incoming << 31)
                    }
                } else {
                    (old << 1) | incoming
                };
                s.set_t(outgoing != 0);
            }
            _ if matches!(
                opcode & 0xF0FF,
                0x4008 | 0x4009 | 0x4018 | 0x4019 | 0x4028 | 0x4029
            ) =>
            {
                let amount = match opcode & 0x30 {
                    0 => 2,
                    0x10 => 8,
                    _ => 16,
                };
                s.r[n] = if opcode & 1 != 0 {
                    s.r[n] >> amount
                } else {
                    s.r[n] << amount
                };
            }
            // System/control transfers share the architectural one-instruction IRQ inhibition.
            _ if matches!(
                opcode & 0xF0FF,
                0x0002 | 0x0012 | 0x0022 | 0x000A | 0x001A | 0x002A
            ) =>
            {
                s.r[n] = special(s, opcode & 15 == 2, (opcode >> 4) & 3);
                inhibit = true;
            }
            _ if matches!(
                opcode & 0xF0FF,
                0x4003 | 0x4013 | 0x4023 | 0x4002 | 0x4012 | 0x4022
            ) =>
            {
                let address = s.r[n].wrapping_sub(4);
                bus.write32(address, special(s, opcode & 1 != 0, (opcode >> 4) & 3))
                    .map_err(access)?;
                s.r[n] = address;
                inhibit = true;
            }
            _ if matches!(
                opcode & 0xF0FF,
                0x400E | 0x401E | 0x402E | 0x400A | 0x401A | 0x402A
            ) =>
            {
                set_special(s, opcode & 15 == 14, (opcode >> 4) & 3, s.r[n]);
                inhibit = true;
            }
            _ if matches!(
                opcode & 0xF0FF,
                0x4007 | 0x4017 | 0x4027 | 0x4006 | 0x4016 | 0x4026
            ) =>
            {
                let value = bus.read32(s.r[n]).map_err(access)?;
                s.r[n] = s.r[n].wrapping_add(4);
                set_special(s, opcode & 1 != 0, (opcode >> 4) & 3, value);
                inhibit = true;
            }
            _ if matches!(opcode & 0xFF00, 0x8900 | 0x8B00 | 0x8D00 | 0x8F00) => {
                let taken = (s.sr & T != 0) == (opcode & 0x200 == 0);
                let target = pc.wrapping_add(4).wrapping_add(signed_imm.wrapping_mul(2));
                // H §4.6 Table 4.10: the successor must execute before IRQ/NMI
                // acceptance even when BT/S or BF/S does not take the branch.
                inhibit = opcode & 0x400 != 0;
                if taken {
                    if opcode & 0x400 != 0 {
                        next_branch = Some(target);
                    } else {
                        sequential_pc = target;
                    }
                }
            }
            _ if matches!(opcode & 0xF0FF, 0x400B | 0x402B | 0x0003 | 0x0023) => {
                next_branch = Some(if opcode & 0x4000 != 0 {
                    s.r[n]
                } else {
                    pc.wrapping_add(4).wrapping_add(s.r[n])
                });
                if opcode & 0x20 == 0 {
                    s.pr = pc.wrapping_add(4);
                }
            }
            _ if matches!(opcode & 0xF000, 0xA000 | 0xB000) => {
                if opcode & 0xF000 == 0xB000 {
                    s.pr = pc.wrapping_add(4);
                }
                let displacement = ((opcode << 4) as i16 as i32) >> 4;
                next_branch = Some(pc.wrapping_add(4).wrapping_add((displacement * 2) as u32));
            }
            _ => return self.illegal(bus.inner),
        }
        s.pc = s.delay_target.take().unwrap_or(sequential_pc);
        s.delay_target = next_branch;
        s.instructions += 1;
        s.address_error_pending |= operands.address_error.get();
        self.interrupt_inhibited = inhibit;
        self.address_error_inhibited =
            next_branch.is_some() || matches!(opcode & 0xFF00, 0x8D00 | 0x8F00);
        Ok(())
    }
}

fn read_operand(bus: &impl Memory, address: u32, width: u32) -> Result<u32, BusError> {
    match width {
        1 => Ok(bus.read8(address)? as i8 as i32 as u32),
        2 => Ok(bus.read16(address)? as i16 as i32 as u32),
        _ => bus.read32(address),
    }
}

fn write_operand(
    bus: &mut impl Memory,
    address: u32,
    width: u32,
    value: u32,
) -> Result<(), BusError> {
    match width {
        1 => bus.write8(address, value as u8),
        2 => bus.write16(address, value as u16),
        _ => bus.write32(address, value),
    }
}

fn special(s: &CpuState, control: bool, index: u16) -> u32 {
    match (control, index) {
        (true, 0) => s.sr,
        (true, 1) => s.gbr,
        (true, _) => s.vbr,
        (false, 0) => s.mach,
        (false, 1) => s.macl,
        (false, _) => s.pr,
    }
}

fn set_special(s: &mut CpuState, control: bool, index: u16, value: u32) {
    match (control, index) {
        (true, 0) => s.sr = value & SR_MASK,
        (true, 1) => s.gbr = value,
        (true, _) => s.vbr = value,
        (false, 0) => s.mach = value,
        (false, 1) => s.macl = value,
        (false, _) => s.pr = value,
    }
}

#[cfg(test)]
mod address_tests;
#[cfg(test)]
mod tests;
