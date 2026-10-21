//! Independent ST-077 §4.2/table 4.14, 4.27/28 and §4.2 timers/IRQ vectors.
use super::*;
fn direct(scsp: &mut Scsp, slot: u32, address: u32, loop_mode: u16, pcm8: bool, pan: u16) {
    let base = slot * 32;
    for (offset, value) in [
        (2, address as u16),
        (4, 0),
        (6, 4),
        (12, 0x100),
        (16, 0),
        (22, 0xE000 | (pan << 8)),
    ] {
        scsp.write16(base + offset, value).unwrap();
    }
    scsp.write16(
        base,
        0x1800 | loop_mode << 5 | if pcm8 { 16 } else { 0 } | (address >> 16) as u16,
    )
    .unwrap();
    scsp.write16(0x400, 0x20F).unwrap();
}
#[test]
fn guest_ram_pcm16_loop_pan_key_off_and_all_thirty_two_slots() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x100..0x108].copy_from_slice(&[0x10, 0, 0xF0, 0, 0x20, 0, 0xE0, 0]);
    direct(&mut scsp, 0, 0x100, 1, false, 15);
    for _ in 0..6 {
        scsp.sample(&mut ram).unwrap();
    }
    assert_eq!(
        scsp.drain(100),
        [0, 4096, 0, -4096, 0, 8192, 0, -8192, 0, 4096, 0, -4096]
    );
    scsp.write16(0, 0x1000).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [0, 0]);
    let mut scsp = Scsp::default();
    ram[0x100] = 1;
    for slot in 0..32 {
        direct(&mut scsp, slot, 0x100, 1, true, 16);
    }
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [8192, 8192]);
    assert_eq!(scsp.stack.iter().filter(|v| **v == 256).count(), 32);
}
#[test]
fn pcm8_signed_source_bit_controls_pitch_and_no_loop_stop() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x100..0x104].copy_from_slice(&[0x80, 0x40, 0xC0, 0x20]);
    direct(&mut scsp, 0, 0x100, 0, true, 0);
    scsp.write16(16, 0x800).unwrap(); // octave +1
    for _ in 0..3 {
        scsp.sample(&mut ram).unwrap();
    }
    assert_eq!(scsp.drain(3), [-32768, -32768, -16384, -16384, 0, 0]);
    let mut scsp = Scsp::default();
    direct(&mut scsp, 0, 0x100, 1, true, 0);
    scsp.write16(0, 0x1800 | 0x10 | 0x20 | 0x600).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [32767, 32767]);
}

#[test]
fn fractional_pcm_interpolation_uses_signed_samples_and_forward_loop_endpoint() {
    // ST-077 §2.2/§3.1: 44.1 kHz first-order interpolation; waveform [0,4096,
    // -4096,8192], half-sample pitch. Expected values are authored arithmetic.
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x100..0x108].copy_from_slice(&[0, 0, 0x10, 0, 0xF0, 0, 0x20, 0]);
    direct(&mut scsp, 0, 0x100, 1, false, 0);
    scsp.write16(16, 0x7800).unwrap(); // OCT=-1: phase increment = 1/2
    scsp.write16(4, 1).unwrap(); // loop starts at sample 1
    for _ in 0..10 {
        scsp.sample(&mut ram).unwrap();
    }
    let actual: Vec<_> = scsp
        .drain(10)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| p[0])
        .collect();
    assert_eq!(actual, [0, 2048, 4096, 0, -4096, 2048, 8192, 6144, 4096, 0]);
    assert_eq!(scsp.stack[0], 4096); // stack also receives interpolated data
}

