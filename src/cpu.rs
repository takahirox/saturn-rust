//! Original, instruction-counted SH-2 subset. See docs/architecture.md for sources.
use crate::bus::{BusError, Memory};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuError {
    NotRunning,
    Bus { pc: u32, source: BusError },
    UnsupportedOpcode { pc: u32, opcode: u16 },
    BranchInDelaySlot { pc: u32, opcode: u16 },
}

impl fmt::Display for CpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => write!(f, "CPU is stopped; reset before stepping"),
            Self::Bus { pc, source } => write!(f, "Fetch at PC 0x{pc:08X}: {source}"),
            Self::UnsupportedOpcode { pc, opcode } => write!(
                f,
                "Unsupported opcode 0x{opcode:04X} at PC 0x{pc:08X}; exception handling is not implemented"
            ),
            Self::BranchInDelaySlot { pc, opcode } => write!(
                f,
                "Branch 0x{opcode:04X} in delay slot at PC 0x{pc:08X}; slot-illegal exception is not implemented"
            ),
        }
    }
}

impl std::error::Error for CpuError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpuStatus {
    Reset,
    Running,
    Halted,
    Faulted(CpuError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuState {
    pub r: [u32; 16],
    /// Address of the next instruction to fetch, including a pending delay slot.
    pub pc: u32,
    pub sr: u32,
    pub pr: u32,
    pub gbr: u32,
    pub vbr: u32,
    pub mach: u32,
    pub macl: u32,
    pub instructions: u64,
    pub delay_target: Option<u32>,
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
            status: CpuStatus::Reset,
        }
    }
}

#[derive(Debug, Default)]
pub struct Cpu {
    state: CpuState,
}

impl Cpu {
    pub fn state(&self) -> &CpuState {
        &self.state
    }

    pub fn start(pc: u32, sp: u32) -> Self {
        let mut state = CpuState {
            pc,
            status: CpuStatus::Running,
            ..CpuState::default()
        };
        state.r[15] = sp;
        Self { state }
    }

    /// Power-on reset vectors are absolute physical addresses 0 and 4, not VBR-relative.
    pub fn power_on(bus: &impl Memory) -> Result<Self, BusError> {
        Ok(Self::start(bus.read32(0)?, bus.read32(4)?))
    }

    /// Debugger stop, not an emulation of SLEEP or a hardware halt signal.
    pub fn halt(&mut self) {
        self.state.status = CpuStatus::Halted;
    }

    /// One successful call retires exactly one instruction; faults are sticky.
    pub fn step(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        match &self.state.status {
            CpuStatus::Running => (),
            CpuStatus::Faulted(error) => return Err(error.clone()),
            _ => return Err(CpuError::NotRunning),
        }
        let result = self.execute(bus);
        if let Err(error) = &result {
            self.state.status = CpuStatus::Faulted(error.clone());
        }
        result
    }

    fn execute(&mut self, bus: &mut impl Memory) -> Result<(), CpuError> {
        let s = &mut self.state;
        let pc = s.pc;
        let opcode = bus
            .read16(pc)
            .map_err(|source| CpuError::Bus { pc, source })?;
        let branch = opcode & 0xF000 == 0xA000 || opcode == 0x000B;
        if branch && s.delay_target.is_some() {
            return Err(CpuError::BranchInDelaySlot { pc, opcode });
        }
        let n = ((opcode >> 8) & 15) as usize;
        let m = ((opcode >> 4) & 15) as usize;
        let immediate = (opcode as u8 as i8 as i32) as u32;
        let mut next_branch = None;
        match opcode {
            0x0009 => (),                       // NOP
            0x000B => next_branch = Some(s.pr), // RTS latches PR before the slot
            _ if opcode & 0xF000 == 0xE000 => s.r[n] = immediate,
            _ if opcode & 0xF00F == 0x6003 => s.r[n] = s.r[m],
            _ if opcode & 0xF000 == 0x7000 => s.r[n] = s.r[n].wrapping_add(immediate),
            _ if opcode & 0xF000 == 0xA000 => {
                let displacement = ((opcode << 4) as i16 as i32) >> 4;
                next_branch = Some(pc.wrapping_add(4).wrapping_add((displacement * 2) as u32));
            }
            _ => return Err(CpuError::UnsupportedOpcode { pc, opcode }),
        }
        s.pc = s.delay_target.take().unwrap_or_else(|| pc.wrapping_add(2));
        s.delay_target = next_branch;
        s.instructions += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{Bus, HIGH_WRAM};

    fn setup(opcodes: &[u16]) -> (Cpu, Bus) {
        let mut bus = Bus::default();
        for (i, opcode) in opcodes.iter().enumerate() {
            bus.write16(HIGH_WRAM + i as u32 * 2, *opcode).unwrap();
        }
        (Cpu::start(HIGH_WRAM, 0x0610_0000), bus)
    }

    #[test]
    fn nop_only_advances_pc_and_instruction_count() {
        let (mut cpu, mut bus) = setup(&[0x0009]);
        let mut expected = cpu.state.clone();
        expected.pc += 2;
        expected.instructions += 1;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state, expected);
    }

    #[test]
    fn mov_immediate_sign_extends_at_boundaries_into_every_register() {
        for n in 0..16 {
            for (immediate, expected) in [
                (0x00, 0),
                (0x7F, 127),
                (0x80, 0xFFFF_FF80),
                (0xFF, u32::MAX),
            ] {
                let (mut cpu, mut bus) = setup(&[0xE000 | (n << 8) | immediate]);
                cpu.step(&mut bus).unwrap();
                assert_eq!(cpu.state.r[n as usize], expected);
                assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
                assert_eq!(cpu.state.sr, 0xF0);
            }
        }
    }

