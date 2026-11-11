//! Differential evidence: ordinary step execution remains an independent scheduler.
use super::*;
use crate::{Boot, Machine, MachineError};

#[test]
fn passive_draw_status_reads_preserve_cpu_bus_waits_restart_and_completion() {
    for fast in [false, true] {
        for width in [2u32, 4] {
            for little in [false, true] {
                for load in [0x6041u16, 0x6042] {
                    let mut bytes = rom(include_str!("../../examples/runtime-poll.hex"));
                    bytes[0x20C..0x20E].copy_from_slice(&load.to_be_bytes());
                    let (mut a, mut b) = pair(&bytes);
                    for m in [&mut a, &mut b] {
                        m.bus.change_system_clock(fast);
                        m.bus.onchip[0]
                            .write32(
                                0xFFFF_FFE4,
                                0xA55A_00CC | (if width == 2 { 2 } else { 3 } << 4),
                            )
                            .unwrap();
                        m.bus.onchip[0]
                            .write32(0xFFFF_FFE0, 0xA55A_03F0 | if little { 0x1000 } else { 0 })
                            .unwrap();
                        for (offset, word) in [
                            (0, 9),
                            (20, 511),
                            (22, 255),
                            (32, 6),
                            (36, 0xC0),
                            (38, 0x801F),
                            (48, 511),
                            (64, 0x8000),
                        ] {
                            m.bus.write16(crate::vdp1::VRAM + offset, word).unwrap();
                        }
                        m.bus.write16(crate::vdp1::REGISTERS + 2, 2).unwrap();
                        m.bus.write16(crate::vdp1::REGISTERS + 4, 1).unwrap();
                    }
                    let mut turns = 0;
                    while a.cpu_state().instructions < 1500 {
                        let count = a.try_cached_batch(128).unwrap();
                        if count == 0 {
                            a.step_inner_with_idle(true).unwrap();
                        }
                        while b.cpu_state().instructions < a.cpu_state().instructions {
                            b.step().unwrap();
                        }
                        same(&mut a, &mut b);
                        a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                        assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                        turns += 1;
                        assert!(turns < 5000);
                    }
                    assert!(
                        a.bus.cached_read_beats > 0,
                        "passive read path must execute"
                    );
                    assert_eq!(b.bus.cached_read_beats, 0);
                    if !little && load == 0x6041 {
                        assert!(
                            a.cpu_state().r[8] > 0,
                            "real draw completion must exit polling and restart drawing"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn uncached_rom_execution_preserves_fetch_beats_waits_and_hardware_edges() {
    for fast in [false, true] {
        for (source, enable) in [
            (include_str!("../../examples/runtime-countdown.hex"), 0x204),
            (include_str!("../../examples/runtime-divide.hex"), 0x204),
            (include_str!("../../examples/runtime-stack.hex"), 0x20E),
            (include_str!("../../examples/runtime-poll-scene.hex"), 0x47E),
        ] {
            let mut bytes = rom(source);
            assert_eq!(&bytes[enable..enable + 2], &0x2100u16.to_be_bytes());
            bytes[enable..enable + 2].copy_from_slice(&0x0009u16.to_be_bytes());
            let (mut a, mut b) = pair(&bytes);
            for m in [&mut a, &mut b] {
                m.bus.change_system_clock(fast);
            }
            for _ in 0..2 {
                a.advance_frame().unwrap();
                reference_frame(&mut b).unwrap();
                same(&mut a, &mut b);
                a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                assert_eq!(a.bus.cache[0].borrow().control() & 1, 0);
                assert_eq!(
                    a.bus.cached_stores, 0,
                    "uncached IF/store arbitration stays on the real bus"
                );
            }
            assert!(a.bus.cached_read_beats > 0);
            assert_eq!(b.bus.cached_read_beats, 0);
        }
    }
}

#[test]
fn uncached_rom_operands_do_not_bypass_alignment_ranges_or_onchip_routes() {
    let source = "00000000 0000 0200 060F F000\n00000024 0000 0300\n\
        00000200 D101 6011 AFFE 0009 FFFF FFFF\n00000300 001B 0009\n";
    for (address, load) in [
        (0xFFFFFFFFu32, 0x6011u16),
        (0x7FFFF, 0x6011),
        (0x80000, 0x6011),
        (0xFFFFFE92, 0x6010),
        (0x7FFFF, 0x6010),
    ] {
        let mut bytes = rom(source);
        bytes[0x208..0x20C].copy_from_slice(&address.to_be_bytes());
        bytes[0x202..0x204].copy_from_slice(&load.to_be_bytes());
        let (mut a, mut b) = pair(&bytes);
        assert_eq!(a.advance_frame(), reference_frame(&mut b));
        same(&mut a, &mut b);
        a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
    }
}

#[test]
fn disabled_peer_deadlines_do_not_limit_the_running_master() {
    let mut bus = Bus::default();
    bus.onchip[1].write32(0xFFFF_FF00, 7).unwrap();
    bus.onchip[1].write32(0xFFFF_FF04, 100).unwrap();
    let remaining = bus.onchip[1].clocks_to_event();
    bus.onchip[1].advance_clocks(remaining - 1);
    assert_eq!(bus.onchip[1].clocks_to_event(), 1);
    assert!(bus.main_deadline() > 1);
    assert!(bus.observable_main_deadline() > 1);
    bus.set_slave_enabled(true);
    assert_eq!(bus.main_deadline(), 1);
    assert_eq!(bus.observable_main_deadline(), 1);
}

#[test]
fn compressed_retirement_cycles_preserve_every_sound_cutoff_position() {
    let mut points = Retirements {
        points: vec![5, 9, 11, 14],
        repeated: Some((4, 3, 9, 7)),
        last_reference: 0,
    };
    let mut literal = vec![5, 9, 11, 14];
    for cycle in 1..=7 {
        literal.extend([9, 11, 14].map(|at| at + cycle * 9));
    }
    points.points.extend([79, 80]);
    literal.extend([79, 80]);
    assert_eq!(points.len(), literal.len());
    assert_eq!(points.last(), 80);
    for (i, &at) in literal.iter().enumerate() {
        assert_eq!(points.at(i), at);
    }
    for cutoff in 0..=100 {
        assert_eq!(
            points.through(cutoff),
            literal.partition_point(|&at| at <= cutoff)
        );
    }
}

#[test]
fn cached_div1_keeps_nonrestoring_division_flags_and_loop_exit() {
    let source = "00000000 0000 0200 060F F000\n\
        00000200 D107 E001 2100 E2F9 E30D D406 0019 4324 3324 4410 8BFB AFFE 0009\n\
        00000220 FFFF FE92 0010 0000\n";
    for counter in [3u32, 1000, 0x100000] {
        for fast in [false, true] {
            let mut bytes = rom(source);
            bytes[0x224..0x228].copy_from_slice(&counter.to_be_bytes());
            let (mut a, mut b) = pair(&bytes);
            for m in [&mut a, &mut b] {
                m.bus.change_system_clock(fast);
            }
            for _ in 0..2 {
                a.advance_frame().unwrap();
                reference_frame(&mut b).unwrap();
                same(&mut a, &mut b);
            }
            if counter == 0x100000 {
                assert!(a.cached_retired > a.cpu_state().instructions * 9 / 10);
            } else {
                assert_eq!(a.cpu_state().r[4], 0);
            }
        }
    }
}

#[test]
fn cached_special_stack_transfers_and_rte_match_each_ordinary_boundary() {
    let bytes = rom(include_str!("../../examples/runtime-stack.hex"));
    let forms = [
        0x4002, 0x4012, 0x4022, 0x4003, 0x4013, 0x4023, 0x4006, 0x4016, 0x4026, 0x4007, 0x4017,
        0x4027, 0x002B,
    ];
    for fast in [false, true] {
        let (mut a, mut b) = pair(&bytes);
        for m in [&mut a, &mut b] {
            m.bus.change_system_clock(fast);
        }
        let mut seen = 0u16;
        for _ in 0..10_000 {
            while a.cpu_ready[0] > a.bus.main_clocks && a.cpu_boundary[0].is_none() {
                a.finish_hardware_boundary_with_idle(false, false).unwrap();
                b.finish_hardware_boundary_with_idle(false, false).unwrap();
            }
            let pc = a.cpu_state().pc as usize;
            let opcode =
                (pc + 2 <= bytes.len()).then(|| u16::from_be_bytes([bytes[pc], bytes[pc + 1]]));
            let form = opcode.and_then(|op| forms.iter().position(|&f| op & 0xF0FF == f));
            let count = if form.is_some() {
                a.try_cached_batch(1).unwrap()
            } else {
                0
            };
            if count == 1 {
                seen |= 1 << form.unwrap();
                b.step().unwrap();
                same(&mut a, &mut b);
            } else {
                a.step().unwrap();
                b.step().unwrap();
            }
            if seen == (1 << forms.len()) - 1 {
                break;
            }
        }
        assert_eq!(
            seen,
            (1 << forms.len()) - 1,
            "fast={fast}, missing={:X}",
            !seen
        );
    }
}

#[test]
fn ready_time_retry_keeps_the_instruction_boundary_when_a_wait_crosses_a_field() {
    let bytes = rom(include_str!("../../examples/runtime-stack.hex"));
    for (cross_field, retry_limit) in [(false, 1), (false, 128), (true, 128)] {
        let (mut a, mut b) = pair(&bytes);
        for m in [&mut a, &mut b] {
            m.bus.change_system_clock(true);
        }
        let mut found = false;
        for _ in 0..10_000 {
            let pc = a.cpu_state().pc as usize;
            if pc + 2 <= bytes.len()
                && u16::from_be_bytes([bytes[pc], bytes[pc + 1]]) & 0xF0FF == 0x4022
                && a.cpu_ready[0] > a.bus.main_clocks
                && a.cpu_boundary[0].is_none()
            {
                found = true;
                break;
            }
            a.step().unwrap();
            b.step().unwrap();
        }
        assert!(
            found,
            "original MUL must leave a pending successor ready time"
        );
        if cross_field {
            for m in [&mut a, &mut b] {
                let edge = (m.frames() + 1) * (455 * 4 * 263);
                while m.bus.main_clocks < edge - 1 {
                    let remaining = (edge - 1 - m.bus.main_clocks) as u32;
                    let (reference, command) = m.bus.advance_cpu_clocks(remaining).unwrap();
                    assert!(command.is_none());
                    m.clocks += u64::from(reference);
                }
                m.cpu_ready[0] = m.bus.main_clocks + 8;
            }
        }
        let cached = a.cached_retired;
        if cross_field {
            a.advance_frame().unwrap();
            reference_frame(&mut b).unwrap();
        } else if retry_limit == 1 {
            a.step_inner_with_idle(true).unwrap();
            b.step().unwrap();
        } else {
            let retired = a.step_inner_with_retry(true, retry_limit).unwrap();
            assert!(retired > 1, "same-field wait must resume a full burst");
            assert_eq!(a.cached_retired, cached + u64::from(retired));
            while b.cpu_state().instructions < a.cpu_state().instructions {
                b.step().unwrap();
            }
        }
        same(&mut a, &mut b);
        if cross_field || retry_limit == 1 {
            assert_eq!(a.cached_retired, cached + 1);
        }
    }
}

#[test]
fn planned_special_store_preserves_self_modifying_successor_and_retained_if_words() {
    let bytes = rom(include_str!("../../examples/runtime-wait.hex"));
    for aligned in [false, true] {
        let base = 0x0606_0000u32;
        let mut words = vec![0xD200u16, 0x421E, 0xD100];
        if aligned {
            words.push(0x0009);
        }
        let store_pc = base + words.len() as u32 * 2;
        words.extend([0x4113, 0x0009, 0x0009, 0x0009, 0xAFFE, 0x0009]);
        if words.len() % 2 != 0 {
            words.push(0x0009);
        }
        let literals = base + words.len() as u32 * 2;
        words[0] |= ((literals - (base + 4)) / 4) as u16;
        words[2] |= ((literals + 4 - ((base + 8) & !3)) / 4) as u16;
        let pointer = ((store_pc + 4) & !3) + 4;
        words.extend([0xE033, 0x0009, (pointer >> 16) as u16, pointer as u16]);
        let (mut a, mut b) = pair(&bytes);
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        let before = a.bus.cached_stores;
        for m in [&mut a, &mut b] {
            m.bus.cancel_cpu_transaction(Processor::Master);
            for (i, &word) in words.iter().enumerate() {
                m.bus.write16(base + i as u32 * 2, word).unwrap();
            }
            m.cpu = Cpu::start(base, 0x060F_F000);
            m.cpu_ready[0] = m.bus.main_clocks;
            m.cpu_boundary[0] = None;
        }
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        same(&mut a, &mut b);
        assert!(a.bus.cached_stores > before);
        assert_eq!(a.cpu_state().r[0], if aligned { 0x33 } else { 0 });
    }
}

#[test]
fn multiple_work_ram_stores_preserve_bus_protocol_aliases_and_cache_forwarding() {
    let source = "00000000 0000 0200 060F F000\n\
        00000200 D10B E001 2100 D10B D50B E000 7001 2102\n\
        00000210 6252 7001 2100 6350 7001 2101 6451 AFF4 0009\n\
        00000230 FFFF FE92 0602 0000 0602 0000\n";
    for (mode, mcr) in [
        (0, 0),
        (1, 0),
        (1, 0x200),
        (2, 0),
        (2, 0x200),
        (3, 0),
        (5, 0x200),
    ] {
        for fast in [false, true] {
            for physical in [LOW_WRAM + 0x2000, HIGH_WRAM + 0x2000] {
                for bypass in [false, true] {
                    for narrow in [false, true] {
                        for little in [false, true] {
                            let mut bytes = rom(source);
                            bytes[0x234..0x238].copy_from_slice(
                                &(physical | if bypass { 0x2000_0000 } else { 0 }).to_be_bytes(),
                            );
                            bytes[0x238..0x23C].copy_from_slice(&physical.to_be_bytes());
                            let (mut a, mut b) = pair(&bytes);
                            // Establish resident, big-endian IF lines before changing
                            // the external data interface, as real cache hits do.
                            for _ in 0..200 {
                                a.step().unwrap();
                                b.step().unwrap();
                            }
                            for m in [&mut a, &mut b] {
                                m.bus.change_system_clock(fast);
                                m.bus.onchip[0]
                                    .write32(
                                        0xFFFF_FFE0,
                                        0xA55A_03F0 | mode | if little { 0x1000 } else { 0 },
                                    )
                                    .unwrap();
                                m.bus.onchip[0]
                                    .write32(
                                        0xFFFF_FFE4,
                                        if narrow { 0xA55A_007C } else { 0xA55A_00FC },
                                    )
                                    .unwrap();
                            }
                            for m in [&mut a, &mut b] {
                                m.bus.onchip[0]
                                    .write32(
                                        0xFFFF_FFEC,
                                        0xA55A_0000 | mcr | if narrow { 0 } else { 0x40 },
                                    )
                                    .unwrap();
                            }
                            let mut multiple = false;
                            while a.cpu_state().instructions < 1500 {
                                let before = a.bus.cached_stores;
                                if a.try_cached_batch(128).unwrap() == 0 {
                                    a.step_inner_with_idle(true).unwrap();
                                }
                                multiple |= a.bus.cached_stores - before > 1;
                                while b.cpu_state().instructions < a.cpu_state().instructions {
                                    b.step().unwrap();
                                }
                                assert_eq!(
                                    a.cpu, b.cpu,
                                    "mode={mode} mcr={mcr:X} fast={fast} physical={physical:X} bypass={bypass} narrow={narrow} little={little}"
                                );
                                assert_eq!(a.bus.main_clocks, b.bus.main_clocks);
                                assert_eq!(a.bus.high, b.bus.high);
                                assert_eq!(a.bus.low, b.bus.low);
                                assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                                a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                                for offset in [0, 2] {
                                    assert_eq!(
                                        a.bus.cache[0]
                                            .borrow()
                                            .cached_only::<2>(physical + offset, &mut [0; 64]),
                                        b.bus.cache[0]
                                            .borrow()
                                            .cached_only::<2>(physical + offset, &mut [0; 64])
                                    );
                                }
                            }
                            same(&mut a, &mut b);
                            assert!(
                                multiple,
                                "must exercise multiple stores: mode={mode} mcr={mcr:X} fast={fast} physical={physical:X} bypass={bypass} narrow={narrow} little={little}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn planned_work_ram_stores_keep_width_updates_and_buffer_configuration() {
    for (opcode, add) in [
        (0x2100u16, 1u16),
        (0x2101, 2),
        (0x2102, 4),
        (0x2104, 1),
        (0x2105, 2),
        (0x2106, 4),
        (0x1101, 4),
        (0x0104, 1),
        (0x0105, 2),
        (0x0106, 4),
        (0x8010, 1),
        (0x8110, 2),
    ] {
        let mut bytes = rom(include_str!("../../examples/runtime-loop.hex"));
        bytes[0x20A..0x20C].copy_from_slice(&(0x7000 | add).to_be_bytes());
        bytes[0x20C..0x20E].copy_from_slice(&opcode.to_be_bytes());
        if opcode & 0xF00C == 0x2004 {
            bytes[0x224..0x228].copy_from_slice(&0x060F_0000u32.to_be_bytes());
        }
        let (mut a, mut b) = pair(&bytes);
        for field in 0..2 {
            a.advance_frame().unwrap();
            reference_frame(&mut b).unwrap();
            same(&mut a, &mut b);
            if field == 0 {
                for m in [&mut a, &mut b] {
                    // Already-resident opcodes remain big-endian; data uses
                    // the live BSC endian and narrow-bus settings instead.
                    m.bus
                        .for_cpu(Processor::Master)
                        .write32(0xFFFF_FFE0, 0xA55A_13F0)
                        .unwrap();
                    m.bus
                        .for_cpu(Processor::Master)
                        .write32(0xFFFF_FFE4, 0xA55A_007C)
                        .unwrap();
                }
            }
        }
        assert!(a.bus.cached_stores > 0, "opcode={opcode:04X}");
    }
}

#[test]
fn idle_vdp1_vram_store_bursts_match_each_boundary_and_active_drawing_stays_serial() {
    for busy in [false, true] {
        for fast in [false, true] {
            for width_bits in [0x10, 0x20, 0x30] {
                for little in [false, true] {
                    // This ordinary-bus BCR mode reserves little-endian 8-bit.
                    if little && width_bits == 0x10 {
                        continue;
                    }
                    for bypass in [false, true] {
                        for opcode in [0x2100u16, 0x2101, 0x2102] {
                            let mut bytes = rom(include_str!("../../examples/runtime-loop.hex"));
                            bytes[0x208..0x20A].copy_from_slice(&0xE080u16.to_be_bytes());
                            bytes[0x20C..0x20E].copy_from_slice(&opcode.to_be_bytes());
                            let physical = crate::vdp1::VRAM + 0x2000;
                            bytes[0x224..0x228].copy_from_slice(
                                &(physical | if bypass { 0x2000_0000 } else { 0 }).to_be_bytes(),
                            );
                            let (mut a, mut b) = pair(&bytes);
                            for _ in 0..200 {
                                a.step().unwrap();
                                b.step().unwrap();
                            }
                            for m in [&mut a, &mut b] {
                                m.bus.change_system_clock(fast);
                                m.bus.onchip[0]
                                    .write32(
                                        0xFFFF_FFE0,
                                        0xA55A_03F0 | if little { 0x1000 } else { 0 },
                                    )
                                    .unwrap();
                                m.bus.onchip[0]
                                    .write32(0xFFFF_FFE4, 0xA55A_00CC | width_bits)
                                    .unwrap();
                                if busy {
                                    // Skip-and-jump to the same command forever, leaving
                                    // the separately written RAM location untouched.
                                    m.bus.vdp1.vram[..2].copy_from_slice(&0x5000u16.to_be_bytes());
                                    m.bus.write16(crate::vdp1::REGISTERS + 4, 1).unwrap();
                                }
                            }
                            let mut multiple = false;
                            while a.cpu_state().instructions < 1000 {
                                let before = a.bus.cached_stores;
                                let retired = a.try_cached_batch(128).unwrap();
                                if retired == 0 {
                                    a.step_inner_with_idle(true).unwrap();
                                }
                                let stores = a.bus.cached_stores - before;
                                multiple |= stores > 1;
                                while b.cpu_state().instructions < a.cpu_state().instructions {
                                    b.step().unwrap();
                                }
                                assert_eq!(
                                    a.cpu, b.cpu,
                                    "busy={busy} fast={fast} width={width_bits:X} little={little} bypass={bypass} op={opcode:X}"
                                );
                                assert_eq!(a.bus.main_clocks, b.bus.main_clocks);
                                assert_eq!(a.bus.vdp1.vram, b.bus.vdp1.vram);
                                assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                                a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                            }
                            same(&mut a, &mut b);
                            assert_eq!(a.bus.vdp1.busy(), busy);
                            assert_eq!(
                                multiple, !busy,
                                "busy={busy} fast={fast} width={width_bits:X} little={little} bypass={bypass} op={opcode:X}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cached_video_stores_publish_before_later_command_and_texture_acquisition() {
    for source in [
        include_str!("../../tests/fixtures/vdp1-command-fetch.hex"),
        include_str!("../../tests/fixtures/vdp1-memory-ownership.hex"),
        include_str!("../../tests/fixtures/sprite-rotation16.hex"),
    ] {
        let mut bytes = rom(source);
        assert!(bytes[0x180..0x198].iter().all(|&b| b == 0));
        let entry = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        bytes[..4].copy_from_slice(&0x180u32.to_be_bytes());
        for (i, word) in [0xD103u16, 0xE001, 0x2100, 0xD103, 0x412B, 0x0009]
            .into_iter()
            .enumerate()
        {
            bytes[0x180 + i * 2..0x182 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        bytes[0x190..0x194].copy_from_slice(&0xFFFF_FE92u32.to_be_bytes());
        bytes[0x194..0x198].copy_from_slice(&entry.to_be_bytes());
        let (mut a, mut b) = pair(&bytes);
        for _ in 0..3 {
            assert_eq!(a.advance_frame(), reference_frame(&mut b));
            same(&mut a, &mut b);
        }
        assert!(a.bus.cached_stores > 0);
    }
}

#[test]
fn rotation_acquisition_and_continuous_back_buffer_drawing_commute_with_cached_cpu() {
    let bytes = rom(include_str!(
        "../../tests/fixtures/rotation-fetch-bitmap.hex"
    ));
    let (mut a, mut b) = pair(&bytes);
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    let before = a.cached_retired;
    for machine in [&mut a, &mut b] {
        assert!(machine.bus.vdp2.rotation_display_active());
        for (i, word) in [0xE000, 0x7001, 0xAFFD, 0x0009].into_iter().enumerate() {
            machine
                .bus
                .write16(0x0607_0000 + i as u32 * 2, word)
                .unwrap();
        }
        machine.cpu = Cpu::start(0x0607_0000, 0x060F_F000);
        machine.cpu_ready[0] = machine.bus.main_clocks;
        machine.cpu_boundary[0] = None;
        // System clip followed by a cyclic full-size polygon. Drawing always
        // targets the back buffer while per-dot rotation reads the front one.
        for (command, words) in [
            (
                0usize,
                [0x0009u16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 511, 255, 0, 0, 0, 0],
            ),
            (
                32,
                [
                    0x1004u16, 4, 0x00C0, 0x801F, 0, 0, 0, 0, 511, 0, 511, 255, 0, 255, 0, 0,
                ],
            ),
        ] {
            for (i, word) in words.into_iter().enumerate() {
                machine.bus.vdp1.vram[command + i * 2..command + i * 2 + 2]
                    .copy_from_slice(&word.to_be_bytes());
            }
        }
        machine
            .bus
            .vdp1
            .write16(crate::vdp1::REGISTERS + 2, 2)
            .unwrap();
        machine
            .bus
            .vdp1
            .write16(crate::vdp1::REGISTERS + 4, 1)
            .unwrap();
    }
    for _ in 0..3 {
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        same(&mut a, &mut b);
        assert_eq!(a.bus.vdp1.draw_buffer(), b.bus.vdp1.draw_buffer());
        assert!(a.bus.vdp1.busy());
    }
    assert!(a.cached_retired > before + 100_000);
}

fn rom(source: &str) -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in source.lines().filter(|s| !s.starts_with('#')) {
        let mut words = line.split_whitespace();
        let Some(at) = words.next() else { continue };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
fn reference_frame(m: &mut Machine) -> Result<(), MachineError> {
    let target = m.frames() + 1;
    for _ in 0..1_000_000 {
        m.step()?;
        if m.frames() >= target {
            return Ok(());
        }
    }
    Err(MachineError::StepLimit(1_000_000))
}
fn same(a: &mut Machine, b: &mut Machine) {
    assert_eq!(a.cpu, b.cpu);
    assert_eq!(a.slave, b.slave);
    assert_eq!(a.cpu_state(), b.cpu_state());
    assert_eq!(a.slave_state(), b.slave_state());
    assert_eq!(a.sound_state(), b.sound_state());
    assert_eq!(a.bus.sound.ram, b.bus.sound.ram);
    for offset in (0x700..=0xEDE).step_by(2) {
        assert_eq!(
            a.bus.sound.scsp.read16(offset),
            b.bus.sound.scsp.read16(offset)
        );
    }
    assert_eq!((a.clocks(), a.frames()), (b.clocks(), b.frames()));
    assert_eq!(
        a.cpu_ready,
        b.cpu_ready,
        "fault={:?}, clock={}, pc={:X}",
        a.fault,
        a.clocks(),
        a.cpu_state().pc
    );
    assert_eq!(a.dma_errors, b.dma_errors);
    assert_eq!(a.bus.irq_observed, b.bus.irq_observed);
    assert_eq!(a.cpu_reset_vectors, b.cpu_reset_vectors);
    assert_eq!(
        (a.nmi_pending, a.nmi_detected),
        (b.nmi_pending, b.nmi_detected)
    );
    assert_eq!(a.fault, b.fault);
    assert_eq!(a.bus.high, b.bus.high);
    assert_eq!(a.bus.low, b.bus.low);
    assert_eq!(a.bus.vdp1.vram, b.bus.vdp1.vram);
    assert_eq!(a.bus.vdp1.draw_buffer(), b.bus.vdp1.draw_buffer());
    assert_eq!(a.bus.vdp2.vram, b.bus.vdp2.vram);
    assert_eq!(
        a.bus.cache[0].borrow().replacement_state(),
        b.bus.cache[0].borrow().replacement_state()
    );
    assert_eq!(a.framebuffer(), b.framebuffer());
    assert_eq!(a.bus.sound.scsp.drain(4096), b.bus.sound.scsp.drain(4096));
}
fn pair(bytes: &[u8]) -> (Machine, Machine) {
    let mut a = Machine::new();
    let mut b = Machine::new();
    a.reset(Boot::ExternalBios(bytes)).unwrap();
    b.reset(Boot::ExternalBios(bytes)).unwrap();
    (a, b)
}
#[test]
fn cache_bursts_match_reference_and_guest_operand_changes_remain_observable() {
    let mut outcomes = Vec::new();
    for add in [1, 7] {
        for store in [true, false] {
            let mut bytes = rom(include_str!("../../examples/runtime-loop.hex"));
            bytes[0x20A..0x20C].copy_from_slice(&(0x7000u16 | add).to_be_bytes());
            if !store {
                bytes[0x20C..0x20E].copy_from_slice(&0x0009u16.to_be_bytes());
            }
            let (mut a, mut b) = pair(&bytes);
            for _ in 0..2 {
                a.advance_frame().unwrap();
                reference_frame(&mut b).unwrap();
                same(&mut a, &mut b);
            }
            assert!(
                a.cached_retired > 0,
                "cache burst path must actually execute"
            );
            assert_eq!(b.cached_retired, 0);
            assert!(
                a.bus.inline_resumes > 0,
                "in-place bus continuation must execute"
            );
            assert_eq!(b.bus.inline_resumes, 0);
            assert_eq!(a.bus.cached_stores > 0, store);
            assert_eq!(b.bus.cached_stores, 0);
            outcomes.push(a.cpu_state().r[0]);
        }
    }
    assert_ne!(outcomes[0], outcomes[2]);
    assert_ne!(outcomes[1], outcomes[3]);
}
#[test]
fn shared_hardware_guests_keep_deadlines_records_faults_and_reset_domains() {
    for source in [
        include_str!("../../examples/runtime-divide.hex"),
        include_str!("../../examples/runtime-scene.hex"),
        include_str!("../../examples/runtime-stack.hex"),
        include_str!("../../examples/runtime-poll.hex"),
        include_str!("../../examples/runtime-poll-scene.hex"),
        include_str!("../../examples/runtime-poll-sound-busy-scene.hex"),
        include_str!("../../examples/runtime-normal-scene.hex"),
        include_str!("../../tests/fixtures/sh7604.hex"),
        include_str!("../../tests/fixtures/dual-sh2.hex"),
        include_str!("../../tests/fixtures/clock.hex"),
        include_str!("../../tests/fixtures/system-reset.hex"),
        include_str!("../../tests/fixtures/sh2-reset-bus.hex"),
        include_str!("../../tests/fixtures/sh2-multiplier.hex"),
        include_str!("../../tests/fixtures/sh2-load.hex"),
        include_str!("../../tests/fixtures/sh2-fetch.hex"),
        include_str!("../../tests/fixtures/scu-dstp-refill.hex"),
        include_str!("../../tests/fixtures/vdp1-progress.hex"),
        include_str!("../../tests/fixtures/vdp1-effects-timing.hex"),
        include_str!("../../tests/fixtures/vdp1-command-fetch.hex"),
        include_str!("../../tests/fixtures/vdp1-memory-ownership.hex"),
        include_str!("../../tests/fixtures/vdp1-hdtv-erase.hex"),
        include_str!("../../tests/fixtures/sprite-rotation16.hex"),
        include_str!("../../tests/fixtures/smpc-fault-word-read.hex"),
        include_str!("../../tests/fixtures/smpc-fault-long.hex"),
        include_str!("../../tests/fixtures/smpc-fault-command.hex"),
    ] {
        let (mut a, mut b) = pair(&rom(source));
        for _ in 0..2 {
            let actual = a.advance_frame();
            let expected = reference_frame(&mut b);
            assert_eq!(actual, expected);
            same(&mut a, &mut b);
            if actual.is_err() {
                break;
            }
        }
    }
}
#[test]
fn live_user_break_comparisons_use_the_reference_path() {
    let bytes = rom(include_str!("../../examples/runtime-loop.hex"));
    let (mut a, mut b) = pair(&bytes);
    for m in [&mut a, &mut b] {
        let chip = &mut m.bus.onchip[0];
        chip.write32(0xFFFF_FF40, 0x20A).unwrap();
        chip.write16(0xFFFF_FF48, 0x54).unwrap();
        chip.write16(0xFFFF_FF78, 0x1000).unwrap();
        chip.read16(0xFFFF_FF78).unwrap();
    }
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    assert_eq!(a.cached_retired, 0);
    assert_eq!(
        a.bus.onchip[0].break_pending(),
        b.bus.onchip[0].break_pending()
    );
}

#[test]
fn nmi_between_frames_and_cold_restart_preserve_prefetch_and_pipeline_results() {
    let mut bytes = rom(include_str!("../../examples/runtime-loop.hex"));
    bytes[0x2C..0x30].copy_from_slice(&0x300u32.to_be_bytes());
    for (i, word) in [0xE877u16, 0x002B, 0x0009].into_iter().enumerate() {
        bytes[0x300 + i * 2..0x302 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    let (mut a, mut b) = pair(&bytes);
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    a.request_nmi();
    b.request_nmi();
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    assert_eq!(a.cpu_state().r[8], 0x77);
    a.reset(Boot::ExternalBios(&bytes)).unwrap();
    b.reset(Boot::ExternalBios(&bytes)).unwrap();
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
}

#[test]
fn newly_asserted_capture_irq_is_sampled_before_any_cached_burst() {
    let mut bytes = rom(include_str!("../../examples/runtime-loop.hex"));
    // Lower SR.I in guest code, then enter the original ADD/store loop.
    for (i, word) in [0xE000u16, 0x400E, 0x7001, 0x2102, 0xAFFC, 0x0009]
        .into_iter()
        .enumerate()
    {
        bytes[0x208 + i * 2..0x20A + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    bytes[0x100..0x104].copy_from_slice(&0x300u32.to_be_bytes());
    for (i, word) in [0xE877u16, 0x002B, 0x0009].into_iter().enumerate() {
        bytes[0x300 + i * 2..0x302 + i * 2].copy_from_slice(&word.to_be_bytes());
    }
    let (mut a, mut b) = pair(&bytes);
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    for m in [&mut a, &mut b] {
        let chip = &mut m.bus.onchip[0];
        chip.write16(crate::onchip::IPRB, 0x0500).unwrap();
        chip.write16(crate::onchip::VCRC, 0x4040).unwrap();
        chip.write8(crate::onchip::FRT, 0x80).unwrap();
        chip.capture_strobe();
    }
    a.advance_frame().unwrap();
    reference_frame(&mut b).unwrap();
    same(&mut a, &mut b);
    assert_eq!(a.cpu_state().r[8], 0x77);
}

#[test]
fn fabricated_mmio_cache_hit_cannot_bypass_the_peripheral_contract() {
    let mut bus = Bus::default();
    let cache = bus.cache[0].get_mut();
    cache.set_control(1);
    cache.array_write(0x6010_0004, [0; 4]).unwrap();
    cache
        .array_write(0xC000_0000, [0xFE, 0xDC, 0xBA, 0x98])
        .unwrap();
    assert!(cache.read_hit(crate::devices::SMPC));
    let before = bus.smpc.command_state();
    {
        let cache = bus.cache[0].borrow();
        let memory = Cached {
            cache: &cache,
            code: None,
            fills: RefCell::default(),
            fill_count: Cell::new(0),
            fill_reads: RefCell::new(None),
            scalar_active: Cell::new(false),
            clock: Cell::new(0),
            lru: RefCell::new(cache.replacement_state()),
            retained: Cell::new(None),
            epochs: Cell::new(0),
            end: Cell::new(256),
            fill_end: 256,
            store: Cell::new(None),
            store_ready: 0,
            store_origin: 0,
            passive: None,
            write_bus: None,
            write_protocol: None,
            idle_vdp1_vram: false,
            ram_stores: Vec::new(),
            write_bounds: (u32::MAX, 0),
            write_state: None,
            write_pc: Cell::new(0),
        };
        assert_eq!(memory.read16(crate::devices::SMPC), Err(BusError::Pending));
    }
    assert!(matches!(
        bus.for_cpu(Processor::Master).read16(crate::devices::SMPC),
        Err(BusError::Smpc(_))
    ));
    assert_eq!(bus.smpc.command_state(), before);
}

#[test]
fn pll_cache_bursts_preserve_fractional_phase_and_pending_main_ticks() {
    let bytes = rom(include_str!("../../examples/runtime-loop.hex"));
    for pal in [false, true] {
        for fast in [false, true] {
            for offset in [0, 1, 15] {
                let (mut a, mut b) = pair(&bytes);
                for m in [&mut a, &mut b] {
                    m.set_pal(pal);
                    m.bus.change_system_clock(fast);
                    for _ in 0..offset {
                        let (elapsed, command) = m.bus.advance_cpu_clocks(1).unwrap();
                        m.clocks += u64::from(elapsed);
                        assert!(command.is_none());
                    }
                }
                for _ in 0..2 {
                    a.advance_frame().unwrap();
                    reference_frame(&mut b).unwrap();
                    same(&mut a, &mut b);
                    assert_eq!(a.bus.pending_main_clocks, b.bus.pending_main_clocks);
                    for interval in [0, 1, 2, 127] {
                        assert_eq!(
                            a.bus.system_clock.preview(interval),
                            b.bus.system_clock.preview(interval)
                        );
                    }
                }
                assert!(
                    a.cached_retired > 0,
                    "pal={pal} fast={fast} offset={offset}"
                );
            }
        }
    }
}

#[test]
fn sound_lookahead_stops_before_control_writes_reset_and_invalid_dsp_programs() {
    let programs: &[&[u16]] = &[
        &[0x4E71, 0x60FC], // NOP; BRA: sound RAM traffic without control writes.
        &[0x33FC, 0x1234, 0, 0x800, 0x60F6], // repeated RAM writes.
        &[0x33FC, 0x8000, 0x0010, 0x0800, 0x60FE], // invalid live DSP program.
        &[0x4E70, 0x60FC], // RESET output with a real external pulse.
        &[0x33FC, 0x0020, 0x0010, 0x042A, 0x60FE], // enable main IRQ routing.
        &[0x33FC, 0x1820, 0x0010, 0x0000, 0x60FE], // live KEYON/PCM work.
        &[0x33FC, 0x1820, 0x0010, 0x0000, 0x60F6], // repeated voice writes retain their boundaries.
        &[0x13FC, 0x007F, 0x0010, 0x0017, 0x60F6], // repeated voice byte lanes.
    ];
    for source in [
        include_str!("../../examples/runtime-wait.hex"),
        include_str!("../../examples/runtime-poll.hex"),
    ] {
        let bytes = rom(source);
        for &program in programs {
            for fast in [false, true] {
                let (mut a, mut b) = pair(&bytes);
                for m in [&mut a, &mut b] {
                    m.bus.change_system_clock(fast);
                }
                a.advance_frame().unwrap();
                reference_frame(&mut b).unwrap();
                same(&mut a, &mut b);
                let before = a.cached_retired;
                for m in [&mut a, &mut b] {
                    let sound = &mut m.bus.sound;
                    sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
                    sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                    for (i, word) in program.iter().enumerate() {
                        sound.ram[0x100 + i * 2..0x102 + i * 2]
                            .copy_from_slice(&word.to_be_bytes());
                    }
                    sound.set_enabled(true).unwrap();
                }
                for _ in 0..2 {
                    let actual = a.advance_frame();
                    let expected = reference_frame(&mut b);
                    assert_eq!(actual, expected, "program={program:X?}, fast={fast}");
                    same(&mut a, &mut b);
                    if actual.is_err() {
                        break;
                    }
                }
                assert!(
                    a.cached_retired > before,
                    "live sound case must exercise cached execution"
                );
            }
        }
    }
}

#[test]
fn live_pcm_and_valid_dsp_memory_programs_commute_with_cache_only_execution() {
    let bytes = rom(include_str!("../../examples/runtime-wait.hex"));
    for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
        let (mut a, mut b) = pair(&bytes);
        for m in [&mut a, &mut b] {
            m.set_pal(pal);
            m.bus.change_system_clock(fast);
        }
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        let before = a.cached_retired;
        for m in [&mut a, &mut b] {
            let sound = &mut m.bus.sound;
            sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
            sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
            sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
            sound.ram[0x1000..0x1002].copy_from_slice(&0x1234u16.to_be_bytes());
            sound.ram[0x2000..0x2008].copy_from_slice(&[0x10, 0, 0x20, 0, 0x30, 0, 0x40, 0]);
            for (offset, value) in [
                (2, 0x2000),
                (6, 4),
                (8, 31),
                (12, 0x100),
                (16, 0x7800),
                (22, 0xE000),
                (0, 0x1820),
                (0x400, 0x20F),
                (0x700, 0x4000),
                (0x780, 0x800),
            ] {
                sound.scsp.write16(offset, value).unwrap();
            }
            for (step, words) in [
                (1, [0, 0, 0xA002, 0x8000]),
                (3, [0, 0x20, 2, 0]),
                (5, [0, 0xA000, 2, 0]),
                (6, [0, 0, 0x1002, 0]),
            ] {
                for (i, value) in words.into_iter().enumerate() {
                    sound
                        .scsp
                        .write16(0x800 + step * 8 + i as u32 * 2, value)
                        .unwrap();
                }
            }
            sound.set_enabled(true).unwrap();
            assert!(sound.cached_lookahead_allowed());
        }
        for _ in 0..3 {
            a.advance_frame().unwrap();
            reference_frame(&mut b).unwrap();
            same(&mut a, &mut b);
        }
        assert!(a.cached_retired > before + 100_000);
    }
}

#[test]
fn cached_gbr_load_wait_loops_preserve_width_data_and_branch_controls() {
    let source = include_str!("../../examples/runtime-wait.hex");
    for (opcode, data, compare) in [
        (0xC400u16, 0x01000000u32, 1u16),
        (0xC500, 0x00010000, 1),
        (0xC600, 1, 1),
        (0xC600, 7, 7),
        (0xC600, 9, 1),
        (0xC400, 0xFF000000, 0xFF),
        (0xC500, 0xFFFF0000, 0xFF),
        (0xC600, 0xFFFFFFFF, 0xFF),
        (0xC600, 0, 0),
    ] {
        let mut bytes = rom(source);
        bytes[0x210..0x212].copy_from_slice(&opcode.to_be_bytes());
        bytes[0x20E..0x210].copy_from_slice(&(0xE400 | compare).to_be_bytes());
        bytes[0x248..0x24C].copy_from_slice(&data.to_be_bytes());
        let (mut a, mut b) = pair(&bytes);
        for _ in 0..2 {
            a.advance_frame().unwrap();
            reference_frame(&mut b).unwrap();
            same(&mut a, &mut b);
        }
        assert!(
            a.cached_retired > a.cpu_state().instructions * 8 / 10,
            "opcode={opcode:X}, data={data:X}, cached={} total={}",
            a.cached_retired,
            a.cpu_state().instructions
        );
        assert_eq!(a.cpu_state().r[6], if data == 9 { 0x55 } else { 0 });
        assert!(
            a.bus.cached_cycle_skipped > 0,
            "fixed-point cycle acceleration must execute"
        );
    }
}

#[test]
fn countdown_cycle_acceleration_keeps_the_zero_exit_and_both_pll_domains() {
    let source = include_str!("../../examples/runtime-countdown.hex");
    for counter in [1u32, 2, 3, 0x10000, 0x100000] {
        for (pal, fast) in [(false, false), (true, true)] {
            let mut bytes = rom(source);
            bytes[0x244..0x248].copy_from_slice(&counter.to_be_bytes());
            let (mut a, mut b) = pair(&bytes);
            for m in [&mut a, &mut b] {
                m.set_pal(pal);
                m.bus.change_system_clock(fast);
            }
            for _ in 0..2 {
                a.advance_frame().unwrap();
                reference_frame(&mut b).unwrap();
                same(&mut a, &mut b);
            }
            assert!(a.bus.cached_cycle_skipped > 0);
            if counter == 0x100000 {
                assert!(a.cpu_state().r[3] > 0);
                assert_eq!(a.cpu_state().r[6], 0);
                assert!(
                    a.bus.cached_cycle_skipped > a.cpu_state().instructions / 2,
                    "pal={pal} fast={fast} skipped={} total={}",
                    a.bus.cached_cycle_skipped,
                    a.cpu_state().instructions
                );
            } else {
                assert_eq!(a.cpu_state().r[3], 0);
                assert_eq!(a.cpu_state().r[6], 0x55);
            }
        }
    }
}

#[test]
fn shared_sound_ram_buffered_writes_cannot_enter_cached_sound_lookahead() {
    let bytes = rom(include_str!("../../examples/runtime-wait.hex"));
    let (mut m, mut reference) = pair(&bytes);
    m.advance_frame().unwrap();
    reference_frame(&mut reference).unwrap();
    same(&mut m, &mut reference);
    let address = crate::sound::SOUND_RAM | 0x20000000;
    assert_eq!(
        m.bus
            .replay_cpu(Processor::Master)
            .write32(address, 0x12345678),
        Err(BusError::Pending)
    );
    m.bus.advance_clocks(1).unwrap();
    m.clocks += 1;
    assert_eq!(
        m.bus
            .replay_cpu(Processor::Master)
            .write32(address, 0x12345678),
        Ok(())
    );
    m.bus.finish_cpu_transaction(Processor::Master);
    m.bus.grant_bus_transaction();
    assert!(
        m.bus.sound_ram_request().is_some(),
        "the shared RAM write must own the external phase"
    );
    assert!(!m.bus.cached_bus_compatible());
    let clock = m.bus.main_clocks;
    assert!(m.bus.cached_batch(&m.cpu, 128).unwrap().is_none());
    assert_eq!(m.bus.main_clocks, clock);
}

#[test]
fn cached_guest_and_live_sound_cpu_share_ram_without_lookahead_crossing() {
    let mut bytes = rom(include_str!("../../tests/fixtures/scsp-cpu-memory.hex"));
    assert!(bytes[0x180..0x198].iter().all(|&byte| byte == 0));
    let entry = u32::from_be_bytes(bytes[..4].try_into().unwrap());
    bytes[..4].copy_from_slice(&0x180u32.to_be_bytes());
    for (index, word) in [0xD103u16, 0xE001, 0x2100, 0xD103, 0x412B, 0x0009]
        .into_iter()
        .enumerate()
    {
        bytes[0x180 + index * 2..0x182 + index * 2].copy_from_slice(&word.to_be_bytes());
    }
    bytes[0x190..0x194].copy_from_slice(&0xFFFF_FE92u32.to_be_bytes());
    bytes[0x194..0x198].copy_from_slice(&entry.to_be_bytes());
    let (mut a, mut b) = pair(&bytes);
    for _ in 0..3 {
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        same(&mut a, &mut b);
    }
    assert!(a.cached_retired > 0);
    assert!(a.sound_state().instructions > 0);
}

#[test]
fn cached_opcode_table_matches_every_literal_isa_classification() {
    for op in 0..=u16::MAX {
        assert_eq!(store_width(op), store_width_literal(op), "op={op:04X}");
        assert_eq!(
            accepted_opcode(op),
            candidate_literal(op) || store_width_literal(op).is_some(),
            "op={op:04X}"
        );
    }
}

#[test]
fn readonly_cache_bursts_preserve_refresh_grants_and_keep_enabled_irq_horizons() {
    for (source, pure) in [
        (include_str!("../../examples/runtime-countdown.hex"), true),
        (include_str!("../../examples/runtime-loop.hex"), false),
    ] {
        for fast in [false, true] {
            for compare in [7, 31, 127] {
                for irq in [false, true] {
                    let bytes = rom(source);
                    let (mut a, mut b) = pair(&bytes);
                    for m in [&mut a, &mut b] {
                        m.bus.change_system_clock(fast);
                        let chip = &mut m.bus.onchip[0];
                        chip.write32(0xFFFF_FFE0, 0xA55A_0004).unwrap();
                        chip.write32(0xFFFF_FFEC, 0xA55A_0048).unwrap();
                        chip.write32(0xFFFF_FFF8, 0xA55A_0000 | compare).unwrap();
                        chip.write32(0xFFFF_FFF4, 0xA55A_0000).unwrap();
                        chip.write32(0xFFFF_FFF0, 0xA55A_0008 | if irq { 0x40 } else { 0 })
                            .unwrap();
                    }
                    let mut crossings = 0;
                    while a.cpu_state().instructions < 5000 {
                        let start = a.bus.main_clocks;
                        let before = a.bus.onchip[0].clocks_to_event();
                        let count = a.try_cached_batch(1024).unwrap();
                        if count == 0 {
                            a.step_inner_with_idle(true).unwrap();
                        } else if a.bus.main_clocks - start > u64::from(before) {
                            crossings += 1;
                        }
                        while b.cpu_state().instructions < a.cpu_state().instructions {
                            b.step().unwrap();
                        }
                        same(&mut a, &mut b);
                        a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                        assert_eq!(a.bus.external_owner, b.bus.external_owner);
                        a.bus.assert_cached_refresh_arbiter_matches(&b.bus);
                    }
                    if !pure {
                        assert!(
                            a.bus.cached_stores > 0,
                            "mixed guest must use cached store acceptance"
                        );
                    }
                    if irq {
                        assert_eq!(crossings, 0, "enabled refresh IRQ remains a strict horizon");
                    } else if pure {
                        assert!(
                            crossings > 0,
                            "must exercise actual refresh-crossing bursts"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cached_memory_mac_keeps_aliasing_saturation_multiplier_and_if_timing() {
    for fast in [false, true] {
        for saturate in [false, true] {
            for alias in [false, true] {
                for negative in [false, true] {
                    for bypass in [false, true] {
                        let base = if negative {
                            LOW_WRAM + 0x200C
                        } else {
                            HIGH_WRAM + 0x200C
                        };
                        let first = base | if bypass { 0x2000_0000 } else { 0 };
                        let second = (base + 0x24) | if bypass { 0x2000_0000 } else { 0 };
                        let mut bytes = vec![0; BIOS_SIZE];
                        bytes[..4].copy_from_slice(&0x200u32.to_be_bytes());
                        bytes[4..8].copy_from_slice(&0x060F_F000u32.to_be_bytes());
                        for (i, op) in [
                            0xD10Fu16,
                            0xE001,
                            0x2100,
                            if saturate { 0xE002 } else { 0xE000 },
                            0x400E,
                            0xD10E,
                            0xD20E,
                            if alias { 0x011F } else { 0x012F },
                            0x001A,
                            0x040A,
                            0xAFF9,
                            0x0009,
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            bytes[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&op.to_be_bytes());
                        }
                        for (at, value) in
                            [(0x240, 0xFFFF_FE92u32), (0x244, first), (0x248, second)]
                        {
                            bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
                        }
                        let (mut a, mut b) = pair(&bytes);
                        for m in [&mut a, &mut b] {
                            m.bus.change_system_clock(fast);
                            for (at, value) in [
                                (base, if negative { 0x8000_0000 } else { 0x7FFF_FFFF }),
                                (base + 4, 0x7FFF_FFFF),
                                (base + 0x24, 0xFFFF_FFFF),
                            ] {
                                m.bus.write32(at, value).unwrap();
                            }
                        }
                        let mut macs = 0;
                        while a.cpu_state().instructions < 128 {
                            let pc = a.cpu_state().pc;
                            let count = a.try_cached_batch(1).unwrap();
                            if count == 0 {
                                a.step_inner_with_idle(true).unwrap();
                            } else if pc == 0x20E {
                                macs += 1;
                            }
                            while b.cpu_state().instructions < a.cpu_state().instructions {
                                b.step().unwrap();
                            }
                            same(&mut a, &mut b);
                            a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                            assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                            assert_eq!(a.bus.external_owner, b.bus.external_owner);
                        }
                        assert_eq!(
                            macs > 0,
                            !bypass,
                            "fast={fast} saturate={saturate} alias={alias} negative={negative} bypass={bypass}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn private_instruction_refills_match_real_cache_eviction_and_write_boundaries() {
    let cases = [
        (HIGH_WRAM, 0, 0, false, 1),
        (HIGH_WRAM, 0, 0, true, 9),
        (HIGH_WRAM, 1, 0, false, 1),
        (HIGH_WRAM, 1, 0x200, true, 9),
        (LOW_WRAM, 2, 0, true, 1),
        (HIGH_WRAM, 2, 0x200, false, 9),
        (LOW_WRAM, 3, 0, false, 1),
        (HIGH_WRAM, 5, 0x200, true, 9),
        (HIGH_WRAM, 0, 0, false, 3), // CCR.ID: no instruction allocation.
        (LOW_WRAM, 0, 0, true, 5),   // CCR.OD: code allocation still operates.
    ];
    let mut exercised_sound = [false; 4];
    let mut exercised_store_boundary = false;
    let mut exercised_two_way = false;
    let mut exercised_low_ram = false;
    for (case, (ram, mode, mcr, narrow, ccr)) in cases.into_iter().enumerate() {
        let base = ram + 0x1000;
        let mut bytes = vec![0; BIOS_SIZE];
        bytes[..4].copy_from_slice(&base.to_be_bytes());
        bytes[4..8].copy_from_slice(&(HIGH_WRAM + WRAM_SIZE as u32 - 16).to_be_bytes());
        for stores in [false, true] {
            for fast in [false, true] {
                let (mut a, mut b) = pair(&bytes);
                for m in [&mut a, &mut b] {
                    m.set_pal(case % 3 == 2);
                    m.bus.change_system_clock(fast);
                    m.bus.onchip[0]
                        .write32(0xFFFF_FFE0, 0xA55A_03F0 | mode)
                        .unwrap();
                    m.bus.onchip[0]
                        .write32(0xFFFF_FFE4, if narrow { 0xA55A_007C } else { 0xA55A_00FC })
                        .unwrap();
                    m.bus.onchip[0]
                        .write32(
                            0xFFFF_FFEC,
                            0xA55A_0000 | mcr | if narrow { 0 } else { 0x40 },
                        )
                        .unwrap();
                    m.bus.cache[0].get_mut().set_control(ccr);
                    // Eight conflicting tags repeatedly evict the same set.
                    for block in 0..8 {
                        let at = base + block * 0x400;
                        let words = if block == 0 {
                            vec![0xDE03, 0x7001, 0x4E1E, 0xA1FB, 0x7101]
                        } else {
                            vec![
                                0x7001,
                                0x7101,
                                if stores { 0xC280 } else { 0x0009 },
                                if block == 7 { 0x4E2B } else { 0xA1FB },
                                0x0009,
                            ]
                        };
                        for (i, op) in words.into_iter().enumerate() {
                            m.bus.write16(at + i as u32 * 2, op).unwrap();
                        }
                    }
                    m.bus.write32(base + 16, base).unwrap();
                    if case % 4 != 0 {
                        // Actual sound RAM reads/writes and a delayed MCIEB
                        // write exercise long-gap checkpoints and earlier prefixes.
                        let sound = &mut m.bus.sound;
                        sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
                        sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                        let program: Vec<u16> = match case % 4 {
                            1 => vec![0x4E71, 0x60FC],
                            2 => vec![0x33FC, 0x1234, 0, 0x800, 0x60F6],
                            _ => {
                                let mut p = vec![0x4E71; 64];
                                p.extend([0x33FC, 0x0020, 0x0010, 0x042A, 0x60FE]);
                                p
                            }
                        };
                        for (i, op) in program.into_iter().enumerate() {
                            sound.ram[0x100 + i * 2..0x102 + i * 2]
                                .copy_from_slice(&op.to_be_bytes());
                        }
                        sound.set_enabled(true).unwrap();
                    }
                }
                let mut turns = 0;
                while a.cpu_state().instructions < 1000 {
                    if a.try_cached_batch(128).unwrap() == 0 {
                        a.step_inner_with_idle(true).unwrap();
                    }
                    while b.cpu_state().instructions < a.cpu_state().instructions {
                        b.step().unwrap();
                    }
                    same(&mut a, &mut b);
                    a.bus.cache[0]
                        .borrow()
                        .assert_same_contents(&b.bus.cache[0].borrow());
                    a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                    assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                    turns += 1;
                    assert!(turns < 5000, "case={case}, stores={stores}, fast={fast}");
                }
                if ccr & 2 != 0 {
                    assert_eq!(a.bus.cached_code_fills, 0);
                } else if a.bus.cached_code_fills != 0 {
                    exercised_sound[case % 4] = true;
                    exercised_store_boundary |= stores;
                    exercised_two_way |= ccr & 8 != 0;
                    exercised_low_ram |= ram == LOW_WRAM;
                }
                // Narrow writes or control edges can keep the bus unavailable
                // for an entire refill. Those cases must retain fallback.
                assert_eq!(b.bus.cached_code_fills, 0);
            }
        }
    }
    assert!(exercised_sound.into_iter().all(|used| used));
    assert!(exercised_store_boundary && exercised_two_way && exercised_low_ram);
}

#[test]
fn data_cache_refills_match_physical_reads_and_cpu_pipeline_at_every_retirement() {
    let mut filled = [false; 4];
    let mut planned = 0;
    let mut loads = 0;
    let mut slots = 0;
    let mut odd_disabled_stays_empty = false;
    for (kind, op) in [0x6014u16, 0x6015, 0x6016, 0x012F].into_iter().enumerate() {
        for ccr in [1u8, 3, 5, 9, 11, 13] {
            for fast in [false, true] {
                let mut bytes = vec![0; BIOS_SIZE];
                bytes[..4].copy_from_slice(&0x200u32.to_be_bytes());
                bytes[4..8].copy_from_slice(&0x060F_F000u32.to_be_bytes());
                for (i, word) in [0xD10Fu16, 0xD20F, op, 0x031A, 0x7301, 0xAFFB, 0x0009]
                    .into_iter()
                    .enumerate()
                {
                    bytes[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&word.to_be_bytes());
                }
                let base = if kind.is_multiple_of(2) {
                    LOW_WRAM
                } else {
                    HIGH_WRAM
                } + 0x200C;
                bytes[0x240..0x244].copy_from_slice(&base.to_be_bytes());
                bytes[0x244..0x248].copy_from_slice(&(base + 0x1000).to_be_bytes());
                let (mut a, mut b) = pair(&bytes);
                for m in [&mut a, &mut b] {
                    m.bus.change_system_clock(fast);
                    m.bus.onchip[0]
                        .write32(
                            0xFFFF_FFE0,
                            0xA55A_03F0 | if kind == 1 { 0x1000 } else { 0 },
                        )
                        .unwrap();
                    m.bus.onchip[0]
                        .write32(
                            0xFFFF_FFE4,
                            if kind == 2 { 0xA55A_007C } else { 0xA55A_00FC },
                        )
                        .unwrap();
                    m.bus.onchip[0]
                        .write32(
                            0xFFFF_FFEC,
                            if kind == 2 { 0xA55A_0000 } else { 0xA55A_0040 },
                        )
                        .unwrap();
                    for offset in 0..0x200u32 {
                        m.bus
                            .write8(base + offset, (offset.wrapping_mul(37) ^ 0xA5) as u8)
                            .unwrap();
                        m.bus
                            .write8(
                                base + 0x1000 + offset,
                                (offset.wrapping_mul(13) ^ 0x69) as u8,
                            )
                            .unwrap();
                    }
                    let cache = m.bus.cache[0].get_mut();
                    cache.set_control(1 | (ccr & 8));
                    for address in (0x200..0x260).step_by(16) {
                        cache
                            .read::<4>(address, true, |at| {
                                Ok(bytes[at as usize..at as usize + 4].try_into().unwrap())
                            })
                            .unwrap();
                    }
                    cache.set_control(ccr);
                }
                while a.cpu_state().instructions < 300 {
                    if a.try_cached_batch(1).unwrap() == 0 {
                        a.step_inner_with_idle(true).unwrap();
                    }
                    while b.cpu_state().instructions < a.cpu_state().instructions {
                        b.step().unwrap();
                    }
                    same(&mut a, &mut b);
                    a.bus.cache[0]
                        .borrow()
                        .assert_same_contents(&b.bus.cache[0].borrow());
                    a.bus.onchip[0].assert_cached_read_bus_matches(&b.bus.onchip[0]);
                    assert_eq!(a.bus.instruction_word[0], b.bus.instruction_word[0]);
                    assert_eq!(a.bus.external_owner, b.bus.external_owner);
                }
                planned += a.bus.cached_wb_scalars;
                loads += a.bus.cached_prepared_loads;
                slots += a.bus.cached_prepared_slots;
                if ccr & 4 != 0 {
                    assert_eq!(a.bus.cached_code_fills, 0);
                    odd_disabled_stays_empty = true;
                } else if a.bus.cached_code_fills > 0 {
                    filled[kind] = true;
                }
            }
        }
    }
    assert!(planned > 0, "prepared scalar path must execute");
    assert!(loads > 0, "prepared load path must execute");
    assert!(slots > 0, "prepared delay-slot path must execute");
    assert!(filled.into_iter().all(|hit| hit));
    assert!(odd_disabled_stays_empty);
}

#[test]
fn cached_sound_with_mounted_media_keeps_cdda_start_and_stop_edges() {
    use crate::{cd::REGISTERS, media::Media};
    let mut audio = vec![0; 2352 * 16];
    for (i, frame) in audio.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        frame[..2].copy_from_slice(&(2048 + (i % 251) as i16).to_le_bytes());
        frame[2..].copy_from_slice(&(-1024i16).to_le_bytes());
    }
    let media = Media::cue(
        "FILE \"data\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"audio\" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00",
        &[("data", &[0; 2048]), ("audio", &audio)],
    ).unwrap();
    let bytes = rom(include_str!("../../examples/runtime-loop.hex"));
    let (mut a, mut b) = pair(&bytes);
    for m in [&mut a, &mut b] {
        m.mount_media(media.clone());
        let sound = &mut m.bus.sound;
        sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
        sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
        sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
        sound.scsp.write16(0x400, 15).unwrap();
        sound.scsp.write16(16 * 32 + 22, 0xFF).unwrap();
        sound.scsp.write16(17 * 32 + 22, 0xEF).unwrap();
        sound.set_enabled(true).unwrap();
    }
    for (command, quiet) in [
        (None, true),
        (Some([0x1080, 151, 0x80, 16]), false),
        (Some([0x11FF, 0xFFFF, 0, 0]), true),
    ] {
        if let Some(command) = command {
            for m in [&mut a, &mut b] {
                for (i, word) in command.into_iter().enumerate() {
                    m.bus
                        .cd
                        .write16(REGISTERS + 0x18 + i as u32 * 4, word)
                        .unwrap();
                }
            }
        }
        a.advance_frame().unwrap();
        reference_frame(&mut b).unwrap();
        assert_eq!(a.bus.cd.audio_quiet(), quiet);
        let pcm = a.drain_audio(4096);
        assert_eq!(pcm, b.drain_audio(4096));
        if !quiet {
            assert!(pcm.iter().any(|&sample| sample != 0));
        }
        same(&mut a, &mut b);
    }
    assert!(a.cached_retired > 1000);
}
