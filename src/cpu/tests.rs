//! Original manual-derived SH-2 instruction vectors.
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
fn unavailable_exception_vector_is_a_sticky_bounded_bus_diagnostic() {
    let (mut cpu, mut bus) = setup(&[0xFFFF]);
    let before = cpu.state.clone();
    let error = CpuError::ExceptionEntry {
        pc: HIGH_WRAM,
        vector: 4,
        source: BusError::Unmapped {
            address: 16,
            width: 4,
        },
    };
    assert_eq!(cpu.step(&mut bus), Err(error.clone()));
    assert_eq!(
        error.to_string(),
        "Exception vector 4 entry at PC 0x06000000: Unmapped 4-byte access at 0x00000010"
    );
    let mut expected = before;
    expected.status = CpuStatus::Faulted(error.clone());
    assert_eq!(cpu.state, expected);
    bus.write16(HIGH_WRAM, 0x0009).unwrap();
    assert_eq!(cpu.step(&mut bus), Err(error));
    assert_eq!(cpu.state, expected);
}

#[test]
fn illegal_slots_dispatch_vector_six_and_save_the_branch_destination() {
    for opcode in [0xA000, 0x000B, 0xFFFF] {
        let (mut cpu, mut bus) = setup(&[0xA001, opcode]);
        cpu.state.vbr = HIGH_WRAM + 0x100;
        bus.write32(cpu.state.vbr + 24, HIGH_WRAM + 0x200).unwrap();
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HIGH_WRAM + 0x200);
        assert_eq!(cpu.state.delay_target, None);
        assert_eq!(bus.read32(cpu.state.r[15]), Ok(HIGH_WRAM + 6));
        assert_eq!(
            cpu.state.last_exception,
            Some(Exception {
                vector: 6,
                saved_pc: HIGH_WRAM + 6
            })
        );
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
        (0x8980, 1, -252_i32),
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
            Err(CpuError::ExceptionEntry { vector: 6, .. })
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

#[test]
fn carry_borrow_and_signed_overflow_match_wide_arithmetic() {
    let values = [0, 1, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFE, u32::MAX];
    for a in values {
        for b in values {
            for t in 0..=1 {
                for op in [0x312E, 0x312A, 0x612A, 0x312F, 0x312B] {
                    let (mut cpu, mut bus) = setup(&[op]);
                    cpu.state.r[1] = a;
                    cpu.state.r[2] = b;
                    cpu.state.sr = 0x3F2 | t;
                    let (wide, flag) = match op {
                        0x312E => {
                            let v = a as i64 + b as i64 + t as i64;
                            (v, v > u32::MAX as i64)
                        }
                        0x312A => {
                            let v = a as i64 - b as i64 - t as i64;
                            (v, v < 0)
                        }
                        0x612A => {
                            let v = -(b as i64) - t as i64;
                            (v, v < 0)
                        }
                        _ => {
                            let v = if op == 0x312F {
                                a as i32 as i64 + b as i32 as i64
                            } else {
                                a as i32 as i64 - b as i32 as i64
                            };
                            (v, !(i32::MIN as i64..=i32::MAX as i64).contains(&v))
                        }
                    };
                    cpu.step(&mut bus).unwrap();
                    assert_eq!(cpu.state.r[1], wide as u32, "{op:04X} {a:08X} {b:08X} {t}");
                    assert_eq!(cpu.state.sr, 0x3F2 | u32::from(flag));
                }
            }
        }
    }
}

#[test]
fn comparisons_and_byte_string_comparison_cover_signed_and_unsigned_order() {
    for (a, b) in [
        (0, 0),
        (u32::MAX, 1),
        (0x80000000, 0x7FFFFFFF),
        (1, u32::MAX),
    ] {
        for (op, expected) in [
            (0x3120, a == b),
            (0x3122, a >= b),
            (0x3123, a as i32 >= b as i32),
            (0x3126, a > b),
            (0x3127, a as i32 > b as i32),
        ] {
            let (mut cpu, mut bus) = setup(&[op]);
            cpu.state.r[1] = a;
            cpu.state.r[2] = b;
            cpu.state.sr = 0x3F3;
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.sr, 0x3F2 | u32::from(expected));
            assert_eq!((cpu.state.r[1], cpu.state.r[2]), (a, b));
        }
    }
    for (value, nonnegative, positive) in
        [(0, true, false), (1, true, true), (u32::MAX, false, false)]
    {
        for (op, expected) in [(0x4111, nonnegative), (0x4115, positive)] {
            let (mut cpu, mut bus) = setup(&[op]);
            cpu.state.r[1] = value;
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.sr & T, u32::from(expected));
        }
    }
    for byte in 0..4 {
        let (mut cpu, mut bus) = setup(&[0x212C]);
        cpu.state.r[1] = 0x11223344;
        cpu.state.r[2] = 0xEEDDCCBB ^ (0xFF << (byte * 8));
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.sr & T, 1);
    }
    let (mut cpu, mut bus) = setup(&[0x212C, 0x88FF]);
    cpu.state.r[1] = 0x11223344;
    cpu.state.r[2] = 0x44332211;
    cpu.state.r[0] = u32::MAX;
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.sr & T, 0);
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.sr & T, 1);
}

