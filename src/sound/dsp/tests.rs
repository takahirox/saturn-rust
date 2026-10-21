//! Independent numeric vectors for ST-077 figs.4.5–4.7/5.1, tables4.26/4.29,
//! and ST-228 Reference2's TEMP/sample and sound-memory delay model.
use super::*;
use crate::sound::SOUND_RAM_SIZE;

#[test]
fn coefficient_internal_ram_and_byte_lanes_obey_physical_widths() {
    let mut scsp = Scsp::default();
    for (base, low_mask) in [(0xC00, 255), (0xE00, 255), (0xE80, 15)] {
        scsp.write16(base, 0xFFFF).unwrap();
        scsp.write16(base + 2, 0xA135).unwrap();
        assert_eq!(scsp.read16(base), Ok(low_mask));
        assert_eq!(scsp.read16(base + 2), Ok(0xA135));
        scsp.write8(base + 2, 0x12).unwrap();
        scsp.write8(base + 3, 0x34).unwrap();
        assert_eq!(scsp.read16(base + 2), Ok(0x1234));
        assert_eq!(scsp.read16(base), Ok(low_mask));
    }
    scsp.write16(0x700, 0xFFFF).unwrap();
    assert_eq!(scsp.read16(0x700), Ok(0xFFF8));
    scsp.write8(0x701, 0x23).unwrap();
    assert_eq!(scsp.read16(0x700), Ok(0xFF20));
    scsp.write16(0x402, 0xFFFF).unwrap();
    assert_eq!(scsp.read16(0x402), Ok(0));
    assert_eq!(scsp.common[1], 0x1FF);
    assert!(scsp.write16(0x7C0, 1).is_err());
    assert!(scsp.read16(0xC01).is_err());
}

#[test]
fn multiply_accumulate_uses_signed_13_bit_y_and_previous_accumulator_output() {
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    dsp.mems[0] = 0x120000;
    dsp.coefficients[0] = 0x4000; // +0.5 in signed Q12, left aligned.
    dsp.coefficients[1] = 0xC000; // -0.5.
    dsp.programmed = true;
    dsp.program[0] = [0, 0xA000, 2, 0]; // X=MEMS0, Y=COEF0, B=0
    dsp.program[1] = [0, 0xA000, 0x1001, 0]; // EF0=previous; ACC += half
    dsp.program[2] = [0, 0xA000, 0x1102, 0x0200]; // EF1=sum; ACC=-half
    dsp.program[3] = [0, 0, 0x1202, 0]; // EF2=-half
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(&dsp.effects[..3], &[2304, 4608, -2304]);
}

#[test]
fn temp_pointer_moves_once_per_sample_and_reads_precede_parallel_writes() {
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    dsp.coefficients[0] = 0x4000;
    dsp.programmed = true;
    dsp.program[0] = [0, 0xAC00, 2, 0]; // EXTS0 * .5
    dsp.program[1] = [0x0080, 0, 2, 0]; // previous result -> TEMP0
    dsp.program[2] = [0x0100, 0, 2, 0]; // B=0 (clear accumulator)
    dsp.program[3] = [0x0100, 0x2000, 2, 0]; // previous sample's TEMP1 * .5
    dsp.program[4] = [0, 0, 0x1002, 0];
    for (input, output) in [(8000, 0), (-4000, 2000), (12000, -1000)] {
        dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [input, 0])
            .unwrap();
        assert_eq!(dsp.effects[0], output);
    }
    // A simultaneous TEMP read/write must multiply the old data.
    dsp.decrement = 0;
    dsp.temp[0] = 0x100000;
    dsp.program[0] = [0x0080, 0x2000, 2, 0];
    dsp.program[1] = [0, 0, 0x1102, 0];
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(dsp.effects[1], 2048);
}