#[test]
fn fractional_pcm8_no_loop_and_forward_loop_have_independent_end_vectors() {
    for (mode, expected) in [
        (0, vec![10, 15, 20, 25, 30, 35, 40, 40, 0]),
        (1, vec![10, 15, 20, 25, 30, 35, 40, 30, 20]),
    ] {
        let mut scsp = Scsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        ram[0x100..0x104].copy_from_slice(&[10, 20, 30, 40]);
        direct(&mut scsp, 0, 0x100, mode, true, 0);
        scsp.write16(16, 0x7800).unwrap();
        scsp.write16(4, 1).unwrap();
        for _ in 0..9 {
            scsp.sample(&mut ram).unwrap();
        }
        let actual: Vec<_> = scsp
            .drain(9)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| p[0] / 256)
            .collect();
        assert_eq!(actual, expected, "mode {mode}");
    }
}
#[test]
fn byte_lanes_do_not_read_write_only_registers_or_pop_midi() {
    let mut scsp = Scsp::default();
    scsp.write16(0x400, 0x200).unwrap();
    scsp.write8(0x401, 15).unwrap();
    assert_eq!(scsp.common[0], 0x20F);
    assert!(scsp.receive_midi(0x91));
    assert!(scsp.receive_midi(0x41));
    assert_eq!(scsp.read16(0x420).unwrap() & 8, 8);
    assert_eq!(scsp.read16(0x404).unwrap() & 255, 0x91);
    assert_eq!(scsp.read16(0x420).unwrap() & 8, 8);
    assert_eq!(scsp.read16(0x404).unwrap() & 255, 0x41);
    assert_eq!(scsp.read16(0x420).unwrap() & 8, 0);
    for x in 0..4 {
        assert!(scsp.receive_midi(x));
    }
    assert!(!scsp.receive_midi(9));
    for x in 0..4 {
        scsp.write16(0x406, x).unwrap();
    }
    assert!(scsp.write16(0x406, 9).is_err());
    for x in 0..4 {
        assert_eq!(scsp.take_midi_output(), Some(x));
    }
    assert_eq!(scsp.read16(0x420).unwrap() & 0x200, 0x200);
}
#[test]
fn timer_prescale_pending_enable_level_and_independent_sound_main_acknowledge() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    scsp.write16(0x418, 0x01FE).unwrap();
    scsp.write16(0x41E, 0x40).unwrap();
    scsp.write16(0x42A, 0x40).unwrap();
    scsp.write16(0x426, 0x40).unwrap();
    scsp.write16(0x428, 0x40).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.interrupt_level(), 0);
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.interrupt_level(), 6);
    assert!(scsp.main_interrupt());
    scsp.write16(0x422, 0x40).unwrap();
    assert_eq!(scsp.interrupt_level(), 0);
    assert!(scsp.main_interrupt());
    scsp.write16(0x42E, 0x40).unwrap();
    assert!(!scsp.main_interrupt());
    for _ in 0..5 {
        scsp.sample(&mut ram).unwrap();
    }
    assert_eq!(scsp.interrupt_level(), 0);
    scsp.write16(0x420, 0x20).unwrap();
    scsp.write16(0x41E, 0x20).unwrap();
    scsp.write16(0x424, 0x20).unwrap();
    assert_eq!(scsp.interrupt_level(), 1);
}
#[test]
fn scsp_dma_moves_register_words_both_directions_clears_gate_and_requests_irq() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x200..0x204].copy_from_slice(&[0, 1, 0, 2]);
    scsp.write16(0x412, 0x200).unwrap();
    scsp.write16(0x414, 2).unwrap();
    scsp.write16(0x416, 0x1004).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.read16(2).unwrap(), 1);
    assert_ne!(scsp.read16(0x416).unwrap() & 0x1000, 0);
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.read16(4).unwrap(), 2);
    assert_eq!(scsp.read16(0x416).unwrap() & 0x1000, 0);
    assert_ne!(scsp.read16(0x420).unwrap() & 16, 0);
    scsp.write16(0x412, 0x204).unwrap();
    scsp.write16(0x414, 2).unwrap();
    scsp.write16(0x416, 0x3004).unwrap();
    scsp.sample(&mut ram).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(&ram[0x204..0x208], &[0, 1, 0, 2]);
    scsp.write16(0x412, 0x204).unwrap();
    scsp.write16(0x414, 2).unwrap();
    scsp.write16(0x416, 0x7004).unwrap();
    scsp.sample(&mut ram).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(&ram[0x204..0x208], &[0; 4]);
}
#[test]
fn sample_delivery_is_bounded_and_unsupported_synthesis_never_succeeds_silently() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    for _ in 0..BUFFER_FRAMES + 5 {
        scsp.sample(&mut ram).unwrap();
    }
    assert_eq!(scsp.generated, (BUFFER_FRAMES + 5) as u64);
    assert_eq!(scsp.dropped, 5);
    assert_eq!(scsp.drain(usize::MAX).len(), BUFFER_FRAMES * 2);
    assert!(scsp.drain(1).is_empty());
    for (address, value) in [
        (8, 1),
        (10, 1),
        (14, 1),
        (18, 1),
        (20, 1),
        (22, 1),
        (0x700, 1),
    ] {
        assert!(scsp.write16(address, value).is_err());
    }
}
#[test]
fn sound_cpu_and_sample_clock_traces_do_not_depend_on_host_advance_chunks() {
    fn trace(chunk: u32) -> (crate::m68k::State, Vec<i16>, u64) {
        let mut sound = Sound::default();
        sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
        // ADDQ.L #1,D0; BRA.S -4. Executed on the independent sound clock.
        sound.ram[0x100..0x104].copy_from_slice(&[0x52, 0x80, 0x60, 0xFC]);
        direct(&mut sound.scsp, 0, 0x200, 1, true, 0);
        sound.ram[0x200..0x204].copy_from_slice(&[1, 2, 3, 4]);
        sound.set_enabled(true).unwrap();
        let mut remaining = 50000;
        while remaining != 0 {
            let n = chunk.min(remaining);
            sound.advance(n).unwrap();
            remaining -= n;
        }
        (
            sound.cpu.state.clone(),
            sound.scsp.drain(1000),
            sound.next_cpu,
        )
    }
    let expected = trace(1);
    assert!(expected.0.instructions > 100);
    assert!(expected.1.iter().any(|v| *v != 0));
    for chunk in [3, 8, 17, 611, 10000] {
        assert_eq!(trace(chunk), expected, "chunk={chunk}");
    }
}
#[test]
fn sound_off_on_reloads_vectors_without_discarding_shared_ram_or_running_scsp() {
    let mut sound = Sound::default();
    sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
    sound.ram[0x100..0x104].copy_from_slice(&[0x52, 0x80, 0x60, 0xFC]);
    sound.set_enabled(true).unwrap();
    sound.advance(10000).unwrap();
    assert!(sound.cpu.state.d[0] > 0);
    sound.set_enabled(false).unwrap();
    let n = sound.scsp.generated;
    sound.advance(10000).unwrap();
    assert_eq!(sound.cpu.state.instructions, 0);
    assert!(sound.scsp.generated > n);
    assert_eq!(sound.ram[0x100], 0x52);
    sound.set_enabled(true).unwrap();
    assert_eq!(sound.cpu.state.pc, 0x100);
    assert_eq!(sound.cpu.state.d[0], 0);
}