#[test]
fn swaps_extensions_extract_and_negate_preserve_non_t_flags() {
    for (op, expected) in [
        (0x6127, 0x76543210),
        (0x6128, 0x89ABEFCD),
        (0x6129, 0xCDEF89AB),
        (0x612B, 0x76543211),
        (0x612C, 0xEF),
        (0x612D, 0xCDEF),
        (0x612E, 0xFFFFFFEF),
        (0x612F, 0xFFFFCDEF),
        (0x212D, 0xCDEF1234),
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0x12345678;
        cpu.state.r[2] = 0x89ABCDEF;
        cpu.state.sr = 0x3F3;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], expected, "{op:04X}");
        assert_eq!(cpu.state.sr, 0x3F3);
    }
}

#[test]
fn shifts_and_rotates_cover_sign_carry_and_preservation() {
    for (op, value, flag) in [
        (0x4100, 2, 1),
        (0x4120, 2, 1),
        (0x4101, 0x40000000, 1),
        (0x4121, 0xC0000000, 1),
        (0x4104, 3, 1),
        (0x4105, 0xC0000000, 1),
        (0x4124, 3, 1),
        (0x4125, 0xC0000000, 1),
        (0x4108, 4, 1),
        (0x4118, 256, 1),
        (0x4128, 65536, 1),
        (0x4109, 0x20000000, 1),
        (0x4119, 0x00800000, 1),
        (0x4129, 0x8000, 1),
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0x80000001;
        cpu.state.sr = 0x3F3;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], value, "{op:04X}");
        assert_eq!(cpu.state.sr, 0x3F2 | flag);
    }
    for (op, expected) in [(0x4124, 2), (0x4125, 0x40000000)] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0x80000001;
        cpu.state.sr = 0x3F2;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], expected);
        assert_eq!(cpu.state.sr, 0x3F3);
    }
    for op in [
        0x4100, 0x4120, 0x4101, 0x4121, 0x4104, 0x4105, 0x4124, 0x4125,
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0;
        cpu.state.sr = 0x3F3;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.sr, 0x3F2);
    }
}

#[test]
fn indirect_auto_update_handles_source_destination_aliases() {
    let address = HIGH_WRAM + 0x200;
    for width in [1_u32, 2, 4] {
        let code = width.trailing_zeros() as u16;
        let (mut cpu, mut bus) = setup(&[0x6214 | code, 0x6114 | code, 0x2114 | code]);
        cpu.state.r[1] = address;
        bus.write32(address, 0x89ABCDEF).unwrap();
        bus.write32(address + 4, 0x11223344).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], address + width);
        assert_eq!(
            cpu.state.r[2],
            match width {
                1 => 0xFFFFFF89,
                2 => 0xFFFF89AB,
                _ => 0x89ABCDEF,
            }
        );
        cpu.step(&mut bus).unwrap(); // n == m suppresses postincrement
        let loaded = cpu.state.r[1];
        assert_eq!(
            loaded,
            match width {
                1 => 0xFFFFFFAB,
                2 => 0xFFFFCDEF,
                _ => 0x11223344,
            }
        );
        cpu.state.r[1] = address;
        cpu.step(&mut bus).unwrap(); // same-register predecrement stores original R1
        assert_eq!(cpu.state.r[1], address - width);
        let read = match width {
            1 => bus.read8(address - 1).unwrap() as u32,
            2 => bus.read16(address - 2).unwrap() as u32,
            _ => bus.read32(address - 4).unwrap(),
        };
        assert_eq!(
            read,
            address
                & match width {
                    1 => 0xFF,
                    2 => 0xFFFF,
                    _ => u32::MAX,
                }
        );
    }
}

