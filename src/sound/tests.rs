//! Independent ST-077 §4.2/table 4.14, 4.27/28 and §4.2 timers/IRQ vectors.
use super::*;

#[test]
fn memory_geometry_is_shared_by_main_and_sound_cpu_and_preserves_inaccessible_bytes() {
    // ST-077 §4.2 table4.32/fig4.58. Address bit17 selects the absent
    // second chip in 1-Mbit mode, bit19 in 4-Mbit mode. Higher unused
    // address pins do not extend the selected DRAM chip.
    let mut sound = Sound::default();
    for (address, value) in [(0, 0x12), (0x1FFFF, 0x34), (0x20000, 0x56), (0x7FFFF, 0x78)] {
        sound.ram[address] = value;
    }
    for (control, expected) in [
        (0, [0x12, 0x34, 0xFF, 0xFF, 0x12, 0x12]),
        (0x200, [0x12, 0x34, 0x56, 0x78, 0xFF, 0x12]),
    ] {
        sound.scsp.write16(0x400, control).unwrap();
        for (address, value) in [0, 0x1FFFF, 0x20000, 0x7FFFF, 0x80000, 0x100000]
            .into_iter()
            .zip(expected)
        {
            assert_eq!(sound.read_ram8(address), value);
            if address < SOUND_RAM_SIZE {
                let bus = SoundBus {
                    ram: &mut sound.ram,
                    scsp: &mut sound.scsp,
                };
                assert_eq!(bus.read8(address as u32), Ok(value));
            }
        }
        let before = sound.ram.clone();
        sound.write_ram8(0xA0000, 0);
        assert_eq!(sound.ram, before); // absent chip in both geometries
    }
    sound.scsp.write16(0x400, 0).unwrap();
    sound.write_ram8(0x40000, 0x9A); // address pins wrap within chip0
    assert_eq!(sound.ram[0], 0x9A);
    assert_eq!(sound.ram[0x20000], 0x56);
    sound.scsp.write16(0x400, 0x200).unwrap();
    assert_eq!(sound.read_ram8(0x20000), 0x56);
    assert_eq!(sound.read_ram8(0x7FFFF), 0x78);
}

#[test]
fn memory_configuration_controls_pcm_dma_and_executing_effects_memory_reads() {
    for (control, expected) in [(0xF, -1), (0x20F, 0x2345)] {
        let mut scsp = Scsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        ram[0x20000..0x20008].copy_from_slice(&[0x23, 0x45, 0x23, 0x45, 0x23, 0x45, 0x23, 0x45]);
        direct(&mut scsp, 0, 0x20000, 1, false, 0);
        scsp.write16(0x400, control).unwrap();
        scsp.sample(&mut ram).unwrap();
        assert_eq!(scsp.drain(1), [expected; 2]);
        // DMA reads the same address through the controller into COEF0.
        scsp.write16(0x412, 0).unwrap();
        scsp.write16(0x414, 0x2700).unwrap();
        scsp.write16(0x416, 0x1002).unwrap();
        scsp.sample(&mut ram).unwrap();
        assert_eq!(scsp.read16(0x700), Ok((expected as u16) & 0xFFF8));
        // TABLE read at RBP=16, MADRS=0 -> byte address 0x20000.
        // N+2 IWT -> MEMS0; DSP values are signed Q23.
        scsp.write16(0x402, 16).unwrap();
        scsp.write16(0x80C, 0xA002).unwrap();
        scsp.write16(0x80E, 0x8000).unwrap();
        scsp.write16(0x81A, 0x20).unwrap();
        scsp.sample(&mut ram).unwrap();
        assert_eq!(
            u32::from(scsp.read16(0xE00).unwrap()) | (u32::from(scsp.read16(0xE02).unwrap()) << 8),
            (i32::from(expected) * 256) as u32 & 0xFFFFFF
        );
    }
}
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
    assert_eq!(scsp.stack.data.iter().filter(|v| **v == 256).count(), 27);
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
    assert_eq!(scsp.stack.data[0], 4096); // stack also receives interpolated data
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
        assert_eq!(scsp.take_midi_output(), None);
        scsp.advance_midi(scsp.midi_clocks_to_event());
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
    scsp.sample_cycle(0, &mut ram).unwrap();
    assert_eq!(scsp.read16(2).unwrap(), 0);
    scsp.sample_cycle(1, &mut ram).unwrap();
    assert_eq!(scsp.read16(2).unwrap(), 1);
    assert_ne!(scsp.read16(0x416).unwrap() & 0x1000, 0);
    scsp.sample_cycle(2, &mut ram).unwrap();
    assert_eq!(scsp.read16(4).unwrap(), 2);
    assert_eq!(scsp.read16(0x416).unwrap() & 0x1000, 0);
    assert_ne!(scsp.read16(0x420).unwrap() & 16, 0);
    scsp.write16(0x412, 0x204).unwrap();
    scsp.write16(0x414, 2).unwrap();
    scsp.write16(0x416, 0x3004).unwrap();
    scsp.sample_cycle(0, &mut ram).unwrap();
    scsp.sample_cycle(1, &mut ram).unwrap();
    scsp.sample_cycle(2, &mut ram).unwrap();
    assert_eq!(&ram[0x204..0x208], &[0, 1, 0, 2]);
    scsp.write16(0x412, 0x204).unwrap();
    scsp.write16(0x414, 2).unwrap();
    scsp.write16(0x416, 0x7004).unwrap();
    scsp.sample_cycle(0, &mut ram).unwrap();
    scsp.sample_cycle(1, &mut ram).unwrap();
    scsp.sample_cycle(2, &mut ram).unwrap();
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
    assert!(scsp.write16(0, 0x180).is_err());
}

