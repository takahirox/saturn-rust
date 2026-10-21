//! Golden vectors are authored from ST-097 §§2–4, corrected by ST-210.
use super::*;
use crate::bus::{Bus, HIGH_WRAM};

#[test]
fn interrupt_priority_mask_ack_and_external_gate_match_table_2_1() {
    let mut scu = Scu::default();
    for bit in [11, 10, 9, 8, 7, 5, 4, 3, 2, 1, 0] {
        scu.signal(bit);
    }
    assert_eq!(scu.interrupt(), None);
    scu.write32(SCU + 0xA0, 0).unwrap();
    for (level, vector) in [
        (15, 0x40),
        (14, 0x41),
        (13, 0x42),
        (12, 0x43),
        (11, 0x44),
        (10, 0x45),
        (8, 0x47),
        (8, 0x48),
        (6, 0x49),
        (6, 0x4A),
        (5, 0x4B),
    ] {
        assert_eq!(scu.interrupt(), Some((level, vector)));
        scu.acknowledge(vector);
    }
    assert_eq!(scu.read32(SCU + 0xA4).unwrap(), 0);
    scu.signal(16);
    assert_eq!(scu.interrupt(), None);
    scu.write32(SCU + 0xA8, 1).unwrap();
    scu.signal(16);
    scu.signal(17);
    assert_eq!(scu.read32(SCU + 0xA8).unwrap(), 0);
    assert_eq!(scu.interrupt(), Some((7, 0x50)));
    scu.acknowledge(0x50);
    assert_eq!(scu.interrupt(), None);
    // Final silicon removes these functions; do not implement the obsolete maps.
    for offset in [0x60, 0x70, 0x74, 0x78, 0x7C, 0xAC] {
        assert!(scu.read32(SCU + offset).is_err());
        assert!(scu.write32(SCU + offset, 0).is_err());
    }
    assert!(scu.write32(SCU + 0xA4, 0).is_err()); // ST-210 #7
    assert!(scu.read32(SCU + 0xA0).is_err()); // IMS write-only
}

fn set_dma(
    s: &mut Scu,
    channel: u32,
    source: u32,
    destination: u32,
    count: u32,
    add: u32,
    mode: u32,
) {
    for (offset, value) in [
        (0, source),
        (4, destination),
        (8, count),
        (12, add),
        (20, mode),
        (16, 0x101),
    ] {
        s.write32(SCU + channel * 32 + offset, value).unwrap();
    }
}
fn drain(s: &mut Scu, b: &mut Bus) {
    for _ in 0..1000 {
        if !s.channels.iter().any(|c| c.pending()) {
            return;
        }
        s.advance_dma(b).unwrap();
    }
    panic!("SCU DMA did not finish");
}

#[test]
fn direct_dma_all_channels_unaligned_tails_and_b_bus_halfword_strides() {
    for channel in 0..3 {
        let mut s = Scu::default();
        let mut b = Bus::default();
        for i in 0..40 {
            b.write8(HIGH_WRAM + i, i as u8).unwrap();
        }
        // ST-097 fig.2.1: unaligned first/last bytes retain their exact order.
        set_dma(
            &mut s,
            channel,
            HIGH_WRAM + 1,
            HIGH_WRAM + 0x105,
            17,
            0x102,
            0x10107,
        );
        drain(&mut s, &mut b);
        for i in 0..17 {
            assert_eq!(b.read8(HIGH_WRAM + 0x105 + i).unwrap(), i as u8 + 1);
        }
        assert_eq!(s.read32(SCU + channel * 32).unwrap(), HIGH_WRAM + 18);
        assert_eq!(s.read32(SCU + channel * 32 + 4).unwrap(), HIGH_WRAM + 0x116);
        assert_eq!(s.status, 1 << (11 - channel));
        let mut s = Scu::default();
        set_dma(
            &mut s,
            channel,
            HIGH_WRAM,
            crate::devices::VRAM + 8,
            8,
            0x105,
            7,
        );
        drain(&mut s, &mut b);
        for (i, expected) in [0x0001, 0x0203, 0x0405, 0x0607].into_iter().enumerate() {
            assert_eq!(
                b.read16(crate::devices::VRAM + 8 + i as u32 * 32).unwrap(),
                expected
            );
        }
    }
}

