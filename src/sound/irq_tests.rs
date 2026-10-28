use super::*;

fn boot() -> Sound {
    let mut sound = Sound::default();
    for line in include_str!("../../tests/fixtures/m68k-irq.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        if !(0x2000..0x3000).contains(&at) {
            continue;
        }
        for (i, word) in words.enumerate() {
            sound.ram[at - 0x2000 + i * 2..at - 0x2000 + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    for offset in [0x41e, 0x424, 0x426, 0x428] {
        sound.scsp.write16(offset, 32).unwrap();
    }
    sound.set_enabled(true).unwrap();
    sound.advance(1000).unwrap();
    assert!(sound.cpu.state.stopped);
    sound
}
fn advance(sound: &mut Sound, target: u64, chunk: u32, inspect: bool) {
    while sound.time < target {
        sound
            .advance((target - sound.time).min(u64::from(chunk)) as u32)
            .unwrap();
        if inspect {
            let _ = sound.cpu.state.clone();
            assert!(sound.scsp.read16(0x420).is_ok());
            let _ = sound.memory_trace();
        }
    }
}
fn division(sound: &mut Sound) {
    sound.scsp.write16(0x420, 32).unwrap();
    for _ in 0..2000 {
        sound.advance(1).unwrap();
        if matches!(
            sound.transactions.waiting,
            Some(Operation::FinishClocks(140))
        ) {
            assert_eq!(sound.cpu.state.pc, 0x320);
            assert_eq!(sound.cpu.state.exceptions, 1);
            assert_eq!(sound.cpu.state.d[0], 7);
            return;
        }
    }
    panic!("first handler did not start DIVU");
}

#[test]
fn rearm_pending_enable_and_level_during_division_survives_replay_once() {
    for route in 0..3 {
        let mut baseline = None;
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut sound = boot();
                division(&mut sound);
                let instructions = sound.cpu.state.instructions;
                let start = sound.transactions.start;
                let finish = sound.cpu_origin
                    + ((start + sound.transactions.stalls + 140) * REFERENCE_HZ)
                        .div_ceil(SOUND_CPU_HZ);
                let (low, high) = match route {
                    0 => ((0x422, 32), (0x420, 32)),
                    1 => ((0x41e, 0), (0x41e, 32)),
                    _ => ((0x428, 0), (0x428, 32)),
                };
                sound.scsp.write16(low.0, low.1).unwrap();
                let low_end = sound.time + 80; // >32 CPU clocks; no electrical short pulse.
                advance(&mut sound, low_end, chunk, inspect);
                assert_ne!(sound.scsp.interrupt_level(), 7);
                sound.scsp.write16(high.0, high.1).unwrap();
                advance(&mut sound, finish - 1, chunk, inspect);
                assert_eq!(
                    (sound.cpu.state.pc, sound.cpu.state.instructions),
                    (0x320, instructions)
                );
                assert_eq!(sound.cpu.state.exceptions, 1);
                assert_eq!(sound.cpu.state.d[0], 7);
                // No nested stack data before the instruction retires.
                assert_eq!(&sound.ram[0xff4..0xffa], &[0; 6]);
                advance(&mut sound, finish, chunk, inspect);
                assert_eq!(sound.cpu.state.d[0], 0x10003);
                assert_eq!(sound.cpu.state.pc, 0x322);
                assert_eq!(
                    sound.transactions.waiting,
                    Some(Operation::InterruptAcknowledge(7))
                );
                let iack_end = sound.next_cpu;
                advance(&mut sound, iack_end - 1, chunk, inspect);
                assert_eq!(&sound.ram[0xff4..0xffa], &[0; 6]);
                advance(&mut sound, iack_end, chunk, inspect);
                assert_eq!(
                    sound.transactions.completed[0],
                    (Operation::InterruptAcknowledge(7), Ok(31))
                );
                assert_eq!(
                    sound.transactions.waiting,
                    Some(Operation::Write(0xff8, 2, 0x322))
                );
                assert_eq!(&sound.ram[0xff4..0xffa], &[0; 6]);
                let stack_end = sound.next_cpu;
                advance(&mut sound, stack_end, chunk, inspect);
                assert_eq!(&sound.ram[0xff8..0xffa], &[3, 0x22]);
                advance(&mut sound, finish + 2000, chunk, inspect);
                assert_eq!(&sound.ram[0xff4..0xffa], &[0x27, 0, 0, 0, 3, 0x22]);
                assert_eq!(sound.cpu.state.exceptions, 2);
                assert_eq!(sound.cpu.state.last_vector, Some(31));
                assert_eq!(sound.cpu.state.d[2], 2);
                assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
                let record = (
                    sound.ram[0x900..0x91c].to_vec(),
                    sound.memory_trace().to_vec(),
                    sound.cpu.state.clone(),
                );
                if let Some(previous) = &baseline {
                    assert_eq!(&record, previous);
                }
                baseline = Some(record);
            }
        }
    }
}