#[test]
fn thirty_two_envelopes_release_retrigger_and_bypass_independently() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x100..0x108].copy_from_slice(&[2, 0, 2, 0, 2, 0, 2, 0]);
    for slot in 0..32 {
        direct(&mut scsp, slot, 0x100, 1, false, 0);
        scsp.write16(slot * 32 + 8, 31).unwrap();
        scsp.write16(slot * 32 + 10, 0x3C1F).unwrap();
        scsp.write16(slot * 32 + 12, 0).unwrap();
    }
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [16384, 16384]);
    // Registering KEY_OFF alone has no effect until a global KYONEX.
    scsp.write16(0, 0x20).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [16384, 16384]);
    scsp.write16(0, 0x1020).unwrap();
    scsp.sample(&mut ram).unwrap();
    let released = scsp.drain(1);
    assert!(released[0] < 16384 && released[0] > 15872);
    assert_eq!(released[0], released[1]);
    scsp.write16(0, 0x1820).unwrap(); // KEY_ON during release restarts
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [16384, 16384]);
    // Repeating KYONEX with KEY_ON unchanged must not restart another attack.
    scsp.write16(8, 31 | 31 << 6).unwrap();
    scsp.write16(10, 0x3FFF).unwrap(); // decay level31, KRS off, RR31
    scsp.write16(0, 0x1820).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert!(scsp.drain(1)[0] < 16384);
    // Direct mode bypasses a running envelope as well as TL and ALFO.
    scsp.write16(12, 0x1FF).unwrap();
    scsp.write16(18, 0xFFFF).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [16384, 16384]);
}

