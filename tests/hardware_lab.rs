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
#[test]
fn mixed_disc_guest_computes_slave_dsp_dma_cd_sound_backup_and_video_results() {
    let iso = include_bytes!("../web/hardware-lab.iso");
    let audio = include_bytes!("../web/hardware-lab-audio.bin");
    let mut m = Machine::new();
    m.reset_media(media(iso, audio)).unwrap();
    run(&mut m);
    let r: Vec<_> = (0..10)
        .map(|i| m.bus().read32(0x06010000 + i * 4).unwrap())
        .collect();
    assert_eq!(r[0..6], [1, 1, 84, 35, 42, 393984]);
    assert!(r[6] >= 1);
    assert_eq!(r[7], 1);
    assert_eq!(r[8], 0x600D);
    assert_eq!(r[9] & 255, 1);
    assert_eq!(m.bus().read32(0x06010028), Ok(0x900));
    assert_eq!(m.bus().read32(0x06011000), Ok(3));
    assert_eq!(m.bus().read32(0x060117FC), Ok(1536));
    assert!(m.slave_state().instructions > 5);
    assert_eq!(m.sound_state().d[0] & 65535, 0x2000);
    assert!(m.sound_state().instructions > 20);
    for _ in 0..3 {
        m.advance_frame().unwrap();
    }
    let samples = m.drain_audio(8192);
    assert!(
        samples
            .chunks(2)
            .any(|p| p[0] != 0 && p[1] != 0 && p[0] != p[1])
    );
    let pixels = m.framebuffer().unwrap();
    assert_eq!(&pixels[0..4], &[255, 255, 255, 255]);
    m.set_buttons(1 << 7);
    m.run_steps(50).unwrap();
    m.set_buttons(0);
    run(&mut m);
    assert_eq!(m.backup_ram()[0], 2);
    assert_eq!(m.bus().read32(0x06010028), Ok(0x900));
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
        assert_eq!(machine.bus().read16(0x05B00EC2), Ok(result as u16));
        machine.drain_audio(8192);
        machine.advance_frame().unwrap();
        // Wet PCM is half/quarter/zero gain panned left; direct PCM retains
        // pan8 (left /16). Dry right is unchanged and CD samples are all zero.
        let expected = [left, 4096, -left, -4096, -left * 2, -8192, left * 2, 8192];
        assert!(machine.drain_audio(8192).windows(8).any(|w| w == expected));
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