#[test]
fn held_seven_requires_a_new_edge_or_a_software_lowered_mask() {
    for lower_mask in [false, true] {
        let mut sound = boot();
        // First handler executes DIVU then MOVE.W #SR,SR before STOP.
        // A continuously asserted 7 must qualify again only with the lower I.
        for (i, word) in [
            0x46fcu16,
            if lower_mask { 0x2600 } else { 0x2700 },
            0x4e72,
            0x2700,
        ]
        .into_iter()
        .enumerate()
        {
            sound.ram[0x322 + i * 2..0x324 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        division(&mut sound);
        sound.advance(3000).unwrap();
        assert_eq!(sound.cpu.state.exceptions, if lower_mask { 2 } else { 1 });
        assert_eq!(sound.cpu.state.d[0], 0x10003);
        if lower_mask {
            assert_eq!(&sound.ram[0xff4..0xffa], &[0x26, 0, 0, 0, 3, 0x26]);
        }
    }
}

#[test]
fn edge_during_pending_stack_work_is_retained_without_changing_replay() {
    for phase in 0..4 {
        let mut baseline = None;
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                let mut sound = boot();
                sound.scsp.write16(0x420, 32).unwrap();
                for _ in 0..2000 {
                    sound.advance(1).unwrap();
                    if sound
                        .transactions
                        .completed
                        .iter()
                        .any(|e| e.0 == Operation::InterruptAcknowledge(7))
                        && sound.transactions.completed.len() == phase + 1
                    {
                        break;
                    }
                }
                assert_eq!(
                    sound.transactions.completed[0].0,
                    Operation::InterruptAcknowledge(7)
                );
                // Real main-CPU DRAM contention extends a pending stack/vector cycle.
                sound.memory.submit(
                    memory::Owner::Main,
                    memory::Request {
                        address: 0x800,
                        width: 4,
                        write: Some(0x12345678),
                    },
                );
                sound.scsp.write16(0x422, 32).unwrap();
                let low_end = sound.time + 40;
                advance(&mut sound, low_end, chunk, inspect);
                assert!(
                    sound.transactions.stalls > 0,
                    "pending RAM must actually stall phase={phase}"
                );
                sound.scsp.write16(0x420, 32).unwrap();
                let end = sound.time + 4000;
                advance(&mut sound, end, chunk, inspect);
                assert_eq!(sound.cpu.state.exceptions, 2, "phase={phase}");
                assert_eq!(
                    sound.cpu.state.d[2], 1,
                    "phase={phase}: {:?}",
                    sound.cpu.state
                );
                assert_eq!(&sound.ram[0x800..0x804], &0x12345678u32.to_be_bytes());
                assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
                let record = (
                    sound.cpu.state.clone(),
                    sound.ram[0x900..0x91c].to_vec(),
                    sound.memory_trace().to_vec(),
                );
                if let Some(previous) = &baseline {
                    assert_eq!(&record, previous);
                }
                baseline = Some(record);
            }
        }
    }
}

#[test]
fn sound_off_cancels_latched_edge_and_frozen_execution() {
    for cold in [false, true] {
        let mut sound = boot();
        division(&mut sound);
        sound.scsp.write16(0x422, 32).unwrap();
        sound.advance(80).unwrap();
        sound.scsp.write16(0x420, 32).unwrap();
        sound.advance(1).unwrap();
        if cold {
            sound = Sound::default();
        } else {
            sound.set_enabled(false).unwrap();
        }
        // Discard the external request too. The abandoned latch must not fire
        // when the new CPU starts with no interrupt source.
        sound.scsp.write16(0x422, 32).unwrap();
        sound.ram[..8].copy_from_slice(&0x00001000_00000400u64.to_be_bytes());
        sound.ram[0x400..0x406].copy_from_slice(&[0x4e, 0x72, 0x27, 0, 0x4e, 0x71]);
        sound.set_enabled(true).unwrap();
        sound.advance(2000).unwrap();
        assert_eq!(sound.cpu.state.exceptions, 0);
        assert_eq!(sound.cpu.state.pc, 0x404);
        assert!(sound.cpu.state.stopped && !sound.cpu.state.halted);
    }
}

#[test]
fn scheduled_timer_output_rearms_seven_during_internal_computation() {
    let mut tested = false;
    for delay in [0, 100, 200, 300, 400, 500] {
        let mut sound = boot();
        sound.advance(delay).unwrap();
        division(&mut sound);
        let finish = sound.cpu_origin
            + ((sound.transactions.start + sound.transactions.stalls + 140) * REFERENCE_HZ)
                .div_ceil(SOUND_CPU_HZ);
        let sample = ((sound.scsp.generated + 1) * REFERENCE_HZ).div_ceil(SAMPLE_RATE);
        if sample < sound.time + 50 || sample >= finish {
            continue;
        }
        sound.scsp.write16(0x422, 32).unwrap();
        for offset in [0x41e, 0x424, 0x426, 0x428] {
            sound.scsp.write16(offset, 64).unwrap();
        }
        sound.scsp.write16(0x418, 254).unwrap();
        advance(&mut sound, sample - 1, 3, true);
        assert_eq!(sound.scsp.interrupt_level(), 0);
        advance(&mut sound, sample, 1, false);
        assert_eq!(sound.scsp.interrupt_level(), 7);
        assert_eq!(sound.cpu.state.pc, 0x320);
        assert_eq!(sound.cpu.state.exceptions, 1);
        sound.advance(2000).unwrap();
        assert_eq!(sound.cpu.state.exceptions, 2);
        assert_eq!(sound.cpu.state.d[0], 0x10003);
        tested = true;
    }
    assert!(tested, "timer must assert inside a pending DIVU");
}

