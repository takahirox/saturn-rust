//! Original guest disc. Results arise only from guest instructions/device MMIO.
use saturn_rust::{Boot, Machine, bus::Memory, media::Media};
fn media(iso: &[u8], audio: &[u8]) -> Media {
    Media::cue(
        include_str!("../web/hardware-lab.cue"),
        &[("hardware-lab.iso", iso), ("hardware-lab-audio.bin", audio)],
    )
    .unwrap()
}
fn run(m: &mut Machine) {
    for _ in 0..40 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(0x06010020) == Ok(0x600D) {
            return;
        }
    }
    panic!(
        "guest did not finish, pc={:08X}, results={:?}",
        m.cpu_state().pc,
        (0..16)
            .map(|i| m.bus().read32(0x06010000 + i * 4))
            .collect::<Vec<_>>()
    );
}
// ST-058-R2 §§6.1/6.4/8.1. A rotates (x,y)->(y,3-x),
// B translates (x,y)->(x+1,y). Coefficients choose A for x<2.
// NBG0's line window includes x=8..15 on even lines and x=10..13
// on odd lines; these authored expectations cover both edges and the lower layer.
const VIDEO_POINTS: [(usize, usize, [u8; 4]); 14] = [
    (0, 0, [255, 255, 255, 255]),
    (1, 0, [0, 0, 255, 255]),
    (2, 0, [255, 255, 255, 255]),
    (3, 0, [255, 0, 0, 255]),
    (0, 1, [255, 0, 0, 255]),
    (1, 1, [255, 255, 255, 255]),
    (2, 1, [255, 0, 0, 255]),
    (8, 0, [255, 0, 0, 255]),
    (9, 0, [0, 255, 0, 255]),
    (8, 1, [0, 0, 0, 255]),
    (10, 1, [0, 255, 0, 255]),
    (13, 1, [255, 0, 0, 255]),
    (14, 1, [0, 0, 0, 255]),
    (0, 8, [0, 0, 0, 255]),
];
fn assert_results(m: &Machine) {
    let r: Vec<_> = (0..11)
        .map(|i| m.bus().read32(0x06010000 + i * 4).unwrap())
        .collect();
    assert_eq!(r[0..6], [1, 1, 84, 35, 42, 393984]);
    assert!(r[6] >= 1);
    assert_eq!(r[7..9], [1, 0x600D]);
    assert_eq!(r[9] & 255, u32::from(m.backup_ram()[0]));
    assert_eq!(r[10], 0x900);
    assert_eq!(m.bus().read32(0x06011000), Ok(3));
    assert_eq!(m.bus().read32(0x060117FC), Ok(1536));
    assert!(m.slave_state().instructions > 5);
    assert!(m.sound_state().instructions > 20);
}
fn assert_video(m: &Machine) {
    let frame = m.framebuffer().unwrap();
    for (x, y, expected) in VIDEO_POINTS {
        assert_eq!(&frame[(y * 320 + x) * 4..][..4], expected, "({x},{y})");
    }
}
#[test]
fn mixed_disc_guest_computes_slave_dsp_dma_cd_sound_backup_and_video_results() {
    let iso = include_bytes!("../web/hardware-lab.iso");
    let audio = include_bytes!("../web/hardware-lab-audio.bin");
    let mut m = Machine::new();
    m.reset_media(media(iso, audio)).unwrap();
    run(&mut m);
    assert_results(&m);
    assert_eq!(m.bus().read32(0x06010024).unwrap() & 255, 1);
    assert_eq!(m.sound_state().d[0] & 65535, 0x2000);
    for _ in 0..3 {
        m.advance_frame().unwrap();
    }
    let samples = m.drain_audio(8192);
    assert!(
        samples
            .chunks(2)
            .any(|p| p[0] != 0 && p[1] != 0 && p[0] != p[1])
    );
    assert_video(&m);
    m.set_buttons(1 << 7);
    m.run_steps(50).unwrap();
    m.set_buttons(0);
    run(&mut m);
    assert_eq!(m.backup_ram()[0], 2);
    assert_results(&m);
    assert_video(&m);
    m.reset(Boot::HlePrototype).unwrap();
    assert_eq!(m.backup_ram()[0], 2);
}

