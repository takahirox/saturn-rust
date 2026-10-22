//! ST-077 §2.2 table2.2: 128 DSP instructions/Fs; §3.2 shared RAM;
//! §5 fig5.1 and ST-228 Reference2: parallel datapath and delayed RAM input.
//! Deadlines and data below are authored from these programming rules.
use crate::sound::*;

fn until(s: &mut Sound, tick: u64, chunk: u64) {
    let target = (tick * REFERENCE_HZ).div_ceil(DSP_HZ);
    while s.time < target {
        s.advance((target - s.time).min(chunk) as u32).unwrap();
    }
}
fn program(s: &mut Sound, step: u32, words: [u16; 4]) {
    for (i, word) in words.into_iter().enumerate() {
        s.scsp
            .write16(0x800 + step * 8 + i as u32 * 2, word)
            .unwrap();
    }
}
fn setup() -> Sound {
    let mut s = Sound::default();
    s.scsp.write16(0x400, 0x20F).unwrap();
    s.scsp.write16(0x700, 0x4000).unwrap(); // signed Q12 coefficient +.5
    s.scsp.write16(0x780, 0x800).unwrap(); // byte address1000
    for (step, words) in [
        (1, [0, 0, 0xA002, 0x8000]), // TABLE/MRD/NOFL
        (3, [0, 0x20, 2, 0]),        // MEMS0 = retained RAM input
        (5, [0, 0xA000, 2, 0]),      // ACC = MEMS0 * COEF0
        (6, [0, 0, 0x1002, 0]),      // EFREG0 = previous ACC >> 8
        (65, [0, 0, 0xA002, 0x8000]),
        (67, [0, 0x21, 2, 0]),
        (68, [0, 0xA040, 2, 0]),
        (69, [0, 0, 0x1102, 0]),
    ] {
        program(&mut s, step, words);
    }
    s.scsp.write16(0x16, 0xE0).unwrap(); // effect0, full gain, center pan
    s.scsp.write16(0x36, 0xE0).unwrap();
    s.ram[0x1000..0x1002].copy_from_slice(&0x1234u16.to_be_bytes());
    s
}

