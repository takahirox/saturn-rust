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
            Self::Bus { pc, source } => write!(f, "Memory access at PC 0x{pc:08X}: {source}"),
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
        // SH-1/SH-2 Programming Manual §6: branches are illegal in a slot.
        let branch = matches!(opcode & 0xF000, 0xA000 | 0xB000)
            || matches!(opcode & 0xFF00, 0x8900 | 0x8B00)
            || opcode & 0xF0FF == 0x402B
            || opcode == 0x000B;
        if branch && s.delay_target.is_some() {
            return Err(CpuError::BranchInDelaySlot { pc, opcode });
        }
        let n = ((opcode >> 8) & 15) as usize;
        let m = ((opcode >> 4) & 15) as usize;
        let immediate = (opcode as u8 as i8 as i32) as u32;
        let mut next_branch = None;
        let mut sequential_pc = pc.wrapping_add(2);
        let access = |source| CpuError::Bus { pc, source };
        match opcode {
            0x0009 => (),                       // NOP
            0x000B => next_branch = Some(s.pr), // RTS latches PR before the slot
            _ if opcode & 0xF000 == 0xE000 => s.r[n] = immediate,
            _ if opcode & 0xF00F == 0x6003 => s.r[n] = s.r[m],
            _ if opcode & 0xF000 == 0x7000 => s.r[n] = s.r[n].wrapping_add(immediate),
            // MOV.B/W indirect loads sign-extend; MOV.L copies all bits.
            _ if opcode & 0xF00F == 0x6000 => {
                s.r[n] = bus.read8(s.r[m]).map_err(access)? as i8 as i32 as u32
            }
            _ if opcode & 0xF00F == 0x6001 => {
                s.r[n] = bus.read16(s.r[m]).map_err(access)? as i16 as i32 as u32
            }
            _ if opcode & 0xF00F == 0x6002 => s.r[n] = bus.read32(s.r[m]).map_err(access)?,
            _ if opcode & 0xF00F == 0x2000 => bus.write8(s.r[n], s.r[m] as u8).map_err(access)?,
            _ if opcode & 0xF00F == 0x2001 => bus.write16(s.r[n], s.r[m] as u16).map_err(access)?,
            _ if opcode & 0xF00F == 0x2002 => bus.write32(s.r[n], s.r[m]).map_err(access)?,
            _ if opcode & 0xF000 == 0xD000 => {
                // In a delay slot, PC-relative operands use branch destination + 2.
                let operand_pc = s
                    .delay_target
                    .map_or(pc.wrapping_add(4), |target| target.wrapping_add(2));
                let address = (operand_pc & !3).wrapping_add((opcode as u8 as u32) * 4);
                s.r[n] = bus.read32(address).map_err(access)?;
            }
            _ if opcode & 0xF00F == 0x300C => s.r[n] = s.r[n].wrapping_add(s.r[m]),
            _ if opcode & 0xF00F == 0x3008 => s.r[n] = s.r[n].wrapping_sub(s.r[m]),
            _ if opcode & 0xF00F == 0x3000 => s.sr = (s.sr & !1) | u32::from(s.r[n] == s.r[m]),
            _ if opcode & 0xF00F == 0x3002 => s.sr = (s.sr & !1) | u32::from(s.r[n] >= s.r[m]),
            _ if opcode & 0xFF00 == 0xC800 => {
                s.sr = (s.sr & !1) | u32::from(s.r[0] & (opcode as u8 as u32) == 0)
            }
            _ if opcode & 0xF0FF == 0x4010 => {
                s.r[n] = s.r[n].wrapping_sub(1);
                s.sr = (s.sr & !1) | u32::from(s.r[n] == 0);
            }
            _ if opcode & 0xF0FF == 0x4000 => {
                s.sr = (s.sr & !1) | (s.r[n] >> 31);
                s.r[n] <<= 1;
            }
            _ if opcode & 0xF0FF == 0x4008 => s.r[n] <<= 2,
            _ if opcode & 0xF0FF == 0x4018 => s.r[n] <<= 8,
            _ if matches!(opcode & 0xFF00, 0x8900 | 0x8B00) => {
                if (s.sr & 1 != 0) == (opcode & 0xFF00 == 0x8900) {
                    sequential_pc = pc
                        .wrapping_add(4)
                        .wrapping_add((immediate as i32 * 2) as u32);
                }
            }
            _ if opcode & 0xF0FF == 0x402B => next_branch = Some(s.r[n]),
            _ if matches!(opcode & 0xF000, 0xA000 | 0xB000) => {
                if opcode & 0xF000 == 0xB000 {
                    s.pr = pc.wrapping_add(4);
                }
                let displacement = ((opcode << 4) as i16 as i32) >> 4;
                next_branch = Some(pc.wrapping_add(4).wrapping_add((displacement * 2) as u32));
            }
            _ => return Err(CpuError::UnsupportedOpcode { pc, opcode }),
        }
        s.pc = s.delay_target.take().unwrap_or(sequential_pc);
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
    #[test]
    fn memory_instructions_obey_sign_extension_width_and_big_endian_access() {
        let (mut cpu, mut bus) = setup(&[0x2310, 0x2311, 0x2312, 0x6430, 0x6531, 0x6632]);
        cpu.state.r[1] = 0x89AB_CDEF;
        cpu.state.r[3] = HIGH_WRAM + 0x100;
        for width in [1, 2, 4] {
            cpu.step(&mut bus).unwrap();
            let expected = match width {
                1 => 0xEF000000,
                2 => 0xCDEF0000,
                _ => 0x89ABCDEF,
            };
            assert_eq!(bus.read32(HIGH_WRAM + 0x100), Ok(expected));
        }
        for _ in 0..3 {
            cpu.step(&mut bus).unwrap();
        }
        assert_eq!(&cpu.state.r[4..7], &[0xFFFFFF89, 0xFFFF89AB, 0x89ABCDEF]);
        assert_eq!(cpu.state.sr, 0xF0);
    }

    #[test]
    fn data_faults_do_not_retire_or_modify_destination_or_pending_slot() {
        for opcode in [0x6120, 0x6121, 0x6122, 0x2210, 0x2211, 0x2212] {
            let (mut cpu, mut bus) = setup(&[0xA001, opcode]);
            cpu.state.r[1] = 0x12345678;
            cpu.state.r[2] = 0x04000001;
            cpu.step(&mut bus).unwrap();
            let before = cpu.state.clone();
            assert!(matches!(cpu.step(&mut bus), Err(CpuError::Bus { .. })));
            assert_eq!(cpu.state.r, before.r);
            assert_eq!(cpu.state.pc, before.pc);
            assert_eq!(cpu.state.delay_target, before.delay_target);
            assert_eq!(cpu.state.instructions, 1);
        }
    }

    #[test]
    fn arithmetic_flags_compare_unsigned_and_shift_carry_without_clobbering_sr() {
        let (mut cpu, mut bus) = setup(&[
            0x312C, 0x3128, 0x3120, 0x3122, 0x4100, 0x4108, 0x4118, 0x4110, 0xC880,
        ]);
        cpu.state.r[1] = 0xFFFFFFFF;
        cpu.state.r[2] = 1;
        cpu.state.sr = 0xF3;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0);
        assert_eq!(cpu.state.sr, 0xF3);
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0xFFFFFFFF);
        cpu.step(&mut bus).unwrap(); // CMP/EQ false
        assert_eq!(cpu.state.sr, 0xF2);
        cpu.step(&mut bus).unwrap(); // unsigned >= true, despite high bit
        assert_eq!(cpu.state.sr, 0xF3);
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0xFFFFFFFE);
        assert_eq!(cpu.state.sr, 0xF3);
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0xFFFFF800);
        assert_eq!(cpu.state.sr, 0xF3);
        cpu.state.r[1] = 1;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0);
        assert_eq!(cpu.state.sr, 0xF3);
        cpu.state.r[0] = 0x80;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.sr, 0xF2);
    }

    #[test]
    fn conditional_branches_have_no_slot_and_use_signed_pc_plus_four() {
        for (opcode, t, target) in [
            (0x89FE, 1, 0),
            (0x8B7F, 0, 258),
            (0x8980, 1, -252),
            (0x8901, 0, 2),
            (0x8B01, 1, 2),
        ] {
            let (mut cpu, mut bus) = setup(&[opcode, 0xE163]);
            cpu.state.sr = 0xF0 | t;
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.pc, HIGH_WRAM.wrapping_add(target as u32));
            assert_eq!(cpu.state.delay_target, None);
            assert_eq!(cpu.state.r[1], 0);
        }
        for opcode in [0x8900, 0x8B00, 0xB000, 0x412B] {
            let (mut cpu, mut bus) = setup(&[0xA000, opcode]);
            cpu.step(&mut bus).unwrap();
            assert!(matches!(
                cpu.step(&mut bus),
                Err(CpuError::BranchInDelaySlot { .. })
            ));
        }
    }

    #[test]
    fn calls_jumps_and_pc_relative_literals_use_architectural_pc() {
        let (mut cpu, mut bus) = setup(&[0xB002, 0xD101, 0x0009, 0x0009, 0x412B, 0x7101]);
        bus.write32(HIGH_WRAM + 12, HIGH_WRAM + 0x100).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pr, HIGH_WRAM + 4);
        cpu.step(&mut bus).unwrap();
        // MOV.L in a slot uses (destination + 2) aligned down, then disp*4.
        assert_eq!(cpu.state.r[1], HIGH_WRAM + 0x100);
        assert_eq!(cpu.state.pc, HIGH_WRAM + 8);
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HIGH_WRAM + 0x100);
        assert_eq!(cpu.state.r[1], HIGH_WRAM + 0x101); // target latched before slot
        for offset in [0, 2] {
            let (mut cpu, mut bus) = setup(&[0x0009, 0x0009]);
            cpu.state.pc += offset;
            bus.write16(HIGH_WRAM + offset, 0xD201).unwrap();
            bus.write32(HIGH_WRAM + 8, 0xFEDCBA98).unwrap();
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.r[2], 0xFEDCBA98);
        }
    }
}