    #[test]
    fn mov_register_copies_all_32_bits_and_leaves_source_and_flags() {
        let (mut cpu, mut bus) = setup(&[0x6F23, 0x6FF3]);
        cpu.state.r[2] = 0x89AB_CDEF;
        cpu.state.sr = 0xF3;
        for _ in 0..2 {
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.r[15], 0x89AB_CDEF);
            assert_eq!(cpu.state.r[2], 0x89AB_CDEF);
            assert_eq!(cpu.state.sr, 0xF3);
        }
    }

    #[test]
    fn add_immediate_sign_extends_and_wraps_without_changing_flags() {
        for (initial, immediate, expected) in [
            (0, 0xFF, u32::MAX),
            (u32::MAX, 1, 0),
            (1, 0x80, 0xFFFF_FF81),
            (0x7FFF_FFFF, 1, 0x8000_0000),
            (1, 0x7F, 128),
        ] {
            let (mut cpu, mut bus) = setup(&[0x7300 | immediate]);
            cpu.state.r[3] = initial;
            cpu.state.sr = 0xF1;
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.r[3], expected);
            assert_eq!(cpu.state.sr, 0xF1);
        }
    }

    #[test]
    fn bra_latches_signed_pc_plus_four_target_and_executes_one_slot() {
        for (opcode, displacement) in [
            (0xA000, 0_i32),
            (0xA001, 1),
            (0xA7FF, 2047),
            (0xA800, -2048),
            (0xAFFF, -1),
        ] {
            let (mut cpu, mut bus) = setup(&[opcode, 0x7101, 0x7102]);
            let target = HIGH_WRAM
                .wrapping_add(4)
                .wrapping_add((displacement * 2) as u32);
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
            assert_eq!(cpu.state.delay_target, Some(target));
            assert_eq!(cpu.state.r[1], 0);
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.pc, target);
            assert_eq!(cpu.state.r[1], 1);
            assert_eq!(cpu.state.delay_target, None);
            assert_eq!(cpu.state.instructions, 2);
        }
    }

    #[test]
    fn rts_returns_to_pr_after_one_slot_and_preserves_pr() {
        let (mut cpu, mut bus) = setup(&[0x000B, 0xE22A, 0xE263]);
        cpu.state.pr = HIGH_WRAM + 0x100;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
        assert_eq!(cpu.state.delay_target, Some(HIGH_WRAM + 0x100));
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HIGH_WRAM + 0x100);
        assert_eq!(cpu.state.r[2], 42);
        assert_eq!(cpu.state.pr, HIGH_WRAM + 0x100);
        assert_eq!(cpu.state.instructions, 2);
    }

    #[test]
    fn unsupported_opcode_error_is_exact_sticky_and_has_no_retired_side_effects() {
        let (mut cpu, mut bus) = setup(&[0xFFFF]);
        let before = cpu.state.clone();
        let error = CpuError::UnsupportedOpcode {
            pc: HIGH_WRAM,
            opcode: 0xFFFF,
        };
        assert_eq!(cpu.step(&mut bus), Err(error.clone()));
        assert_eq!(
            error.to_string(),
            "Unsupported opcode 0xFFFF at PC 0x06000000; exception handling is not implemented"
        );
        let mut expected = before;
        expected.status = CpuStatus::Faulted(error.clone());
        assert_eq!(cpu.state, expected);
        bus.write16(HIGH_WRAM, 0x0009).unwrap();
        assert_eq!(cpu.step(&mut bus), Err(error));
        assert_eq!(cpu.state, expected);
    }

    #[test]
    fn bad_slot_does_not_commit_pending_branch() {
        for opcode in [0xA000, 0x000B, 0xFFFF] {
            let (mut cpu, mut bus) = setup(&[0xA001, opcode]);
            cpu.step(&mut bus).unwrap();
            let result = cpu.step(&mut bus);
            if opcode == 0xFFFF {
                assert!(matches!(result, Err(CpuError::UnsupportedOpcode { .. })));
            } else {
                assert_eq!(
                    result,
                    Err(CpuError::BranchInDelaySlot {
                        pc: HIGH_WRAM + 2,
                        opcode
                    })
                );
            }
            assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
            assert_eq!(cpu.state.delay_target, Some(HIGH_WRAM + 6));
            assert_eq!(cpu.state.instructions, 1);
        }
    }

    #[test]
    fn fetch_faults_are_explicit_and_stopped_states_do_not_execute() {
        let mut bus = Bus::default();
        for pc in [0x0400_0000, HIGH_WRAM + 1] {
            let mut cpu = Cpu::start(pc, 0);
            assert!(matches!(cpu.step(&mut bus), Err(CpuError::Bus { pc: p, .. }) if pc == p));
            assert_eq!(cpu.state.instructions, 0);
            assert_eq!(cpu.state.pc, pc);
        }
        let mut cpu = Cpu::default();
        assert_eq!(cpu.step(&mut bus), Err(CpuError::NotRunning));
        cpu = Cpu::start(HIGH_WRAM, 0);
        cpu.halt();
        assert_eq!(cpu.step(&mut bus), Err(CpuError::NotRunning));
        assert_eq!(cpu.state.status, CpuStatus::Halted);
    }
}
