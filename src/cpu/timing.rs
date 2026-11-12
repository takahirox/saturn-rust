//! SH-1/SH-2 Programming Manual §7.3/table7.2: EX-to-EX execution
//! states without contention. SH-1-only instructions are excluded by execute.
//! Fetch/operand waits belong to the CPU-local bus. Load WB/EX interlocks
//! are tracked per CPU below; multiplier access is in multiplier.rs.

pub(super) const fn execution_states(op: u16, t: bool) -> u32 {
    match op {
        0x000B => 2,                             // RTS
        0x002B => 4,                             // RTE
        0x001B => 3, // SLEEP (subsequent sleeping polls issue no instruction)
        _ if op & 0xFF00 == 0xC300 => 8, // TRAPA
        _ if matches!(op >> 12, 0xA | 0xB) => 2, // BRA/BSR
        _ if matches!(op & 0xF0FF, 0x0003 | 0x0023 | 0x400B | 0x402B) => 2,
        _ if matches!(op & 0xFF00, 0x8900 | 0x8B00 | 0x8D00 | 0x8F00) => {
            let taken = t == (op & 0x0200 == 0);
            if !taken {
                1
            } else if op & 0x0400 == 0 {
                3
            } else {
                2
            }
        }
        _ if matches!(op & 0xFF00, 0xCC00 | 0xCD00 | 0xCE00 | 0xCF00) => 3,
        _ if op & 0xF0FF == 0x401B => 4,                  // TAS.B
        _ if matches!(op & 0xF00F, 0x000F | 0x400F) => 2, // MAC.L/MAC.W, fig7.23/34
        _ if matches!(op & 0xF00F, 0x0007 | 0x3005 | 0x300D) => 2, // MUL.L/DMUL
        _ if matches!(op & 0xF0FF, 0x4007 | 0x4017 | 0x4027) => 3, // LDC.L
        _ if matches!(op & 0xF0FF, 0x4003 | 0x4013 | 0x4023) => 2, // STC.L
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manual_no_contention_vectors_cover_every_non_single_state_family() {
        // Literal expectations from table7.2, independently of CPU results.
        for (op, states) in [
            (0x000B, 2),
            (0x002B, 4),
            (0x001B, 3),
            (0xC380, 8),
            (0xA800, 2),
            (0xB7FF, 2),
            (0x0103, 2),
            (0x0F23, 2),
            (0x4B0B, 2),
            (0x452B, 2),
            (0xCCFF, 3),
            (0xCD00, 3),
            (0xCE80, 3),
            (0xCF01, 3),
            (0x4A1B, 4),
            (0x013F, 2),
            (0x4EDF, 2),
            (0x0217, 2),
            (0x37E5, 2),
            (0x3F0D, 2),
            (0x4E07, 3),
            (0x4217, 3),
            (0x4F27, 3),
            (0x4303, 2),
            (0x4B13, 2),
            (0x4C23, 2),
        ] {
            assert_eq!(execution_states(op, false), states, "{op:04X}");
            assert_eq!(execution_states(op, true), states, "{op:04X}");
        }
        for high in [0x89, 0x8B, 0x8D, 0x8F] {
            for disp in 0..=255 {
                let op = high << 8 | disp;
                let taken = high & 2 == 0;
                assert_eq!(execution_states(op, !taken), 1);
                assert_eq!(
                    execution_states(op, taken),
                    if high & 4 == 0 { 3 } else { 2 }
                );
            }
        }
    }
    #[test]
    fn single_state_transfer_arithmetic_logic_shift_and_system_vectors() {
        for op in [
            0x0009, 0x0008, 0x0018, 0x0028, 0x0019, 0xEFFF, 0x713F, 0x6123, 0x6120, 0x6121, 0x6122,
            0x6124, 0x6125, 0x6126, 0x2210, 0x2211, 0x2212, 0x2214, 0x2215, 0x2216, 0x2127, 0x212F,
            0x312C, 0x312E, 0x312F, 0x3128, 0x312A, 0x312B, 0x3124, 0x2128, 0x2129, 0x212A, 0x212B,
            0x4100, 0x4101, 0x4121, 0x400E, 0x401E, 0x402E, 0x4006, 0x4016, 0x4026, 0x4002, 0x4012,
            0x4022,
        ] {
            assert_eq!(execution_states(op, false), 1, "{op:04X}");
        }
    }
}

// Instruction properties depend only on the ISA word, not guest memory. Keep
// the literal classifiers as the source of the table and the reference oracle.
#[derive(Clone, Copy)]
pub(super) struct Instruction(u64);
impl Instruction {
    const fn new(op: u16) -> Self {
        let load = load_destination(op);
        let destination = match load {
            Some(register) => register,
            None => 17,
        };
        let mut uses = ex_register_uses(op, false);
        let mut forwarded = ex_register_uses(op, true);
        if matches!(op & 0xF00F, 0x000F | 0x400F) {
            uses = 0;
            forwarded = 0;
        } else if let Some(register) = load {
            uses &= !(1 << register);
            forwarded &= !(1 << register);
        }
        let forward = load.is_some() && !matches!(op & 0xF0FF, 0x000A | 0x001A | 0x4026);
        Self(
            uses as u64
                | (forwarded as u64) << 17
                | (destination as u64) << 34
                | (forward as u64) << 39
                | (execution_states(op, false) as u64) << 40
                | (execution_states(op, true) as u64) << 44
                | (control_flags(op) as u64) << 48
                | (prepared_class(op) as u64) << 56,
        )
    }

    pub fn prepared_scalar(self) -> Option<super::scalar::Kind> {
        super::scalar::decode_prepared((self.0 >> 56) as u8)
    }
    pub fn prepared_load(self, opcode: u16, s: &super::CpuState) -> Option<(u32, u32)> {
        let class = (self.0 >> 56) as u8;
        if class & 0x40 == 0 {
            return None;
        }
        let descriptor = class & 0x3F;
        let width = u32::from(descriptor & 7);
        let m = (opcode >> 4 & 15) as usize;
        let imm = u32::from(opcode as u8);
        let pc = || {
            s.delay_target
                .map_or(s.pc.wrapping_add(4), |t| t.wrapping_add(2))
        };
        let address = match descriptor >> 3 {
            1 => s.r[m],
            2 => s.r[m].wrapping_add(s.r[0]),
            3 => s.r[m].wrapping_add(u32::from(opcode & 15) * 4),
            4 => s.r[m].wrapping_add(u32::from(opcode & 15) * width),
            5 => s.gbr.wrapping_add(imm * width),
            6 => pc().wrapping_add(imm * 2),
            7 => (pc() & !3).wrapping_add(imm * 4),
            _ => return None,
        };
        Some((address, width))
    }

    pub fn control_flags(self) -> u8 {
        (self.0 >> 48) as u8
    }

    pub fn states(self, t: bool) -> u32 {
        (self.0 >> (40 + u32::from(t) * 4) & 15) as u32
    }
}

pub(super) const UNCONDITIONAL: u8 = 1;
pub(super) const REWRITES_PC: u8 = 2;
pub(super) const DEFERRED_IF: u8 = 4;
pub(super) const LDC_LONG: u8 = 8;
pub(super) const STC_LONG: u8 = 16;
pub(super) const BYTE_MEMORY: u8 = 32;

pub(super) const fn control_flags(op: u16) -> u8 {
    let unconditional = matches!(op & 0xF000, 0xA000 | 0xB000)
        || matches!(op & 0xF0FF, 0x0003 | 0x0023 | 0x400B | 0x402B)
        || matches!(op, 0x000B | 0x002B);
    let rewrites =
        unconditional || matches!(op & 0xFF00, 0x8900 | 0x8B00 | 0x8D00 | 0x8F00 | 0xC300);
    let deferred = matches!(op & 0xF00F, 0x0007 | 0x3005 | 0x300D | 0x000F | 0x400F);
    let ldc = matches!(op & 0xF0FF, 0x4007 | 0x4017 | 0x4027);
    let stc = matches!(op & 0xF0FF, 0x4003 | 0x4013 | 0x4023);
    let byte = matches!(op & 0xFF00, 0xCC00..=0xCF00) || op & 0xF0FF == 0x401B;
    ((unconditional as u8) * UNCONDITIONAL)
        | ((rewrites as u8) * REWRITES_PC)
        | ((deferred as u8) * DEFERRED_IF)
        | ((ldc as u8) * LDC_LONG)
        | ((stc as u8) * STC_LONG)
        | ((byte as u8) * BYTE_MEMORY)
}

// One prepared-operation class shares the existing ISA metadata load. Zero is
// unsupported; 1..63 are scalar tags, 0x40..0x7F ordinary MOV descriptors.
const fn prepared_class(op: u16) -> u8 {
    if let Some(kind) = super::scalar::decode_literal(op) {
        kind as u8 + 1
    } else {
        let load = prepared_load_descriptor(op);
        if load == 0 { 0 } else { 0x40 | load }
    }
}

// Ordinary MOV families only. LDS.L PR retains its dedicated system/MDU path.
const fn prepared_load_descriptor(op: u16) -> u8 {
    match op >> 12 {
        0 if matches!(op & 15, 0xC..=0xE) => (2 << 3) | (1 << (op & 3)),
        5 => (3 << 3) | 4,
        6 if matches!(op & 15, 0..=2 | 4..=6) => (1 << 3) | (1 << (op & 3)),
        8 if matches!(op >> 8 & 15, 4 | 5) => (4 << 3) | (1 << (op >> 8 & 1)),
        9 => (6 << 3) | 2,
        0xC if matches!(op >> 8 & 15, 4..=6) => (5 << 3) | (1 << (op >> 8 & 3)),
        0xD => (7 << 3) | 4,
        _ => 0,
    }
}

const fn make_instructions() -> [Instruction; 65536] {
    let mut table = [Instruction(0); 65536];
    let mut op = 0;
    while op < table.len() {
        table[op] = Instruction::new(op as u16);
        op += 1;
    }
    table
}

#[allow(long_running_const_eval)]
static INSTRUCTIONS: [Instruction; 65536] = make_instructions();

pub(super) fn instruction(opcode: u16) -> Instruction {
    INSTRUCTIONS[usize::from(opcode)]
}

/// Programming Manual §7.5/fig.7.9. WB follows the successor's EX stage.
/// A conflicting successor splits the slot once; repeated loads and MAC use
/// the documented MA forwarding path. The state belongs to each CPU.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct LoadPipeline {
    destination: Option<u8>,
    /// Only memory loads use the store-data forwarding rule; multiplier/system
    /// register writeback retains its separate, established interlocks.
    forward_store_data: bool,
    /// The previous MOV's real IF already split its MA/EX slot.
    fetch_split: bool,
    /// Shared by MOV loads/stores and multiplier memory phases. A waited successor fetch
    /// consumes this final MA/EX clock; replay journals the observed clock.
    next_ex: Option<u64>,
    pub issue_overlap: u32,
}
impl LoadPipeline {
    /// A split IF has already consumed the preceding load's WB interlock.
    /// A retained MA/EX clock still needs the ordinary issue path.
    pub fn scalar_ready(&self) -> bool {
        self.next_ex.is_none() && (self.destination.is_none() || self.fetch_split)
    }