#[test]
fn shifter_clips_or_wraps_and_effect_writes_accumulate_with_sixteen_bit_width() {
    for (shift, expected) in [(0, 24576), (1, 32767), (2, -16384), (3, 24576)] {
        let mut dsp = Dsp::default();
        let mut ram = vec![0; SOUND_RAM_SIZE];
        dsp.temp[0] = 0x600000;
        dsp.programmed = true;
        dsp.program[0] = [0, 0, 0, 0]; // ACC=TEMP0
        dsp.program[1] = [0, 0, 0x1000 | (shift << 4), 0];
        dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
            .unwrap();
        assert_eq!(dsp.effects[0], expected);
    }
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    dsp.temp[0] = 0x600000;
    dsp.programmed = true;
    dsp.program[1] = [0, 0, 0x1000, 0];
    dsp.program[2] = [0, 0, 0x1000, 0];
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(dsp.effects[0], -16384); // 24576 + 24576 wraps.
}

#[test]
fn ring_table_linear_reads_use_two_step_latency_and_preserve_read_before_write() {
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    ram[0x4004..0x4006].copy_from_slice(&0x1234u16.to_be_bytes());
    dsp.programmed = true;
    dsp.addresses[0] = 2;
    dsp.program[1] = [0, 0, 0xA002, 0x8000]; // TABLE/MRD, NOFL
    dsp.program[2] = [0, 0x20, 2, 0]; // too early -> old memory latch
    dsp.program[3] = [0, 0x21, 2, 0]; // N+2 -> MEMS1
    dsp.program[4] = [0, 0xA040, 2, 0]; // MEMS1 * .5
    dsp.program[5] = [0, 0, 0x1002, 0];
    dsp.coefficients[0] = 0x4000;
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 2, [0; 2])
        .unwrap(); // RBP2=0x4000 byte base
    assert_eq!(dsp.mems[0], 0);
    assert_eq!(dsp.mems[1], 0x123400);
    assert_eq!(dsp.effects[0], 2330);
    // Ring address mask applies before RBP; NXADR wraps at each RBL.
    for rbl in 0..4 {
        dsp.decrement = 1;
        dsp.addresses[0] = ((0x2000u32 << rbl) - 2) as u16;
        dsp.program[1] = [0, 0, 0x2002, 0x8001];
        ram[0x4000..0x4002].copy_from_slice(&0xA980u16.to_be_bytes());
        dsp.sample(
            &mut ram,
            RamConfig { four_mbit: true },
            2 | (rbl << 7),
            [0; 2],
        )
        .unwrap();
        assert_eq!(dsp.mems[1], -22144 * 256);
    }
    // Simultaneous MRD/MWT returns the old RAM word and stores the shifter.
    dsp.decrement = 0;
    dsp.temp[0] = 0x250000;
    dsp.program[0] = [0, 0, 0, 0];
    dsp.program[1] = [0, 0, 0xE002, 0x8000];
    dsp.addresses[0] = 0;
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 2, [0; 2])
        .unwrap();
    assert_eq!(&ram[0x4000..0x4002], &[0x25, 0]);
    assert_eq!(dsp.mems[1], -22144 * 256);
}

#[test]
fn floating_memory_format_has_independent_bit_patterns_and_bounded_error() {
    for (value, word) in [
        (0, 0x6000),
        (-1, 0xE7FF),
        (0x400000, 0),
        (-0x800000, 0x8000),
        (0x200000, 0x0800),
        (-0x400000, 0x8800),
        (0x7FF800, 0x07FF),
    ] {
        assert_eq!(pack(value), word);
        assert_eq!(unpack(word), value);
    }
    for value in (-0x800000..=0x7FFFFF).step_by(7919) {
        let decoded = unpack(pack(value));
        assert!(decoded <= value && value - decoded <= 2047);
    }
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    dsp.programmed = true;
    dsp.temp[0] = 0x200000;
    dsp.program[1] = [0, 0, 0xC002, 0]; // floating TABLE write
    dsp.program[3] = [0, 0, 0xA002, 0]; // floating TABLE read
    dsp.program[5] = [0, 0x20, 2, 0];
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(&ram[..2], &[8, 0]);
    assert_eq!(dsp.mems[0], 0x200000);
}

