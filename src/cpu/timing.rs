//! SH-1/SH-2 Programming Manual §7.3/table7.2: EX-to-EX execution
//! states without contention. SH-1-only instructions are excluded by execute.
//! Fetch/operand waits belong to the CPU-local bus. Load WB/EX interlocks
//! are tracked per CPU below; multiplier and IF/MA overlap remain separate.

pub(super) fn execution_states(op: u16, t: bool) -> u32 {
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
        _ if matches!(op & 0xF00F, 0x000F | 0x400F) => 3, // MAC.L/MAC.W
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
            (0x013F, 3),
            (0x4EDF, 3),
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

/// Programming Manual §7.5/fig.7.9. WB follows the successor's EX stage.
/// A conflicting successor splits the slot once; repeated loads and MAC use
/// the documented MA forwarding path. The state belongs to each CPU.
#[derive(Debug, Default, Clone)]
pub(super) struct LoadPipeline {
    destination: Option<u8>,
}
impl LoadPipeline {
    pub fn issue(&mut self, opcode: u16) -> u32 {
        let load = load_destination(opcode);
        let hazard = self.destination.is_some_and(|register| {
            load != Some(register)
                && !matches!(opcode & 0xF00F, 0x000F | 0x400F)
                && register_uses(opcode) & (1 << register) != 0
        });
        self.destination = load;
        u32::from(hazard)
    }
}
fn load_destination(op: u16) -> Option<u8> {
    match op >> 12 {
        0 if matches!(op & 15, 0xC..=0xE) => Some((op >> 8 & 15) as u8),
        5 | 9 | 0xD => Some((op >> 8 & 15) as u8),
        6 if matches!(op & 15, 0..=2 | 4..=6) => Some((op >> 8 & 15) as u8),
        8 if matches!(op & 0xF00, 0x400 | 0x500) => Some(0),
        0xC if matches!(op >> 8 & 15, 4..=6) => Some(0),
        _ => None,
    }
}
fn register_uses(op: u16) -> u16 {
    let n = 1 << (op >> 8 & 15);
    let m = 1 << (op >> 4 & 15);
    match op >> 12 {
        0 => match op & 0xFF {
            0x02 | 0x12 | 0x22 | 0x0A | 0x1A | 0x2A | 0x29 | 0x03 | 0x23 => n,
            _ if matches!(op & 15, 4..=6 | 0xC..=0xE) => n | m | 1,
            _ if matches!(op & 15, 7 | 15) => n | m,
            _ => 0,
        },
        1 | 2 | 3 | 5 | 6 => n | m,
        4 | 7 | 9 | 0xD | 0xE => n,
        8 => match op >> 8 & 15 {
            0 | 1 | 4 | 5 => m | 1,
            8 => 1,
            _ => 0,
        },
        0xC => 1, // GBR displacements, MOVA and immediate R0 logic
        _ => 0,
    }
}

#[cfg(test)]
mod load_pipeline_tests {
    use super::*;
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
