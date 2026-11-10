//! Register-only SH-2 semantics shared by ordinary and block execution.
use super::{CpuState, M, Q, T};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    ClearT,
    SetT,
    Div0U,
    Nop,
    MoveImmediate,
    AddImmediate,
    Move,
    MoveT,
    SwapBytes,
    SwapWords,
    Extract,
    ExtendUnsignedByte,
    ExtendUnsignedWord,
    ExtendSignedByte,
    ExtendSignedWord,
    Add,
    Subtract,
    CarryArithmetic,
    OverflowArithmetic,
    Negate,
    Equal,
    HigherSame,
    GreaterEqual,
    Higher,
    Greater,
    EqualImmediate,
    PositiveZero,
    Positive,
    StringEqual,
    Decrement,
    Div0S,
    Div1,
    Test,
    And,
    Xor,
    Or,
    Not,
    ImmediateLogic,
    ShiftOne,
    ShiftMany,
}

const fn make_decoder() -> [Option<Kind>; 65536] {
    let mut table = [None; 65536];
    let mut opcode = 0;
    while opcode < table.len() {
        table[opcode] = decode_literal(opcode as u16);
        opcode += 1;
    }
    table
}

#[allow(long_running_const_eval)]
static DECODER: [Option<Kind>; 65536] = make_decoder();

pub(super) fn decode(opcode: u16) -> Option<Kind> {
    DECODER[usize::from(opcode)]
}

pub(super) const fn decode_literal(opcode: u16) -> Option<Kind> {
    match opcode {
        0x0008 => Some(Kind::ClearT),
        0x0018 => Some(Kind::SetT),
        0x0019 => Some(Kind::Div0U),
        0x0009 => Some(Kind::Nop),
        _ if opcode & 0xF000 == 0xE000 => Some(Kind::MoveImmediate),
        _ if opcode & 0xF000 == 0x7000 => Some(Kind::AddImmediate),
        _ if opcode & 0xF00F == 0x6003 => Some(Kind::Move),
        _ if opcode & 0xF0FF == 0x0029 => Some(Kind::MoveT),
        _ if opcode & 0xF00F == 0x6008 => Some(Kind::SwapBytes),
        _ if opcode & 0xF00F == 0x6009 => Some(Kind::SwapWords),
        _ if opcode & 0xF00F == 0x200D => Some(Kind::Extract),
        _ if opcode & 0xF00F == 0x600C => Some(Kind::ExtendUnsignedByte),
        _ if opcode & 0xF00F == 0x600D => Some(Kind::ExtendUnsignedWord),
        _ if opcode & 0xF00F == 0x600E => Some(Kind::ExtendSignedByte),
        _ if opcode & 0xF00F == 0x600F => Some(Kind::ExtendSignedWord),
        _ if opcode & 0xF00F == 0x300C => Some(Kind::Add),
        _ if opcode & 0xF00F == 0x3008 => Some(Kind::Subtract),
        _ if matches!(opcode & 0xF00F, 0x300E | 0x300A | 0x600A) => Some(Kind::CarryArithmetic),
        _ if matches!(opcode & 0xF00F, 0x300F | 0x300B) => Some(Kind::OverflowArithmetic),
        _ if opcode & 0xF00F == 0x600B => Some(Kind::Negate),
        _ if opcode & 0xF00F == 0x3000 => Some(Kind::Equal),
        _ if opcode & 0xF00F == 0x3002 => Some(Kind::HigherSame),
        _ if opcode & 0xF00F == 0x3003 => Some(Kind::GreaterEqual),
        _ if opcode & 0xF00F == 0x3006 => Some(Kind::Higher),
        _ if opcode & 0xF00F == 0x3007 => Some(Kind::Greater),
        _ if opcode & 0xFF00 == 0x8800 => Some(Kind::EqualImmediate),
        _ if opcode & 0xF0FF == 0x4011 => Some(Kind::PositiveZero),
        _ if opcode & 0xF0FF == 0x4015 => Some(Kind::Positive),
        _ if opcode & 0xF00F == 0x200C => Some(Kind::StringEqual),
        _ if opcode & 0xF0FF == 0x4010 => Some(Kind::Decrement),
        _ if opcode & 0xF00F == 0x2007 => Some(Kind::Div0S),
        _ if opcode & 0xF00F == 0x3004 => Some(Kind::Div1),
        _ if opcode & 0xF00F == 0x2008 => Some(Kind::Test),
        _ if opcode & 0xF00F == 0x2009 => Some(Kind::And),
        _ if opcode & 0xF00F == 0x200A => Some(Kind::Xor),
        _ if opcode & 0xF00F == 0x200B => Some(Kind::Or),
        _ if opcode & 0xF00F == 0x6007 => Some(Kind::Not),
        _ if matches!(opcode & 0xFF00, 0xC800..=0xCB00) => Some(Kind::ImmediateLogic),
        _ if matches!(
            opcode & 0xF0FF,
            0x4000 | 0x4020 | 0x4001 | 0x4021 | 0x4004 | 0x4005 | 0x4024 | 0x4025
        ) =>
        {
            Some(Kind::ShiftOne)
        }
        _ if matches!(
            opcode & 0xF0FF,
            0x4008 | 0x4009 | 0x4018 | 0x4019 | 0x4028 | 0x4029
        ) =>
        {
            Some(Kind::ShiftMany)
        }
        _ => None,
    }
}