#[test]
fn reset_release_prefetch_finishes_at_forty_then_first_moveq_retires_at_forty_four_clocks() {
    let mut sound = Sound::default();
    sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
    sound.ram[0x100..0x104].copy_from_slice(&[0x70, 42, 0x4E, 0x71]);
    sound.set_enabled(true).unwrap();
    sound
        .advance((40 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32)
        .unwrap();
    assert_eq!(sound.cpu.state.instructions, 0);
    assert_eq!(sound.cpu.state.d[0], 0);
    let deadline = (44 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32;
    sound.advance(deadline - sound.time as u32 - 1).unwrap();
    assert_eq!(sound.cpu.state.instructions, 0);
    sound.advance(1).unwrap();
    assert_eq!(sound.cpu.state.instructions, 1);
    assert_eq!(sound.cpu.state.d[0], 42);
}

#[test]
fn sound_longword_accesses_sample_and_publish_each_word_on_its_own_bus_deadline() {
    // MC68000UM §§5.1/8.3: four CPU clocks per word. The reset
    // sequence has already fetched opcode and successor before operand cycles.
    // Read/write order is independently authored, including predecrement.
    for opcode in [0x2010u16, 0x2080, 0x2100] {
        for chunk in [1, 3, 17, 1000] {
            let mut sound = Sound::default();
            sound.ram[..8].copy_from_slice(&[0, 0, 0x10, 0, 0, 0, 1, 0]);
            sound.ram[0x100..0x102].copy_from_slice(&opcode.to_be_bytes());
            sound.ram[0x102..0x104].copy_from_slice(&0x4E71u16.to_be_bytes());
            sound.ram[0x200..0x204].copy_from_slice(&0x11223344u32.to_be_bytes());
            sound.set_enabled(true).unwrap();
            sound.cpu.state.a[0] = if opcode == 0x2100 { 0x204 } else { 0x200 };
            sound.cpu.state.d[0] = 0x55667788;
            let advance_to = |sound: &mut Sound, ticks: u64| {
                let deadline = (ticks * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
                while sound.time < deadline {
                    sound
                        .advance((deadline - sound.time).min(chunk) as u32)
                        .unwrap();
                }
            };
            advance_to(&mut sound, 43);
            assert_eq!(&sound.ram[0x200..0x204], &0x11223344u32.to_be_bytes());
            assert_eq!(sound.cpu.state.instructions, 0);
            advance_to(&mut sound, 44);
            match opcode {
                0x2010 => {
                    // High source half is now latched; the low half must see
                    // the main CPU's later write through the shared controller.
                    sound.write_ram8(0x200, 0xAA);
                    sound.write_ram8(0x201, 0xBB);
                    sound.write_ram8(0x202, 0xCC);
                    sound.write_ram8(0x203, 0xDD);
                }
                0x2080 => assert_eq!(&sound.ram[0x200..0x204], &[0x55, 0x66, 0x33, 0x44]),
                _ => assert_eq!(&sound.ram[0x200..0x204], &[0x11, 0x22, 0x77, 0x88]),
            }
            assert_eq!(sound.cpu.state.instructions, 0);
            advance_to(&mut sound, 48);
            assert_eq!(sound.cpu.state.instructions, 0);
            advance_to(&mut sound, 52);
            assert_eq!(sound.cpu.state.instructions, 1);
            if opcode == 0x2010 {
                assert_eq!(sound.cpu.state.d[0], 0x1122CCDD);
            } else {
                assert_eq!(&sound.ram[0x200..0x204], &0x55667788u32.to_be_bytes());
            }
        }
    }
}

#[test]
fn sound_reset_cancels_an_uncompleted_word_and_mmio_reads_are_never_repeated() {
    let mut sound = Sound::default();
    sound.ram[..8].copy_from_slice(&[0, 0, 0x10, 0, 0, 0, 1, 0]);
    // MOVE.L (A0),D0, whose low word is a different MMIO register.
    sound.ram[0x100..0x104].copy_from_slice(&[0x20, 0x10, 0x4E, 0x71]);
    sound.scsp.receive_midi(0x35);
    sound.scsp.receive_midi(0x46);
    sound.set_enabled(true).unwrap();
    sound.cpu.state.a[0] = 0x100404;
    sound
        .advance((56 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32)
        .unwrap();
    assert_eq!(sound.cpu.state.d[0], 0x08350000);
    assert_eq!(sound.scsp.read16(0x404).unwrap() & 255, 0x46);

    sound.set_enabled(false).unwrap();
    sound.ram[0x100..0x104].copy_from_slice(&[0x20, 0x80, 0x4E, 0x71]); // MOVE.L D0,(A0)
    sound.set_enabled(true).unwrap();
    sound.cpu.state.a[0] = 0x200;
    sound.cpu.state.d[0] = 0x12345678;
    let first = sound.cpu_origin + (44 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ);
    sound.advance((first - sound.time) as u32).unwrap();
    assert_eq!(&sound.ram[0x200..0x204], &[0x12, 0x34, 0, 0]);
    sound.set_enabled(false).unwrap();
    sound.write_ram8(0x202, 0xAB);
    sound.write_ram8(0x203, 0xCD);
    sound.advance(10000).unwrap();
    assert_eq!(&sound.ram[0x200..0x204], &[0x12, 0x34, 0xAB, 0xCD]);
    assert_eq!(sound.cpu.state.instructions, 0);
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
    let retained = sound.cpu.state.d[0];
    sound.set_enabled(false).unwrap();
    let n = sound.scsp.generated;
    sound.advance(10000).unwrap();
    assert_eq!(sound.cpu.state.instructions, 0);
    assert!(sound.scsp.generated > n);
    assert_eq!(sound.ram[0x100], 0x52);
    sound.set_enabled(true).unwrap();
    assert_eq!(sound.cpu.state.pc, 0);
    assert_eq!(sound.cpu.state.d[0], retained);
    sound
        .advance((32 * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ) as u32)
        .unwrap();
    assert_eq!(sound.cpu.state.pc, 0x100);
    assert_eq!(sound.cpu.state.d[0], retained);
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
    sound.scsp.write16(0x700, 0x4000).unwrap();
    sound.scsp.write16(0xE02, 0x1200).unwrap();
    sound.scsp.write16(0x802, 0xA000).unwrap();
    sound.scsp.write16(0x804, 2).unwrap();
    sound.set_enabled(true).unwrap();
    sound.advance(10000).unwrap();
    assert_eq!(sound.cpu.state.d[0], 37);
    assert!(sound.cpu.state.stopped);
    assert_eq!(sound.ram[0x101], 37);
    assert_eq!(sound.scsp.common[0], 0);
    assert_eq!(sound.scsp.read16(0x700), Ok(0));
    assert_eq!(sound.scsp.read16(0xE02), Ok(0));
    assert_eq!(sound.scsp.read16(0x802), Ok(0));
    assert!(sound.scsp.generated > 10);
}

#[test]
fn fractional_reverse_and_alternating_loops_interpolate_in_traversal_direction() {
    // ST-077 §2.2/§3.1, figure 4.10: attack then reverse/alternative address
    // sequences, followed by first-order interpolation. Half pitch with LSA=1,
    // LEA=4. A reverse wrap joins sample 1 to sample 3; ping-pong holds endpoints.
    // Test both encodings, including negative signed samples; all values are
    // authored arithmetic means of the source samples, independent of the core.
    for (mode, expected) in [
        (
            2,
            [
                10, 15, 40, 35, 30, 25, 20, 30, 40, 35, 30, 25, 20, 30, 40, 35,
            ],
        ),
        (
            3,
            [
                10, 15, 20, 25, 30, 35, 40, 40, 40, 35, 30, 25, 20, 20, 20, 25,
            ],
        ),
    ] {
        for pcm8 in [false, true] {
            for sign in [-1i16, 1] {
                let mut s = Scsp::default();
                let mut ram = vec![0; SOUND_RAM_SIZE];
                for (i, value) in [10, 20, 30, 40].into_iter().enumerate() {
                    if pcm8 {
                        ram[0x100 + i] = (value * sign) as i8 as u8;
                    } else {
                        ram[0x100 + i * 2..0x102 + i * 2]
                            .copy_from_slice(&(value * sign * 256).to_be_bytes());
                    }
                }
                direct(&mut s, 0, 0x100, mode, pcm8, 0);
                s.write16(4, 1).unwrap();
                s.write16(16, 0x7800).unwrap();
                for _ in 0..expected.len() {
                    s.sample(&mut ram).unwrap();
                }
                let actual: Vec<_> = s
                    .drain(expected.len())
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|p| p[0])
                    .collect();
                let expected: Vec<_> = expected.into_iter().map(|v| v * sign * 256).collect();
                assert_eq!(actual, expected, "mode={mode}, pcm8={pcm8}, sign={sign}");
            }
        }
    }
}

#[test]
fn total_level_held_attack_amplitude_lfo_and_direct_bypass_affect_real_stereo() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    for chunk in ram[0x100..0x108].as_chunks_mut::<2>().0 {
        chunk.copy_from_slice(&16384i16.to_be_bytes());
    }
    direct(&mut scsp, 0, 0x100, 1, false, 0);
    scsp.write16(12, 16).unwrap(); // SDIR=0, TL16=1/2
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [0, 0]); // zero AR stays at full attenuation
    scsp.write16(8, 0x20).unwrap(); // EGHOLD=1, AR=0: held unity attack
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [8192, 8192]);
    // Square ALFO held at reset starts at 0dB, then reaches 24dB attenuation.
    scsp.write16(18, 0x8000 | (31 << 10) | (1 << 3) | 7)
        .unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [8192, 8192]);
    scsp.write16(18, (31 << 10) | (1 << 3) | 7).unwrap();
    for _ in 0..130 {
        scsp.sample(&mut ram).unwrap();
    }
    let audio = scsp.drain(130);
    assert_eq!(&audio[..2], &[8192, 8192]);
    // 255/256 of four binary octaves attenuation: about 517.6.
    assert!((517..=519).contains(&audio[audio.len() - 2]));
    assert_eq!(audio[audio.len() - 2], audio[audio.len() - 1]);
    scsp.write16(12, 0x1FF).unwrap(); // direct bypasses TL255 and ALFO
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [16384, 16384]);
}