#[test]
fn indexed_displaced_and_gbr_addressing_obey_width_and_sign_extension() {
    for (store, load, width, offset) in [
        (0x0214, 0x032C, 1, 8),
        (0x0215, 0x032D, 2, 8),
        (0x0216, 0x032E, 4, 8),
        (0x121F, 0x532F, 4, 60),
        (0x802F, 0x842F, 1, 15),
        (0x812F, 0x852F, 2, 30),
        (0xC0FF, 0xC4FF, 1, 255),
        (0xC1FF, 0xC5FF, 2, 510),
        (0xC2FF, 0xC6FF, 4, 1020),
    ] {
        let address = HIGH_WRAM + 0x400;
        let (mut cpu, mut bus) = setup(&[store, load]);
        cpu.state.r[0] = if store & 0xF000 == 0 { 8 } else { 0x89ABCDEF };
        cpu.state.r[1] = 0x89ABCDEF;
        cpu.state.r[2] = address;
        cpu.state.gbr = address;
        cpu.step(&mut bus).unwrap();
        let expected = match width {
            1 => 0xFFFFFFEF,
            2 => 0xFFFFCDEF,
            _ => 0x89ABCDEF,
        };
        assert_eq!(
            read_operand(&bus, address + offset, width),
            Ok(expected),
            "{store:04X}"
        );
        cpu.step(&mut bus).unwrap();
        let dest = if load & 0xF000 == 0x8000 || load & 0xF000 == 0xC000 {
            0
        } else {
            3
        };
        assert_eq!(cpu.state.r[dest], expected, "{load:04X}");
        assert_eq!(cpu.state.sr, 0xF0);
    }
}

#[test]
fn pc_relative_word_and_mova_use_signed_data_and_slot_destination() {
    for offset in [0, 2] {
        let (mut cpu, mut bus) = setup(&[0x0009, 0x0009]);
        cpu.state.pc += offset;
        bus.write16(HIGH_WRAM + offset, 0x91FF).unwrap();
        bus.write16(HIGH_WRAM + offset + 4 + 510, 0x8001).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], 0xFFFF8001);
    }
    for op in [0x9101, 0xC701] {
        let (mut cpu, mut bus) = setup(&[0xA003, op]);
        bus.write16(HIGH_WRAM + 14, 0xFFFE).unwrap();
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HIGH_WRAM + 10);
        if op == 0x9101 {
            assert_eq!(cpu.state.r[1], 0xFFFFFFFE);
        } else {
            assert_eq!(cpu.state.r[0], HIGH_WRAM + 16);
        }
    }
}