    pub fn same_shifted(&self, prior: &Self, elapsed: u64) -> bool {
        let Self {
            destination,
            forward_store_data,
            fetch_split,
            next_ex,
            issue_overlap,
        } = self;
        *destination == prior.destination
            && *forward_store_data == prior.forward_store_data
            && *fetch_split == prior.fetch_split
            && *issue_overlap == prior.issue_overlap
            && *next_ex == prior.next_ex.map(|at| at + elapsed)
    }

    pub fn shift_cached_clock(&mut self, clocks: u64) {
        self.next_ex = self.next_ex.map(|at| at + clocks);
    }

    pub fn issue_clock(&mut self, now: u64) -> u64 {
        self.issue_overlap = u32::from(self.next_ex.take().is_some_and(|ex| now == ex + 1));
        now - u64::from(self.issue_overlap)
    }

    pub fn retain_ex(&mut self, ex: u64) {
        self.next_ex = Some(ex);
    }

    pub fn load_split(&mut self, split: bool) {
        self.fetch_split = split;
    }

    pub fn issue_instruction(&mut self, instruction: Instruction) -> u32 {
        let uses = (instruction.0 >> (u32::from(self.forward_store_data) * 17)) as u32 & 0x1FFFF;
        let hazard = !self.fetch_split
            && self
                .destination
                .is_some_and(|register| uses & (1 << register) != 0);
        let destination = (instruction.0 >> 34 & 31) as u8;
        self.fetch_split = false;
        self.destination = (destination < 17).then_some(destination);
        self.forward_store_data = instruction.0 & (1 << 39) != 0;
        u32::from(hazard)
    }

