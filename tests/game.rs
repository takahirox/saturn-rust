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
fn ready(m: &mut Machine) {
    for _ in 0..80 {
        frames(m, 1, 0);
        if state(m)[4] >= 2 {
            return;
        }
    }
    panic!("Guest did not finish its own bitmap initialization");
}
#[test]
fn actual_disc_guest_moves_collects_completes_and_restarts() {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    assert_eq!(m.cpu_state().pc, 0x100); // Original HLE reset precedes AIP.
    ready(&mut m);
    assert_eq!(&state(&m)[..4], &[32, 112, 0, 0]);
    assert!(m.cpu_state().instructions * 8 < m.clocks()); // External BSC Tw clocks.
    assert!(m.clocks() >= m.frames() * saturn_rust::timing::CLOCKS_PER_FIELD);
    // At most one fetched instruction with one operand access crosses a field:
    // eight issue clocks plus two external accesses with six Tw clocks each.
    assert!(m.clocks() < m.frames() * saturn_rust::timing::CLOCKS_PER_FIELD + 20);
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
    for _ in 0..80 {
        frames(&mut m, 1, 8);
        if state(&m)[3] == 1 {
            break;
        }
    }
    assert_eq!(&state(&m)[..4], &[256, 112, 5, 1]);
    let won = m.framebuffer().unwrap();
    assert_eq!(&won[(150 * 320 + 100) * 4..][..4], &[0, 255, 0, 255]);
    frames(&mut m, 4, 1);
    assert_eq!(&state(&m)[..4], &[256, 112, 5, 1]);
    frames(&mut m, 40, 128); // Includes guest clearing VRAM with BSC waits.
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