pub(super) fn execute(kind: Kind, opcode: u16, s: &mut CpuState) {
    let n = ((opcode >> 8) & 15) as usize;
    let m = ((opcode >> 4) & 15) as usize;
    let imm = opcode as u8 as u32;
    let signed_imm = opcode as u8 as i8 as i32 as u32;
    match kind {
        Kind::ClearT => s.sr &= !T,          // CLRT
        Kind::SetT => s.sr |= T,             // SETT
        Kind::Div0U => s.sr &= !(M | Q | T), // DIV0U
        Kind::Nop => (),
        Kind::MoveImmediate => s.r[n] = signed_imm,
        Kind::AddImmediate => s.r[n] = s.r[n].wrapping_add(signed_imm),
        Kind::Move => s.r[n] = s.r[m],
        Kind::MoveT => s.r[n] = s.sr & T, // MOVT
        Kind::SwapBytes => s.r[n] = (s.r[m] & 0xFFFF0000) | (s.r[m] as u16).swap_bytes() as u32,
        Kind::SwapWords => s.r[n] = s.r[m].rotate_left(16),
        Kind::Extract => s.r[n] = (s.r[m] << 16) | (s.r[n] >> 16), // XTRCT
        Kind::ExtendUnsignedByte => s.r[n] = s.r[m] & 0xFF,
        Kind::ExtendUnsignedWord => s.r[n] = s.r[m] & 0xFFFF,
        Kind::ExtendSignedByte => s.r[n] = s.r[m] as i8 as i32 as u32,
        Kind::ExtendSignedWord => s.r[n] = s.r[m] as i16 as i32 as u32,
        Kind::Add => s.r[n] = s.r[n].wrapping_add(s.r[m]),
        Kind::Subtract => s.r[n] = s.r[n].wrapping_sub(s.r[m]),
        Kind::CarryArithmetic => {
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
        Kind::OverflowArithmetic => {
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
        Kind::Negate => s.r[n] = s.r[m].wrapping_neg(),
        Kind::Equal => s.set_t(s.r[n] == s.r[m]),
        Kind::HigherSame => s.set_t(s.r[n] >= s.r[m]),
        Kind::GreaterEqual => s.set_t(s.r[n] as i32 >= s.r[m] as i32),
        Kind::Higher => s.set_t(s.r[n] > s.r[m]),
        Kind::Greater => s.set_t(s.r[n] as i32 > s.r[m] as i32),
        Kind::EqualImmediate => s.set_t(s.r[0] == signed_imm),
        Kind::PositiveZero => s.set_t(s.r[n] as i32 >= 0),
        Kind::Positive => s.set_t(s.r[n] as i32 > 0),
        Kind::StringEqual => s.set_t((s.r[n] ^ s.r[m]).to_be_bytes().contains(&0)),
        Kind::Decrement => {
            s.r[n] = s.r[n].wrapping_sub(1);
            s.set_t(s.r[n] == 0);
        }
        Kind::Div0S => {
            // DIV0S
            let q = s.r[n] >> 31;
            let m_bit = s.r[m] >> 31;
            s.sr = (s.sr & !(Q | M | T)) | (q << 8) | (m_bit << 9) | (q ^ m_bit);
        }
        Kind::Div1 => {
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
        Kind::Test => s.set_t(s.r[n] & s.r[m] == 0),
        Kind::And => s.r[n] &= s.r[m],
        Kind::Xor => s.r[n] ^= s.r[m],
        Kind::Or => s.r[n] |= s.r[m],
        Kind::Not => s.r[n] = !s.r[m],
        Kind::ImmediateLogic => match (opcode >> 8) & 3 {
            0 => s.set_t(s.r[0] & imm == 0),
            1 => s.r[0] &= imm,
            2 => s.r[0] ^= imm,
            _ => s.r[0] |= imm,
        },
        Kind::ShiftOne => {
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
        Kind::ShiftMany => {
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
        } // System/control transfers share the architectural one-instruction IRQ inhibition.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bus::{BusError, Memory},
        cpu::{Cpu, CpuStatus},
    };

    struct Code {
        opcode: u16,
    }
    impl Memory for Code {
        fn read16(&self, address: u32) -> Result<u16, BusError> {
            Ok(u16::from_be_bytes([
                self.read8(address)?,
                self.read8(address + 1)?,
            ]))
        }
        fn read32(&self, address: u32) -> Result<u32, BusError> {
            Ok(u32::from(self.read16(address)?) << 16 | u32::from(self.read16(address + 2)?))
        }
        fn write16(&mut self, _: u32, _: u16) -> Result<(), BusError> {
            panic!("register-only guest")
        }
        fn write32(&mut self, _: u32, _: u32) -> Result<(), BusError> {
            panic!("register-only guest")
        }
        fn read8(&self, address: u32) -> Result<u8, BusError> {
            let word = if address & !1 == 0x100 {
                self.opcode
            } else {
                0x0009
            };
            Ok(if address & 1 == 0 {
                (word >> 8) as u8
            } else {
                word as u8
            })
        }
        fn write8(&mut self, _: u32, _: u8) -> Result<(), BusError> {
            panic!("register-only guest")
        }
        fn sh2_clock(&self) -> Option<u64> {
            Some(100)
        }
        fn sh2_fetch_slots(&self) -> bool {
            true
        }
        fn sh2_speculative_fetch(&self, _: u32) -> bool {
            true
        }
        fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
            assert_eq!(clocks, 0, "steady single-state instruction must not wait");
            Ok(())
        }
    }

    #[test]
    fn every_register_opcode_keeps_complete_single_step_pipeline_state() {
        for opcode in 0..=u16::MAX {
            if decode_literal(opcode).is_none() {
                continue;
            }
            for seed in [0u32, 0x8000_0001, 0xFFFF_FFFF] {
                let mut code = Code { opcode };
                let mut ordinary = Cpu::default();
                ordinary.state.pc = 0x100;
                ordinary.state.status = CpuStatus::Running;
                ordinary.state.sr = seed & 0x3F3;
                ordinary.state.r = std::array::from_fn(|i| {
                    seed.wrapping_add(0x1234_5679u32.wrapping_mul(i as u32))
                });
                ordinary.fetch.prime(&code, 0x100);
                let mut block = ordinary.clone();
                ordinary.step(&mut code).unwrap();
                assert_eq!(block.cached_scalar_block(&mut code, 1), 1, "{opcode:04X}");
                assert_eq!(block, ordinary, "opcode={opcode:04X}, seed={seed:08X}");
            }
        }
    }
}