#[test]
fn indirect_table_uses_final_spec_order_and_end_in_read_address() {
    let mut s = Scu::default();
    let mut b = Bus::default();
    b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
    b.write32(HIGH_WRAM + 0x104, 0x9ABCDEF0).unwrap();
    for (i, value) in [
        4,
        crate::devices::VRAM,
        HIGH_WRAM + 0x100,
        4,
        crate::devices::VRAM + 0x20,
        0x80000000 | (HIGH_WRAM + 0x104),
    ]
    .into_iter()
    .enumerate()
    {
        b.write32(HIGH_WRAM + i as u32 * 4, value).unwrap();
    }
    set_dma(&mut s, 0, 0, HIGH_WRAM, 0, 0x101, 0x01000107);
    drain(&mut s, &mut b);
    assert_eq!(b.read32(crate::devices::VRAM).unwrap(), 0x12345678);
    assert_eq!(b.read32(crate::devices::VRAM + 0x20).unwrap(), 0x9ABCDEF0);
    assert_eq!(s.read32(SCU + 4).unwrap(), HIGH_WRAM + 24);
    assert_eq!(s.status, 1 << 11);
}

#[test]
fn dma_preempts_resumes_retriggers_once_and_rejects_prohibited_regions() {
    let mut s = Scu::default();
    let mut b = Bus::default();
    b.write32(HIGH_WRAM, 0x12345678).unwrap();
    set_dma(&mut s, 0, HIGH_WRAM, crate::devices::VRAM, 4, 0x101, 7);
    s.advance_dma(&mut b).unwrap();
    set_dma(&mut s, 2, HIGH_WRAM, crate::devices::VRAM + 16, 4, 0x101, 7);
    while s.channels[2].pending() {
        s.advance_dma(&mut b).unwrap();
    }
    assert_eq!(b.read32(crate::devices::VRAM).unwrap(), 0);
    assert_eq!(b.read32(crate::devices::VRAM + 16).unwrap(), 0x12345678);
    drain(&mut s, &mut b);
    assert_eq!(b.read32(crate::devices::VRAM).unwrap(), 0x12345678);
    let mut s = Scu::default();
    set_dma(
        &mut s,
        1,
        HIGH_WRAM,
        crate::devices::VRAM + 0x100,
        4,
        0x101,
        0x100,
    ); // VBlank-in, update destination
    s.signal(0);
    s.advance_dma(&mut b).unwrap();
    s.signal(0);
    s.signal(0);
    drain(&mut s, &mut b);
    assert_eq!(s.read32(SCU + 0x24).unwrap(), crate::devices::VRAM + 0x108);
    assert_eq!(b.read32(crate::devices::VRAM + 0x104).unwrap(), 0x12345678);
    for (src, dst) in [
        (crate::bus::LOW_WRAM, crate::devices::VRAM),
        (crate::devices::VRAM, HIGH_WRAM),
        (HIGH_WRAM, 0x02000000),
    ] {
        let mut s = Scu::default();
        set_dma(&mut s, 0, src, dst, 4, 0x102, 7);
        drain(&mut s, &mut b);
        assert_eq!(s.status, 1 << 12);
        assert!(!s.channels[0].pending());
    }
}

fn run_dsp(program: &[u32], data: &[u32]) -> dsp::Dsp {
    let mut d = dsp::Dsp::default();
    d.set_control(0x8000);
    for word in program {
        d.write_program(*word, SCU + 0x84).unwrap();
    }
    d.set_data_address(0, SCU + 0x88).unwrap();
    for word in data {
        d.write_data(*word, SCU + 0x8C).unwrap();
    }
    d.set_control(0x8000);
    d.set_control(0x10000);
    for _ in 0..100 {
        d.step().unwrap();
        if !d.running() {
            return d;
        }
    }
    panic!("DSP did not terminate");
}