    pub fn issue(&mut self, opcode: u16) -> u32 {
        let load = load_destination(opcode);
        let hazard = !self.fetch_split
            && self.destination.is_some_and(|register| {
                load != Some(register)
                    && !matches!(opcode & 0xF00F, 0x000F | 0x400F)
                    && ex_register_uses(opcode, self.forward_store_data) & (1 << register) != 0
            });
        self.fetch_split = false;
        self.destination = load;
        self.forward_store_data =
            load.is_some() && !matches!(opcode & 0xF0FF, 0x000A | 0x001A | 0x4026);
        u32::from(hazard)
    }
}
const fn load_destination(op: u16) -> Option<u8> {
    // §7.7.6 figs7.86/7.90: LDS.L to PR and STS from MACH/MACL
    // have the same WB stage as memory-to-GPR loads. PR is register16 in
    // this private dependency mask, separate from every general register.
    if op & 0xF0FF == 0x4026 {
        return Some(16);
    }
    if matches!(op & 0xF0FF, 0x000A | 0x001A) {
        return Some((op >> 8 & 15) as u8);
    }
    match op >> 12 {
        0 if matches!(op & 15, 0xC..=0xE) => Some((op >> 8 & 15) as u8),
        5 | 9 | 0xD => Some((op >> 8 & 15) as u8),
        6 if matches!(op & 15, 0..=2 | 4..=6) => Some((op >> 8 & 15) as u8),
        8 if matches!(op & 0xF00, 0x400 | 0x500) => Some(0),
        0xC if matches!(op >> 8 & 15, 4..=6) => Some(0),
        _ => None,
    }
}
// Ordinary store data is needed only in MA and can forward from load WB
// (Renesas Rev.5.00 §7.2.2, p.419/fig7.14). Keep address/index uses in EX,
// including aliases where the data register also supplies the address.
const fn ex_register_uses(op: u16, forward_store_data: bool) -> u32 {
    let n = 1 << (op >> 8 & 15);
    let m = 1 << (op >> 4 & 15);
    let pr = 1 << 16;
    if op == 0x000B {
        return pr;
    }
    if op >> 12 == 0xB {
        return pr;
    }
    if matches!(op & 0xF0FF, 0x002A | 0x4022 | 0x402A | 0x400B | 0x0003) {
        return n | pr;
    }
    match op >> 12 {
        0 => match op & 0xFF {
            0x02 | 0x12 | 0x22 | 0x0A | 0x1A | 0x2A | 0x29 | 0x03 | 0x23 => n,
            _ if forward_store_data && matches!(op & 15, 4..=6) => n | 1,
            _ if matches!(op & 15, 4..=6 | 0xC..=0xE) => n | m | 1,
            _ if matches!(op & 15, 7 | 15) => n | m,
            _ => 0,
        },
        1 if forward_store_data => n,
        2 if forward_store_data && matches!(op & 15, 0..=2 | 4..=6) => n,
        1 | 2 | 3 | 5 | 6 => n | m,
        4 | 7 | 9 | 0xD | 0xE => n,
        8 => match op >> 8 & 15 {
            0 | 1 if forward_store_data => m,
            0 | 1 | 4 | 5 => m | 1,
            8 => 1,
            _ => 0,
        },
        0xC if forward_store_data && matches!(op >> 8 & 15, 0..=2) => 0,
        0xC => 1, // GBR displacements, MOVA and immediate R0 logic
        _ => 0,
    }
}

