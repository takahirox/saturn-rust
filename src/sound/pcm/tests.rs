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
            let t = u64::from(slot) * 4 + 1;
            until(&mut s, t, chunk); // first address retained, data still live
            assert!(s.ram_busy());
            s.ram[0x1000..0x1002].copy_from_slice(&0x1800u16.to_be_bytes());
            until(&mut s, t + 1, chunk);
            assert_eq!(s.scsp.pcm_slot.as_ref().unwrap().samples, [0x1800, 0]);
            s.ram[0x1000..0x1004].copy_from_slice(&[0x70, 0, 0x28, 0]);
            until(&mut s, t + 2, chunk); // second address retained
            assert!(s.ram_busy());
            assert_eq!(s.scsp.frame_mix, [0; 2]);
            s.ram[0x1002..0x1004].copy_from_slice(&0x3800u16.to_be_bytes());
            until(&mut s, t + 3, chunk);
            assert_eq!(s.scsp.frame_mix, [0x2800; 2]); // (0x1800+0x3800)/2
            s.ram[0x1000..0x1004].fill(0);
            until(&mut s, 128, chunk);
            assert_eq!(s.scsp.drain(1), [0x2800; 2]);
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
        until(&mut s, 2, chunk); // -32768 retained
        s.ram[0x1000..0x1002].copy_from_slice(&[0x7F, 0x60]);
        until(&mut s, 4, chunk);
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
        until(&mut s, 1, chunk); // first 4-Mbit read granted
        s.scsp.write16(0x400, 15).unwrap(); // 1-Mbit: absent chip
        // A live SA write cannot redirect a slot whose OP2 already ran.
        s.scsp.write16(2, 0x2000).unwrap();
        until(&mut s, 2, chunk);
        assert_eq!(s.scsp.pcm_slot.as_ref().unwrap().samples[0], 8192);
        until(&mut s, 3, chunk); // second read accepts absent-chip geometry
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
                    vec![1, 3, 125, 127]
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
                dsp || tick % 2 == 1 || matches!(tick, 126 | 128),
                "tick={tick}"
            );
        }
        assert_eq!(s.scsp.refresh_pending, if dsp { 2 } else { 0 });
        assert_eq!(s.scsp.drain(1), [1024; 2]);
        if dsp {
            for step in (1..128).step_by(2) {
                s.scsp.write16(0x804 + step * 8, 2).unwrap();
            }
            until(&mut s, 129, 1); // PCM0 first
            assert_eq!(s.scsp.refresh_pending, 2);
            until(&mut s, 130, 1); // retained refresh request
            assert_eq!(s.scsp.refresh_pending, 1);
            assert!(s.ram_busy());
            until(&mut s, 131, 1); // PCM0 second
            assert_eq!(s.scsp.refresh_pending, 1);
            until(&mut s, 132, 1);
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
        // DSP1 reads the word at1000; DMA cannot start until cycle3.
        s.scsp.write16(0x780, 0x800).unwrap();
        for (i, word) in [0, 0, 0xA002, 0x8000].into_iter().enumerate() {
            s.scsp.write16(0x808 + i as u32 * 2, word).unwrap();
        }
        for (at, value) in [(0x412, 0x1000), (0x414, 0x700), (0x416, 0x1004)] {
            s.scsp.write16(at, value).unwrap();
        }
        until(&mut s, 3, chunk);
        assert_eq!(s.scsp.read16(0x700), Ok(0));
        assert!(s.scsp.dma_word.is_none());
        until(&mut s, 4, chunk); // DMA accepted
        assert!(s.scsp.dma_word.is_some());
        s.ram[0x1000..0x1002].copy_from_slice(&0x2340u16.to_be_bytes());
        until(&mut s, 5, chunk);
        assert_eq!(s.scsp.read16(0x700), Ok(0x2340));
        until(&mut s, 6, chunk);
        assert_eq!(s.scsp.read16(0x702), Ok(0x5678));
        assert_eq!(s.scsp.read16(0x420).unwrap() & 16, 16);
    }
}
#[test]
fn reset_cancels_either_read_and_completed_operands_cannot_leak_into_the_next_voice() {
    for tick in 1..=3 {
        for chunk in [1, 3, 17, 10000] {
            let mut s = Sound::default();
            voice(&mut s, 0, 0, true);
            s.ram[0x1000..0x1008].fill(0x12);
            until(&mut s, tick, chunk);
            s.reset_output_start = Some(s.time + 1);
            s.reset_output_end = Some(s.time + 10);
            until(&mut s, 128, chunk);
            assert_eq!(s.scsp.drain(1), [0; 2]);
            assert!(s.scsp.pcm_slot.is_none() && s.scsp.pcm_word.is_none());
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
            (4, [0, 0xA800, 2, 0]),
            (5, [0, 0, 0x1102, 0]),
        ] {
            for (part, word) in words.into_iter().enumerate() {
                s.scsp
                    .write16(0x800 + step * 8 + part as u32 * 2, word)
                    .unwrap();
            }
        }
        until(&mut s, 2, chunk);
        assert_eq!(s.scsp.read16(0xEC0), Ok(0)); // PCM not completed
        until(&mut s, 6, chunk);
        assert_eq!(s.scsp.read16(0xEC2), Ok(4096)); // actual 8192 * .5
    }
}
