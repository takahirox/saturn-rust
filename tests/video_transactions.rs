//! Original synthetic guest acceptance for ST-013 §§2/4 and ST-058 §§2/3/4.
use saturn_rust::{Boot, Machine, bus::Memory};
fn load(text: &str) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn patch(rom: &mut [u8], before: u32, after: u32) {
    let positions: Vec<_> = rom
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == before.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(positions.len(), 1);
    rom[positions[0]..positions[0] + 4].copy_from_slice(&after.to_be_bytes());
}
fn run(m: &mut Machine, rom: &[u8], base: u32, count: u32) -> Vec<u32> {
    m.reset(Boot::ExternalBios(rom)).unwrap();
    for _ in 0..100 {
        m.run_steps(5000).unwrap();
        if m.bus().read32(base + (count - 1) * 4) == Ok(0x600D) {
            break;
        }
    }
    let records: Vec<_> = (0..count)
        .map(|i| m.bus().read32(base + i * 4).unwrap())
        .collect();
    assert_eq!(
        records.last(),
        Some(&0x600D),
        "PC={:X}, records={records:X?}",
        m.cpu_state().pc
    );
    assert_eq!(m.cpu_state().exceptions, 0);
    records
}
#[test]
fn guest_preclipping_reverses_texture_reads_before_hidden_end_codes() {
    let original = load(include_str!("fixtures/vdp1-preclip.hex"));
    let mut m = Machine::new();
    for disable in [false, false, true, true] {
        let mut image = original.clone();
        image[0x100..0x102].copy_from_slice(&(if disable { 0x800u16 } else { 0 }).to_be_bytes());
        let colors = if disable {
            [0; 4]
        } else {
            [0x8003, 0x8004, 0x8005, 0x8006]
        };
        let mut expected = colors.to_vec();
        expected.extend([0, 2, 12, 0x2000, 0x600D]);
        assert_eq!(run(&mut m, &image, 0x0600E600, 9), expected);
    }
}
#[test]
fn guest_character_word_rewrite_cannot_replace_already_fetched_nibbles() {
    let original = load(include_str!("fixtures/vdp1-character-fetch.hex"));
    let mut m = Machine::new();
    for (texture, color) in [(0x2222u16, 0x8012), (0x5555, 0x8015)] {
        let mut rom = original.clone();
        rom[0x100..0x102].copy_from_slice(&texture.to_be_bytes());
        let mut expected = vec![0x8011; 3];
        expected.extend([color; 4]);
        expected.extend([0x8014; 4]);
        expected.extend([2, 12, 0x2000, 0x600D]);
        for _ in 0..2 {
            assert_eq!(run(&mut m, &rom, 0x0600E500, 15), expected);
        }
    }
}

#[test]
fn guest_palette_rewrites_during_scaling_affect_only_the_next_lookup_sprite() {
    let original = load(include_str!("fixtures/vdp1-lookup-fetch.hex"));
    let mut m = Machine::new();
    for (old, new) in [(0x8000u16, 0x83E0u16), (0xFC00, 0x8010)] {
        let mut rom = original.clone();
        rom[0x100..0x102].copy_from_slice(&old.to_be_bytes());
        rom[0x102..0x104].copy_from_slice(&new.to_be_bytes());
        let mut expected = vec![u32::from(old + 1); 4];
        expected.extend((0..16).map(|i| u32::from(new + i)));
        expected.extend([2, 12, 0x2000, 0x600D]);
        for _ in 0..2 {
            assert_eq!(run(&mut m, &rom, 0x0600E400, 24), expected);
        }
    }
}

