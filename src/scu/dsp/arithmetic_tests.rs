//! Literal oracles from ST-097 §4.5 ADD/SUB/AD2 and SOA #8 §§3/5.1.
//! V replacement: ST-240-B-SP1-052295 printed p.5, Bugs.
use super::*;
use crate::scu::SCU;

fn program(a: u64, p: u64, words: &[u32]) -> Dsp {
    let mut d = Dsp {
        a,
        p,
        ..Dsp::default()
    };
    for &word in words {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x18000);
    d
}

#[test]
fn arithmetic_literal_width_flags_and_parallel_result_do_not_require_writeback() {
    // op, A, P, complete ALU result, S/Z/C/V. High A survives 32-bit ops.
    for (op, a, p, result, flags) in [
        (4, 0x1234_7fff_ffff, 1, 0x1234_8000_0000, 0x480000),
        (4, 0xabcd_8000_0000, 0xffff_ffff, 0xabcd_7fff_ffff, 0x180000),
        (4, 0x1234_ffff_ffff, 1, 0x1234_0000_0000, 0x300000),
        (4, 0, 0, 0, 0x200000),
        (4, 0x80000000, 0, 0x80000000, 0x400000),
        (5, 0x80000000, 1, 0x7fffffff, 0x080000),
        (5, 0x7fffffff, 0xffffffff, 0x80000000, 0x580000),
        (5, 0, 1, 0xffffffff, 0x500000),
        (5, 1, 1, 0, 0x200000),
        (5, 0xffffffff, 0, 0xffffffff, 0x400000),
        (6, 0x7fff_ffff_ffff, 1, 0x8000_0000_0000, 0x480000),
        (
            6,
            0x8000_0000_0000,
            0xffff_ffff_ffff,
            0x7fff_ffff_ffff,
            0x180000,
        ),
        (6, 0xffff_ffff_ffff, 1, 0, 0x300000),
        (6, 0, 0, 0, 0x200000),
        (6, 0x8000_0000_0000, 0, 0x8000_0000_0000, 0x400000),
        (6, 0x7fffffff, 1, 0x80000000, 0),
        (6, 0xffffffff, 1, 0x1_00000000, 0),
    ] {
        for y in [0, 0x20000, 0x40000] {
            // retain A, CLR A, MOV ALU,A
            for old_v in [0, OVERFLOW] {
                // Parallel ALL -> MC2; next NOP ALH -> MC3; END.
                let mut d = program(a, p, &[op << 26 | y | 0x3209, 0x330a, 0xf0000000]);
                d.flags.set(old_v);
                d.step().unwrap();
                assert_eq!(d.flags.get(), flags, "op={op}, a={a:X}, p={p:X}");
                assert_eq!(d.data[2][0], result as u32);
                assert_eq!(
                    d.a,
                    match y {
                        0 => a,
                        0x20000 => 0,
                        _ => result,
                    }
                );
                d.step().unwrap();
                assert_eq!(d.data[3][0], (d.a >> 16) as u32);
                d.step().unwrap();
                assert_eq!(d.control() & 0x7c0000, flags);
                assert_eq!(d.control() & 0x7c0000, flags);
            }
        }
    }
}

#[test]
fn arithmetic_replaces_overflow_without_any_intervening_status_read() {
    for (op, a, p, first, second) in [
        (4, 0x7fffffff, 1, 0x480000, 0x400000),
        (4, 0x80000000, 0xffffffff, 0x180000, 0x100000),
        (5, 0x80000000, 1, 0x080000, 0),
        (5, 0x7fffffff, 0xffffffff, 0x580000, 0x500000),
        (6, 0x7fff_ffff_ffff, 1, 0x480000, 0x400000),
        (6, 0x8000_0000_0000, 0xffff_ffff_ffff, 0x180000, 0x100000),
    ] {
        let word = op << 26 | 0x40000;
        let mut d = program(a, p, &[word, word, 0xf8000000]);
        d.step().unwrap();
        assert_eq!(d.flags.get(), first);
        d.step().unwrap();
        assert_eq!(d.flags.get(), second);
        assert!(d.step().unwrap());
        assert_eq!(d.control() & 0x7c0000, second | END);
        assert_eq!(d.control() & 0x7c0000, second);
        assert!(!d.step().unwrap());
    }
}

