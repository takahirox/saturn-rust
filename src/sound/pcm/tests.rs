//! Independently authored ST-077 §3.2/fig.3.2 and table4.14 vectors.
//! Mutations are placed on either side of actual DRAM deadlines, never
//! inferred from the output buffer or a production interpolation helper.
use super::*;

fn until(s: &mut Sound, tick: u64, chunk: u64) {
    let target = (tick * REFERENCE_HZ).div_ceil(DSP_HZ);
    while s.time < target {
        s.advance((target - s.time).min(chunk) as u32).unwrap();
    }
}
fn voice(s: &mut Sound, slot: u32, control: u16, direct: bool) {
    for (offset, value) in [
        (2, 0x1000),
        (6, 4),
        (8, 31),
        (12, if direct { 0x100 } else { 0 }),
        (16, 0x7800),
        (22, 0xE000),
    ] {
        s.scsp.write16(slot * 32 + offset, value).unwrap();
    }
    s.scsp.write16(slot * 32, 0x1820 | control).unwrap();
    s.scsp.write16(0x400, 0x20F).unwrap();
}
#[test]
fn two_operand_reads_retain_separate_data_and_publish_only_after_interpolation() {
    for slot in [0, 7, 31] {
        for chunk in [1, 3, 17, 10000] {
            let mut s = Sound::default();
            voice(&mut s, slot, 0, true);
            s.scsp.voices[slot as usize].phase = 0x8000;
            s.ram[0x1000..0x1004].copy_from_slice(&[0x10, 0, 0x20, 0]);
            let t = u64::from(slot) * 4 + 9; // OP3 after OP1/OP2
            until(&mut s, t, chunk); // first address retained, data still live
            assert!(s.ram_busy());
            s.ram[0x1000..0x1002].copy_from_slice(&0x1800u16.to_be_bytes());
            until(&mut s, t + 1, chunk);
            assert_eq!(
                s.scsp.pcm_pipeline[2].as_ref().unwrap().samples,
                [0x1800, 0]
            );
            s.ram[0x1000..0x1004].copy_from_slice(&[0x70, 0, 0x28, 0]);
            until(&mut s, t + 2, chunk); // second address retained
            assert!(s.ram_busy());
            assert_eq!(s.scsp.frame_mix, [0; 2]);
            s.ram[0x1002..0x1004].copy_from_slice(&0x3800u16.to_be_bytes());
            until(&mut s, t + 3, chunk);
            assert_eq!(s.scsp.frame_mix, [0; 2]); // OP3 is not level completion
            s.ram[0x1000..0x1004].fill(0);
            until(&mut s, t + 16, chunk); // OP7 after both level intervals
            assert_eq!(s.scsp.frame_mix, [0x2800; 2]); // (0x1800+0x3800)/2
            until(&mut s, if slot == 31 { 256 } else { 128 }, chunk);
            assert_eq!(
                s.scsp.drain(2),
                if slot == 31 {
                    vec![0, 0, 0x2800, 0x2800]
                } else {
                    vec![0x2800; 2]
                }
            );
        }
    }
}
#[test]
fn pcm8_reads_separate_bytes_even_when_they_share_one_word() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        voice(&mut s, 0, 16, true);
        s.scsp.voices[0].phase = 0x8000;
        s.ram[0x1000..0x1002].copy_from_slice(&[0x80, 0x40]);
        until(&mut s, 10, chunk); // -32768 retained
        s.ram[0x1000..0x1002].copy_from_slice(&[0x7F, 0x60]);
        until(&mut s, 12, chunk);
        until(&mut s, 128, chunk);
        assert_eq!(s.scsp.drain(1), [-4096; 2]); // (-32768+24576)/2
    }
}
#[test]
fn each_grant_retains_geometry_but_later_reads_observe_new_memory_configuration() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        voice(&mut s, 0, 2, true); // address21000
        s.scsp.voices[0].phase = 0x8000;
        s.ram[0x21000..0x21004].copy_from_slice(&[0x20, 0, 0x40, 0]);
        until(&mut s, 9, chunk); // first 4-Mbit read granted
        s.scsp.write16(0x400, 15).unwrap(); // 1-Mbit: absent chip
        // A live SA write cannot redirect a slot whose OP2 already ran.
        s.scsp.write16(2, 0x2000).unwrap();
        until(&mut s, 10, chunk);
        assert_eq!(s.scsp.pcm_pipeline[2].as_ref().unwrap().samples[0], 8192);
        until(&mut s, 11, chunk); // second read accepts absent-chip geometry
        s.scsp.write16(0x400, 0x20F).unwrap();
        until(&mut s, 128, chunk);
        assert_eq!(s.scsp.drain(1), [4095; 2]); // (8192-1)>>1
    }
}
#[test]
fn maximum_eg_suppresses_reads_but_sdir_and_unrouted_voices_still_fetch() {
    for direct in [false, true] {
        for source in [0, 1, 2] {
            let mut s = Sound::default();
            voice(&mut s, 0, source << 7, direct);
            s.scsp.write16(8, 0).unwrap(); // AR=0, EG remains3FF
            s.scsp.write16(22, 0).unwrap(); // no output send
            let mut busy = Vec::new();
            for tick in 1..=128 {
                until(&mut s, tick, 3);
                if s.ram_busy() {
                    busy.push(tick);
                }
            }
            assert_eq!(
                busy,
                if direct && source == 0 {
                    vec![9, 11, 125, 127]
                } else {
                    vec![125, 127]
                }
            );
            assert_eq!(s.scsp.voices[0].phase, 0x8000);
            assert!(s.scsp.voices[0].active);
            assert_eq!(s.scsp.drain(1), [0; 2]);
            assert_ne!(s.scsp.read16(0x420).unwrap() & 0x400, 0);
        }
    }
}
#[test]
fn all_slots_own_two_cycles_and_defer_refresh_until_the_next_available_cycle() {
    for dsp in [false, true] {
        let mut s = Sound::default();
        for slot in 0..32 {
            voice(&mut s, slot, 0, true);
        }
        s.ram[0x1000..0x1008].copy_from_slice(&[0, 32, 0, 32, 0, 32, 0, 32]);
        if dsp {
            for step in (1..128).step_by(2) {
                s.scsp.write16(0x804 + step * 8, 0xA002).unwrap();
            }
        }
        for tick in 1..=128 {
            until(&mut s, tick, 1);
            assert_eq!(
                s.ram_busy(),
                (dsp && tick % 2 == 0) || (tick >= 9 && tick % 2 == 1) || matches!(tick, 126 | 128),
                "tick={tick}"
            );
        }
        assert_eq!(s.scsp.refresh_pending, if dsp { 2 } else { 0 });
        assert_eq!(s.scsp.drain(1), [832; 2]); // 26 completed levels at cold start
        until(&mut s, 256, 1);
        assert_eq!(s.scsp.drain(1), [1024; 2]); // all 32 once pipeline is full
        if dsp {
            for step in (1..128).step_by(2) {
                s.scsp.write16(0x804 + step * 8, 2).unwrap();
            }
            until(&mut s, 257, 1); // PCM30 first (OP3 follows OP1 by two slots)
            assert_eq!(s.scsp.refresh_pending, 4);
            until(&mut s, 258, 1); // retained refresh request
            assert_eq!(s.scsp.refresh_pending, 3);
            assert!(s.ram_busy());
            until(&mut s, 259, 1); // PCM30 second
            assert_eq!(s.scsp.refresh_pending, 3);
            until(&mut s, 260, 1);
            assert_eq!(s.scsp.refresh_pending, 2);
        }
        if dsp {
            until(&mut s, 264, 1);
            assert_eq!(s.scsp.refresh_pending, 0);
        }
    }
}
#[test]
fn pcm_and_dsp_precede_dma_while_the_accepted_dma_word_is_not_lost() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        voice(&mut s, 0, 0, true);
        s.ram[0x1000..0x1004].copy_from_slice(&[0x12, 0x30, 0x56, 0x78]);
        until(&mut s, 8, chunk); // retain OP1/OP2 before requests arrive
        // DSP9 reads the word at1000; DMA cannot start until cycle11.
        s.scsp.write16(0x780, 0x800).unwrap();
        for (i, word) in [0, 0, 0xA002, 0x8000].into_iter().enumerate() {
            s.scsp.write16(0x848 + i as u32 * 2, word).unwrap();
        }
        for (at, value) in [(0x412, 0x1000), (0x414, 0x700), (0x416, 0x1004)] {
            s.scsp.write16(at, value).unwrap();
        }
        until(&mut s, 11, chunk);
        assert_eq!(s.scsp.read16(0x700), Ok(0));
        assert!(s.scsp.dma_word.is_none());
        until(&mut s, 12, chunk); // DMA accepted
        assert!(s.scsp.dma_word.is_some());
        s.ram[0x1000..0x1002].copy_from_slice(&0x2340u16.to_be_bytes());
        until(&mut s, 13, chunk);
        assert_eq!(s.scsp.read16(0x700), Ok(0x2340));
        until(&mut s, 14, chunk);
        assert_eq!(s.scsp.read16(0x702), Ok(0x5678));
        assert_eq!(s.scsp.read16(0x420).unwrap() & 16, 16);
    }
}
#[test]
fn reset_cancels_either_read_and_completed_operands_cannot_leak_into_the_next_voice() {
    for tick in 9..=11 {
        for chunk in [1, 3, 17, 10000] {
            let mut s = Sound::default();
            voice(&mut s, 0, 0, true);
            s.ram[0x1000..0x1008].fill(0x12);
            until(&mut s, tick, chunk);
            s.reset_output_start = Some(s.time + 1);
            s.reset_output_end = Some(s.time + 10);
            until(&mut s, 128, chunk);
            assert_eq!(s.scsp.drain(1), [0; 2]);
            assert!(
                s.scsp
                    .pcm_pipeline
                    .iter()
                    .flatten()
                    .all(|p| !p.active && !p.fetch)
                    && s.scsp.pcm_word.is_none()
            );
            voice(&mut s, 0, 0, true);
            s.ram[0x1000..0x1008].fill(0x23);
            until(&mut s, 256, chunk);
            assert_eq!(s.scsp.drain(1), [0x2323; 2]);
        }
    }
}

