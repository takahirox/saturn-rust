//! Integration with retained memory words, independent of sampling helpers.
use super::*;
fn word(v: &mut Vdp1, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn setup(mode: u16) -> Vdp1 {
    let mut v = Vdp1::default();
    for (at, value) in [
        (0, 9),
        (20, 511),
        (22, 255),
        (32, 1),
        (36, 0xC0 | mode << 3),
        (38, if mode == 1 { 0x200 } else { 0x100 }),
        (40, 0x100),
        (42, 0x101),
        (52, 3),
        (64, 0x8000),
    ] {
        word(&mut v, at, value);
    }
    for i in 0..16 {
        word(&mut v, 0x1000 + i * 2, 0x9000 + i as u16);
    }
    for i in 0..8 {
        word(&mut v, 0x800 + i * 2, 0x1234);
    }
    v.write16(REGISTERS + 2, 2).unwrap();
    v.write16(REGISTERS + 4, 1).unwrap();
    v
}
fn advance(v: &mut Vdp1, clocks: u32, chunk: u32, vr: bool, fb: bool) -> bool {
    let mut left = clocks;
    let mut end = false;
    while left > 0 {
        let n = left.min(chunk);
        end |= v.advance_with_bus(n, vr, fb).unwrap();
        left -= n;
    }
    end
}
fn pixel(v: &Vdp1, x: usize) -> u16 {
    u16::from_be_bytes(v.draw_buffer()[x * 2..x * 2 + 2].try_into().unwrap())
}
#[test]
fn scaled_words_acquire_at_the_deadline_and_retain_packed_samples_under_contention() {
    for chunk in [1, 3, 17, 10000] {
        for mode in 0..=5 {
            for boundary in [0, 1, 2] {
                let mut v = setup(mode);
                let start = 64 + if mode == 1 { 32 } else { 0 };
                advance(&mut v, start, chunk, false, false);
                let address = if mode == 5 { 0x802 } else { 0x800 }; // first source = 1
                // Write after zero/one/two acquisition clocks, with contention
                // retaining the in-flight phase or completed operand.
                advance(&mut v, boundary, chunk, false, true);
                advance(&mut v, 101, chunk, true, true);
                word(&mut v, address, 0xABCD);
                advance(&mut v, 2 - boundary, chunk, false, true);
                assert_eq!(
                    v.drawing.as_ref().unwrap().texture,
                    Some((address, if boundary == 2 { 0x1234 } else { 0xABCD }))
                );
                word(&mut v, address, 0x5678);
                let snapshot = (v.phase, v.copr, v.drawing.as_ref().unwrap().texture);
                for _ in 0..4 {
                    let _ = v.read16(REGISTERS + 0x10).unwrap();
                    let _ = v.display_pixel(0, 0);
                    let _ = v.draw_buffer();
                    let _ = v.vram[address];
                }
                assert_eq!(
                    (v.phase, v.copr, v.drawing.as_ref().unwrap().texture),
                    snapshot
                );
                advance(&mut v, 100, chunk, false, true);
                assert_eq!(pixel(&v, 0), 0);
                advance(&mut v, 1, chunk, true, false); // framebuffer publishes retained operand
                let raw = if boundary == 2 { 0x1234 } else { 0xABCD };
                let expected = match mode {
                    0 => 0x100 | (raw >> 8 & 15),
                    1 => 0x9000 | (raw >> 8 & 15),
                    2 => 0x100 | (raw & 63),
                    3 => 0x100 | (raw & 127),
                    4 => 0x100 | (raw & 255),
                    _ => raw,
                };
                assert_eq!(
                    pixel(&v, 0),
                    expected,
                    "mode={mode} boundary={boundary} chunk={chunk}"
                );
                if mode <= 1 {
                    // Source 3 shares the retained word with source 1.
                    advance(&mut v, 1, chunk, true, false);
                    assert_eq!(
                        pixel(&v, 1),
                        if mode == 0 {
                            0x100 | (raw & 15)
                        } else {
                            0x9000 | (raw & 15)
                        }
                    );
                }
                assert!(advance(&mut v, 100, chunk, false, false));
                let completed = v.draw_buffer().to_vec();
                assert!(!advance(&mut v, 100, chunk, false, false));
                assert_eq!(v.draw_buffer(), completed);
                assert_eq!(v.edsr, 2);
            }
        }
    }
}
#[test]
fn stop_restart_and_reset_discard_scaled_sampling_and_unpublished_operands() {
    for chunk in [1, 3, 17, 10000] {
        for cancellation in 0..3 {
            for elapsed in [65, 66, 67] {
                let mut v = setup(0);
                advance(&mut v, elapsed, chunk, false, false);
                match cancellation {
                    0 => {
                        v.write16(REGISTERS + 12, 0).unwrap();
                        advance(&mut v, 30, chunk, true, true);
                        assert!(!v.busy());
                        assert_eq!(v.edsr, 0);
                    }
                    1 => {
                        word(&mut v, 0, 0x8000);
                        v.write16(REGISTERS + 4, 1).unwrap();
                        assert!(advance(&mut v, 32, chunk, false, false));
                    }
                    _ => {
                        let mut bus = crate::bus::Bus::default();
                        bus.vdp1 = v;
                        bus.system_reset();
                        v = bus.vdp1;
                    }
                }
                let captured = v.draw_buffer().to_vec();
                assert!(!advance(&mut v, 100, chunk, false, false));
                assert_eq!(v.draw_buffer(), captured);
                assert!(v.drawing.is_none());
                assert!(v.pending_pixel.is_none());
                // Fresh commands/texture after cancellation begin at source 1.
                word(&mut v, 0, 9);
                word(&mut v, 20, 511);
                word(&mut v, 22, 255);
                for (at, value) in [
                    (32, 1),
                    (36, 0xC0),
                    (38, 0x100),
                    (40, 0x100),
                    (42, 0x101),
                    (52, 3),
                    (64, 0x8000),
                ] {
                    word(&mut v, at, value);
                }
                for i in 0..8 {
                    word(&mut v, 0x800 + i * 2, 0x5678);
                }
                v.draw_buffer_mut().fill(0);
                v.write16(REGISTERS + 4, 1).unwrap();
                assert!(advance(&mut v, 120, chunk, false, false));
                assert_eq!(
                    (0..4).map(|x| pixel(&v, x)).collect::<Vec<_>>(),
                    [0x106, 0x108, 0x106, 0x108]
                );
            }
        }
    }
}