#[test]
fn nop_transfers_logical_and_shifts_preserve_v_before_the_next_arithmetic() {
    // Flags below include retained V after overflowing ADD. Logical ops clear C.
    for (instruction, flags) in [
        (0, 0x480000),
        (0x00003209, 0x480000), // NOP, ALL -> MC2
        (0x00020000, 0x480000), // CLR A changes no flags
        (0x94000001, 0x480000), // MVI 1,P
        (0x01960000, 0x480000), // M0 -> A, M1 -> P
        (0x04000000, 0x280000),
        (0x08000000, 0x480000),
        (0x0c000000, 0x480000),
        (0x20000000, 0x480000),
        (0x24000000, 0x080000),
        (0x28000000, 0x380000),
        (0x2c000000, 0x180000),
        (0x3c000000, 0x080000),
    ] {
        let mut d = program(
            0x7fffffff,
            1,
            &[0x10040000, instruction, 0x00020000, 0x10000000],
        );
        d.step().unwrap();
        d.step().unwrap();
        assert_eq!(d.flags.get(), flags, "instruction={instruction:08X}");
        d.step().unwrap(); // CLR A; following ADD is always non-overflowing
        d.step().unwrap();
        assert_eq!(d.flags.get() & OVERFLOW, 0);
    }
}

#[test]
fn actual_multiply_and_transfers_construct_48_bit_overflow_in_both_directions() {
    for (rx, expected) in [
        (0x10000, [0x400000000000, 0x800000000000, 0xc00000000000, 0]),
        (
            0xffff0000,
            [0xc00000000000, 0x800000000000, 0x400000000000, 0],
        ),
    ] {
        // M0 -> X, M1 -> Y; MUL -> P / CLR A; four AD2 / ALU -> A.
        let mut d = program(
            0,
            0,
            &[
                0x02084000, 0x01020000, 0x18040000, 0x18040000, 0x18040000, 0x18040000, 0xf0000000,
            ],
        );
        d.data[0][0] = rx;
        d.data[1][0] = 0x40000000;
        d.step().unwrap();
        d.step().unwrap();
        let flags = if rx == 0x10000 {
            [0, 0x480000, 0x400000, 0x300000]
        } else {
            [0x400000, 0x500000, 0x180000, 0x300000]
        };
        for (value, flags) in expected.into_iter().zip(flags) {
            d.step().unwrap();
            assert_eq!(d.a, value);
            assert_eq!(d.flags.get(), flags);
        }
    }
}

#[test]
fn halted_reads_acknowledge_only_e_and_pause_step_end_events_remain_distinct() {
    for end in [0xf0000000, 0xf8000000] {
        let mut d = program(0x7fffffff, 1, &[0x10040000, end, 0xf8000000]);
        d.set_control(0x2010000);
        assert!(!d.step().unwrap());
        assert_eq!(d.pc, 0);
        d.set_control(0x4010000);
        assert!(!d.step().unwrap());
        d.set_control(0); // stop then single-step END/ENDI
        d.set_control(0x20000);
        assert_eq!(d.step().unwrap(), end == 0xf8000000);
        assert_eq!(
            d.control() & 0x7c0000,
            0x480000 | if end == 0xf8000000 { END } else { 0 }
        );
        for _ in 0..3 {
            assert_eq!(d.control() & 0x7c0000, 0x480000);
            assert!(!d.step().unwrap());
        }
        d.set_control(0x20000);
        assert!(d.step().unwrap()); // E was acknowledged; a new ENDI emits once
        assert!(!d.step().unwrap());
        assert_eq!(d.control() & 0x7c0000, 0x4c0000);
        d = Dsp::default();
        assert_eq!(d.control(), 0);
        assert!(!d.step().unwrap());
    }
}
