//! Independent ST-013-R3 ch.4 update timing and §§4.2/4.4 vectors.
//! Literal rectangles below use EW X units of sixteen bytes and inclusive Y.
use super::*;

fn reg(v: &mut Vdp1, offset: u32, value: u16) {
    v.write16(REGISTERS + offset, value).unwrap();
}
fn settings(v: &mut Vdp1, values: [u16; 3]) {
    for (offset, value) in [6, 8, 10].into_iter().zip(values) {
        reg(v, offset, value);
    }
}
const A: [u16; 3] = [0x1234, 0x0201, 0x0602]; // bytes16..48, rows1..=2
const B: [u16; 3] = [0x5678, 0x0402, 0x0803]; // bytes32..64, rows2..=3
const C: [u16; 3] = [0x9ABC, 0x0004, 0x0204]; // bytes0..16, row4

fn fixture(mode: u16, change: u16) -> Vdp1 {
    let mut v = Vdp1::default();
    reg(&mut v, 0, mode);
    reg(&mut v, 4, 0);
    settings(&mut v, A);
    v.buffers[0].fill(0xEE);
    v.buffers[1].fill(0xDD);
    reg(&mut v, 2, change);
    v.vblank_out(); // explicitly capture A and display physical buffer0
    v
}
fn rectangle(buffer: &[u8], stride: usize, color: [u8; 2], bounds: [usize; 4]) {
    let [left, right, top, bottom] = bounds;
    for y in 0..6 {
        for x in 0..80 {
            let expected = if (left..right).contains(&x) && (top..bottom).contains(&y) {
                color[x & 1]
            } else {
                0xEE
            };
            assert_eq!(buffer[y * stride + x], expected, "byte{x}, row{y}");
        }
    }
}

#[test]
fn vblank_erase_uses_captured_a_until_the_next_change_captures_b() {
    for mode in [8, 9, 11] {
        for change in [0, 3] {
            // automatic and manual change
            let mut v = fixture(mode, change);
            settings(&mut v, C);
            settings(&mut v, B); // last of multiple writes wins only at change
            reg(&mut v, 2, 2);
            v.vblank_out(); // manual erase/held fields cannot capture B
            v.vblank_out();
            v.vblank_in(1708, 39);
            v.advance(1280).unwrap();
            v.hblank_in();
            v.advance(2).unwrap();
            settings(&mut v, C); // cannot change accepted color/bounds/buffer
            reg(&mut v, 2, change); // request is deferred until VBlank-OUT
            v.advance(100).unwrap();
            let stride = match mode {
                8 | 9 => 1024,
                11 => 512,
                _ => unreachable!(),
            };
            rectangle(&v.buffers[0], stride, [0x12, 0x34], [16, 48, 1, 3]);
            assert!(v.buffers[1].iter().all(|&b| b == 0xDD));
            settings(&mut v, B);
            v.vblank_out(); // B now captured; display physical buffer1
            assert_eq!(v.draw, 0);
            v.buffers[1].fill(0xEE);
            settings(&mut v, C);
            v.vblank_in(1708, 39);
            v.advance(1280).unwrap();
            v.hblank_in();
            v.advance(300).unwrap();
            rectangle(&v.buffers[1], stride, [0x56, 0x78], [32, 64, 2, 4]);
            rectangle(&v.buffers[0], stride, [0x12, 0x34], [16, 48, 1, 3]);
        }
    }
}

