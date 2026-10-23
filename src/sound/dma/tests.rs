//! ST-077 §3.2 pp.20–21 and §4.2 pp.101–102, figs3.2/4.66,
//! tables4.39–40. Independent data/deadline vectors, without CPU helpers.
use crate::sound::*;

fn until(s: &mut Sound, tick: u64, chunk: u64) {
    let target = (tick * REFERENCE_HZ).div_ceil(DSP_HZ);
    while s.time < target {
        s.advance((target - s.time).min(chunk) as u32).unwrap();
    }
}
fn setup(address: u32, register: u16, control: u16) -> Sound {
    let mut s = Sound::default();
    s.scsp.write16(0x400, 0x20F).unwrap();
    s.scsp.write16(0x412, address as u16).unwrap();
    s.scsp
        .write16(0x414, ((address >> 4) as u16 & 0xF000) | register)
        .unwrap();
    s.scsp.write16(0x416, control).unwrap();
    s.scsp.write16(0x42A, 16).unwrap();
    s.scsp.write16(0x41E, 16).unwrap();
    s.scsp.write16(0x424, 16).unwrap();
    s
}
#[test]
fn dma_words_and_completion_interrupts_publish_at_memory_cycle_boundaries() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = setup(0x1000, 0x10, 0x1008);
        s.ram[0x1000..0x1008].copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
        until(&mut s, 1, chunk); // accepted, not sampled or published
        assert!(s.ram_busy());
        assert_eq!(s.scsp.read16(0x10), Ok(0));
        s.ram[0x1000..0x1002].copy_from_slice(&0x4321u16.to_be_bytes());
        for (tick, reg, expected) in [(2, 0x10, 0x4321), (3, 0x12, 0x5678), (4, 0x14, 0x3C)] {
            let target = (tick * REFERENCE_HZ).div_ceil(DSP_HZ);
            s.advance((target - s.time - 1) as u32).unwrap();
            assert_eq!(s.scsp.read16(reg), Ok(0));
            until(&mut s, tick, chunk);
            assert_eq!(s.scsp.read16(reg), Ok(expected));
            assert_eq!(s.scsp.interrupt_level(), 0);
            assert!(!s.scsp.main_interrupt());
            assert_eq!(s.scsp.read16(0x416).unwrap() & 0x1000, 0x1000);
        }
        until(&mut s, 5, chunk);
        assert!(!s.ram_busy());
        assert_eq!(s.scsp.read16(0x16), Ok(0xDEF0));
        assert_eq!(s.scsp.read16(0x416).unwrap() & 0x1000, 0);
        assert_eq!(s.scsp.interrupt_level(), 1);
        assert!(s.scsp.main_interrupt());
        s.scsp.write16(0x422, 16).unwrap();
        assert_eq!(s.scsp.interrupt_level(), 0);
        assert!(s.scsp.main_interrupt());
        s.scsp.write16(0x42E, 16).unwrap();
        assert!(!s.scsp.main_interrupt());
        assert_eq!(s.scsp.generated, 0); // no audio frame was needed
    }
}
#[test]
fn dma_retains_accepted_word_direction_address_gate_and_geometry_until_completion() {
    for chunk in [1, 3, 17, 10000] {
        for gate in [false, true] {
            let mut s = setup(0x7FFFE, 0x700, if gate { 0x7004 } else { 0x3004 });
            s.scsp.write16(0x700, 0x1230).unwrap();
            s.scsp.write16(0x702, 0x5678).unwrap();
            until(&mut s, 1, chunk);
            // Writes cannot redirect the word already accepted by the DRAM
            // controller; source register data is still live until completion.
            s.scsp.write16(0x412, 0x1000).unwrap();
            s.scsp.write16(0x414, 0x780).unwrap();
            s.scsp.write16(0x416, 4).unwrap(); // DEXE=0 preserves busy
            s.scsp.write16(0x400, 15).unwrap(); // switch to 1-Mbit geometry
            s.scsp.write16(0x700, 0x3450).unwrap();
            until(&mut s, 2, chunk);
            assert_eq!(&s.ram[0x7FFFE..], &if gate { [0, 0] } else { [0x34, 0x50] });
            assert_eq!(s.scsp.dma_remaining, 2);
            // The second word sees the new DDIR and geometry; address80000
            // wraps to chip0 in 1-Mbit mode, and COEF1 receives live RAM data.
            s.ram[..2].copy_from_slice(&0x6789u16.to_be_bytes());
            until(&mut s, 3, chunk);
            assert_eq!(s.scsp.read16(0x702), Ok(0x6788));
            assert_eq!(s.scsp.read16(0x416).unwrap() & 0x1000, 0);
        }
    }
}
#[test]
fn refresh_and_dsp_precede_internal_dma_without_losing_an_accepted_word() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        s.scsp.write16(0x400, 0x20F).unwrap();
        s.scsp.write16(0x780, 0x800).unwrap();
        s.ram[0x1000..0x1004].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        // At instruction123 DSP reads address1000. IWT at125 records it.
        for (step, words) in [(123, [0, 0, 0xA002, 0x8000]), (125, [0, 0x20, 2, 0])] {
            for (i, word) in words.into_iter().enumerate() {
                s.scsp
                    .write16(0x800 + step * 8 + i as u32 * 2, word)
                    .unwrap();
            }
        }
        until(&mut s, 123, chunk);
        s.scsp.write16(0x412, 0x1000).unwrap();
        s.scsp.write16(0x414, 0x700).unwrap();
        s.scsp.write16(0x416, 0x1004).unwrap();
        until(&mut s, 124, chunk); // DSP123 owns RAM
        assert!(s.ram_busy());
        assert_eq!(s.scsp.read16(0x700), Ok(0));
        s.ram[0x1000..0x1002].copy_from_slice(&0x4321u16.to_be_bytes());
        until(&mut s, 125, chunk); // refresh124 owns RAM
        assert!(s.ram_busy());
        assert_eq!(s.scsp.read16(0x700), Ok(0));
        until(&mut s, 126, chunk); // DMA accepts first word
        assert_eq!(s.scsp.read16(0xE02), Ok(0x1234));
        assert_eq!(s.scsp.read16(0x700), Ok(0));
        until(&mut s, 127, chunk); // word completes, refresh126 owns RAM
        assert_eq!(s.scsp.read16(0x700), Ok(0x4320));
        assert_eq!(s.scsp.dma_remaining, 2);
        until(&mut s, 128, chunk); // DMA accepts second word
        assert_eq!(s.scsp.read16(0x702), Ok(0));
        until(&mut s, 129, chunk);
        assert_eq!(s.scsp.read16(0x702), Ok(0x5678));
        assert_eq!(s.scsp.read16(0x420).unwrap() & 16, 16);
    }
}
#[test]
fn reset_cancels_the_unfinished_dma_word_but_sound_off_keeps_the_device_running() {
    for tick in 1..=4 {
        for chunk in [1, 3, 17, 10000] {
            let mut s = setup(0x1000, 0x700, 0x3008);
            for (at, value) in [
                (0x700, 0x1230),
                (0x702, 0x5678),
                (0x704, 0x9AB8),
                (0x706, 0xDEF0),
            ] {
                s.scsp.write16(at, value).unwrap();
            }
            until(&mut s, tick, chunk);
            let retained = s.ram.clone();
            s.reset_output_start = Some(s.time + 1);
            s.reset_output_end = Some(s.time + 10);
            until(&mut s, 130, chunk);
            assert_eq!(s.ram, retained);
            assert_eq!(s.scsp.read16(0x416), Ok(0));
            assert_eq!(s.scsp.read16(0x420).unwrap() & 16, 0);
            assert_eq!(s.scsp.drain(1), [0, 0]);
        }
    }
    let mut s = setup(0x1000, 0x700, 0x3002);
    s.scsp.write16(0x700, 0x3450).unwrap();
    until(&mut s, 1, 1);
    s.set_enabled(false).unwrap();
    until(&mut s, 2, 1);
    assert_eq!(&s.ram[0x1000..0x1002], &[0x34, 0x50]);
    assert_eq!(s.scsp.read16(0x420).unwrap() & 16, 16);
}