#[test]
fn logical_register_immediate_and_byte_memory_operations() {
    for (op, expected) in [
        (0x2129, 0x005500AA),
        (0x212A, 0xFF00FF00),
        (0x212B, 0xFF55FFAA),
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0xAA55AAAA;
        cpu.state.r[2] = 0x555555AA;
        cpu.state.sr = 0x3F3;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], expected);
        assert_eq!(cpu.state.sr, 0x3F3);
    }
    for (op, expected) in [(0xC9A5, 0x81), (0xCAA5, 0x12345664), (0xCBA5, 0x123456E5)] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[0] = 0x123456C1;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[0], expected);
    }
    let address = HIGH_WRAM + 0x100;
    for (op, byte, t) in [
        (0xCC0F, 0xA0, 1),
        (0xCCF0, 0xA0, 0),
        (0xCD0F, 0, 0),
        (0xCEFF, 0x5F, 0),
        (0xCF0F, 0xAF, 0),
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.gbr = address - 3;
        cpu.state.r[0] = 3;
        bus.write8(address, 0xA0).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(bus.read8(address), Ok(byte));
        assert_eq!(cpu.state.sr, 0xF0 | t);
    }
    for initial in [0, 1, 0x80] {
        let (mut cpu, mut bus) = setup(&[0x411B]);
        cpu.state.r[1] = address;
        bus.write8(address, initial).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(bus.read8(address), Ok(initial | 0x80));
        assert_eq!(cpu.state.sr & T, u32::from(initial == 0));
    }
    let (mut cpu, mut bus) = setup(&[0x2128, 0x0018, 0x0129, 0x0008, 0x0229]);
    cpu.state.r[1] = 0x80;
    cpu.state.r[2] = 0x7F;
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.sr & T, 1);
    for _ in 0..4 {
        cpu.step(&mut bus).unwrap();
    }
    assert_eq!((cpu.state.r[1], cpu.state.r[2]), (1, 0));
}

#[test]
fn multiply_width_signedness_and_full_64_bit_accumulation() {
    for (op, mach, macl) in [
        (0x0127, 0xAABBCCDD, 0xFFFFFFFE),
        (0x212E, 0xAABBCCDD, 0xFFFE),
        (0x212F, 0xAABBCCDD, 0xFFFFFFFE),
        (0x3125, 0x7FFFFFFF, 0xFFFFFFFE),
        (0x312D, 0, 0xFFFFFFFE),
    ] {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.r[1] = 0x80000001;
        cpu.state.r[2] = 0xFFFFFFFE;
        cpu.state.mach = 0xAABBCCDD;
        cpu.state.sr = 0x3F3;
        cpu.step(&mut bus).unwrap();
        assert_eq!((cpu.state.mach, cpu.state.macl), (mach, macl), "{op:04X}");
        assert_eq!(cpu.state.sr, 0x3F3);
    }
    let (mut cpu, mut bus) = setup(&[0x0028]);
    cpu.state.set_accumulator(u64::MAX);
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.accumulator(), 0);
}

#[test]
fn mac_signed_products_saturation_and_same_register_postincrements() {
    for (opcode, width, a, b, acc, saturated, expected) in [
        (0x412F, 2, 0xFFFE, 3, 5_u64, false, u64::MAX),
        (0x012F, 4, 0xFFFFFFFE, 3, 5, false, u64::MAX),
        (0x412F, 2, 1, 1, u64::MAX, false, 0),
        (0x012F, 4, 1, 1, u64::MAX, false, 0),
        (
            0x412F,
            2,
            1,
            1,
            0x123456787FFFFFFF,
            true,
            0x123456787FFFFFFF,
        ),
        (
            0x412F,
            2,
            0xFFFF,
            1,
            0x1234567880000000,
            true,
            0x1234567880000000,
        ),
        (
            0x012F,
            4,
            1,
            1,
            0x00007FFFFFFFFFFF,
            true,
            0x00007FFFFFFFFFFF,
        ),
        (
            0x012F,
            4,
            0xFFFFFFFF,
            1,
            0xFFFF800000000000,
            true,
            0xFFFF800000000000,
        ),
        (
            0x012F,
            4,
            0x80000000,
            0x80000000,
            0,
            true,
            0x00007FFFFFFFFFFF,
        ),
    ] {
        let (mut cpu, mut bus) = setup(&[opcode]);
        cpu.state.r[1] = HIGH_WRAM + 0x200;
        cpu.state.r[2] = HIGH_WRAM + 0x300;
        cpu.state.set_accumulator(acc);
        cpu.state.sr = 0x3F1 | if saturated { S } else { 0 };
        write_operand(&mut bus, cpu.state.r[1], width, a).unwrap();
        write_operand(&mut bus, cpu.state.r[2], width, b).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.accumulator(), expected, "{opcode:04X}");
        assert_eq!(cpu.state.r[1], HIGH_WRAM + 0x200 + width);
        assert_eq!(cpu.state.r[2], HIGH_WRAM + 0x300 + width);
    }
    for (opcode, width) in [(0x411F, 2), (0x011F, 4)] {
        let (mut cpu, mut bus) = setup(&[opcode]);
        cpu.state.r[1] = HIGH_WRAM + 0x200;
        write_operand(&mut bus, cpu.state.r[1], width, 3).unwrap();
        write_operand(&mut bus, cpu.state.r[1] + width, width, 5).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.macl, 15);
        assert_eq!(cpu.state.r[1], HIGH_WRAM + 0x200 + width * 2);
    }
}

