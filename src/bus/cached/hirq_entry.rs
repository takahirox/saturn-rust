//! Learn a resident prefix that often reaches a physical CD status word.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::bus) struct Entry {
    pc: u32,
    opcode: u16,
}

#[derive(Clone, Copy)]
pub(in crate::bus) struct Hint {
    pub(super) entry: Entry,
    pub(super) address: u32,
}

impl Entry {
    pub(super) fn new(cpu: &Cpu) -> Option<Self> {
        Some(Self {
            pc: cpu.state().pc,
            opcode: cpu.decoded_opcode()?,
        })
    }

    pub(super) fn starts_readonly_prefix(self) -> bool {
        matches!(
            Cpu::cached_class(self.opcode),
            CachedClass::Scalar | CachedClass::Control
        )
    }

    pub(super) fn slot(self) -> usize {
        ((self.pc >> 1) ^ u32::from(self.opcode)) as usize & 15
    }
}

pub(super) fn next_word(cpu: &Cpu, ready: u64) -> Option<u32> {
    let opcode = cpu.decoded_opcode()?;
    let address = cpu.cached_load_address(opcode)?;
    if !(0x2000_0000..0x4000_0000).contains(&address)
        || Bus::physical(address) != crate::cd::REGISTERS + 8
    {
        return None;
    }
    (cpu.prepare_cached_instruction(opcode, ready)?.operand == Some((address, 2)))
        .then_some(address)
}