#[test]
fn guest_measures_gouraud_fetches_and_six_clock_shadow_and_translucent_pixels() {
    // ST-013 §5.3 tables5.2/5.3 and §6.3 color calculations. Sixteen
    // independent rows, 128 dots: six-clock RMW costs 640 additional clocks;
    // four Gouraud word reads add 128 clocks. FRT divides elapsed clocks by8.
    let original = load(include_str!("fixtures/vdp1-effects-timing.hex"));
    let mut m = Machine::new();
    let mut reference_times = Vec::new();
    for repeat in 0..2 {
        let mut times = Vec::new();
        for (mode, pixel) in [
            (0u16, 0x8010),
            (1, 0xBC00),
            (2, 0x8008),
            (3, 0xBC08),
            (4, 0x8010),
            (6, 0x8008),
            (7, 0xBC08),
        ] {
            let mut rom = original.clone();
            rom[0x100..0x102].copy_from_slice(&(0xC0 | mode).to_be_bytes());
            let r = run(&mut m, &rom, 0x0600E300, 8);
            assert_eq!(&r[1..], &[pixel, pixel, pixel, 2, 0x2000, 68, 0x600D]);
            times.push(r[0]);
        }
        assert_eq!(times[0], times[2]); // half-luminance reads no background
        assert_eq!(times[4], times[5]);
        for (a, b, minimum, maximum) in [
            (4, 0, 14, 20),
            (1, 0, 78, 84),
            (3, 0, 78, 84),
            (6, 4, 78, 84),
        ] {
            assert!(
                (minimum..=maximum).contains(&(times[a] - times[b])),
                "{times:?}"
            );
        }
        if repeat == 0 {
            reference_times = times;
        } else {
            assert_eq!(times, reference_times);
        }
    }
    // Mutate the real guest non-texture color; no host framebuffer writes.
    let mut changed = original;
    changed[0x102..0x104].copy_from_slice(&0x83E0u16.to_be_bytes());
    let r = run(&mut m, &changed, 0x0600E300, 8);
    assert_eq!(&r[1..], &[0x83E0, 0x83E0, 0x83E0, 2, 0x2000, 68, 0x600D]);
}