#[test]
fn dsp_input_routing_observes_a_completed_slot_without_an_early_or_canned_sample() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        voice(&mut s, 0, 0, true);
        s.scsp.write16(20, 7).unwrap(); // ISEL0 / full gain
        s.scsp.write16(0x700, 0x4000).unwrap(); // half gain
        s.ram[0x1000..0x1004].copy_from_slice(&[0x20, 0, 0x40, 0]);
        for (step, words) in [
            (0, [0u16, 0xA800, 2, 0]),
            (1, [0, 0, 0x1002, 0]),
            (24, [0, 0xA800, 2, 0]),
            (25, [0, 0, 0x1102, 0]),
        ] {
            for (part, word) in words.into_iter().enumerate() {
                s.scsp
                    .write16(0x800 + step * 8 + part as u32 * 2, word)
                    .unwrap();
            }
        }
        until(&mut s, 2, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0)); // PCM not completed
        until(&mut s, 26, chunk);
        assert_eq!(s.scsp.read16(0xEC2), Ok(4096)); // actual 8192 * .5
    }
}

#[test]
fn primary_pipeline_deadlines_keep_direct_and_executing_dsp_private_until_level_completion() {
    // Authored from fig4.30 and fig4.18 before production changes: OP1=4*s,
    // OP3=4*s+8, OP7=4*s+24. A full IMXL send yields PCM<<4 MIXS;
    // DSP .5 and its one-instruction shifter delay yield PCM/2 EFREG.
    for slot in [0, 7, 31] {
        for pcm8 in [false, true] {
            for chunk in [1, 3, 17, 10000] {
                let mut s = Sound::default();
                voice(&mut s, slot, if pcm8 { 16 } else { 0 }, true);
                s.scsp.voices[slot as usize].phase = 0x8000;
                s.scsp.write16(slot * 32 + 20, 7).unwrap();
                s.scsp.write16(0x700, 0x4000).unwrap();
                if pcm8 {
                    s.ram[0x1000..0x1002].copy_from_slice(&[0x80, 0x60]);
                } else {
                    s.ram[0x1000..0x1004].copy_from_slice(&[0x80, 0, 0x60, 0]);
                }
                let publish = u64::from(slot) * 4 + 25;
                for (tick, words) in [
                    (publish - 1, [0, 0xA800, 2, 0]),
                    (publish, [0, 0xA800, 0x1002, 0]),
                    (publish + 1, [0, 0, 0x1102, 0]),
                ] {
                    for (part, word) in words.into_iter().enumerate() {
                        s.scsp
                            .write16(
                                0x800 + ((tick - 1) & 127) as u32 * 8 + part as u32 * 2,
                                word,
                            )
                            .unwrap();
                    }
                }
                until(&mut s, u64::from(slot) * 4 + 12, chunk);
                s.ram[0x1000..0x1004].fill(0x7F); // retained operands must win
                until(&mut s, publish - 1, chunk);
                assert_eq!(s.scsp.frame_mix, [0; 2]);
                assert_eq!(s.scsp.read16(0xE82), Ok(0));
                until(&mut s, publish, chunk);
                assert_eq!(s.scsp.frame_mix, [-4096; 2]);
                assert_eq!(s.scsp.read16(0xE82), Ok(0xF000));
                assert_eq!(s.scsp.read16(0xEC0), Ok(0)); // read just before publication
                until(&mut s, publish + 1, chunk);
                assert_eq!(s.scsp.read16(0xEC2), Ok((-2048i16) as u16));
                until(&mut s, publish + 3, chunk);
                assert_eq!(s.scsp.frame_mix, [-4096; 2]); // once only
            }
        }
    }
}

