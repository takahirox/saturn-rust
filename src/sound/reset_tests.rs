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
        assert_eq!(sound.cpu.state.d[0], 0x25);
        assert_eq!(sound.cpu.state.pc, 0x202);
        until(&mut sound, 44, chunk);
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
        until(&mut sound, 40, 7);
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
        until(&mut sound, 40, 17);
        assert!(!sound.cpu.state.halted);
        assert_eq!(sound.cpu.state.d[0], 0x25);
    }
}

#[test]
fn reset_prefetch_uses_24_bit_bus_pins_without_truncating_architectural_pc() {
    let mut sound = fixture();
    sound.ram[4..8].copy_from_slice(&0x01000200u32.to_be_bytes());
    sound.set_enabled(true).unwrap();
    until(&mut sound, 40, 1);
    assert_eq!(sound.cpu.state.pc, 0x01000202);
    assert_eq!(sound.cpu.state.d[0], 0x25);
    assert_eq!(sound.cpu.state.instructions, 1);
    assert!(!sound.cpu.state.halted);
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
        until(&mut sound, 40, chunk);
        sound.cpu.state.sr = 0x2000;
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