#[test]
fn guest_observes_partial_drawing_stop_transfer_over_and_unfinished_vblank_erase() {
    let rom = load(include_str!("fixtures/vdp1-progress.hex"));
    let expected: Vec<_> = include_str!("fixtures/vdp1-progress.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let mut m = Machine::new();
    for _ in 0..2 {
        assert_eq!(run(&mut m, &rom, 0x0600D800, 10), expected);
    }
    for (before, after, indices) in [(0x801F, 0x83E0, &[0][..]), (0x1234, 0x2468, &[6, 7][..])] {
        let mut changed = rom.clone();
        patch(&mut changed, before, after);
        let mut e = expected.clone();
        for &i in indices {
            e[i] = after;
        }
        assert_eq!(run(&mut m, &changed, 0x0600D800, 10), e);
    }
}

#[test]
fn guest_write_after_ctrl_fetch_does_not_replace_an_in_flight_vdp1_command() {
    let original = load(include_str!("fixtures/vdp1-command-fetch.hex"));
    let mut m = Machine::new();
    for color in [0x801Fu32, 0x83E0] {
        let mut rom = original.clone();
        patch(&mut rom, 0x801F, color);
        for _ in 0..2 {
            assert_eq!(
                run(&mut m, &rom, 0x0600E100, 5),
                [color, color, 8, 2, 0x600D]
            );
        }
    }
    // Move the same guest store before PTMR: the genuine END command is now
    // fetched first, so no clip or line executes and COPR stays zero.
    let mut before_start = original;
    let at: Vec<_> = before_start
        .windows(4)
        .enumerate()
        .filter_map(|(i, bytes)| (bytes == [0x24, 0x01, 0x25, 0x61]).then_some(i))
        .collect();
    assert_eq!(at.len(), 1);
    before_start[at[0]..at[0] + 4].copy_from_slice(&[0x25, 0x61, 0x24, 0x01]);
    assert_eq!(
        run(&mut m, &before_start, 0x0600E100, 5),
        [0, 0, 0, 2, 0x600D]
    );
}

#[test]
fn guest_frt_measures_framebuffer_ownership_pausing_drawing_but_independent_vram_does_not() {
    let original = load(include_str!("fixtures/vdp1-memory-ownership.hex"));
    let mut m = Machine::new();
    let mut times = Vec::new();
    for repeat in 0..2 {
        for color in [0x801Fu32, 0x83E0] {
            let mut pair = Vec::new();
            for framebuffer in [false, true] {
                let mut rom = original.clone();
                patch(&mut rom, 0x801F, color);
                if !framebuffer {
                    patch(&mut rom, 0x25C81000, 0x25C01000);
                }
                let r = run(&mut m, &rom, 0x0600E200, 5);
                assert_eq!(&r[1..], &[color, color, 2, 0x600D]);
                assert!(r[0] > 500 && r[0] < 2000, "{r:X?}");
                pair.push(r[0]);
            }
            assert!(
                pair[1] > pair[0] + 50,
                "framebuffer wait versus VRAM: {pair:?}"
            );
            if repeat == 0 && times.is_empty() {
                times = pair;
            } else {
                assert_eq!(pair, times);
            }
        }
    }
}
#[test]
fn guest_exclusive_modes_multiplex_adjacent_pixels_and_sample_480_line_counters() {
    let original = load(include_str!("fixtures/exclusive-monitor.hex"));
    let mut m = Machine::new();
    for (mode, width) in [
        (0x8004u32, 320),
        (0x8005, 352),
        (0x8006, 640),
        (0x8007, 704),
    ] {
        let mut rom = original.clone();
        patch(&mut rom, 0x8006, mode);
        for _ in 0..2 {
            let records = run(&mut m, &rom, 0x0600DC00, 5);
            assert_eq!(records, [8, 0, 480, mode, 0x600D]);
            assert_eq!(m.video_dimensions(), (width, 480));
            let f = m.framebuffer().unwrap();
            for y in [0, 479] {
                assert_eq!(
                    &f[y * width * 4..][..16],
                    &[
                        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255
                    ]
                );
            }
        }
    }
    // NBG1 supplies the odd output dots in exclusive high-resolution mode.
    // Change only its scroll operand; both SH-2 and video hardware still run.
    let mut changed = original.clone();
    // The sole literal 1 is pooled, so patching it would also change Y zoom.
    // Mutate the actual guest MOV.W write address from SCXIN1 to SCYIN1;
    // this removes the odd-dot +1 and visibly changes the pixel sequence.
    let addresses: Vec<_> = changed
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == 0x25F80080u32.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(addresses.len(), 2); // both conditional setup paths
    for at in addresses {
        changed[at..at + 4].copy_from_slice(&0x25F80084u32.to_be_bytes());
    }
    run(&mut m, &changed, 0x0600DC00, 5);
    let f = m.framebuffer().unwrap();
    assert_eq!(
        &f[..16],
        &[255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255]
    );
}

#[test]
fn guest_measures_grants_and_blocked_cycles_for_both_split_vram_ports_and_hires() {
    let rom = load(include_str!("fixtures/vram-grants.hex"));
    let mut m = Machine::new();
    for _ in 0..2 {
        let r = run(&mut m, &rom, 0x0600DE00, 7);
        // FRT /8: eight granted reads finish in <480 main clocks. A denied
        // port waits most of the 1,280-clock active line before HBlank.
        for i in [0, 2, 4] {
            assert!((1..60).contains(&r[i]), "{r:?}");
        }
        for i in [1, 3, 5] {
            assert!((80..220).contains(&r[i]), "{r:?}");
        }
    }
    // Leave BGON reset. All cycle-zero commands now target a disabled
    // layer and must release the CPU port, without host-side result writes.
    let mut changed = rom.clone();
    patch(&mut changed, 0x25F80020, 0x25F80022); // BGON stays reset (disabled)
    let r = run(&mut m, &changed, 0x0600DE00, 7);
    for value in &r[..6] {
        assert!((1..60).contains(value), "{r:?}");
    }
}

#[test]
fn color_depth_disables_only_the_documented_companion_layers_even_when_transparent() {
    use saturn_rust::{devices::VDP2, vdp2::Vdp2};
    // ST-058 §4.5 p.60. NBG0/1 are disabled, so they cannot hide the
    // companion pixel by priority or opaque color; this tests decoder sharing.
    for (layer, n0, n1, visible) in [
        (1, 0, 0, true),
        (1, 4, 0, false),
        (2, 0, 0, true),
        (2, 1, 0, true),
        (2, 2, 0, false),
        (2, 3, 0, false),
        (2, 4, 0, false),
        (3, 0, 0, true),
        (3, 2, 0, true),
        (3, 4, 0, false),
        (3, 0, 2, false),
        (3, 0, 3, false),
    ] {
        let mut v = Vdp2::default();
        for (offset, value) in [
            (0, 0x8000),
            (0xAC, 3),
            (0xAE, 0xF800),
            (0x20, 1 << layer),
            (0x28, (n0 << 4) | (n1 << 12)),
            (0x30 + layer * 2, 0x8000),
            (0xF8 + (layer / 2) * 2, 3 << ((layer & 1) * 8)),
            (0x88, 1),
            (0x8C, 1),
        ] {
            v.write16(VDP2 + offset, value).unwrap();
        }
        v.vram[..2].copy_from_slice(&0x20u16.to_be_bytes());
        v.vram[0x400] = 0x10;
        v.cram[2..4].copy_from_slice(&0x1Fu16.to_be_bytes());
        assert_eq!(
            &v.framebuffer().unwrap()[..4],
            if visible {
                &[255, 0, 0, 255]
            } else {
                &[0, 0, 0, 255]
            },
            "layer={layer}, NBG0 depth={n0}, NBG1 depth={n1}"
        );
    }
}

#[test]
fn guest_erase_register_capture_survives_pending_writes_and_cold_boots() {
    let original = load(include_str!("fixtures/vdp1-erase-latch.hex"));
    let mut m = Machine::new();
    for (a, b, left, right, write) in [
        (0x801Fu16, 0x83E0u16, 0x0402u16, 0x0803u16, 1u16),
        (0xFC00, 0xFFFF, 0x0202, 0x0603, 1),
        (0x801F, 0x83E0, 0x0402, 0x0803, 0),
    ] {
        let mut image = original.clone();
        for (i, word) in [a, b, left, right, write].into_iter().enumerate() {
            image[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        // Literal ST-013 §4.4 rectangles: A x8..24/y1..3; B either
        // x16..32/y2..4 or x8..24/y2..4. Omitted writes leave A captured.
        let second = if write == 0 { a } else { b };
        let expected_a = [
            208 * 0x8000 + 32 * u32::from(a),
            0x8000,
            u32::from(a),
            u32::from(a),
            0x8000,
            u32::from(a),
            0x8000,
            0x8000,
            0x8000,
        ];
        let samples_b = if write == 0 {
            expected_a
        } else if left == 0x0402 {
            [
                208 * 0x8000 + 32 * u32::from(b),
                0x8000,
                0x8000,
                u32::from(b),
                u32::from(b),
                u32::from(b),
                u32::from(b),
                0x8000,
                0x8000,
            ]
        } else {
            [
                208 * 0x8000 + 32 * u32::from(b),
                0x8000,
                0x8000,
                u32::from(b),
                0x8000,
                u32::from(b),
                0x8000,
                0x8000,
                0x8000,
            ]
        };
        let mut expected = expected_a.to_vec();
        expected.extend(samples_b);
        expected.extend([0x1010, 0, 0, 0, 0x600D]);
        for _ in 0..2 {
            assert_eq!(run(&mut m, &image, 0x0600E700, 23), expected);
            m.advance_frame().unwrap();
            let f = m.framebuffer().unwrap();
            for y in 0..6 {
                for x in 0..40 {
                    let inside = if write == 0 {
                        (8..24).contains(&x) && (1..3).contains(&y)
                    } else {
                        (if left == 0x0402 { 16..32 } else { 8..24 }).contains(&x)
                            && (2..4).contains(&y)
                    };
                    let rgb = if !inside {
                        [0, 0, 0, 255]
                    } else {
                        match second {
                            0x801F => [255, 0, 0, 255],
                            0x83E0 => [0, 255, 0, 255],
                            0xFFFF => [255; 4],
                            _ => unreachable!(),
                        }
                    };
                    assert_eq!(
                        &f[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4],
                        &rgb,
                        "x{x}, y{y}"
                    );
                }
            }
        }
    }
}