#[test]
fn fm_carrier_reads_executing_muted_modulator_with_generation_and_inhibit_controls() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    for chunk in ram[0x100..0x108].as_chunks_mut::<2>().0 {
        chunk.copy_from_slice(&8192i16.to_be_bytes());
    }
    for i in 0..64 {
        ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&((i as i16) * 256).to_be_bytes());
    }
    direct(&mut scsp, 0, 0x100, 1, false, 0);
    scsp.write16(22, 0).unwrap(); // modulator is audible only through FM
    direct(&mut scsp, 5, 0x200, 1, false, 0);
    scsp.write16(5 * 32 + 6, 64).unwrap();
    scsp.write16(5 * 32 + 14, 0x5000 | (0x3B << 6) | 0x3B)
        .unwrap();
    for _ in 0..3 {
        scsp.sample(&mut ram).unwrap();
    }
    assert_eq!(scsp.drain(3), [2048, 2048, 2304, 2304, 2560, 2560]);
    // 8192 -> -8192 shifts the carrier by -8 samples instead of +8.
    for chunk in ram[0x100..0x108].as_chunks_mut::<2>().0 {
        chunk.copy_from_slice(&(-8192i16).to_be_bytes());
    }
    for _ in 0..6 {
        scsp.sample(&mut ram).unwrap();
    }
    let audio = scsp.drain(6);
    assert_eq!(&audio[audio.len() - 2..], &[0, 0]); // carrier phase8 minus8
    scsp.write16(12, 0x300).unwrap(); // STWINH: preserve modulator stack data
    for chunk in ram[0x100..0x108].as_chunks_mut::<2>().0 {
        chunk.copy_from_slice(&16384i16.to_be_bytes());
    }
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [256, 256]); // preserved -8 shift, not new +16
    assert_eq!(scsp.voices[5].phase, 10 << 16); // FM never changes the phase generator
}