#[test]
fn div0s_initializes_q_m_t_for_every_sign_pair() {
    for q in 0..=1 {
        for m in 0..=1 {
            let (mut cpu, mut bus) = setup(&[0x2127, 0x0019]);
            cpu.state.r[1] = q << 31;
            cpu.state.r[2] = m << 31;
            cpu.state.sr = 0x3F3;
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.sr, 0xF2 | (q << 8) | (m << 9) | (q ^ m));
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.sr, 0xF2);
        }
    }
}

#[test]
fn unsigned_division_guest_sequences_produce_host_quotients() {
    let mut program = vec![0x0019]; // DIV0U
    for _ in 0..32 {
        program.extend([0x4124, 0x3024]);
    } // ROTCL R1; DIV1 R2,R0
    program.push(0x4124);
    for dividend in [0, 1, 7, 0x7FFFFFFF, 0x80000000, u32::MAX] {
        for divisor in [1, 2, 3, 7, 0x7FFFFFFF, 0x80000000, u32::MAX] {
            let (mut cpu, mut bus) = setup(&program);
            cpu.state.r[1] = dividend;
            cpu.state.r[2] = divisor;
            for _ in &program {
                cpu.step(&mut bus).unwrap();
            }
            assert_eq!(
                cpu.state.r[1],
                dividend / divisor,
                "{dividend:08X}/{divisor:08X}"
            );
            let remainder =
                cpu.state.r[0].wrapping_add(if cpu.state.sr & Q != 0 { divisor } else { 0 });
            assert_eq!(remainder, dividend % divisor);
        }
    }
}

#[test]
fn div1_signed_mode_vectors_cover_add_subtract_carry_and_borrow() {
    // (old SR bits Q/M/T, dividend, divisor, new dividend, new SR bits Q/M/T)
    for (flags, dividend, divisor, result, expected) in [
        (0x000, 1, 3, 0xFFFFFFFF, 0x100),
        (0x001, 1, 3, 0, 1),
        (0x100, 0xFFFFFFFF, 1, 0xFFFFFFFF, 0x100),
        (0x200, 0, 1, 1, 0x301),
        (0x200, 0xFFFFFFFF, 3, 1, 0x301),
        (0x300, 1, 3, 0xFFFFFFFF, 0x200),
        (0x301, 1, 3, 0, 0x301),
        (0x300, 0x80000000, 1, 0xFFFFFFFF, 0x301),
    ] {
        let (mut cpu, mut bus) = setup(&[0x3124]);
        cpu.state.r[1] = dividend;
        cpu.state.r[2] = divisor;
        cpu.state.sr = 0xF2 | flags;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], result);
        assert_eq!(cpu.state.sr, 0xF2 | expected, "{flags:03X} {dividend:08X}");
    }
}