#[test]
fn every_effect_store_replaces_at_its_own_deadline_including_zero_and_negative_data() {
    // ST-228 p.31 says that only the last STR to an EFREG is valid.
    // ST-077 fig.5.1 routes the prior accumulator through the shifter to
    // EFREG; the same instruction computes the next accumulator in parallel.
    // These expectations do not call the production DSP arithmetic helpers.
    for channel in 0..16 {
        for chunk in [1, 3, 17, 10000] {
            let mut s = Sound::default();
            s.scsp.write16(0x400, 0x20F).unwrap();
            s.scsp.write16(0x700, 0x4000).unwrap(); // .5
            s.scsp.write16(0xE02, 0x6000).unwrap(); // MEMS0=+24576 PCM
            s.scsp.write16(0xE06, 0xA000).unwrap(); // MEMS1=-24576 PCM
            let store = 0x1002 | channel << 8;
            program(&mut s, 0, [0, 0xA000, 2, 0]);
            program(&mut s, 1, [0, 0xA040, store, 0]);
            program(&mut s, 2, [0, 0, store, 0]);
            program(&mut s, 3, [0, 0, store, 0]);
            let port = 0xEC0 + u32::from(channel) * 2;
            until(&mut s, 2, chunk);
            assert_eq!(s.scsp.read16(port), Ok(12288));
            // A CPU overwrite is also replaced by the subsequent DSP store.
            s.scsp.write16(port, 0x1357).unwrap();
            let next = (3 * REFERENCE_HZ).div_ceil(DSP_HZ);
            s.advance((next - s.time - 1) as u32).unwrap();
            assert_eq!(s.scsp.read16(port), Ok(0x1357));
            until(&mut s, 3, chunk);
            assert_eq!(s.scsp.read16(port), Ok((-12288i16) as u16));
            until(&mut s, 4, chunk);
            assert_eq!(s.scsp.read16(port), Ok(0));
            for other in 0..16 {
                if other != channel {
                    assert_eq!(s.scsp.read16(0xEC0 + u32::from(other) * 2), Ok(0));
                }
            }
        }
    }
}
#[test]
fn dsp_reads_and_register_results_use_each_instruction_deadline_not_a_sample_snapshot() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup();
        until(&mut s, 1, chunk); // instruction0
        let before_read = (2 * REFERENCE_HZ).div_ceil(DSP_HZ);
        s.advance((before_read - s.time - 1) as u32).unwrap();
        s.ram[0x1000..0x1002].copy_from_slice(&0x2345u16.to_be_bytes());
        until(&mut s, 2, chunk); // instruction1 MRD retains2345
        s.ram[0x1000..0x1002].copy_from_slice(&0x3456u16.to_be_bytes());
        until(&mut s, 3, chunk);
        assert_eq!(s.scsp.read16(0xE02), Ok(0)); // IWT not issued yet
        until(&mut s, 4, chunk);
        assert_eq!(s.scsp.read16(0xE00), Ok(0));
        assert_eq!(s.scsp.read16(0xE02), Ok(0x2345));
        // A coefficient changed before multiplication participates; a later
        // change cannot retroactively change its accumulator/shifter result.
        s.scsp.write16(0x700, 0x2000).unwrap();
        until(&mut s, 6, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0));
        s.scsp.write16(0x700, 0x4000).unwrap();
        until(&mut s, 7, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0x8D1)); // 2345 /4
        assert_eq!(s.scsp.generated, 0);
        until(&mut s, 70, chunk);
        assert_eq!(s.scsp.read16(0xE06), Ok(0x3456));
        assert_eq!(s.scsp.read16(0xEC2), Ok(0x1A2B)); // late MRD, half gain
        until(&mut s, 127, chunk);
        assert!(s.scsp.drain(1).is_empty());
        until(&mut s, 128, chunk);
        assert_eq!(s.scsp.generated, 1);
        assert_eq!(s.scsp.drain(1), [8956, 8956]); // 08D1 +1A2B
        until(&mut s, 129, chunk); // next frame clears EFREG, not MEMS
        assert_eq!(s.scsp.read16(0xEC0), Ok(0));
        assert_eq!(s.scsp.read16(0xEC2), Ok(0));
        assert_eq!(s.scsp.read16(0xE02), Ok(0x2345));
    }
}
#[test]
fn program_and_address_writes_affect_only_unexecuted_instructions() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup();
        until(&mut s, 7, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0x91A));
        program(&mut s, 5, [0; 4]); // too late to change EFREG0
        s.scsp.write16(0x780, 0x801).unwrap();
        s.ram[0x1002..0x1004].copy_from_slice(&0x4567u16.to_be_bytes());
        until(&mut s, 66, chunk);
        s.scsp.write16(0x780, 0x800).unwrap(); // cannot redirect completed read
        program(&mut s, 68, [0, 0xA040, 2, 0x200]); // late COEF1 = -.5
        s.scsp.write16(0x702, 0xC000).unwrap();
        until(&mut s, 70, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0x91A));
        assert_eq!(s.scsp.read16(0xEC2), Ok(0xDD4C)); // floor(-4567/2)
        assert_eq!(s.scsp.read16(0xE06), Ok(0x4567));
    }
}
#[test]
fn reset_discards_partial_effects_and_audio_but_sound_off_leaves_dsp_running() {
    for chunk in [1, 3, 17, 10000] {
        for tick in [1, 2, 3, 4, 6, 7, 65, 66, 67, 69, 70, 127] {
            let mut s = setup();
            until(&mut s, tick, chunk);
            s.scsp.set_reset(true);
            until(&mut s, 128, chunk);
            assert_eq!(s.scsp.read16(0xE02), Ok(0));
            assert_eq!(s.scsp.read16(0xEC0), Ok(0));
            assert_eq!(s.scsp.drain(1), [0, 0]);
            assert_eq!(&s.ram[0x1000..0x1002], &[0x12, 0x34]);
            s.scsp.set_reset(false);
            until(&mut s, 256, chunk);
            assert_eq!(s.scsp.drain(1), [0, 0]); // no stale output/program
            // Reprogram at the current sound phase; no phase restart.
            s.scsp.write16(0xE02, 0x2000).unwrap();
            s.scsp.write16(0x700, 0x4000).unwrap();
            program(&mut s, 5, [0, 0xA000, 2, 0]);
            program(&mut s, 6, [0, 0, 0x1002, 0]);
            s.set_enabled(false).unwrap();
            until(&mut s, 263, chunk);
            assert_eq!(s.scsp.read16(0xEC0), Ok(0x1000));
        }
    }
}
#[test]
fn timer_and_sample_irqs_stay_at_frame_completion_when_dsp_steps_are_distributed() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup();
        s.scsp.write16(0x418, 0xFE).unwrap();
        until(&mut s, 127, chunk);
        assert_eq!(s.scsp.read16(0x418), Ok(0xFE));
        assert_eq!(s.scsp.read16(0x420).unwrap() & 0x440, 0);
        until(&mut s, 128, chunk);
        assert_eq!(s.scsp.read16(0x418), Ok(0xFF));
        assert_eq!(s.scsp.read16(0x420).unwrap() & 0x440, 0x440);
    }
}