#[test]
fn internal_noise_and_zero_sources_execute_and_noise_ignores_lfo_reset_frequency() {
    fn samples(control: u16, source: u16) -> Vec<i16> {
        let mut scsp = Scsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        direct(&mut scsp, 0, 0x100, 1, false, 0);
        scsp.write16(18, control).unwrap();
        scsp.write16(0, 0x1820 | (source << 7)).unwrap();
        for _ in 0..16 {
            scsp.sample(&mut ram).unwrap();
        }
        scsp.drain(16)
    }
    let noise = samples(0, 1);
    assert_eq!(noise, samples(0xFC00, 1));
    assert_eq!(
        &noise[..8],
        &[-21279, -21279, -7568, -7568, 28984, 28984, 14492, 14492]
    );
    assert_eq!(samples(0, 2), [0; 32]);
    assert!(noise.as_chunks::<2>().0.iter().all(|s| s[0] == s[1]));
}

#[test]
fn midi_serial_deadlines_irq_and_reset_are_independent_of_host_consumption() {
    for chunk in [1, 7, 1000, 10000] {
        let mut sound = Sound::default();
        for byte in [0x90, 0x40, 0x7F, 0x80] {
            sound.scsp.write16(0x406, byte).unwrap();
        }
        sound.scsp.write16(0x41E, 0x200).unwrap();
        sound.scsp.write16(0x42A, 0x200).unwrap();
        sound.scsp.write16(0x424, 0x80).unwrap();
        sound.scsp.write16(0x426, 0x80).unwrap();
        // Four bytes * ten bits / 31250 Hz, rounded once at the deadline.
        let mut left = 34_398;
        while left != 0 {
            let n = left.min(chunk);
            sound.advance(n).unwrap();
            left -= n;
        }
        assert_eq!(sound.scsp.interrupt_level(), 0);
        assert!(!sound.scsp.main_interrupt());
        assert_eq!(sound.scsp.take_midi_output(), Some(0x90));
        assert_eq!(sound.scsp.take_midi_output(), Some(0x40));
        assert_eq!(sound.scsp.take_midi_output(), Some(0x7F));
        assert_eq!(sound.scsp.take_midi_output(), None);
        sound.advance(1).unwrap();
        assert_eq!(sound.scsp.interrupt_level(), 3);
        assert!(sound.scsp.main_interrupt());
        assert_eq!(sound.scsp.read8(0x404).unwrap() & 0x18, 8);
        assert_eq!(sound.scsp.take_midi_output(), Some(0x80));
        sound.scsp.write16(0x422, 0x200).unwrap();
        assert_eq!(sound.scsp.interrupt_level(), 0);
        assert!(sound.scsp.main_interrupt());
        sound.scsp.write16(0x42E, 0x200).unwrap();
        sound.scsp.write16(0x406, 0xA5).unwrap();
        sound.advance(8599).unwrap();
        assert_eq!(sound.scsp.take_midi_output(), None);
        sound.scsp = Scsp {
            generated: sound.scsp.generated,
            ..Scsp::default()
        }; // MC68k RESET preserves the sample clock.
        sound.advance(10000).unwrap();
        assert_eq!(sound.scsp.take_midi_output(), None);
        assert_eq!(sound.scsp.interrupt_level(), 0);
    }
}