#[test]
fn idle_device_reserves_exactly_two_refresh_cycles_per_sample_without_clock_drift() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        let mut busy = Vec::new();
        for tick in 1..=512 {
            until(&mut s, tick, chunk);
            if s.ram_busy() {
                busy.push(tick);
            }
        }
        assert_eq!(busy, [125, 127, 253, 255, 381, 383, 509, 511]);
        assert_eq!(s.scsp.generated, 4);
        assert_eq!(s.scsp.drain(4), [0; 8]);
    }
}

#[test]
fn reset_prefetch_of_internal_scsp_registers_does_not_request_dram() {
    // PC is sampled at sound-clock32. By then all DRAM vector reads have
    // completed. The two reset prefetches address the internal register bus;
    // a permanently queued main-memory request cannot hold them indefinitely.
    let mut s = Sound::default();
    s.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0x10, 0, 0]);
    s.set_enabled(true).unwrap();
    s.advance((32 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32)
        .unwrap();
    assert_eq!(s.cpu.state.pc, 0x100000);
    s.main_ram_request(Some(memory::Request {
        address: 0x1000,
        width: 2,
        write: None,
    }));
    s.advance((48 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32)
        .unwrap();
    assert!(s.cpu.state.instructions > 0);
    assert_eq!(s.cpu.state.exceptions, 0);
    assert!(!s.cpu.state.halted);
}

#[test]
fn last_cycle_dma_write_is_visible_before_the_next_frame_captures_pcm() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = Sound::default();
        for (at, value) in [
            (0x400, 0x20F),
            (2, 0x1000),
            (6, 1),
            (12, 0x100),
            (22, 0xE000),
            (0, 0x1820),
            (0x700, 0x1230),
        ] {
            s.scsp.write16(at, value).unwrap();
        }
        until(&mut s, 127, chunk);
        s.scsp.write16(0x412, 0x1000).unwrap();
        s.scsp.write16(0x414, 0x700).unwrap();
        s.scsp.write16(0x416, 0x3002).unwrap();
        until(&mut s, 128, chunk); // grant the only word; first frame ends
        assert_eq!(s.scsp.drain(1), [0, 0]);
        assert_eq!(&s.ram[0x1000..0x1002], &[0, 0]);
        until(&mut s, 129, chunk); // complete word before source capture
        assert_eq!(&s.ram[0x1000..0x1002], &[0x12, 0x30]);
        until(&mut s, 256, chunk);
        assert_eq!(s.scsp.drain(1), [0x1230; 2]);
    }
}