#[test]
fn dsp_parallel_buses_compute_signed_product_and_accumulate_with_old_operands() {
    // X=M0,Y=M1; next cycle P=X*Y,A=0; next AD2,ALU->A;
    // next parallel ADD old A+P -> data RAM2 and ALU->A; ENDI.
    let d = run_dsp(
        &[0x02084000, 0x01020000, 0x18040000, 0x10043209, 0xF8000000],
        &[0xFFFF_FFF9],
    );
    // RAM1 defaults to 0; rerun with a nonzero second bank through host port.
    assert_eq!(d.control() & 0x10000, 0);
    let mut d = dsp::Dsp::default();
    for (bank, value) in [(0, 0xFFFF_FFF9), (1, 6)] {
        d.set_data_address(bank * 64, SCU + 0x88).unwrap();
        d.write_data(value, SCU + 0x8C).unwrap();
    }
    for word in [0x02084000, 0x01020000, 0x18040000, 0x10043209, 0xF8000000] {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x8000);
    d.set_control(0x10000);
    for _ in 0..5 {
        d.step().unwrap();
    }
    d.set_data_address(128, SCU + 0x88).unwrap();
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), (-84i32) as u32);
    let flags = d.control();
    assert_eq!(flags & (1 << 22), 1 << 22);
    assert_ne!(flags & (1 << 18), 0);
    assert_eq!(d.control() & (1 << 18), 0); // E read-to-clear
}

#[test]
fn dsp_alu_golden_results_carry_sign_zero_and_48_bit_accumulation() {
    // Load A from M0, P from M1, then chosen ALU->A and ALL->RAM2 in parallel.
    for (op, a, p, result, flags) in [
        (1, 0xF0F0F0F0, 0x0F0FF00F, 0x0000F000, 0),
        (2, 0x80000000, 1, 0x80000001, 1 << 22),
        (3, 0xFFFFFFFF, 0xFFFFFFFF, 0, 1 << 21),
        (4, 0xFFFFFFFF, 1, 0, (1 << 21) | (1 << 20)),
        (4, 0x7FFFFFFF, 1, 0x80000000, (1 << 22) | (1 << 19)),
        (5, 0, 1, 0xFFFFFFFF, (1 << 22) | (1 << 20)),
        (8, 0x80000001, 0, 0xC0000000, (1 << 22) | (1 << 20)),
        (9, 0x80000001, 0, 0xC0000000, (1 << 22) | (1 << 20)),
        (10, 0x80000001, 0, 2, 1 << 20),
        (11, 0x80000001, 0, 3, 1 << 20),
        (15, 0x01020304, 0, 0x02030401, 1 << 20),
    ] {
        let mut d = dsp::Dsp::default();
        for (bank, value) in [(0, a), (1, p)] {
            d.set_data_address(bank * 64, SCU + 0x88).unwrap();
            d.write_data(value, SCU + 0x8C).unwrap();
        }
        for word in [0x01960000, op << 26 | 0x00043209, 0xF0000000] {
            d.write_program(word, SCU + 0x84).unwrap();
        }
        d.set_control(0x8000);
        d.set_control(0x10000);
        for _ in 0..3 {
            d.step().unwrap();
        }
        d.set_data_address(128, SCU + 0x88).unwrap();
        assert_eq!(d.read_data(SCU + 0x8C).unwrap(), result, "ALU {op}");
        assert_eq!(d.control() & 0x780000, flags, "flags {op}");
    }
    // Sign-extended -1 + 1 in AD2 clears all 48 bits, sets carry and zero.
    let d = run_dsp(
        &[0x94000001, 0x00060000, 0x18040000, 0xF0000000],
        &[0xFFFFFFFF],
    );
    assert_eq!(d.control() & 0x700000, 0x300000);
}

#[test]
fn dsp_delayed_jumps_conditional_immediates_btm_and_lps_execute_real_instructions() {
    // MVI #3,LOP; LPS; MOV #7,MC0 repeated 4 times; JMP 7;
    // delay MOV #-1,MC1; skipped MOV #9,MC1; ENDI.
    let mut d = run_dsp(
        &[
            0xA8000003, 0xE8000000, 0x00001007, 0xD0000007, 0x000011FF, 0x00001109, 0, 0xF8000000,
        ],
        &[],
    );
    d.set_data_address(0, SCU + 0x88).unwrap();
    for _ in 0..4 {
        assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 7);
    }
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 0);
    d.set_data_address(64, SCU + 0x88).unwrap();
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 0xFFFFFFFF);
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 0);
    // Establish Z with XOR, conditional MVI Z writes, NZ does not.
    let mut d = run_dsp(&[0x0C000000, 0x8308002A, 0x86080063, 0xF0000000], &[]);
    d.set_data_address(0, SCU + 0x88).unwrap();
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 42);
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 0);
    // BTM executes its prefetched successor before returning TOP.
    let mut d = run_dsp(
        &[
            0xA8000002, 0x00001B02, 0x00001006, 0xE0000000, 0x00001107, 0xF0000000,
        ],
        &[],
    );
    for (bank, value) in [(0, 6), (1, 7)] {
        d.set_data_address(bank * 64, SCU + 0x88).unwrap();
        for _ in 0..3 {
            assert_eq!(d.read_data(SCU + 0x8C).unwrap(), value);
        }
        assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 0);
    }
}

