//! Relaxed-timing playback runs the same original guests as the exact
//! scheduler. Results that do not depend on cycle-exact interleaving must
//! match: game logic, slave/DSP/DMA/CD/sound computations and video output.
use saturn_rust::{Boot, Machine, bus::Memory, media::Media};

fn star_state(m: &Machine) -> [u32; 4] {
    std::array::from_fn(|i| m.bus().read32(0x06010000 + i as u32 * 4).unwrap())
}

fn fields(m: &mut Machine, n: usize, buttons: u16) {
    m.set_buttons(buttons);
    for _ in 0..n {
        m.advance_frame().unwrap();
    }
}

#[test]
fn fast_playback_plays_the_original_disc_game_like_the_exact_scheduler() {
    let iso = include_bytes!("../web/star-collector.iso");
    let mut m = Machine::new();
    m.set_fast_mode(true);
    m.reset(Boot::HleDisc(iso)).unwrap();
    assert!(m.fast_mode());
    for _ in 0..80 {
        fields(&mut m, 1, 0);
        if m.bus().read32(0x06010010).unwrap() >= 2 {
            break;
        }
    }
    fields(&mut m, 2, 0);
    assert_eq!(star_state(&m), [32, 112, 0, 0]);
    let initial = m.framebuffer().unwrap();
    fields(&mut m, 8, 8);
    assert_eq!(star_state(&m), [64, 112, 1, 0]);
    fields(&mut m, 2, 0);
    let collected = m.framebuffer().unwrap();
    assert_eq!(&collected[(27 * 320 + 45) * 4..][..4], &[255, 255, 0, 255]);
    assert_ne!(initial, collected);
    for _ in 0..80 {
        fields(&mut m, 1, 8);
        if star_state(&m)[3] == 1 {
            break;
        }
    }
    assert_eq!(star_state(&m), [256, 112, 5, 1]);
    fields(&mut m, 2, 0);
    let won = m.framebuffer().unwrap();
    assert_eq!(&won[(150 * 320 + 100) * 4..][..4], &[0, 255, 0, 255]);
    // Guest time still follows the raster: one field per call.
    assert!(m.clocks() >= m.frames() * saturn_rust::timing::CLOCKS_PER_FIELD);
}

fn hardware_lab() -> Media {
    Media::cue(
        include_str!("../web/hardware-lab.cue"),
        &[
            (
                "hardware-lab.iso",
                include_bytes!("../web/hardware-lab.iso"),
            ),
            (
                "hardware-lab-audio.bin",
                include_bytes!("../web/hardware-lab-audio.bin"),
            ),
        ],
    )
    .unwrap()
}

fn lab_results(m: &Machine) -> Vec<u32> {
    (0..11)
        .map(|i| m.bus().read32(0x06010000 + i * 4).unwrap())
        .collect()
}

#[test]
fn fast_playback_completes_the_mixed_hardware_guest_with_exact_results() {
    let mut exact = Machine::new();
    exact.reset_media(hardware_lab()).unwrap();
    let mut fast = Machine::new();
    fast.set_fast_mode(true);
    fast.reset_media(hardware_lab()).unwrap();
    let mut finished = [None, None];
    for field in 0..600 {
        for (i, m) in [&mut exact, &mut fast].into_iter().enumerate() {
            if finished[i].is_none() {
                m.advance_frame().unwrap();
                m.drain_audio(8192);
                if m.bus().read32(0x06010020) == Ok(0x600D) {
                    finished[i] = Some(field);
                }
            }
        }
        if finished.iter().all(Option::is_some) {
            break;
        }
    }
    assert!(finished.iter().all(Option::is_some), "{finished:?}");
    let (expected, actual) = (lab_results(&exact), lab_results(&fast));
    // r[6] counts guest polling iterations, which depend on interleaving.
    assert_eq!(expected[..6], actual[..6]);
    assert!(actual[6] >= 1);
    assert_eq!(expected[7..], actual[7..]);
    assert_eq!(fast.bus().read32(0x06011000), Ok(3));
    assert_eq!(fast.bus().read32(0x060117FC), Ok(1536));
    assert!(fast.slave_state().instructions > 5);
    assert!(fast.sound_state().instructions > 20);
    for m in [&mut exact, &mut fast] {
        for _ in 0..3 {
            m.advance_frame().unwrap();
        }
    }
    // Same authored video, and the guest's stereo waveform reaches the output.
    assert_eq!(exact.framebuffer().unwrap(), fast.framebuffer().unwrap());
    let samples = fast.drain_audio(8192);
    assert!(
        samples
            .chunks(2)
            .any(|p| p[0] != 0 && p[1] != 0 && p[0] != p[1])
    );
}

#[test]
fn debugger_steps_after_fast_fields_use_the_exact_scheduler() {
    let iso = include_bytes!("../web/star-collector.iso");
    let mut m = Machine::new();
    m.set_fast_mode(true);
    m.reset(Boot::HleDisc(iso)).unwrap();
    fields(&mut m, 20, 0);
    let before = m.cpu_state().instructions;
    m.run_steps(100).unwrap();
    assert!(m.cpu_state().instructions > before);
    // Playback resumes from the exact scheduler's CPU state.
    fields(&mut m, 20, 0);
    assert!(m.fault().is_none());
    m.set_fast_mode(false);
    fields(&mut m, 2, 0);
    assert!(m.fault().is_none());
}