#[test]
fn scheduled_fm_reads_all_selectors_and_generations_at_the_five_interval_boundary() {
    // The standalone Stack test remains unchanged. Here actual PG, OP2,
    // two DRAM reads, interpolation and OP7 produce every stack value.
    for carrier in 0..32usize {
        for select in 0..64u16 {
            let mut scsp = Scsp::default();
            let mut ram = vec![0; SOUND_RAM_SIZE];
            for source in 0..32 {
                let base = source as u32 * 32;
                for (at, value) in [
                    (2, 0x2000 + source as u16 * 0x100),
                    (6, 4),
                    (12, 0x100),
                    (16, 0x4000),
                    (0, 0x1820),
                ] {
                    scsp.write16(base + at, value).unwrap();
                }
            }
            scsp.write16(carrier as u32 * 32 + 14, 0x5000 | select << 6 | select)
                .unwrap();
            for tick in 1..=640usize {
                let step = (tick - 1) & 127;
                // Author distinct operands for each OP1 generation before its
                // OP3 grants, including slots30/31 crossing the frame edge.
                if tick >= 9 && step % 4 == 0 {
                    let source = (step / 4 + 30) & 31;
                    let generation = (tick - 9) / 128;
                    let value = ((generation + 1) * 100 + source) as u16;
                    let start = 0x2000 + source * 0x100;
                    for i in 0..4 {
                        ram[start + i * 2..start + i * 2 + 2].copy_from_slice(&value.to_be_bytes());
                    }
                }
                scsp.sample_cycle(step, &mut ram).unwrap();
                if tick == 517 + carrier * 4 {
                    // OP2 frame4, all generations initialized
                    let source = (carrier + usize::from(select)) & 31;
                    let newest = if source + 5 <= carrier {
                        4
                    } else if source < carrier + 28 {
                        3
                    } else {
                        2
                    };
                    let generation = newest - usize::from(!(0x1C..=0x3B).contains(&select));
                    let expected = ((generation + 1) * 100 + source) as i64;
                    let slot = scsp.pcm_pipeline[1].as_ref().unwrap();
                    // octave -8: 256 Q16 units/sample, four preceding PGs.
                    assert_eq!(
                        slot.fraction,
                        4 * 256 + expected * 64,
                        "carrier={carrier} select={select:02x}"
                    );
                }
            }
        }
    }
}