#[test]
fn programming_an_idle_dsp_joins_the_current_phase_without_replaying_elapsed_steps() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        s.scsp.write16(0x400, 0x20F).unwrap();
        until(&mut s, 64, chunk); // empty instruction slots were skipped
        s.scsp.write16(0xE02, 0x2000).unwrap();
        s.scsp.write16(0x700, 0x4000).unwrap();
        program(&mut s, 5, [0, 0xA000, 2, 0]);
        program(&mut s, 6, [0, 0, 0x1002, 0]);
        program(&mut s, 68, [0, 0xA000, 2, 0]);
        program(&mut s, 69, [0, 0, 0x1102, 0]);
        until(&mut s, 70, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0)); // step5 must not replay
        assert_eq!(s.scsp.read16(0xEC2), Ok(0x1000));
        until(&mut s, 135, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0x1000));
    }
}
#[test]
fn dsp_dram_ownership_defers_a_simultaneous_sound_cpu_reset_read_without_sampling_early() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup();
        s.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 2, 0]);
        s.ram[0x200..0x204].copy_from_slice(&[0x4E, 0x72, 0x27, 0]);
        // DSP instruction9 and the first reset read both finish at reference48.
        program(&mut s, 9, [0, 0, 0xA002, 0x8000]);
        s.set_enabled(true).unwrap();
        let cpu_until = |s: &mut Sound, clock: u64| {
            let target = (clock * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
            while s.time < target {
                s.advance((target - s.time).min(chunk) as u32).unwrap();
            }
        };
        cpu_until(&mut s, 20);
        assert!(s.ram_busy());
        assert_eq!(s.cpu.state.a[7], 0);
        s.ram[..2].copy_from_slice(&6u16.to_be_bytes());
        cpu_until(&mut s, 23); // first read has now completed after three waits
        s.ram[..4].copy_from_slice(&0x00051234u32.to_be_bytes());
        cpu_until(&mut s, 26);
        assert_eq!(s.cpu.state.a[7], 0);
        cpu_until(&mut s, 27);
        assert_eq!(s.cpu.state.a[7], 0x00061234); // retained high, later low
        cpu_until(&mut s, 80);
        assert!(s.cpu.state.stopped && !s.cpu.state.halted);
        assert_eq!(s.cpu.state.exceptions, 0);
    }
}