#[test]
fn dsp_ports_pause_step_wrap_and_illegal_commands_are_observable() {
    let mut d = dsp::Dsp::default();
    d.set_control(0x80FF);
    d.write_program(0x0000102A, SCU + 0x84).unwrap();
    d.write_program(0xF0000000, SCU + 0x84).unwrap();
    d.set_control(0x80FF);
    d.set_control(0x20000);
    d.step().unwrap();
    assert_eq!(d.control() & 255, 0);
    d.set_data_address(0, SCU + 0x88).unwrap();
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 42);
    d.set_control(0x2010000);
    let before = d.control() & 255;
    d.step().unwrap();
    assert_eq!(d.control() & 255, before);
    assert!(d.write_program(0, SCU + 0x84).is_err());
    assert!(d.read_data(SCU + 0x8C).is_err());
    d.set_control(0x4010000);
    d.step().unwrap();
    assert_eq!(d.control() & 0x10000, 0);
    d.set_data_address(63, SCU + 0x88).unwrap();
    d.write_data(1, SCU + 0x8C).unwrap();
    d.write_data(2, SCU + 0x8C).unwrap();
    d.set_data_address(0, SCU + 0x88).unwrap();
    assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 2);
    let mut d = dsp::Dsp::default();
    d.write_program(0x40000000, SCU + 0x84).unwrap();
    d.set_control(0x8000);
    d.set_control(0x10000);
    assert_eq!(
        d.step(),
        Err(ScuError::IllegalDsp {
            pc: 0,
            instruction: 0x40000000
        })
    );
}

#[test]
fn raster_timers_zero_means_512_dots_no_reload_while_running_and_line_selection() {
    let mut s = Scu::default();
    s.write32(SCU + 0x90, 2).unwrap();
    s.write32(SCU + 0x94, 0).unwrap();
    s.write32(SCU + 0x98, 0x101).unwrap();
    s.hblank_in();
    s.advance_clocks(1708).unwrap();
    assert_eq!(s.status & 0x18, 0);
    s.hblank_in();
    s.advance_clocks(339).unwrap();
    assert_eq!(s.status & 0x18, 8);
    s.advance_clocks(1).unwrap();
    assert_eq!(s.status & 0x18, 0x18);
    s.acknowledge(0x44);
    s.hblank_in();
    s.advance_clocks(2048).unwrap();
    assert_eq!(s.status & 0x10, 0); // compare-line mode suppresses next line
    s.write32(SCU + 0x98, 1).unwrap();
    s.hblank_in();
    s.advance_clocks(2048).unwrap();
    assert_eq!(s.status & 0x10, 0x10);
    s.write32(SCU + 0x98, 0).unwrap();
    s.acknowledge(0x44);
    s.hblank_in();
    s.advance_clocks(10000).unwrap();
    assert_eq!(s.status & 0x10, 0);
    s.write32(SCU + 0x90, 0).unwrap();
    s.write32(SCU + 0x98, 1).unwrap();
    s.vblank_out();
    assert_eq!(s.status & 8, 8);
}

