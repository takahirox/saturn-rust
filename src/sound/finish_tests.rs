//! Elapsed retirement barriers versus the original journaled barrier path.
use super::*;

fn make(program: &[u16], busy: bool) -> Sound {
    let mut s = Sound::default();
    for vector in 2..64 {
        s.ram[vector * 4..vector * 4 + 4].copy_from_slice(&0x3A0u32.to_be_bytes());
    }
    s.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
    s.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
    for vector in [2, 3] {
        s.ram[vector * 4..vector * 4 + 4].copy_from_slice(&0x3C0u32.to_be_bytes());
    }
    s.ram[25 * 4..26 * 4].copy_from_slice(&0x380u32.to_be_bytes());
    let put = |s: &mut Sound, at: usize, words: &[u16]| {
        for (i, word) in words.iter().enumerate() {
            s.ram[at + i * 2..at + i * 2 + 2].copy_from_slice(&word.to_be_bytes());
        }
    };
    put(&mut s, 0x200, &[0x46FC, 0x2000]); // Permit the source-authored IRQ.
    put(&mut s, 0x204, program);
    put(&mut s, 0x380, &[0x33FC, 0x20, 0x10, 0x422, 0x4E73]);
    put(&mut s, 0x3A0, &[0x4E73]); // Ordinary/trace frame.
    put(&mut s, 0x3C0, &[0x4E72, 0x2700]); // Retain the group-zero frame.
    s.scsp.write16(0x424, 0x20).unwrap();
    if busy {
        for (i, b) in s.ram[0x1000..0x1100].iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37);
        }
        for (at, value) in [
            (2, 0x1000),
            (6, 128),
            (12, 0x100),
            (16, 0x7800),
            (22, 0xE000),
            (0, 0x1820),
            (0x400, 0x20F),
            (0x780, 0x1000),
            (0x80C, 0xC000),
        ] {
            s.scsp.write16(at, value).unwrap();
        }
    }
    s.set_enabled(true).unwrap();
    s
}

#[test]
fn elapsed_barriers_match_literal_cycles_for_arithmetic_trace_fault_reset_and_dram_contention() {
    let programs: &[&[u16]] = &[
        &[0x4E71, 0x60FC],
        &[0x7003, 0xC0FC, 17, 0x60F8], // MULU needs future internal clocks.
        &[0x7009, 0x80FC, 3, 0x60F8],  // DIVU needs future internal clocks.
        &[0x46FC, 0xA000, 0x4E71, 0x60FC], // Trace precedes the next IRQ boundary.
        &[0x4E70, 0x4E71, 0x60FA],
        &[0x48E7, 0xFFFE, 0x4CDF, 0x7FFF, 0x60F6],
        &[0x4E72, 0x2000, 0x60FA],
        &[0x3039, 9, 0, 0x60F8], // Source address bus error.
    ];
    for &program in programs {
        for busy in [false, true] {
            for chunk in [1, 3, 19, 127] {
                let (mut fast, mut literal) = (make(program, busy), make(program, busy));
                literal.transactions.literal_finishes = true;
                let events = [257u64, 1021, 4093, 8191, 16381, 24000];
                for target in events {
                    while fast.time < target {
                        let clocks = (target - fast.time).min(chunk) as u32;
                        assert_eq!(fast.advance(clocks), literal.advance(clocks));
                        assert_eq!(fast.cpu.state, literal.cpu.state);
                        assert_eq!(
                            (
                                fast.time,
                                fast.cpu_ticks,
                                fast.next_cpu,
                                fast.ram_busy_until
                            ),
                            (
                                literal.time,
                                literal.cpu_ticks,
                                literal.next_cpu,
                                literal.ram_busy_until
                            )
                        );
                        assert_eq!(
                            (fast.reset_output_start, fast.reset_output_end),
                            (literal.reset_output_start, literal.reset_output_end)
                        );
                    }
                    for s in [&mut fast, &mut literal] {
                        match target {
                            257 => s.scsp.write16(0x41E, 0x20).unwrap(),
                            1021 => s.scsp.write16(0x420, 0x20).unwrap(),
                            4093 => s.scsp.write16(0x422, 0x20).unwrap(),
                            8191 => s.scsp.write16(0x400, 0xF).unwrap(),
                            16381 => s.write_ram8(0x801, 0x56),
                            _ => (),
                        }
                    }
                }
                assert!(
                    fast.transactions.elided_finishes > 0,
                    "fast path must execute"
                );
                assert_eq!(literal.transactions.elided_finishes, 0);
                assert_eq!(fast.ram, literal.ram);
                assert_eq!(fast.memory.trace, literal.memory.trace);
                assert_eq!(fast.scsp.dsp, literal.scsp.dsp);
                assert_eq!(fast.scsp.slots, literal.scsp.slots);
                assert_eq!(fast.scsp.common, literal.scsp.common);
                assert_eq!(fast.scsp.timer_phase, literal.scsp.timer_phase);
                assert_eq!(fast.scsp.output, literal.scsp.output);
                assert_eq!(
                    (
                        fast.scsp.generated,
                        fast.scsp.dropped,
                        fast.scsp.pending.get(),
                        fast.scsp.main_pending.get()
                    ),
                    (
                        literal.scsp.generated,
                        literal.scsp.dropped,
                        literal.scsp.pending.get(),
                        literal.scsp.main_pending.get()
                    )
                );
            }
        }
    }
}