#[cfg(test)]
mod load_pipeline_tests {
    use super::*;
    #[test]
    fn system_register_writeback_uses_the_actual_destination_and_implicit_pr() {
        // §7.7.6 figs7.86/7.90, §7.5 fig7.9: STS MACH/MACL and
        // LDS.L PR have WB. STS PR and LDS register forms do not.
        for n in 0..16 {
            for system in [0x000A, 0x001A] {
                let load = system | n << 8;
                for (successor, expected) in [
                    (0x7001 | n << 8, 1),
                    (0x6003 | n << 4, 1),
                    (0x2F02 | n << 4, 1), // multiplier WB retains its interlock
                    (0xE000 | n << 8, 1),
                    (0x0009, 0),
                    (0xB000 | n << 8, 0), // BSR displacement is not Rn
                ] {
                    let mut p = LoadPipeline::default();
                    p.issue(load);
                    assert_eq!(p.issue(successor), expected, "{load:04X}->{successor:04X}");
                }
            }
            for successor in [0x000B, 0x002A, 0x4022, 0x402A, 0x400B, 0x0003, 0xBFFF] {
                let mut p = LoadPipeline::default();
                p.issue(0x4026 | n << 8);
                assert_eq!(p.issue(successor), 1);
                assert_eq!(p.issue(successor), 0);
            }
            for successor in [0x0009, 0x0023, 0x402B, 0xAFFF, 0x4026 | n << 8] {
                let mut p = LoadPipeline::default();
                p.issue(0x4026 | n << 8);
                assert_eq!(p.issue(successor), 0);
            }
            for transfer in [0x002A, 0x400A, 0x401A, 0x402A] {
                let mut p = LoadPipeline::default();
                p.issue(transfer | n << 8);
                assert_eq!(p.issue(0x7001 | n << 8), 0);
            }
        }
    }
    #[test]
    fn all_load_widths_and_registers_split_only_the_dependent_successor() {
        // §7.5: same-destination repeated loads and MAC do not split.
        for n in 0..16 {
            for load in [
                0x6000, 0x6001, 0x6002, 0x6004, 0x6005, 0x6006, 0x5000, 0x000C, 0x000D, 0x000E,
                0x9000, 0xD000,
            ] {
                let load = load | n << 8;
                for successor in [
                    0x7001 | n << 8,
                    0x6003 | n << 4,
                    0x6003 | n << 8,
                    0xE000 | n << 8,
                    0x400B | n << 8,
                    0x2012 | n << 8,
                ] {
                    let mut pipeline = LoadPipeline::default();
                    assert_eq!(pipeline.issue(load), 0);
                    assert_eq!(pipeline.issue(successor), 1, "{load:04X}->{successor:04X}");
                    assert_eq!(pipeline.issue(successor), 0);
                }
                for successor in [0x0009, 0xA000, 0x6002 | n << 8, 0x400F | n << 8] {
                    let mut pipeline = LoadPipeline::default();
                    pipeline.issue(load);
                    assert_eq!(pipeline.issue(successor), 0, "{load:04X}->{successor:04X}");
                }
                let mut pipeline = LoadPipeline::default();
                pipeline.issue(load);
                pipeline.issue(9);
                assert_eq!(pipeline.issue(0x7001 | n << 8), 0);
            }
        }
    }
    #[test]
    fn ordinary_store_data_forwards_but_address_and_index_still_interlock() {
        // Renesas Rev.5.00 §7.2.2, p.419/fig7.14: load WB and store MA
        // forward data in the same cycle. Address operands are needed in EX.
        for n in 0..16 {
            for m in 0..16 {
                for width in 0..3 {
                    let mut stores = vec![
                        (0x2000 | n << 8 | m << 4 | width, 1 << n),
                        (0x2004 | n << 8 | m << 4 | width, 1 << n),
                        (0x0004 | n << 8 | m << 4 | width, (1 << n) | 1),
                        (0xC0FF | width << 8, 0),
                    ];
                    if width < 2 {
                        stores.push((0x800F | width << 8 | n << 4, 1 << n));
                    } else {
                        stores.push((0x100F | n << 8 | m << 4, 1 << n));
                    }
                    for (store, address_registers) in stores {
                        for loaded in 0..16 {
                            for split in [false, true] {
                                let mut pipeline = LoadPipeline::default();
                                pipeline.issue(0x60F2 | loaded << 8);
                                pipeline.load_split(split);
                                let expected =
                                    u32::from(!split && address_registers & (1 << loaded) != 0);
                                assert_eq!(
                                    pipeline.issue(store),
                                    expected,
                                    "R{loaded}->{store:04X}, split={split}"
                                );
                                assert_eq!(pipeline.issue(store), 0);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn displacement_and_gbr_loads_target_r0_without_treating_displacements_as_registers() {
        for load in [0x84FF, 0x85FF, 0xC4FF, 0xC5FF, 0xC6FF] {
            for (next, expected) in [
                (0x7001, 1),
                (0x7101, 0),
                (0xCA55, 1),
                (0xC755, 1),
                (0x8BFF, 0),
                (0x4F2B, 0),
            ] {
                let mut pipeline = LoadPipeline::default();
                pipeline.issue(load);
                assert_eq!(pipeline.issue(next), expected, "{load:04X}->{next:04X}");
            }
        }
    }
}

/// Ordinary MOV and LDS.L PR operand phase (P figs7.11/7.86). Address decoding
/// is shared by all widths; execution retains sign extension and update semantics.
pub(super) fn mov_load_address(op: u16, s: &super::CpuState) -> Option<u32> {
    if op & 0xF0FF == 0x4026 {
        return Some(s.r[(op >> 8 & 15) as usize]);
    }
    let m = (op >> 4 & 15) as usize;
    let imm = u32::from(op as u8);
    let pc = s
        .delay_target
        .map_or(s.pc.wrapping_add(4), |t| t.wrapping_add(2));
    match op >> 12 {
        0 if matches!(op & 15, 0xC..=0xE) => Some(s.r[m].wrapping_add(s.r[0])),
        5 => Some(s.r[m].wrapping_add(u32::from(op & 15) * 4)),
        6 if matches!(op & 15, 0..=2 | 4..=6) => Some(s.r[m]),
        8 if matches!(op >> 8 & 15, 4 | 5) => {
            Some(s.r[m].wrapping_add(u32::from(op & 15) * (1 << (op >> 8 & 1))))
        }
        9 => Some(pc.wrapping_add(imm * 2)),
        0xC if matches!(op >> 8 & 15, 4..=6) => {
            Some(s.gbr.wrapping_add(imm * (1 << (op >> 8 & 3))))
        }
        0xD => Some((pc & !3).wrapping_add(imm * 4)),
        _ => None,
    }
}

/// Ordinary stores, including STS.L PR (P fig7.87), share the MA/IF slot
/// rules, but introduce no WB hazard.
/// Address and data remain in the instruction's retained CPU snapshot during
/// replay. See docs/sh2-store-pipeline.md for acceptance/publication deadlines.
pub(super) fn mov_store_address(op: u16, s: &super::CpuState) -> Option<u32> {
    let n = (op >> 8 & 15) as usize;
    let m = (op >> 4 & 15) as usize;
    if op & 0xF0FF == 0x4022 {
        return Some(s.r[n].wrapping_sub(4));
    }
    match op >> 12 {
        0 if matches!(op & 15, 4..=6) => Some(s.r[n].wrapping_add(s.r[0])),
        1 => Some(s.r[n].wrapping_add(u32::from(op & 15) * 4)),
        2 if matches!(op & 15, 0..=2 | 4..=6) => {
            Some(s.r[n].wrapping_sub(if op & 4 != 0 { 1 << (op & 3) } else { 0 }))
        }
        8 if matches!(op >> 8 & 15, 0 | 1) => {
            Some(s.r[m].wrapping_add(u32::from(op & 15) * (1 << (op >> 8 & 1))))
        }
        0xC if matches!(op >> 8 & 15, 0..=2) => Some(
            s.gbr
                .wrapping_add(u32::from(op as u8) * (1 << (op >> 8 & 3))),
        ),
        _ => None,
    }
}

pub(super) struct MovAccess {
    end: u64,
    split: bool,
}
impl MovAccess {
    pub fn start(
        bus: &mut impl crate::bus::Memory,
        address: u32,
        write: bool,
        successor: u32,
        ex: u64,
    ) -> Result<Self, crate::bus::BusError> {
        let end = ex + 2;
        if let Some(now) = bus.sh2_clock() {
            let scheduled = bus.sh2_schedule_memory_access(address, write, end);
            let boundary = end - u64::from(scheduled);
            bus.sh2_pipeline_wait(boundary.saturating_sub(now) as u32)?;
        }
        Ok(Self {
            end,
            // IF beside MA belongs to the successor's second successor.
            // A delay-slot MOV follows the redirected target stream, so its
            // own PC no longer determines the IF/retained-if pairing.
            split: bus.sh2_fetch_slots() && successor & 2 == 0,
        })
    }

    pub fn split(&self) -> u32 {
        u32::from(self.split)
    }

    pub fn finish(
        self,
        pipeline: &mut LoadPipeline,
        bus: &mut impl crate::bus::Memory,
    ) -> Result<(), crate::bus::BusError> {
        let now = bus.sh2_clock().unwrap_or(self.end);
        // Invalid operand cycles suppress the device access, but still occupy
        // MA. They must not publish auto-update/branch state during EX.
        bus.sh2_pipeline_wait(self.end.saturating_sub(now) as u32)?;
        // A real IF and a dependency cause the same split (P figs7.6/7.9).
        // With retained if, recover the final clock of the shared MA/EX slot.
        pipeline.load_split(self.split);
        if !self.split {
            let end = now.max(self.end);
            pipeline.retain_ex(end - 1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod instruction_table_tests {
    use super::*;

    #[test]
    fn every_instruction_matches_literal_timing_and_all_load_dependency_states() {
        for opcode in 0..=u16::MAX {
            let instruction = instruction(opcode);
            assert_eq!(
                instruction.prepared_scalar().map(|k| k as u8),
                super::super::scalar::decode_literal(opcode).map(|k| k as u8)
            );
            assert_eq!(
                instruction.control_flags(),
                control_flags(opcode),
                "{opcode:04X}"
            );
            for t in [false, true] {
                assert_eq!(
                    instruction.states(t),
                    execution_states(opcode, t),
                    "{opcode:04X}/{t}"
                );
            }
            for previous in 0..=17 {
                for split in [false, true] {
                    for forward in [false, true] {
                        let mut literal = LoadPipeline {
                            destination: (previous < 17).then_some(previous),
                            fetch_split: split,
                            forward_store_data: forward,
                            next_ex: Some(123),
                            issue_overlap: 1,
                        };
                        let mut table = literal.clone();
                        assert_eq!(
                            table.issue_instruction(instruction),
                            literal.issue(opcode),
                            "{opcode:04X}/{previous}/{split}/{forward}"
                        );
                        assert_eq!(table, literal, "{opcode:04X}/{previous}/{split}/{forward}");
                    }
                }
            }
        }
    }
}
