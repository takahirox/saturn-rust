//! Original SYS_EXECDMP caller and mixed media, ST-162-R1 System Program 6.1.
//! Every control, TOC request, pixel and audio route executes in guest code.
use saturn_rust::{Boot, Machine, bus::Memory, media::Media};

const ISO: &[u8] = include_bytes!("fixtures/hle-player.iso");
const AUDIO: &[u8] = include_bytes!("fixtures/hle-player-audio.bin");
const CUE: &str = include_str!("fixtures/hle-player.cue");

fn boot(m: &mut Machine, audio: &[u8]) {
    m.reset_media(
        Media::cue(
            CUE,
            &[("hle-player.iso", ISO), ("hle-player-audio.bin", audio)],
        )
        .unwrap(),
    )
    .unwrap();
}
fn state(m: &Machine) -> [u32; 3] {
    std::array::from_fn(|i| m.bus().read32(0x06000F00 + i as u32 * 4).unwrap())
}
fn frames(m: &mut Machine, count: usize, buttons: u16) {
    m.set_buttons(buttons);
    for _ in 0..count {
        m.advance_frame().unwrap();
    }
}
fn ready(m: &mut Machine, expected: [u32; 3]) {
    for _ in 0..100 {
        frames(m, 1, 0);
        if state(m) == expected {
            frames(m, 2, 0);
            assert_eq!(m.bus().read32(0x06009010), Ok(0), "non-returning service");
            assert_eq!(m.bus().read32(0x06000C00), Ok(0x13579BDF));
            assert_eq!(m.backup_ram()[0], 42);
            return;
        }
    }
    panic!(
        "CD player startup: state={:?}, CPU={:?}",
        state(m),
        m.cpu_state()
    );
}
fn rail(m: &Machine, track: usize, playing: bool) {
    let pixels = m.framebuffer().unwrap();
    assert_eq!(m.video_dimensions(), (320, 224));
    for y in 96..128 {
        for x in 24..280 {
            let rgb = if (100..124).contains(&y) && (32 + track * 2..38 + track * 2).contains(&x) {
                if track == 0 {
                    [255, 0, 0]
                } else if playing {
                    [0, 255, 0]
                } else {
                    [255, 255, 0]
                }
            } else {
                [33, 33, 33]
            };
            assert_eq!(
                &pixels[(y * 320 + x) * 4..][..4],
                &[rgb[0], rgb[1], rgb[2], 255],
                "({x},{y})"
            );
        }
    }
    assert_eq!(&pixels[(32 * 320 + 32) * 4..][..4], &[255, 255, 255, 255]);
    assert_eq!(&pixels[(32 * 320 + 40) * 4..][..4], &[8, 16, 16, 255]);
}
fn signal(m: &mut Machine, stereo: [i16; 2]) {
    let samples = m.drain_audio(8192);
    assert!(samples.as_chunks::<2>().0.contains(&stereo));
    assert!(
        samples
            .as_chunks::<2>()
            .0
            .iter()
            .all(|frame| *frame == stereo || *frame == [0, 0])
    );
}

#[test]
fn application_returns_to_a_nonreturning_guest_cd_player_with_real_controls_audio_and_pixels() {
    let mut m = Machine::new();
    for _ in 0..2 {
        boot(&mut m, AUDIO);
        ready(&mut m, [2, 0, 3]);
        rail(&m, 2, false);
        frames(&mut m, 4, 64); // A
        assert_eq!(state(&m), [2, 1, 3]);
        rail(&m, 2, true);
        signal(&mut m, [4096, -8192]);
        frames(&mut m, 3, 16); // B, distinct edge while A is released
        assert_eq!(state(&m), [2, 0, 3]);
        rail(&m, 2, false);
        m.drain_audio(8192);
        frames(&mut m, 3, 0);
        assert!(m.drain_audio(8192).iter().all(|s| *s == 0));
        frames(&mut m, 4, 128); // Start resumes the actual paused FAD.
        signal(&mut m, [4096, -8192]);
        frames(&mut m, 4, 8); // right pauses, selects track3
        assert_eq!(state(&m), [3, 0, 3]);
        rail(&m, 3, false);
        frames(&mut m, 4, 8); // held key cannot select again
        assert_eq!(state(&m), [3, 0, 3]);
        frames(&mut m, 2, 0);
        m.drain_audio(8192);
        frames(&mut m, 4, 64);
        signal(&mut m, [-12288, 16384]);
        frames(&mut m, 4, 4); // left -> track2
        assert_eq!(state(&m), [2, 0, 3]);
        frames(&mut m, 2, 0);
        frames(&mut m, 4, 4); // wrap, skip data track1 -> track3
        assert_eq!(state(&m), [3, 0, 3]);
        frames(&mut m, 2, 0);
        frames(&mut m, 75, 64); // actual CD-DA end, no synthetic playing flag
        assert_eq!(state(&m), [3, 0, 3]);
        frames(&mut m, 2, 0);
        m.drain_audio(8192);
        frames(&mut m, 4, 64); // restart completed track from its own INDEX01
        assert_eq!(state(&m), [3, 1, 3]);
        signal(&mut m, [-12288, 16384]);
        assert_eq!(m.bus().read32(0x06009010), Ok(0));
        assert!((0x18000..0x1D000).contains(&m.cpu_state().pc));
    }
}

#[test]
fn guest_player_handles_data_only_media_and_changed_track_samples() {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    ready(&mut m, [0, 0, 1]);
    for buttons in [64, 0, 8, 0, 4, 0, 128, 16] {
        frames(&mut m, 4, buttons);
        assert_eq!(state(&m), [0, 0, 1]);
    }
    rail(&m, 0, false);
    assert!(m.drain_audio(8192).iter().all(|s| *s == 0));

    let mut changed = AUDIO.to_vec();
    for sample in changed.as_chunks_mut::<4>().0.iter_mut().take(44100) {
        sample.copy_from_slice(&[0, 0x20, 0, 0xC0]); // +8192,-16384, little endian
    }
    boot(&mut m, &changed);
    ready(&mut m, [2, 0, 3]);
    m.drain_audio(8192);
    frames(&mut m, 4, 64);
    signal(&mut m, [8192, -16384]);
    rail(&m, 2, true);
}

#[test]
fn guest_player_decodes_both_analog_report_ids_and_held_axes_through_intback() {
    let mut m = Machine::new();
    boot(&mut m, AUDIO);
    ready(&mut m, [2, 0, 3]);
    for pad3d in [false, true] {
        let axis = |m: &mut Machine, x| {
            if pad3d {
                m.set_3d_pad(0, Some([x, 128, 0, 0]));
            } else {
                m.set_analog(0, Some([x, 128, 0]));
            }
        };
        axis(&mut m, 255);
        frames(&mut m, 6, 0);
        assert_eq!(state(&m), [3, 0, 3]);
        frames(&mut m, 4, 0);
        assert_eq!(state(&m), [3, 0, 3]);
        axis(&mut m, 128);
        frames(&mut m, 3, 0);
        axis(&mut m, 0);
        frames(&mut m, 6, 0);
        assert_eq!(state(&m), [2, 0, 3]);
        axis(&mut m, 128);
        frames(&mut m, 3, 0);
        m.drain_audio(8192);
        frames(&mut m, 4, 64);
        assert_eq!(state(&m), [2, 1, 3]);
        signal(&mut m, [4096, -8192]);
        frames(&mut m, 4, 16);
        assert_eq!(state(&m), [2, 0, 3]);
    }
    rail(&m, 2, false);
}
