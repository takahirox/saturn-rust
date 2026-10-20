use saturn_rust::{Boot, Machine, bus::Memory};
const ISO: &[u8] = include_bytes!("../web/star-collector.iso");
fn state(m: &Machine) -> [u32; 5] {
    std::array::from_fn(|i| m.bus().read32(0x06010000 + i as u32 * 4).unwrap())
}
fn frames(m: &mut Machine, n: usize, buttons: u16) {
    m.set_buttons(buttons);
    for _ in 0..n {
        m.advance_frame().unwrap();
    }
}
#[test]
fn actual_disc_guest_moves_collects_completes_and_restarts() {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    assert_eq!(m.cpu_state().pc, 0x06002E20);
    frames(&mut m, 20, 0);
    assert_eq!(&state(&m)[..4], &[32, 112, 0, 0]);
    assert_eq!(m.cpu_state().instructions, m.frames() * 80_000);
    let initial = m.framebuffer().unwrap();
    frames(&mut m, 8, 8);
    assert_eq!(&state(&m)[..4], &[64, 112, 1, 0]);
    let collected = m.framebuffer().unwrap();
    assert_ne!(initial, collected);
    assert_eq!(&collected[(27 * 320 + 45) * 4..][..4], &[255, 255, 0, 255]);
    assert_ne!(
        &initial[(27 * 320 + 45) * 4..][..4],
        &collected[(27 * 320 + 45) * 4..][..4]
    );
    frames(&mut m, 48, 8);
    assert_eq!(&state(&m)[..4], &[256, 112, 5, 1]);
    let won = m.framebuffer().unwrap();
    assert_eq!(&won[(150 * 320 + 100) * 4..][..4], &[0, 255, 0, 255]);
    frames(&mut m, 4, 1);
    assert_eq!(&state(&m)[..4], &[256, 112, 5, 1]);
    frames(&mut m, 20, 128);
    frames(&mut m, 2, 0);
    assert_eq!(&state(&m)[..4], &[32, 112, 0, 0]);
    assert_eq!(initial, m.framebuffer().unwrap());
    assert!(m.cpu_state().instructions > 1_000_000);
    // A second sequence misses the star and cannot score just by advancing time.
    frames(&mut m, 4, 1);
    frames(&mut m, 8, 8);
    assert_eq!(&state(&m)[..4], &[64, 96, 0, 0]);
    frames(&mut m, 4, 2);
    assert_eq!(&state(&m)[..4], &[64, 112, 1, 0]);
}
