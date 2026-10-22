//! Original reproducible guest disc, with identical browser/Wasm assertions.
use saturn_rust::{Boot, Machine, bus::Memory};
const ISO: &[u8] = include_bytes!("../web/prism-courier.iso");
fn state(m: &Machine) -> [u32; 5] {
    std::array::from_fn(|i| m.bus().read32(0x06018000 + i as u32 * 4).unwrap())
}
fn frames(m: &mut Machine, count: usize, buttons: u16) -> bool {
    m.set_buttons(buttons);
    let mut stereo = false;
    for _ in 0..count {
        m.advance_frame().unwrap();
        stereo |= m
            .drain_audio(8192)
            .as_chunks::<2>()
            .0
            .iter()
            .any(|p| p[0] != 0 && p[1] != 0 && p[0] != p[1]);
    }
    stereo
}
fn pixel(m: &Machine, x: usize, y: usize) -> Vec<u8> {
    m.framebuffer().unwrap()[(y * 320 + x) * 4..][..4].to_vec()
}
fn ready(m: &mut Machine) -> bool {
    let mut stereo = false;
    for _ in 0..100 {
        stereo |= frames(m, 1, 0);
        if state(m)[4] > 12 {
            return stereo;
        }
    }
    panic!("Guest did not finish its own video/sound initialization");
}

#[test]
fn original_disc_runs_guest_vdp1_vdp2_sound_input_loss_win_and_restart() {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    assert!(ready(&mut m), "Guest 68k must compute nonzero stereo");
    assert_eq!(&state(&m)[..4], &[24, 112, 0, 0]);
    assert!(m.sound_state().instructions >= 27);
    assert_eq!(m.sound_state().last_vector, Some(28));
    assert_eq!(m.bus().read32(0x05A04000).unwrap(), 0x1000F000);
    assert!(m.bus().read32(0x06018014).unwrap() > 10);
    assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 0x4D);
    assert_eq!(pixel(&m, 27, 115), [0, 255, 255, 255]);
    assert_eq!(pixel(&m, 150, 50), [255, 0, 0, 255]);
    assert_eq!(pixel(&m, 285, 110), [0, 255, 0, 255]);
    let initial = m.framebuffer().unwrap();
    assert_eq!(m.audio_dropped(), 0);
    frames(&mut m, 12, 1); // steer into the upper barrier
    frames(&mut m, 32, 8);
    assert_eq!(&state(&m)[..4], &[140, 64, 3, 2]);
    assert_eq!(pixel(&m, 100, 20), [255, 0, 0, 255]);
    frames(&mut m, 2, 128);
    frames(&mut m, 3, 0);
    assert_eq!(&state(&m)[..4], &[24, 112, 0, 0]);
    assert_eq!(m.framebuffer().unwrap(), initial);
    frames(&mut m, 64, 8); // stay within the clear central passage
    frames(&mut m, 2, 0);
    assert_eq!(&state(&m)[..4], &[280, 112, 7, 1]);
    assert_eq!(pixel(&m, 100, 20), [0, 255, 0, 255]);
    frames(&mut m, 3, 1);
    assert_eq!(&state(&m)[..4], &[280, 112, 7, 1]);
    frames(&mut m, 2, 128);
    frames(&mut m, 3, 0);
    assert_eq!(&state(&m)[..4], &[24, 112, 0, 0]);
    assert_eq!(m.framebuffer().unwrap(), initial);
    m.reset(Boot::HleDisc(ISO)).unwrap();
    ready(&mut m);
    assert_eq!(m.framebuffer().unwrap(), initial);
}

#[test]
fn modified_guest_texture_changes_pixels_without_host_game_detection() {
    let mut disc = ISO.to_vec();
    for word in disc[21 * 2048 + 0x2400..21 * 2048 + 0x2480]
        .as_chunks_mut::<2>()
        .0
    {
        if *word != [0, 0] {
            word.copy_from_slice(&0xFC00u16.to_be_bytes());
        }
    }
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&disc)).unwrap();
    ready(&mut m);
    assert_eq!(&state(&m)[..4], &[24, 112, 0, 0]);
    assert_eq!(pixel(&m, 27, 115), [0, 0, 255, 255]);
}