#[test]
fn control_and_system_register_transfers_cover_every_register_and_form() {
    for (index, control) in [
        (0, true),
        (1, true),
        (2, true),
        (0, false),
        (1, false),
        (2, false),
    ] {
        let reg = index << 4;
        let get = 0x0100 | reg | if control { 2 } else { 10 };
        let push = 0x4300 | reg | if control { 3 } else { 2 };
        let set = 0x4200 | reg | if control { 14 } else { 10 };
        let pop = 0x4300 | reg | if control { 7 } else { 6 };
        let (mut cpu, mut bus) = setup(&[get, push, set, pop]);
        // Distinct values and direct field assertions keep this independent of
        // the interpreter's register-selector helpers.
        let initial = [
            0x3F3, 0x10203040, 0x55667788, 0xDEADBEEF, 0x89ABCDEF, 0xFEDCBA98,
        ];
        let selected = index as usize + if control { 0 } else { 3 };
        let original = initial[selected];
        [
            cpu.state.sr,
            cpu.state.gbr,
            cpu.state.vbr,
            cpu.state.mach,
            cpu.state.macl,
            cpu.state.pr,
        ] = initial;
        cpu.state.r[2] = 0xFFFFFFFF;
        cpu.state.r[3] = HIGH_WRAM + 0x200;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[1], original);
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.r[3], HIGH_WRAM + 0x1FC);
        assert_eq!(bus.read32(cpu.state.r[3]), Ok(original));
        cpu.step(&mut bus).unwrap();
        let mut changed = initial;
        changed[selected] = if selected == 0 { 0x3F3 } else { u32::MAX };
        assert_eq!(
            [
                cpu.state.sr,
                cpu.state.gbr,
                cpu.state.vbr,
                cpu.state.mach,
                cpu.state.macl,
                cpu.state.pr
            ],
            changed
        );
        cpu.step(&mut bus).unwrap();
        assert_eq!(
            [
                cpu.state.sr,
                cpu.state.gbr,
                cpu.state.vbr,
                cpu.state.mach,
                cpu.state.macl,
                cpu.state.pr
            ],
            initial
        );
        assert_eq!(cpu.state.r[3], HIGH_WRAM + 0x200);
    }
}

#[test]
fn far_branches_latch_register_targets_and_call_return_addresses() {
    for (opcode, call, relative) in [
        (0x410B, true, false),
        (0x412B, false, false),
        (0x0103, true, true),
        (0x0123, false, true),
    ] {
        let (mut cpu, mut bus) = setup(&[opcode, 0x7102]);
        cpu.state.r[1] = if relative {
            0xFFFFFFFC
        } else {
            HIGH_WRAM + 0x100
        };
        cpu.state.pr = 42;
        cpu.step(&mut bus).unwrap();
        let target = if relative {
            HIGH_WRAM
        } else {
            HIGH_WRAM + 0x100
        };
        assert_eq!(cpu.state.delay_target, Some(target));
        assert_eq!(cpu.state.pr, if call { HIGH_WRAM + 4 } else { 42 });
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, target);
    }
}

#[test]
fn delayed_conditional_branch_latches_t_and_untaken_branch_has_no_slot() {
    for opcode in [0x8D03, 0x8F03] {
        for taken in [false, true] {
            let (mut cpu, mut bus) = setup(&[opcode, 0x0018]);
            let t = (opcode == 0x8D03) == taken;
            cpu.state.sr = 0xF0 | u32::from(t);
            cpu.step(&mut bus).unwrap();
            assert_eq!(
                cpu.state.delay_target,
                if taken { Some(HIGH_WRAM + 10) } else { None }
            );
            cpu.step(&mut bus).unwrap();
            assert_eq!(cpu.state.pc, HIGH_WRAM + if taken { 10 } else { 4 });
            assert_eq!(cpu.state.sr & T, 1);
        }
    }
}

#[test]
fn trapa_rte_round_trip_restores_masked_sr_before_the_delay_slot() {
    let (mut cpu, mut bus) = setup(&[0xC37F, 0x7101]);
    cpu.state.sr = 0x3F3;
    cpu.state.vbr = HIGH_WRAM + 0x400;
    let handler = HIGH_WRAM + 0x800;
    bus.write32(cpu.state.vbr + 0x7F * 4, handler).unwrap();
    bus.write16(handler, 0x002B).unwrap();
    bus.write16(handler + 2, 0x0129).unwrap(); // MOVT R1 observes restored T
    let stack = cpu.state.r[15];
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.pc, handler);
    assert_eq!(bus.read32(stack - 8), Ok(HIGH_WRAM + 2));
    assert_eq!(bus.read32(stack - 4), Ok(0x3F3));
    cpu.state.sr = 0;
    bus.write32(stack - 4, 0xFFFFFFFF).unwrap();
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.sr, SR_MASK);
    assert_eq!(cpu.state.r[15], stack);
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.r[1], 1);
    assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
    assert_eq!(cpu.state.instructions, 3);
    assert_eq!(cpu.state.exceptions, 1);
}