#[test]
fn dsp_dma_input_output_hold_and_end_continuation_use_real_bus_memory() {
    let mut d = dsp::Dsp::default();
    let mut b = Bus::default();
    for (i, value) in [7, 11, 13, 17].into_iter().enumerate() {
        b.write32(HIGH_WRAM + 0x5000 + i as u32 * 4, value).unwrap();
    }
    let program = [
        0x99801400, 0xC0008004, 0xD3400002, 0, 0x00001C00, 0x00020000, 0x01C00000, 0x10040000,
        0x01C00000, 0x10040000, 0x01C00000, 0x10040000, 0x01C00000, 0x10040000, 0x00003109,
        0x9D801800, 0x00001D00, 0xC000D101, 0xF8000000,
    ]; // DMAH output + immediate END
    for word in program {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x8000);
    d.set_control(0x10000);
    let mut irq = 0;
    for _ in 0..100 {
        irq += u32::from(d.step().unwrap());
        d.dma_beat(&mut b).unwrap();
        if !d.running() {
            break;
        }
    }
    assert_eq!(irq, 1);
    assert!(!d.running());
    assert_eq!(b.read32(HIGH_WRAM + 0x6000).unwrap(), 48);
    // Hold retained WA0: a second DMA from a different RAM value overwrites same word.
    d.set_data_address(64, SCU + 0x88).unwrap();
    d.write_data(0x12345678, SCU + 0x8C).unwrap();
    d.set_control(0x8000);
    for word in [0x00001D00, 0xC0009101, 0xF0000000] {
        d.write_program(word, SCU + 0x84).unwrap();
    }
    d.set_control(0x8000);
    d.set_control(0x10000);
    for _ in 0..4 {
        d.step().unwrap();
        d.dma_beat(&mut b).unwrap();
    }
    assert_eq!(b.read32(HIGH_WRAM + 0x6000).unwrap(), 0x12345678);
    assert_eq!(b.read32(HIGH_WRAM + 0x6004).unwrap(), 0);
}

#[test]
fn dsp_a_bus_output_fixed_increment_hold_and_register_counts_have_real_address_effects() {
    // ST-097 §4.5 pp.134/136/138/140: add0/add1 are both legal;
    // DMAH retains WA0, while ordinary DMA updates it by the selected add.
    for add in [0u32, 1] {
        for hold in [false, true] {
            for counter in [3, 0x2000, 0x2004] {
                // immediate, M0, MC0
                let mut d = dsp::Dsp::default();
                let mut b = Bus::default();
                d.set_data_address(0, SCU + 0x88).unwrap();
                d.write_data(3, SCU + 0x8C).unwrap();
                d.set_data_address(64, SCU + 0x88).unwrap();
                for value in [7, 11, 17] {
                    d.write_data(value, SCU + 0x8C).unwrap();
                }
                let command = 0xC0001100 | add << 15 | u32::from(hold) << 14 | counter;
                for word in [0x9D800400, command, 0xF8000000] {
                    // WA0=06001000/4
                    d.write_program(word, SCU + 0x84).unwrap();
                }
                d.set_control(0x18000);
                let mut irq = 0;
                for _ in 0..20 {
                    irq += u32::from(d.step().unwrap());
                    d.dma_beat(&mut b).unwrap();
                }
                assert_eq!(irq, 1);
                assert!(!d.running());
                assert_eq!(
                    b.read32(HIGH_WRAM + 0x1000).unwrap(),
                    if add == 0 { 17 } else { 7 }
                );
                assert_eq!(
                    b.read32(HIGH_WRAM + 0x1004).unwrap(),
                    if add == 0 { 0 } else { 11 }
                );
                assert_eq!(
                    b.read32(HIGH_WRAM + 0x1008).unwrap(),
                    if add == 0 { 0 } else { 17 }
                );
                // Continue using WA0 without rewriting it. Reset CT1 and copy
                // one word with add1. HOLD and add0 both retain original WA0.
                d.set_control(0x8000);
                for word in [0x00001D00, 0xC0009101, 0xF0000000] {
                    d.write_program(word, SCU + 0x84).unwrap();
                }
                d.set_control(0x18000);
                for _ in 0..10 {
                    d.step().unwrap();
                    d.dma_beat(&mut b).unwrap();
                }
                let destination = HIGH_WRAM + 0x1000 + if hold { 0 } else { 12 * add };
                assert_eq!(b.read32(destination).unwrap(), 7);
            }
        }
    }
}