#[test]
fn scheduled_stwinh_preserves_both_generations_across_wrap() {
    let mut s = Sound::default();
    voice(&mut s, 31, 0, true);
    s.ram[0x1000..0x1008].fill(0x12);
    until(&mut s, 512, 3);
    let before = s.scsp.stack.data;
    assert!(before.iter().filter(|&&v| v == 0x1212).count() >= 2);
    s.scsp.write16(31 * 32 + 12, 0x300).unwrap(); // SDIR + STWINH
    s.ram[0x1000..0x1008].fill(0x23);
    // The prior OP1 snapshot may still publish; wait until inhibited work
    // has traversed OP7, then verify both banks remain byte-for-byte fixed.
    until(&mut s, 768, 17);
    let inhibited = s.scsp.stack.data;
    until(&mut s, 1280, 10000);
    assert_eq!(s.scsp.stack.data, inhibited);
    assert!(s.scsp.drain(10).contains(&0x2323)); // STWINH inhibits only stack
}

#[test]
fn idle_keyon_joins_its_slot_and_retained_work_drains_after_activity_stops() {
    for phase in [2, 17, 63, 126] {
        for slot in [0, 7, 31] {
            for chunk in [1, 3, 17, 10000] {
                let mut s = Sound::default();
                until(&mut s, phase, chunk);
                voice(&mut s, slot, 0, true);
                s.ram[0x1000..0x1008].fill(0x12);
                let first = u64::from(slot) * 4 + 1;
                let first = first + if first <= phase { 128 } else { 0 };
                until(&mut s, first + 13, chunk);
                assert_eq!(s.scsp.voices[slot as usize].phase, 0x8000);
                // Force the generator's idle state after PG, with in-flight
                // work still pending. The scheduler must drain that work.
                s.scsp.voices[slot as usize].active = false;
                until(&mut s, first + 23, chunk);
                assert_eq!(s.scsp.frame_mix, [0; 2]);
                until(&mut s, first + 24, chunk);
                assert_eq!(s.scsp.frame_mix, [0x1212; 2]);
                until(&mut s, first + 400, chunk);
                let audio = s.scsp.drain(8);
                assert_eq!(audio.iter().filter(|&&v| v == 0x1212).count(), 2);
                assert_eq!(s.scsp.voices[slot as usize].phase, 0x8000);
            }
        }
    }
}