#[test]
fn scsp_dma_zero_execute_write_preserves_busy_and_completion_interrupt() {
    // ST-077 §4.2 DEXE/table4.40: a zero write cannot clear busy early.
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x100..0x108].copy_from_slice(&[0, 0xAA, 0, 0x33, 0, 0x77, 0x11, 0x22]);
    scsp.write16(0x412, 0x100).unwrap();
    scsp.write16(0x414, 0x10).unwrap();
    scsp.write16(0x416, 0x1008).unwrap();
    scsp.write16(0x42A, 16).unwrap();
    scsp.sample_cycle(0, &mut ram).unwrap();
    for n in 0..4 {
        scsp.write16(0x416, 8).unwrap();
        assert_eq!(scsp.read16(0x416).unwrap() & 0x1000, 0x1000);
        assert!(!scsp.main_interrupt());
        assert_eq!(scsp.read16(0x420).unwrap() & 16, 0);
        assert_eq!(scsp.dma_remaining, 8 - n * 2);
        scsp.sample_cycle(n as usize + 1, &mut ram).unwrap();
    }
    assert_eq!(scsp.read16(0x416).unwrap() & 0x1000, 0);
    assert!(scsp.main_interrupt());
    assert_eq!(scsp.read16(0x420).unwrap() & 16, 16);
    for (at, value) in [(0x10, 0xAA), (0x12, 0x33), (0x14, 0x77), (0x16, 0x1122)] {
        assert_eq!(scsp.read16(at), Ok(value));
    }
}