#[test]
fn y_fraction_and_address_latches_use_previous_step_values() {
    let mut dsp = Dsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    dsp.mems[0] = 0x200000;
    dsp.programmed = true;
    dsp.program[0] = [0, 0xC000, 10, 0]; // YRL; old Y=0
    dsp.program[1] = [0, 0xC000, 0x1002, 0]; // now Y=0x400
    dsp.program[2] = [0, 0, 0x1102, 0];
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(&dsp.effects[..2], &[0, 2048]);
    // FRCL loads the previous shifter result, consumed next instruction.
    dsp.decrement = 0;
    dsp.temp[0] = 0x200000;
    dsp.program.fill([0; 4]);
    dsp.program[1] = [0, 0, 0x42, 0]; // FRCL=0x400
    dsp.program[2] = [0, 0x8000, 2, 0];
    dsp.program[3] = [0, 0, 0x1002, 0];
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(dsp.effects[0], 2048);
    dsp.program.fill([0; 4]);
    dsp.mems[0] = 0x020000; // integer address 2
    dsp.program[0] = [0, 0, 0x82, 0]; // ADRL
    dsp.program[1] = [0, 0, 0xA002, 0x8002]; // ADREB, TABLE, linear
    dsp.program[3] = [0, 0x20, 2, 0];
    ram[4..6].copy_from_slice(&0x4321u16.to_be_bytes());
    dsp.sample(&mut ram, RamConfig { four_mbit: true }, 0, [0; 2])
        .unwrap();
    assert_eq!(dsp.mems[0], 0x432100);
}

#[test]
fn slot_send_wraps_mix_without_saturation_and_dsp_output_obeys_routing() {
    let mut scsp = Scsp::default();
    let mut ram = vec![0; SOUND_RAM_SIZE];
    scsp.dsp.add_slot(7, 32767);
    scsp.dsp.add_slot(7, 32767);
    assert_eq!(scsp.dsp.mix[0], -32);
    scsp.write16(0x400, 15).unwrap();
    scsp.write16(0x700, 0x4000).unwrap();
    // Half EXTS0 -> EFREG0 -> left only. Half EXTS1 -> EFREG1 -> right only.
    for (i, words) in [
        [0, 0xAC00, 2, 0],
        [0, 0, 0x1002, 0],
        [0, 0xAC40, 2, 0],
        [0, 0, 0x1102, 0],
    ]
    .into_iter()
    .enumerate()
    {
        for (j, word) in words.into_iter().enumerate() {
            scsp.write16(0x800 + (i * 8 + j * 2) as u32, word).unwrap();
        }
    }
    scsp.write16(0x16, 0xFF).unwrap();
    scsp.write16(0x36, 0xEF).unwrap();
    scsp.external = [8000, -12000];
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [4000, -6000]);
    assert_eq!(scsp.read16(0xEC0), Ok(4000));
    assert_eq!(scsp.read16(0xEC2), Ok((-6000i16) as u16));
    scsp.write16(0x16, 0).unwrap();
    scsp.write16(0x36, 0).unwrap();
    scsp.sample(&mut ram).unwrap();
    assert_eq!(scsp.drain(1), [0, 0]);
    assert_eq!(scsp.dsp.mix, [0; 16]);
}

#[test]
fn illegal_memory_steps_and_reserved_ports_fail_without_faking_effects() {
    for words in [
        [0, 0, 0x2000, 0],
        [0, 0, 0x4000, 0],
        [0x8000, 0, 0, 0],
        [0, 0, 0, 0x180],
        [0, 0x8FC0, 0, 0],
    ] {
        let mut dsp = Dsp {
            programmed: true,
            ..Dsp::default()
        };
        dsp.program[0] = words;
        assert!(
            dsp.sample(
                &mut vec![0; SOUND_RAM_SIZE],
                RamConfig { four_mbit: true },
                0,
                [0; 2]
            )
            .is_err()
        );
    }
}