#[test]
fn illegal_and_all_pc_writing_slot_instructions_dispatch_without_retiring() {
    let invalid = [0xFFFF, 0x0000, 0xF123];
    let forbidden = [
        0xA000, 0xB000, 0x8900, 0x8B00, 0x8D00, 0x8F00, 0x410B, 0x412B, 0x0103, 0x0123, 0x000B,
        0x002B, 0xC300,
    ];
    for op in invalid.into_iter().chain(forbidden) {
        let (mut cpu, mut bus) = setup(&[0xA001, op]);
        cpu.state.vbr = HIGH_WRAM + 0x100;
        bus.write32(cpu.state.vbr + 24, HIGH_WRAM + 0x200).unwrap();
        cpu.step(&mut bus).unwrap();
        let before = cpu.state.clone();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.instructions, 1);
        assert_eq!(cpu.state.pr, before.pr);
        assert_eq!(cpu.state.r[..15], before.r[..15]);
        assert_eq!(
            cpu.state.last_exception,
            Some(Exception {
                vector: 6,
                saved_pc: HIGH_WRAM + 6
            })
        );
    }
    for op in invalid {
        let (mut cpu, mut bus) = setup(&[op]);
        cpu.state.vbr = HIGH_WRAM + 0x100;
        bus.write32(cpu.state.vbr + 16, HIGH_WRAM + 0x200).unwrap();
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.instructions, 0);
        assert_eq!(
            cpu.state.last_exception,
            Some(Exception {
                vector: 4,
                saved_pc: HIGH_WRAM
            })
        );
    }
}

#[test]
fn sleep_is_bounded_and_interrupt_entry_obeys_mask_vector_and_wakes_cpu() {
    let (mut cpu, mut bus) = setup(&[0x001B, 0x7101]);
    cpu.state.vbr = HIGH_WRAM + 0x100;
    cpu.state.sr = 0x62;
    bus.write32(cpu.state.vbr + 64 * 4, HIGH_WRAM + 0x400)
        .unwrap();
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.status, CpuStatus::Sleeping);
    let sleeping = cpu.state.clone();
    for _ in 0..100 {
        cpu.step(&mut bus).unwrap();
    }
    assert_eq!(cpu.state, sleeping);
    for level in [0, 1, 6, 16, 255] {
        assert!(!cpu.accept_interrupt(&mut bus, level, 64).unwrap());
    }
    assert_eq!(cpu.state, sleeping);
    assert!(cpu.accept_interrupt(&mut bus, 7, 64).unwrap());
    assert_eq!(cpu.state.status, CpuStatus::Running);
    assert_eq!(cpu.state.sr, 0x72);
    assert_eq!(cpu.state.pc, HIGH_WRAM + 0x400);
    assert_eq!(bus.read32(cpu.state.r[15]), Ok(HIGH_WRAM + 2));
    assert_eq!(bus.read32(cpu.state.r[15] + 4), Ok(0x62));
    assert_eq!(cpu.state.instructions, 1);
}

#[test]
fn interrupts_wait_for_slots_and_successors_of_all_special_transfers() {
    for op in [
        0x0002, 0x0012, 0x0022, 0x000A, 0x001A, 0x002A, 0x4103, 0x4113, 0x4123, 0x4102, 0x4112,
        0x4122, 0x410E, 0x411E, 0x412E, 0x410A, 0x411A, 0x412A, 0x4107, 0x4117, 0x4127, 0x4106,
        0x4116, 0x4126,
    ] {
        let (mut cpu, mut bus) = setup(&[op, 0x0009]);
        cpu.state.r[1] = HIGH_WRAM + 0x100;
        bus.write32(cpu.state.r[1], 0).unwrap();
        cpu.step(&mut bus).unwrap();
        assert!(!cpu.accept_nmi(&mut bus).unwrap(), "{op:04X}");
        cpu.step(&mut bus).unwrap();
        // Some transfers changed VBR; provide vector 11 at its new location.
        if cpu.state.vbr == 0 {
            cpu.state.vbr = HIGH_WRAM + 0x200;
        }
        bus.write32(cpu.state.vbr + 44, HIGH_WRAM + 0x300).unwrap();
        assert!(cpu.accept_nmi(&mut bus).unwrap(), "{op:04X}");
        assert_eq!(cpu.state.sr & 0xF0, 0xF0);
    }
    let (mut cpu, mut bus) = setup(&[0xA001, 0x0009]);
    cpu.step(&mut bus).unwrap();
    assert!(!cpu.accept_nmi(&mut bus).unwrap());
    cpu.step(&mut bus).unwrap();
    cpu.state.vbr = HIGH_WRAM + 0x100;
    bus.write32(cpu.state.vbr + 44, HIGH_WRAM + 0x200).unwrap();
    assert!(cpu.accept_nmi(&mut bus).unwrap());
    assert_eq!(
        cpu.state.last_exception,
        Some(Exception {
            vector: 11,
            saved_pc: HIGH_WRAM + 6
        })
    );
}