#[test]
fn reset_cancels_every_retained_stage_but_cpu_disable_keeps_synthesis_running() {
    for tick in [
        1, 5, 9, 11, 12, 13, 17, 21, 24, 125, 129, 133, 137, 145, 148,
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut s = Sound::default();
            for slot in [0, 31] {
                voice(&mut s, slot, 0, true);
                s.scsp.write16(slot * 32 + 20, 7).unwrap();
            }
            s.ram[0x1000..0x1008].fill(0x12);
            until(&mut s, tick, chunk);
            s.scsp.drain(8); // already emitted audio belongs to the old domain
            s.scsp.set_reset(true);
            assert!(s.scsp.pcm_pipeline.iter().all(Option::is_none));
            assert!(s.scsp.pcm_word.is_none());
            assert_eq!(s.scsp.stack.data, [0; 64]);
            assert_eq!(s.scsp.dsp.mix, [0; 16]);
            s.scsp.set_reset(false);
            until(&mut s, 384, chunk);
            assert!(s.scsp.drain(8).iter().all(|&v| v == 0));
        }
    }
    let mut s = Sound::default();
    voice(&mut s, 31, 0, true);
    s.ram[0x1000..0x1008].fill(0x23);
    until(&mut s, 140, 3);
    s.set_enabled(false).unwrap();
    until(&mut s, 256, 3);
    assert_eq!(s.scsp.drain(2), [0, 0, 0x2323, 0x2323]);
}