#[test]
fn every_dsp_condition_selects_immediate_and_delayed_jump_from_golden_flags() {
    // ST-097 §4.5 pp.122–131/143–152: polarity bit 24 for JMP and bit 24
    // for conditional MVI, with independent condition fields below. Page 123
    // duplicates Z's pattern for NZ; NC/NS/NZS establish the clear-polarity rule.
    let cases = [
        (0x21, 0x61, [true, false, true]),
        (0x01, 0x41, [false, true, false]), // Z/NZ
        (0x22, 0x62, [false, true, false]),
        (0x02, 0x42, [true, false, true]), // S/NS
        (0x23, 0x63, [true, true, true]),
        (0x03, 0x43, [false, false, false]), // ZS/NZS
        (0x24, 0x64, [false, false, true]),
        (0x04, 0x44, [true, true, false]), // C/NC
        (0x28, 0x68, [false, false, false]),
        (0x08, 0x48, [true, true, true]), // T0/NT0
    ];
    for (mvi_condition, jump_condition, expected) in cases {
        for (i, (a, p)) in [(0, 0), (0xFFFFFFFF, 0), (0xFFFFFFFF, 1)]
            .into_iter()
            .enumerate()
        {
            let mut d = run_dsp(
                &[
                    0x94000000 | p,
                    0x00060000,
                    0x10040000,
                    0x8A00002A | (mvi_condition << 19),
                    0xD0000008 | (jump_condition << 19),
                    0x00001307,
                    0x00001309,
                    0xF0000000,
                    0xF0000000,
                ],
                &[a],
            );
            d.set_data_address(128, SCU + 0x88).unwrap();
            assert_eq!(
                d.read_data(SCU + 0x8C).unwrap(),
                if expected[i] { 42 } else { 0 }
            );
            d.set_data_address(192, SCU + 0x88).unwrap();
            assert_eq!(d.read_data(SCU + 0x8C).unwrap(), 7); // prefetched successor always executes
            assert_eq!(
                d.read_data(SCU + 0x8C).unwrap(),
                if expected[i] { 0 } else { 9 }
            );
        }
    }
}

#[test]
fn parallel_ram_sources_observe_one_old_address_and_increment_each_bank_once() {
    // All three buses read MC0 in the same operation. They sample -7 together;
    // X*Y =49 and a later M0 reads the next word (11), not a third/fourth word.
    let mut d = run_dsp(
        &[
            0x02493104, 0x01020000, 0x18040000, 0x00003209, 0x00003300, 0xF0000000,
        ],
        &[0xFFFFFFF9, 11],
    );
    for (address, expected) in [(64, 0xFFFFFFF9), (128, 49), (192, 11)] {
        d.set_data_address(address, SCU + 0x88).unwrap();
        assert_eq!(d.read_data(SCU + 0x8C).unwrap(), expected);
    }
}

#[test]
fn higher_dma_channel_can_preempt_between_dsp_b_bus_halves_without_losing_the_word() {
    let mut s = Scu::default();
    let mut b = Bus::default();
    s.write32(SCU + 0x88, 0).unwrap();
    s.write32(SCU + 0x8c, 0x11223344).unwrap();
    for word in [0x9d700400, 0xc0009001, 0xf8000000] {
        s.write32(SCU + 0x84, word).unwrap();
    }
    s.write32(SCU + 0x80, 0x18000).unwrap();
    s.advance_clocks(6).unwrap(); // WA0=05C01000/4, DMA1 M0,D0,1, ENDI.
    s.advance_dma(&mut b).unwrap();
    assert_eq!(b.read32(crate::vdp1::VRAM + 0x1000).unwrap(), 0x11220000);
    b.write32(HIGH_WRAM, 0x55667788).unwrap();
    set_dma(&mut s, 0, HIGH_WRAM, HIGH_WRAM + 0x100, 4, 0x102, 7);
    for _ in 0..3 {
        s.advance_dma(&mut b).unwrap();
        assert_eq!(b.read32(crate::vdp1::VRAM + 0x1000).unwrap(), 0x11220000);
        assert!(s.dsp.dma_pending());
    }
    assert_eq!(b.read32(HIGH_WRAM + 0x100).unwrap(), 0x55667788);
    assert_eq!(s.status & 0x820, 0x820); // ENDI occurred before DMA0 completion.
    s.advance_dma(&mut b).unwrap();
    assert_eq!(b.read32(crate::vdp1::VRAM + 0x1000).unwrap(), 0x11223344);
    assert!(!s.dsp.dma_pending());
}