#[test]
fn all_65536_encodings_match_the_independent_manual_inventory() {
    // This verifies decoding, not arithmetic correctness. The separate semantic
    // tests above exercise flags, data, addresses, aliases and control flow.
    let mut inventory = vec![None; 65536];
    let mut patterns = 0;
    for line in include_str!("../../tests/fixtures/sh2-encodings.txt").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let (name, bits) = line.split_once(' ').unwrap();
        assert_eq!(bits.len(), 16);
        let mut mask = 0_u16;
        let mut value = 0_u16;
        for ch in bits.chars() {
            mask <<= 1;
            value <<= 1;
            match ch {
                '0' => mask |= 1,
                '1' => {
                    mask |= 1;
                    value |= 1;
                }
                'n' | 'm' | 'i' | 'd' => (),
                _ => panic!("Invalid inventory pattern {bits}"),
            }
        }
        for (opcode, entry) in inventory.iter_mut().enumerate() {
            if opcode as u16 & mask == value {
                assert!(entry.replace(name).is_none(), "Overlapping pattern {name}");
            }
        }
        patterns += 1;
    }
    assert_eq!(patterns, 142);
    assert_eq!(
        inventory.iter().filter(|item| item.is_some()).count(),
        53752
    );

    // A uniform, fully mapped test bus isolates the CPU decoder from Saturn MMIO.
    // Operand addresses are distinct from instruction/vector/stack storage.
    let mut bus = FlatMemory(vec![0; 65536]);
    for (opcode, expected) in inventory.into_iter().enumerate() {
        let mut cpu = Cpu::start(0x100, 0x8000);
        cpu.state.r[..15].fill(0x1000);
        cpu.state.gbr = 0x2000;
        cpu.state.vbr = 0x3000;
        bus.write16(0x100, opcode as u16).unwrap();
        cpu.step(&mut bus)
            .unwrap_or_else(|e| panic!("{opcode:04X} {expected:?}: {e}"));
        assert_eq!(
            cpu.state.instructions,
            u64::from(expected.is_some()),
            "{opcode:04X} {expected:?}"
        );
        if expected.is_none() {
            assert_eq!(
                cpu.state.last_exception,
                Some(Exception {
                    vector: 4,
                    saved_pc: 0x100
                })
            );
        }
    }
}

struct FlatMemory(Vec<u8>);
impl Memory for FlatMemory {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        Ok(self.0[a as usize])
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        assert_eq!(a % 2, 0);
        Ok(u16::from_be_bytes(
            self.0[a as usize..a as usize + 2].try_into().unwrap(),
        ))
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        assert_eq!(a % 4, 0);
        Ok(u32::from_be_bytes(
            self.0[a as usize..a as usize + 4].try_into().unwrap(),
        ))
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.0[a as usize] = v;
        Ok(())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        assert_eq!(a % 2, 0);
        self.0[a as usize..a as usize + 2].copy_from_slice(&v.to_be_bytes());
        Ok(())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        assert_eq!(a % 4, 0);
        self.0[a as usize..a as usize + 4].copy_from_slice(&v.to_be_bytes());
        Ok(())
    }
}
