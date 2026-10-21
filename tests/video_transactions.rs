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
