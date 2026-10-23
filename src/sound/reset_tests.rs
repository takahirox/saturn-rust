//! MC68000UM §5.5 fig.5-30 and table8-14. Authored bus-phase vectors;
//! expected values come from writes placed either side of each read deadline.
use super::*;

fn until(sound: &mut Sound, clocks: u64, chunk: u64) {
    let target = sound.cpu_origin + (clocks * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
    while sound.time < target {
        sound
            .advance((target - sound.time).min(chunk) as u32)
            .unwrap();
    }
}
fn fixture() -> Sound {
    let mut sound = Sound::default();
    sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
    sound.ram[0x200..0x204].copy_from_slice(&[0x70, 0x25, 0x4E, 0x71]);
    sound
}

#[test]
fn ea_internal_clocks_delay_operand_sampling_and_sound_off_cancels_the_wait() {
    for chunk in [1, 3, 17, 10000] {
        for indexed in [false, true] {
            // Reset40, MOVEA extension cycles44/48, next issue52.
            // Next program fetch56, then the EA wait ends58. Indexed mode
            // also consumes its extension/refill at60 and waits until62.
            // MOVE.W's operand therefore samples at62 /66 respectively.
            let sample = if indexed { 66 } else { 62 };
            let mut sound = fixture();
            sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
            let mut words: Vec<u16> = vec![
                0x207C,
                0,
                if indexed { 0x600 } else { 0x602 },
                if indexed { 0x3030 } else { 0x3020 },
            ];
            if indexed {
                words.push(0x1000);
            } // d8(A0,D1.W)
            words.extend([0x33C0, 0, 0x800, 0x4E72, 0x2700, 0x4E71, 0x4E71]);
            for (i, word) in words.into_iter().enumerate() {
                sound.ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
            }
            sound.ram[0x600..0x602].copy_from_slice(&0x1234u16.to_be_bytes());
            sound.set_enabled(true).unwrap();
            until(&mut sound, sample - 1, chunk);
            assert_eq!(sound.cpu.state.d[0], 0);
            sound.ram[0x600..0x602].copy_from_slice(&0x5678u16.to_be_bytes());
            until(&mut sound, sample, chunk);
            assert_eq!(sound.cpu.state.d[0], 0x5678, "indexed={indexed}");
            until(&mut sound, 160, chunk);
            assert_eq!(&sound.ram[0x800..0x802], &0x5678u16.to_be_bytes());
            assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
            assert_eq!(sound.cpu.state.exceptions, 0);
        }
    }
    for boundary in [56, 57, 58, 59, 61] {
        let mut sound = fixture();
        sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        for (i, word) in [0x207Cu16, 0, 0x602, 0x3020, 0x4E72, 0x2700, 0x4E71]
            .into_iter()
            .enumerate()
        {
            sound.ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        sound.set_enabled(true).unwrap();
        until(&mut sound, boundary, 1);
        sound.set_enabled(false).unwrap();
        sound.ram[0x600..0x602].copy_from_slice(&0x7654u16.to_be_bytes());
        sound.advance(1000).unwrap();
        assert_eq!(sound.cpu.state.d[0], 0);
        sound.set_enabled(true).unwrap();
        until(&mut sound, 100, 7);
        assert_eq!(sound.cpu.state.d[0], 0x7654);
    }
}

#[test]
fn reset_vectors_and_prefetch_are_six_independent_reads_not_an_early_snapshot() {
    for chunk in [1, 3, 17, 10000] {
        let mut sound = fixture();
        sound.set_enabled(true).unwrap();
        assert_eq!(sound.cpu.state.pc, 0);
        assert_eq!(sound.cpu.state.a[7], 0);
        until(&mut sound, 19, chunk);
        sound.ram[0..2].copy_from_slice(&0x0006u16.to_be_bytes());
        until(&mut sound, 20, chunk); // SSP high latched, low still live
        sound.ram[0..4].copy_from_slice(&0x00054320u32.to_be_bytes());
        until(&mut sound, 23, chunk);
        assert_eq!(sound.cpu.state.a[7], 0);
        until(&mut sound, 24, chunk);
        assert_eq!(sound.cpu.state.a[7], 0x00064320);
        assert_eq!(sound.cpu.state.ssp, 0x00064320);
        until(&mut sound, 28, chunk); // PC high latched
        sound.ram[4..8].copy_from_slice(&0x00010200u32.to_be_bytes());
        until(&mut sound, 31, chunk);
        assert_eq!(sound.cpu.state.pc, 0);
        until(&mut sound, 32, chunk);
        assert_eq!(sound.cpu.state.pc, 0x200);
        sound.ram[6..8].copy_from_slice(&0x0100u16.to_be_bytes());
        until(&mut sound, 36, chunk); // opcode 7025 retained from 200
        sound.ram[0x200..0x204].copy_from_slice(&[0x70, 0x11, 0x72, 0x36]);
        until(&mut sound, 39, chunk);
        assert_eq!(sound.cpu.state.instructions, 0);
        until(&mut sound, 40, chunk);
        assert_eq!(sound.cpu.state.instructions, 0);
        assert_eq!(sound.cpu.state.d[0], 0);
        until(&mut sound, 44, chunk); // first MOVEQ's four clocks follow reset
        assert_eq!(sound.cpu.state.d[0], 0x25);
        assert_eq!(sound.cpu.state.pc, 0x202);
        until(&mut sound, 47, chunk);
        assert_eq!(sound.cpu.state.d[1], 0); // next prefetch still in flight
        until(&mut sound, 48, chunk);
        assert_eq!(sound.cpu.state.d[1], 0x36);
        assert_eq!(sound.cpu.state.exceptions, 0);
    }
}

#[test]
fn sound_off_cancels_each_partial_reset_and_on_starts_a_new_sequence() {
    for boundary in [0, 19, 20, 23, 24, 27, 28, 31, 32, 35, 36, 39] {
        let mut sound = fixture();
        sound.set_enabled(true).unwrap();
        until(&mut sound, boundary, 7);
        sound.set_enabled(false).unwrap();
        sound.ram[0..8].copy_from_slice(&0x00054320_00000200u64.to_be_bytes());
        sound.advance(2000).unwrap();
        assert_eq!(sound.cpu.state.instructions, 0);
        sound.set_enabled(true).unwrap();
        until(&mut sound, 44, 7);
        assert_eq!(sound.cpu.state.a[7], 0x54320, "cancel at {boundary}");
        assert_eq!(sound.cpu.state.pc, 0x202);
        assert_eq!(sound.cpu.state.d[0], 0x25);
    }
}

#[test]
fn reset_prefetch_fault_halts_without_stack_writes_or_recursive_exception() {
    for pc in [0x201u32, 0x80000, 0x100018] {
        let mut sound = fixture();
        sound.ram[4..8].copy_from_slice(&pc.to_be_bytes());
        let ram = sound.ram.clone();
        sound.set_enabled(true).unwrap();
        until(&mut sound, 35, 1);
        assert!(!sound.cpu.state.halted);
        until(&mut sound, 36, 1);
        assert!(sound.cpu.state.halted);
        until(&mut sound, 200, 17);
        assert_eq!(sound.cpu.state.instructions, 0);
        assert_eq!(sound.cpu.state.exceptions, 0);
        assert_eq!(sound.ram, ram);
        sound.set_enabled(false).unwrap();
        sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        sound.set_enabled(true).unwrap();
        until(&mut sound, 44, 17);
        assert!(!sound.cpu.state.halted);
        assert_eq!(sound.cpu.state.d[0], 0x25);
    }
}

#[test]
fn reset_prefetch_uses_24_bit_bus_pins_without_truncating_architectural_pc() {
    let mut sound = fixture();
    sound.ram[4..8].copy_from_slice(&0x01000200u32.to_be_bytes());
    sound.set_enabled(true).unwrap();
    until(&mut sound, 44, 1);
    assert_eq!(sound.cpu.state.pc, 0x01000202);
    assert_eq!(sound.cpu.state.d[0], 0x25);
    assert_eq!(sound.cpu.state.instructions, 1);
    assert!(!sound.cpu.state.halted);
}

#[test]
fn unlk_samples_each_frame_word_at_its_own_sound_bus_completion() {
    // PRM p.4-194; UM §§5.1/6.3.9.1. Reset/prefetch 0..40;
    // MOVEA completes its extensions at44/48 and its 12 clocks at52;
    // UNLK prefetch56, frame-high60, frame-low64. The shared RAM can change
    // between those cycles, but an already completed read is retained.
    for chunk in [1, 3, 17, 10000] {
        let mut sound = fixture();
        sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        let words: [u16; 11] = [
            0x2E7C, 0, 0x6000, // MOVEA.L #$6000,A7
            0x4E5F, // UNLK A7
            0x23CF, 0, 0x800, // MOVE.L A7,$800
            0x4E72, 0x2700, 0x4E71, 0x4E71,
        ];
        for (i, word) in words.into_iter().enumerate() {
            sound.ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        sound.scsp.write16(0x400, 0x200).unwrap(); // Connected full sound RAM.
        sound.ram[0x6000..0x6004].copy_from_slice(&0x00066200u32.to_be_bytes());
        sound.set_enabled(true).unwrap();
        until(&mut sound, 59, chunk);
        assert_eq!(sound.cpu.state.a[7], 0x6000);
        until(&mut sound, 60, chunk);
        sound.ram[0x6000..0x6004].copy_from_slice(&0x00076300u32.to_be_bytes());
        until(&mut sound, 63, chunk);
        assert_eq!(sound.cpu.state.a[7], 0x6000); // no partial register load
        until(&mut sound, 64, chunk);
        assert_eq!(sound.cpu.state.a[7], 0x00066304);
        until(&mut sound, 120, chunk);
        assert_eq!(&sound.ram[0x800..0x804], &0x00066304u32.to_be_bytes());
        assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
        assert_eq!(sound.cpu.state.exceptions, 0);
    }
}

#[test]
fn reset_instruction_holds_devices_for_124_clocks_with_four_clock_tail() {
    for chunk in [1, 7, 10000] {
        let mut sound = fixture();
        sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        sound.ram[0x200..0x208].copy_from_slice(&[0x4E, 0x70, 0x70, 37, 0x4E, 0x72, 0x27, 0]);
        sound.scsp.write16(0x400, 0x20F).unwrap();
        sound.cpu.state.d[3] = 0x12345678;
        sound.set_enabled(true).unwrap();
        until(&mut sound, 43, chunk);
        assert!(!sound.scsp.reset_held);
        assert_eq!(sound.scsp.common[0], 0x20F);
        until(&mut sound, 44, chunk);
        assert!(sound.scsp.reset_held);
        sound.scsp.write16(0x400, 0x20F).unwrap();
        sound.scsp.write8(0x401, 15).unwrap();
        assert_eq!(sound.scsp.common[0], 0);
        assert!(!sound.scsp.receive_midi(0x3A));
        until(&mut sound, 167, chunk);
        assert!(sound.scsp.reset_held);
        assert_eq!(sound.cpu.state.d[0], 0);
        until(&mut sound, 168, chunk);
        assert!(!sound.scsp.reset_held);
        sound.scsp.write16(0x400, 0x20F).unwrap();
        until(&mut sound, 171, chunk);
        assert_eq!(sound.cpu.state.d[0], 0);
        until(&mut sound, 180, chunk);
        assert_eq!(sound.cpu.state.d[0], 37);
        assert_eq!(sound.cpu.state.d[3], 0x12345678);
        assert_eq!(sound.scsp.common[0], 0x20F);
        assert_eq!(sound.cpu.state.exceptions, 0);
        assert_eq!(sound.ram[0x203], 37);
    }
}

#[test]
fn disabling_cpu_during_reset_output_does_not_strand_external_devices_in_reset() {
    for edge in [44, 80, 167] {
        let mut sound = fixture();
        sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
        sound.ram[0x200..0x204].copy_from_slice(&[0x4E, 0x70, 0x70, 37]);
        sound.set_enabled(true).unwrap();
        until(&mut sound, edge, 1);
        assert!(sound.scsp.reset_held);
        sound.set_enabled(false).unwrap();
        until(&mut sound, 168, 7);
        assert!(!sound.scsp.reset_held);
        assert_eq!(sound.cpu.state.instructions, 0);
        assert_eq!(sound.cpu.state.d[0], 0);
        sound.scsp.write16(0x400, 0x20F).unwrap();
        assert_eq!(sound.scsp.common[0], 0x20F);
    }
}

#[test]
fn traced_reset_finishes_the_output_pulse_before_any_trace_frame_bus_cycle() {
    let mut sound = fixture();
    sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
    sound.ram[0x200..0x204].copy_from_slice(&[0x4E, 0x70, 0x4E, 0x71]);
    sound.ram[36..40].copy_from_slice(&0x300u32.to_be_bytes());
    sound.ram[0x300..0x304].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
    sound.ram[0xFFA..0x1000].fill(0xAA);
    sound.set_enabled(true).unwrap();
    sound.cpu.state.sr = 0xA700;
    until(&mut sound, 167, 1);
    assert!(sound.scsp.reset_held);
    assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
    assert_eq!(sound.cpu.state.exceptions, 0);
    until(&mut sound, 171, 1);
    assert!(!sound.scsp.reset_held);
    assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
    until(&mut sound, 240, 7);
    assert_eq!(&sound.ram[0xFFA..0x1000], &[0xA7, 0, 0, 0, 2, 2]);
    assert!(sound.cpu.state.stopped);
    assert_eq!(sound.cpu.state.exceptions, 1);
    assert_eq!(sound.cpu.state.last_vector, Some(9));
    assert!(!sound.cpu.state.halted);
}

#[test]
fn irq_acknowledgement_is_a_separate_four_clock_cycle_before_stack_writes() {
    for chunk in [1, 7, 10000] {
        let mut sound = fixture();
        sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
        sound.ram[27 * 4..28 * 4].copy_from_slice(&0x300u32.to_be_bytes());
        sound.ram[0x300..0x304].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
        sound.ram[0xFFA..0x1000].fill(0xAA);
        sound.set_enabled(true).unwrap();
        sound.cpu.state.sr = 0x2000;
        until(&mut sound, 40, chunk);
        for offset in [0x41E, 0x424, 0x426, 0x420] {
            sound.scsp.write16(offset, 0x20).unwrap();
        }
        until(&mut sound, 47, chunk);
        assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
        // The accepted level remains3 even if the source is cleared in flight.
        sound.scsp.write16(0x422, 0x20).unwrap();
        until(&mut sound, 51, chunk);
        assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
        until(&mut sound, 52, chunk);
        assert_eq!(&sound.ram[0xFFE..0x1000], &[2, 2]);
        assert_eq!(&sound.ram[0xFFA..0xFFE], &[0xAA; 4]);
        until(&mut sound, 200, chunk);
        assert_eq!(&sound.ram[0xFFA..0x1000], &[0x20, 0, 0, 0, 2, 2]);
        assert_eq!(sound.cpu.state.last_vector, Some(27));
        assert_eq!(sound.cpu.state.exceptions, 1);
        assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
    }
}

#[test]
fn register_results_and_retirement_wait_for_the_instruction_internal_clocks() {
    // MC68000UM tables8-4/8-7: literal 16-bit-bus instruction times.
    // DIV deliberately retains the table's maximum functional timing policy;
    // MUL and shifts use their documented operand-dependent clock counts.
    for (opcode, input, source, clocks, expected) in [
        (0x80C1u16, 7, 2, 140, 0x00010003),       // DIVU D1,D0
        (0x81C1, 0xFFFFFFF9, 2, 158, 0xFFFFFFFD), // DIVS D1,D0
        (0xC0C1, 7, 2, 40, 14),                   // MULU, one source bit
        (0xC1C1, 0xFFFFFFF9, 2, 42, 0xFFFFFFF2),  // MULS, two transitions
        (0xD081, 7, 2, 8, 9),                     // ADD.L D1,D0
        (0xE3A8, 0x1000, 2, 12, 0x4000),          // LSL.L D1,D0
        (0xE3A8, 1, 63, 134, 0),
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut sound = fixture();
            sound.ram[4..8].copy_from_slice(&0x200u32.to_be_bytes());
            for (i, word) in [opcode, 0x4E72, 0x2700, 0x4E71].into_iter().enumerate() {
                sound.ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
            }
            sound.set_enabled(true).unwrap();
            sound.cpu.state.d[0] = input;
            sound.cpu.state.d[1] = source;
            let deadline = 40 + clocks;
            until(&mut sound, deadline - 1, chunk);
            assert_eq!(sound.cpu.state.d[0], input, "opcode={opcode:04X}");
            assert_eq!(sound.cpu.state.pc, 0x200);
            assert_eq!(sound.cpu.state.instructions, 0);
            until(&mut sound, deadline, chunk);
            assert_eq!(sound.cpu.state.d[0], expected, "opcode={opcode:04X}");
            assert_eq!(sound.cpu.state.pc, 0x202);
            assert_eq!(sound.cpu.state.instructions, 1);
            until(&mut sound, deadline + 32, chunk);
            assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
            assert_eq!(sound.cpu.state.exceptions, 0);
        }
    }
}

#[test]
fn traced_division_cannot_stack_or_fetch_its_vector_during_the_divide() {
    for (opcode, dividend, clocks, result, sr) in [
        (0x80C1u16, 7, 140, 0x10003, 0xA700u16),
        (0x81C1, 0xFFFFFFF9, 158, 0xFFFFFFFD, 0xA708),
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut sound = fixture();
            sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
            sound.ram[0x200..0x204].copy_from_slice(&[opcode.to_be_bytes(), [0x4E, 0x71]].concat());
            sound.ram[36..40].copy_from_slice(&0x300u32.to_be_bytes());
            sound.ram[0x300..0x304].copy_from_slice(&[0x70, 0x0B, 0x4E, 0x71]);
            sound.ram[0x400..0x404].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
            sound.ram[0xFFA..0x1000].fill(0xAA);
            sound.set_enabled(true).unwrap();
            sound.cpu.state.d[0] = dividend;
            sound.cpu.state.d[1] = 2;
            sound.cpu.state.sr = 0xA700;
            let deadline = 40 + clocks;
            until(&mut sound, deadline - 1, chunk);
            assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
            assert_eq!(sound.cpu.state.exceptions, 0);
            // A real shared-bus vector change while DIV is executing must be
            // seen later, rather than using a handler fetched before completion.
            sound.ram[36..40].copy_from_slice(&0x400u32.to_be_bytes());
            until(&mut sound, deadline + 3, chunk);
            assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
            assert_eq!(sound.cpu.state.d[0], result);
            assert_eq!(sound.cpu.state.pc, 0x202);
            assert_eq!(sound.cpu.state.instructions, 1);
            until(&mut sound, deadline + 4, chunk);
            assert_eq!(&sound.ram[0xFFE..0x1000], &[2, 2]);
            until(&mut sound, deadline + 34, chunk);
            assert_eq!(sound.cpu.state.d[0], result);
            assert_eq!(sound.cpu.state.last_vector, Some(9));
            assert_eq!(sound.cpu.state.instructions, 1);
            assert_eq!(&sound.ram[0xFFA..0xFFC], &sr.to_be_bytes());
            assert_eq!(&sound.ram[0xFFC..0x1000], &[0, 0, 2, 2]);
            until(&mut sound, deadline + 66, chunk);
            assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
        }
    }
}

#[test]
fn sound_off_discards_unretired_division_and_on_retains_only_completed_registers() {
    for boundary in [40, 41, 44, 100, 179] {
        for chunk in [1, 7, 10000] {
            let mut sound = fixture();
            sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
            sound.ram[0x200..0x204].copy_from_slice(&[0x80, 0xC1, 0x4E, 0x71]);
            sound.ram[0x300..0x30C]
                .copy_from_slice(&[0x23, 0xC0, 0, 0, 8, 0, 0x4E, 0x72, 0x27, 0, 0x4E, 0x71]);
            sound.set_enabled(true).unwrap();
            sound.cpu.state.d[0] = 7;
            sound.cpu.state.d[1] = 2;
            until(&mut sound, boundary, chunk);
            sound.set_enabled(false).unwrap();
            assert_eq!(sound.cpu.state.d[0], 7);
            sound.ram[4..8].copy_from_slice(&0x300u32.to_be_bytes());
            sound.advance(1000).unwrap();
            assert_eq!(sound.cpu.state.d[0], 7);
            sound.set_enabled(true).unwrap();
            until(&mut sound, 100, chunk);
            assert_eq!(&sound.ram[0x800..0x804], &7u32.to_be_bytes());
            assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
            assert_eq!(sound.cpu.state.exceptions, 0);
        }
    }
}

#[test]
fn reset_during_trace_keeps_the_already_completed_division_result() {
    for after_retirement in [0, 1, 3, 4, 7, 8, 12, 16, 20, 24, 33] {
        for chunk in [1, 7, 10000] {
            let mut sound = fixture();
            sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
            sound.ram[0x200..0x204].copy_from_slice(&[0x81, 0xC1, 0x4E, 0x71]);
            sound.ram[36..40].copy_from_slice(&0x400u32.to_be_bytes());
            sound.ram[0x400..0x404].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
            sound.ram[0x300..0x30C]
                .copy_from_slice(&[0x23, 0xC0, 0, 0, 8, 0, 0x4E, 0x72, 0x27, 0, 0x4E, 0x71]);
            sound.set_enabled(true).unwrap();
            sound.cpu.state.d[0] = 0xFFFFFFF9;
            sound.cpu.state.d[1] = 2;
            sound.cpu.state.sr = 0xA700;
            until(&mut sound, 198 + after_retirement, chunk);
            assert_eq!(sound.cpu.state.d[0], 0xFFFFFFFD);
            assert_eq!(sound.cpu.state.instructions, 1);
            sound.set_enabled(false).unwrap();
            assert_eq!(sound.cpu.state.d[0], 0xFFFFFFFD);
            assert_eq!(sound.cpu.state.sr, 0x2708);
            sound.ram[4..8].copy_from_slice(&0x300u32.to_be_bytes());
            sound.set_enabled(true).unwrap();
            until(&mut sound, 100, chunk);
            assert_eq!(&sound.ram[0x800..0x804], &0xFFFFFFFDu32.to_be_bytes());
            assert_eq!(sound.cpu.state.exceptions, 0);
            assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
        }
    }
}

#[test]
fn irq_arriving_during_division_waits_for_retirement_and_its_own_exception_clocks() {
    for chunk in [1, 3, 17, 10000] {
        let mut sound = fixture();
        sound.ram[..8].copy_from_slice(&0x00001000_00000200u64.to_be_bytes());
        sound.ram[0x200..0x204].copy_from_slice(&[0x80, 0xC1, 0x4E, 0x71]);
        sound.ram[27 * 4..28 * 4].copy_from_slice(&0x300u32.to_be_bytes());
        sound.ram[0x300..0x304].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
        sound.ram[0xFFA..0x1000].fill(0xAA);
        sound.set_enabled(true).unwrap();
        sound.cpu.state.d[0] = 7;
        sound.cpu.state.d[1] = 2;
        sound.cpu.state.sr = 0x2000;
        until(&mut sound, 100, chunk);
        for offset in [0x41E, 0x424, 0x426, 0x420] {
            sound.scsp.write16(offset, 0x20).unwrap();
        }
        until(&mut sound, 179, chunk);
        assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
        assert_eq!(sound.cpu.state.d[0], 7);
        until(&mut sound, 180, chunk);
        assert_eq!(sound.cpu.state.d[0], 0x10003);
        assert_eq!(sound.cpu.state.instructions, 1);
        until(&mut sound, 187, chunk); // IACK184, first stack write188
        assert_eq!(&sound.ram[0xFFA..0x1000], &[0xAA; 6]);
        until(&mut sound, 188, chunk);
        assert_eq!(&sound.ram[0xFFE..0x1000], &[2, 2]);
        until(&mut sound, 223, chunk);
        assert_eq!(sound.cpu.state.exceptions, 0);
        until(&mut sound, 224, chunk); // table8-14: IRQ44 clocks
        assert_eq!(sound.cpu.state.exceptions, 1);
        assert_eq!(sound.cpu.state.last_vector, Some(27));
        assert_eq!(sound.cpu.state.pc, 0x300);
        until(&mut sound, 256, chunk);
        assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
    }
}

#[test]
fn real_68k_mvol_byte_stores_and_queued_samples_are_chunk_invariant() {
    let mut baseline = None;
    for chunk in [1, 3, 17, 10000] {
        let mut sound = fixture();
        sound.scsp.write16(0x400, 0x20f).unwrap();
        // Original 68k code: complete low-byte MVOL writes while a direct
        // constant voice runs, with long DBRA dwells across sample deadlines.
        let mut words = Vec::new();
        for gain in [13, 0, 1, 15] {
            words.extend([0x13fcu16, gain, 0x10, 0x401, 0x303c, 127, 0x51c8, 0xfffe]);
        }
        words.extend([0x4e72, 0x2700]);
        for (i, word) in words.into_iter().enumerate() {
            sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        sound.ram[0x1000..0x1008].copy_from_slice(&[0x40, 0, 0x40, 0, 0x40, 0, 0x40, 0]);
        for (offset, value) in [(2, 0x1000), (6, 4), (12, 0x100), (22, 0xe000), (0, 0x1820)] {
            sound.scsp.write16(offset, value).unwrap();
        }
        sound.set_enabled(true).unwrap();
        until(&mut sound, 8000, chunk);
        assert!(sound.cpu.state.stopped);
        assert_eq!(sound.scsp.common[0], 0x20f);
        assert_eq!(sound.scsp.read16(0x400), Ok(0));
        let samples = sound.scsp.drain(8192);
        let left: Vec<_> = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                assert_eq!(p[0], p[1]);
                p[0]
            })
            .collect();
        let runs: Vec<_> = left
            .iter()
            .enumerate()
            .filter_map(|(i, &v)| (i == 0 || left[i - 1] != v).then_some(v))
            .collect();
        // Initial setup completes before the first output; none of these
        // queued samples is retroactively changed by the later unity store.
        assert_eq!(runs, [8192, 0, 128, 16384]);
        let trace = (samples, sound.scsp.generated, sound.cpu.state.instructions);
        if let Some(ref previous) = baseline {
            assert_eq!(&trace, previous);
        } else {
            baseline = Some(trace);
        }
    }
}