#[test]
fn simultaneous_pipeline_publications_memory_records_and_audio_are_chunk_and_inspection_invariant()
{
    let mut reference = None;
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let mut s = Sound::default();
            for slot in [0, 7, 31] {
                voice(&mut s, slot, 0, true);
                s.scsp.write16(slot * 32 + 20, 1).unwrap(); // 1/64 MIXS prevents overflow
            }
            s.ram[0x1000..0x1008].fill(0x10);
            let mut publications = Vec::new();
            for tick in [24, 25, 52, 53, 128, 148, 149, 180, 181, 256, 384] {
                until(&mut s, tick, chunk);
                if inspect {
                    for at in [0x408, 0x600, 0x63E, 0xE80, 0xE82, 0xEC0] {
                        s.scsp.read16(at).unwrap();
                    }
                }
                publications.push((tick, s.scsp.frame_mix, s.scsp.dsp.mix[0]));
            }
            assert_eq!(
                publications,
                [
                    (24, [0; 2], 0),
                    (25, [0x1010; 2], 0x404),
                    (52, [0x1010; 2], 0x404),
                    (53, [0x2020; 2], 0x808),
                    (128, [0x2020; 2], 0),
                    (148, [0; 2], 0),
                    (149, [0x1010; 2], 0x404),
                    (180, [0x2020; 2], 0x808),
                    (181, [0x3030; 2], 0xC0C),
                    (256, [0x3030; 2], 0),
                    (384, [0x3030; 2], 0),
                ]
            ); // Each 0x1010 voice contributes exactly 0x404 at IMXL1.
            let result = (publications, s.memory_trace().to_vec(), s.scsp.drain(3));
            assert_eq!(result.2, [0x2020, 0x2020, 0x3030, 0x3030, 0x3030, 0x3030]);
            if let Some(reference) = &reference {
                assert_eq!(&result, reference);
            } else {
                reference = Some(result);
            }
        }
    }
}

#[test]
fn idle_deadline_skipping_matches_dense_execution_for_lfo_stack_and_later_noise_activation() {
    for phase in [2, 17, 63, 126, 255] {
        let mut sparse = Sound::default();
        let mut dense = Sound::default();
        dense.scsp.write16(0x800, 0).unwrap(); // run every DSP deadline
        for s in [&mut sparse, &mut dense] {
            for slot in 0..32 {
                s.scsp.write16(slot * 32 + 18, 0x7F7F).unwrap();
            }
            until(s, phase, 17);
            voice(s, 7, 0x80, true); // internal noise, after distributed idle clocks
            until(s, 640, 17);
        }
        assert_eq!(sparse.scsp.stack.data, dense.scsp.stack.data);
        assert_eq!(sparse.scsp.drain(8), dense.scsp.drain(8));
        for slot in 0..32 {
            assert_eq!(sparse.scsp.lfo[slot].noise(), dense.scsp.lfo[slot].noise());
            assert_eq!(
                sparse.scsp.lfo[slot].step(0x7F7F, 65536),
                dense.scsp.lfo[slot].step(0x7F7F, 65536)
            );
            assert_eq!(
                sparse.scsp.lfo[slot].attenuation(0x7F7F),
                dense.scsp.lfo[slot].attenuation(0x7F7F)
            );
        }
    }
}

#[test]
fn an_inflight_envelope_cannot_overwrite_a_later_keyoff_or_retrigger() {
    let mut s = Sound::default();
    voice(&mut s, 0, 0, true);
    until(&mut s, 5, 1); // OP2 retained the attack calculation
    s.scsp.write16(0, 0x1020).unwrap(); // KYONEX / KEYOFF
    assert!(s.scsp.voices[0].envelope.releasing());
    until(&mut s, 13, 1); // OP4 must not restore the old attack
    assert!(s.scsp.voices[0].envelope.releasing());
    s.scsp.write16(0, 0x1820).unwrap(); // start a new generation
    let generation = s.scsp.voices[0].generation;
    until(&mut s, 25, 1);
    assert_eq!(s.scsp.voices[0].generation, generation);
    assert_eq!(s.scsp.voices[0].phase, 0);
}