#[test]
fn slot_monitor_uses_documented_ca_bits_all_selectors_and_independent_byte_lanes() {
    // ST-077 §4.1 fig.4.3 / §4.2 p.89. CA occupies bits10:7;
    // write-only MSLC is bits15:11. One CA unit is 4096 samples,
    // independent of PCM width, SA, pitch and the other slot selectors.
    for pcm8 in [false, true] {
        let mut scsp = Scsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        for slot in 0..32 {
            direct(&mut scsp, slot, 0x1234, 1, pcm8, 0);
            scsp.write16(slot * 32 + 6, 65535).unwrap();
            scsp.voices[slot as usize].phase = i64::from(slot % 16 * 4096 + 123) << 16;
        }
        scsp.sample(&mut ram).unwrap();
        for slot in 0..32 {
            let expected = ((slot % 16) << 7) as u16;
            scsp.write16(0x408, (slot << 11) as u16).unwrap();
            assert_eq!(scsp.read16(0x408), Ok(expected));
            assert_eq!(scsp.read8(0x408), Ok((expected >> 8) as u8));
            assert_eq!(scsp.read8(0x409), Ok(expected as u8));
            // A low-byte write must preserve the latched MSLC, rather than
            // copying CA's read value back into the write-only selector.
            scsp.write8(0x409, 0).unwrap();
            assert_eq!(scsp.read16(0x408), Ok(expected));
            assert_eq!(scsp.common[4], (slot << 11) as u16);
            scsp.write8(0x408, (((31 - slot) << 3) & 255) as u8)
                .unwrap();
            assert_eq!(scsp.read16(0x408), Ok((((31 - slot) % 16) << 7) as u16));
        }
    }
}

#[test]
fn slot_monitor_retains_the_current_output_address_across_pitch_and_loop_boundaries() {
    // Read before/after output at the 4K boundary. Looking at the phase
    // after its increment reports CA too early. Forward/reverse/alternate
    // loops must report the address actually used after loop normalization.
    for pcm8 in [false, true] {
        for (mode, samples, expected) in [
            (0, 4, [0, 0x80, 0x80, 0x80]),
            (1, 4, [0, 0x80, 0, 0x80]),
            (2, 4, [0x80, 0, 0x80, 0]),
            (3, 4, [0, 0x80, 0x80, 0]),
        ] {
            let mut scsp = Scsp::default();
            let mut ram = vec![0; SOUND_RAM_SIZE];
            direct(&mut scsp, 0, 0x1234, mode, pcm8, 0);
            scsp.write16(4, 4095).unwrap();
            scsp.write16(6, 4097).unwrap();
            scsp.voices[0].phase = 4095 << 16;
            for value in expected.into_iter().take(samples) {
                scsp.sample(&mut ram).unwrap();
                assert_eq!(scsp.read16(0x408), Ok(value), "PCM8={pcm8}, loop={mode}");
                assert_eq!(scsp.read16(0x408), Ok(value)); // no read side effect
            }
            // A restarted key begins at SA rather than retaining the prior
            // voice's monitored address. Power/LSI reset also clears it.
            scsp.write16(0, 0x1000).unwrap();
            scsp.write16(0, 0x1820 | if pcm8 { 16 } else { 0 }).unwrap();
            assert_eq!(scsp.read16(0x408), Ok(0));
            assert_eq!(Scsp::default().read16(0x408), Ok(0));
        }
    }
}

#[test]
fn slot_monitor_follows_the_fm_waveform_address_without_moving_the_phase_generator() {
    // ST-077 table4.17: MDL5 with a half-scale past-slot source displaces
    // the waveform by sixteen samples, crossing the 4K monitor boundary.
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    direct(&mut scsp, 5, 0, 1, false, 0);
    scsp.write16(5 * 32 + 6, 8192).unwrap();
    scsp.write16(5 * 32 + 14, 0x5000).unwrap();
    scsp.voices[5].phase = 4095 << 16;
    scsp.stack.data.fill(16384);
    scsp.sample(&mut ram).unwrap();
    scsp.write16(0x408, 5 << 11).unwrap();
    assert_eq!(scsp.read16(0x408), Ok(0x80));
    assert_eq!(scsp.voices[5].phase, 4096 << 16);
}