#[test]
fn reverse_and_alternative_loops_follow_the_manual_attack_and_loop_sequences() {
    for (mode, expected) in [
        (1, vec![10, 20, 30, 40, 20, 30, 40, 20, 30]),
        (2, vec![10, 40, 30, 20, 40, 30, 20, 40, 30]),
        (3, vec![10, 20, 30, 40, 40, 30, 20, 20, 30]),
    ] {
        let mut scsp = Scsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        ram[0x100..0x104].copy_from_slice(&[10, 20, 30, 40]);
        direct(&mut scsp, 0, 0x100, mode, true, 0);
        scsp.write16(4, 1).unwrap();
        for _ in 0..9 {
            scsp.sample(&mut ram).unwrap();
        }
        let actual: Vec<_> = scsp
            .drain(9)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| s[0] / 256)
            .collect();
        assert_eq!(actual, expected, "loop mode {mode}");
    }
}
#[test]
fn midi_status_byte_does_not_consume_data_and_overflow_is_observable_until_reset() {
    let scsp = Scsp::default();
    for v in 0..4 {
        assert!(scsp.receive_midi(v));
    }
    assert!(!scsp.receive_midi(99));
    assert_eq!(scsp.read8(0x404).unwrap() & 6, 6);
    assert_eq!(scsp.read8(0x404).unwrap() & 6, 6);
    assert_eq!(scsp.read8(0x405).unwrap(), 0);
    assert_eq!(scsp.read8(0x405).unwrap(), 1);
    assert_eq!(scsp.read8(0x404).unwrap() & 6, 4);
}
#[test]
fn reset_instruction_pulses_external_scsp_without_erasing_sound_ram_or_cpu_state() {
    let mut sound = Sound::default();
    sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
    sound.ram[0x100..0x108].copy_from_slice(&[0x70, 37, 0x4E, 0x70, 0x4E, 0x72, 0x27, 0]);
    sound.scsp.write16(0x400, 0x20F).unwrap();
    sound.set_enabled(true).unwrap();
    sound.advance(10000).unwrap();
    assert_eq!(sound.cpu.state.d[0], 37);
    assert!(sound.cpu.state.stopped);
    assert_eq!(sound.ram[0x101], 37);
    assert_eq!(sound.scsp.common[0], 0);
    assert!(sound.scsp.generated > 10);
}