#[test]
fn sound_acknowledgement_does_not_clear_main_pending_and_main_clear_does_not_rearm_sound() {
    let mut sound = boot();
    sound.scsp.write16(0x42a, 32).unwrap();
    sound.scsp.write16(0x42c, 32).unwrap();
    division(&mut sound);
    assert!(sound.scsp.main_interrupt());
    sound.scsp.write16(0x422, 32).unwrap();
    sound.advance(80).unwrap();
    assert!(sound.scsp.main_interrupt());
    sound.scsp.write16(0x420, 32).unwrap();
    sound.advance(1).unwrap();
    sound.scsp.write16(0x42e, 32).unwrap();
    assert!(!sound.scsp.main_interrupt());
    assert_eq!(sound.scsp.interrupt_level(), 7);
    sound.advance(3000).unwrap();
    assert_eq!(sound.cpu.state.exceptions, 2);
    assert_eq!(sound.cpu.state.d[2], 2);
    assert_eq!(sound.scsp.read16(0x420).unwrap() & 32, 32);
}

#[test]
fn rearm_after_completed_midi_read_does_not_replay_mmio_or_publish_partial_instruction() {
    let mut sound = boot();
    // MOVE.L $100404,$800: first source word pops MIDI; the second word
    // and two destination words must remain separately scheduled.
    for (i, word) in [0x23f9u16, 0x10, 0x404, 0, 0x800, 0x4e72, 0x2700]
        .into_iter()
        .enumerate()
    {
        sound.ram[0x320 + i * 2..0x322 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    for byte in [0x11, 0x22, 0x33] {
        assert!(sound.scsp.receive_midi(byte));
    }
    sound.scsp.write16(0x420, 32).unwrap();
    for _ in 0..2000 {
        sound.advance(1).unwrap();
        if sound.transactions.waiting == Some(Operation::Write(0x940, 2, 0)) {
            break;
        }
    }
    assert_eq!(
        sound.transactions.waiting,
        Some(Operation::Write(0x940, 2, 0))
    );
    sound.scsp.write16(0x422, 32).unwrap();
    let low_start = sound.time;
    for _ in 0..2000 {
        sound.advance(1).unwrap();
        if sound
            .transactions
            .completed
            .iter()
            .any(|e| e.0 == Operation::Read(0x100404, 2))
        {
            break;
        }
    }
    assert_eq!(
        sound.transactions.waiting,
        Some(Operation::Read(0x100406, 2))
    );
    assert!(sound.time - low_start >= 64);
    assert_eq!(sound.scsp.midi_in.borrow().len(), 2);
    assert_eq!(sound.cpu.state.pc, 0x320);
    assert_eq!(&sound.ram[0x800..0x804], &[0; 4]);
    sound.scsp.write16(0x420, 32).unwrap();
    sound.advance(2000).unwrap();
    assert_eq!(
        sound
            .scsp
            .midi_in
            .borrow()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [0x22, 0x33]
    );
    // The status high byte is the ordinary MIDI-empty output flag (08).
    assert_eq!(&sound.ram[0x800..0x804], &[8, 0x11, 0, 0]);
    assert_eq!(sound.cpu.state.exceptions, 2);
    assert_eq!(sound.cpu.state.d[2], 2);
    assert_eq!(&sound.ram[0xff4..0xffa], &[0x27, 0, 0, 0, 3, 0x2a]);
}

#[test]
fn serial_completion_precedes_cpu_issue_at_equal_emulator_deadline() {
    // Emulator ordering probe, not a physical IPL synchronization claim.
    let mut tested = false;
    for delay in 0..40 {
        let mut sound = boot();
        sound.advance(delay).unwrap();
        for offset in [0x424, 0x426, 0x428] {
            sound.scsp.write16(offset, 0x80).unwrap();
        }
        sound.scsp.write16(0x41e, 0x200).unwrap();
        sound.scsp.write16(0x406, 0x3c).unwrap();
        let serial = sound.time + u64::from(sound.scsp.midi_clocks_to_event());
        advance(&mut sound, serial - 1, 17, true);
        if sound.next_cpu != serial {
            continue;
        }
        assert_eq!(sound.scsp.interrupt_level(), 0);
        sound.advance(1).unwrap();
        assert_eq!(sound.scsp.interrupt_level(), 7);
        assert_eq!(
            sound.transactions.waiting,
            Some(Operation::InterruptAcknowledge(7))
        );
        sound.advance(1000).unwrap();
        assert_eq!(sound.cpu.state.exceptions, 1);
        assert_eq!(sound.scsp.take_midi_output(), Some(0x3c));
        tested = true;
    }
    assert!(
        tested,
        "serial completion and CPU issue must share a tested deadline"
    );
}