#[test]
fn guest_uploaded_effects_program_changes_exact_stereo_samples_and_computed_result() {
    let original = include_bytes!("../web/hardware-lab.iso");
    let silent_cd = vec![0; include_bytes!("../web/hardware-lab-audio.bin").len()];
    for (coefficient, result, left) in
        [(0x4000u16, 0x900, 2304), (0x2000, 0x480, 1280), (0, 0, 256)]
    {
        let mut iso = original.to_vec();
        // Change only the immediate in the original 68k MOVE.W #coef,$100700.
        // No host state writes after boot, and no emulator-side disc detection.
        let signature = [0x33, 0xFC, 0x40, 0, 0, 0x10, 7, 0];
        let locations: Vec<_> = iso
            .windows(8)
            .enumerate()
            .filter_map(|(i, w)| (w == signature).then_some(i))
            .collect();
        assert_eq!(locations.len(), 1);
        iso[locations[0] + 2..locations[0] + 4].copy_from_slice(&coefficient.to_be_bytes());
        let mut machine = Machine::new();
        machine.reset_media(media(&iso, &silent_cd)).unwrap();
        run(&mut machine);
        assert_eq!(machine.bus().read32(0x06010028), Ok(result));
        // EFREG is cleared at each sample start and written by instruction27.
        // Video upload duration can put this host preview in that clear window.
        // Observe the real port over a bounded interval; do not assume a held
        // sample result before its DSP store (ST-077 fig5.1 / ST-228 Ref2).
        let mut observed = false;
        for _ in 0..128 {
            let port = machine.bus().read16(0x05B00EC2).unwrap();
            assert!(port == 0 || port == result as u16);
            observed |= port == result as u16;
            machine.run_steps(1).unwrap();
        }
        assert!(observed, "DSP never published {result:04X}");
        machine.drain_audio(8192);
        machine.advance_frame().unwrap();
        // The guest may finish immediately before a raster boundary. Include
        // one complete field so every rotation of the four-sample waveform
        // contains the full authored eight-value stereo vector.
        machine.advance_frame().unwrap();
        // Wet PCM is half/quarter/zero gain panned left; direct PCM retains
        // pan8 (left /16). Dry right is unchanged and CD samples are all zero.
        let expected = [left, 4096, -left, -4096, -left * 2, -8192, left * 2, 8192];
        let samples = machine.drain_audio(8192);
        assert!(
            samples.windows(8).any(|w| w == expected),
            "coefficient {coefficient:04X}, samples {:?}",
            &samples[..samples.len().min(24)]
        );
    }
}
#[test]
fn runtime_sector_mutation_changes_guest_sum_and_media_reset_repeats() {
    let mut iso = include_bytes!("../web/hardware-lab.iso").to_vec();
    iso[60 * 2048 + 3] = 9;
    let audio = include_bytes!("../web/hardware-lab-audio.bin");
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset_media(media(&iso, audio)).unwrap();
        run(&mut m);
        assert_eq!(m.bus().read32(0x06010014), Ok(393990));
    }
}

#[test]
fn disc_matrix_and_window_operands_change_independent_pixels() {
    let original = include_bytes!("../web/hardware-lab.iso");
    let audio = include_bytes!("../web/hardware-lab-audio.bin");
    for (address, before, after, points) in [
        (
            0x25E70048u32,
            3u32 << 16,
            2u32 << 16,
            vec![
                (0, 0, [0, 0, 255, 255]),
                (1, 0, [0, 255, 0, 255]),
                (2, 0, [255, 255, 255, 255]),
            ],
        ),
        (
            0x25E69000,
            16 << 16 | 30,
            20 << 16 | 30,
            vec![
                (8, 0, [0, 0, 0, 255]),
                (9, 0, [0, 0, 0, 255]),
                (10, 0, [255, 0, 0, 255]),
                (15, 0, [0, 255, 0, 255]),
            ],
        ),
    ] {
        let mut iso = original.to_vec();
        // Only the guest's literal after LOAD <address>,R1; LOAD <value>,R0.
        // Search inside the video routine, excluding runtime sector data.
        let matches: Vec<_> = (21 * 2048 + 0x2200..21 * 2048 + 0x6000)
            .filter(|&i| iso[i..i + 4] == address.to_be_bytes())
            .collect();
        assert_eq!(matches.len(), 1);
        let at = matches[0] + 12;
        assert_eq!(&iso[at..at + 4], before.to_be_bytes());
        iso[at..at + 4].copy_from_slice(&after.to_be_bytes());
        let mut m = Machine::new();
        m.reset_media(media(&iso, audio)).unwrap();
        run(&mut m);
        assert_results(&m);
        // Scene creation may finish after its first visible scanlines. Wait
        // for a complete field containing the new guest matrix/window data.
        m.advance_frame().unwrap();
        m.advance_frame().unwrap();
        let frame = m.framebuffer().unwrap();
        for (x, y, expected) in points {
            assert_eq!(
                &frame[(y * 320 + x) * 4..][..4],
                expected,
                "operand={address:X},({x},{y})"
            );
        }
    }
}