#[test]
fn dsp_wait_states_extend_instruction_retirement_without_replaying_operand_reads() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup();
        s.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 2, 0]);
        s.ram[0x200..0x206].copy_from_slice(&[0xC0, 0xD0, 0x4E, 0x72, 0x27, 0]);
        s.ram[0x800..0x802].copy_from_slice(&3u16.to_be_bytes());
        // The first data read, at sound clock44, coincides with DSP21 MRD.
        program(&mut s, 21, [0, 0, 0xA002, 0x8000]);
        s.set_enabled(true).unwrap();
        s.cpu.state.a[0] = 0x800;
        s.cpu.state.d[0] = 7;
        let cpu_until = |s: &mut Sound, clock: u64| {
            let target = (clock * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
            while s.time < target {
                s.advance((target - s.time).min(chunk) as u32).unwrap();
            }
        };
        cpu_until(&mut s, 47); // operand sampled after three wait states
        s.ram[0x800..0x802].copy_from_slice(&5u16.to_be_bytes());
        // UM table8-4: MULU(source3)38 +2*2 +EA4 =46 clocks.
        // Reset40 +MUL46 +three sound-memory waits =89.
        cpu_until(&mut s, 88);
        assert_eq!(s.cpu.state.d[0], 7);
        assert_eq!(s.cpu.state.instructions, 0);
        cpu_until(&mut s, 89);
        assert_eq!(s.cpu.state.d[0], 21);
        assert_eq!(s.cpu.state.instructions, 1);
        cpu_until(&mut s, 120);
        assert!(s.cpu.state.stopped && !s.cpu.state.halted);
        assert_eq!(s.cpu.state.exceptions, 0);
    }
}

#[test]
fn reset_output_after_a_dsp_stalled_prefetch_starts_after_the_wait_and_remains_bounded() {
    for chunk in [1, 3, 17, 10000] {
        for cancel in [false, true] {
            let mut s = setup();
            s.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 2, 0]);
            s.ram[0x200..0x208].copy_from_slice(&[0x4E, 0x71, 0x4E, 0x70, 0x4E, 0x72, 0x27, 0]);
            program(&mut s, 23, [0, 0, 0xA002, 0x8000]);
            s.set_enabled(true).unwrap();
            let cpu_until = |s: &mut Sound, clock: u64| {
                let target = (clock * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
                while s.time < target {
                    s.advance((target - s.time).min(chunk) as u32).unwrap();
                }
            };
            cpu_until(&mut s, 50);
            assert!(!s.scsp.reset_held);
            assert_eq!(s.cpu.state.instructions, 1);
            if cancel {
                s.set_enabled(false).unwrap();
            }
            cpu_until(&mut s, 51);
            assert_eq!(s.scsp.reset_held, !cancel);
            cpu_until(&mut s, 174);
            assert_eq!(s.scsp.reset_held, !cancel);
            cpu_until(&mut s, 175);
            assert!(!s.scsp.reset_held);
            if !cancel {
                cpu_until(&mut s, 178);
                assert_eq!(s.cpu.state.instructions, 1);
                cpu_until(&mut s, 179);
                assert_eq!(s.cpu.state.instructions, 2);
            }
            assert_eq!(&s.ram[0x1000..0x1002], &[0x12, 0x34]);
            assert!(!s.cpu.state.halted);
        }
    }
}

#[test]
fn simultaneous_reset_edges_never_drop_a_frame_or_reexecute_a_dsp_instruction() {
    for chunk in [1, 17, 10000] {
        for start in 1..=128 {
            let mut s = setup();
            s.reset_output_start = Some((start * REFERENCE_HZ).div_ceil(DSP_HZ));
            s.reset_output_end = Some(((start + 3) * REFERENCE_HZ).div_ceil(DSP_HZ));
            until(&mut s, 256, chunk);
            assert_eq!(
                s.scsp.generated,
                2,
                "reset at step{}, chunk{chunk}",
                start - 1
            );
            assert_eq!(s.scsp.read16(0xEC0), Ok(0));
            assert!(!s.scsp.reset_held);
            assert_eq!(s.scsp.drain(2), [0; 4]);
        }
    }
}