#[test]
fn display_erase_manual_sequence_retains_a_and_captures_only_the_last_written_rectangle() {
    for mode in [0, 1] {
        let mut v = fixture(mode, 3);
        settings(&mut v, B);
        reg(&mut v, 2, 2);
        v.vblank_out(); // manual erase uses A, no change
        v.erase_display_line(1, 320);
        settings(&mut v, C);
        v.erase_display_line(2, 320);
        rectangle(&v.buffers[0], 1024, [0x12, 0x34], [16, 48, 1, 3]);
        v.buffers[0].fill(0xEE);
        v.vblank_out(); // request consumed: held erase must do nothing
        v.erase_display_line(2, 320);
        assert!(v.buffers[0].iter().all(|&b| b == 0xEE));
        reg(&mut v, 2, 3);
        v.vblank_out(); // capture C, no manual display erase on change
        v.buffers[1].fill(0xEE);
        v.erase_display_line(4, 320);
        assert!(v.buffers[1].iter().all(|&b| b == 0xEE));
        reg(&mut v, 2, 2);
        v.vblank_out();
        settings(&mut v, B);
        v.erase_display_line(4, 320);
        rectangle(&v.buffers[1], 1024, [0x9A, 0xBC], [0, 16, 4, 5]);
        assert_eq!(v.read16(REGISTERS + 0x16), Ok(0x1010 | mode));
    }
}

#[test]
fn automatic_display_erase_captures_at_change_and_follows_scanlines() {
    let mut v = fixture(0, 0);
    settings(&mut v, B);
    v.erase_display_line(1, 320);
    v.erase_display_line(2, 320);
    rectangle(&v.buffers[0], 1024, [0x12, 0x34], [16, 48, 1, 3]);
    v.vblank_out(); // automatic change captures B
    v.buffers[1].fill(0xEE);
    settings(&mut v, C);
    v.erase_display_line(2, 320);
    assert_eq!(v.buffers[1][3 * 1024 + 32], 0xEE);
    v.erase_display_line(3, 320);
    rectangle(&v.buffers[1], 1024, [0x56, 0x78], [32, 64, 2, 4]);
}

#[test]
fn vblank_out_cancels_partial_a_before_capturing_b_without_late_writes() {
    let mut v = fixture(8, 3);
    v.vblank_in(1708, 39);
    v.advance(1280).unwrap();
    v.hblank_in();
    v.advance(2).unwrap(); // first opportunity: exactly one sixteen-bit pixel
    settings(&mut v, B);
    reg(&mut v, 2, 3);
    v.vblank_out(); // cancel A before switching; capture B
    settings(&mut v, C);
    v.advance(10000).unwrap();
    assert_eq!(&v.buffers[0][1040..1044], &[0x12, 0x34, 0xEE, 0xEE]);
    assert_eq!(&v.buffers[0][2064..2068], &[0xEE; 4]);
    assert!(v.buffers[1].iter().all(|&b| b == 0xDD));
    v.buffers[1].fill(0xEE);
    v.vblank_in(1708, 39);
    v.advance(1280).unwrap();
    v.hblank_in();
    v.advance(300).unwrap();
    rectangle(&v.buffers[1], 1024, [0x56, 0x78], [32, 64, 2, 4]);
    assert_eq!(&v.buffers[0][1040..1044], &[0x12, 0x34, 0xEE, 0xEE]);
}

#[test]
fn modr_reports_written_settings_while_drawing_and_erase_latches_wait_for_change() {
    let mut v = fixture(8, 3);
    settings(&mut v, B);
    reg(&mut v, 2, 0x1F); // EOS/DIE/DIL and manual change request
    reg(&mut v, 4, 2);
    // §4.9: readback is the written mode, even before internal capture.
    assert_eq!(v.read16(REGISTERS + 0x16), Ok(0x11F8));
    assert_eq!(v.draw_control, 0);
    v.vblank_in(1708, 39);
    v.advance(1280).unwrap();
    v.hblank_in();
    v.advance(300).unwrap();
    rectangle(&v.buffers[0], 1024, [0x12, 0x34], [16, 48, 1, 3]);
    v.vblank_out();
    assert_eq!(v.draw_control, 0x1C);
    assert_eq!(v.read16(REGISTERS + 0x16), Ok(0x11F8));
    assert!(v.busy()); // automatic PTMR=2 start remains at change
}
