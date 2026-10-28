//! Legal manual/persistent VBE sequences and explicitly separate bad-window probes.
use super::*;
fn reg(v: &mut Vdp1, offset: u32, value: u16) {
    v.write16(REGISTERS + offset, value).unwrap();
}
fn sample(v: &mut Vdp1) {
    v.vblank_in(1708, 39);
    v.advance(1280).unwrap();
    v.hblank_in();
}
#[test]
fn one_sample_one_job_persistence_then_legal_disable_preserves_both_physical_buffers() {
    for mode in [0, 1, 2, 3] {
        let mut v = Vdp1::default();
        reg(&mut v, 0, mode);
        for (o, n) in [(6, 0x1234), (8, 0x0201), (10, 0x0602), (2, 3)] {
            reg(&mut v, o, n);
        }
        v.vblank_out(); // captured A; buffer0 display
        v.buffers[0].fill(0xEE);
        v.buffers[1].fill(0xDD);
        v.vblank_in(1708, 39);
        reg(&mut v, 0, mode | 8);
        reg(&mut v, 0, mode);
        reg(&mut v, 0, mode | 8);
        v.advance(1280).unwrap();
        assert!(v.erasing.is_none());
        v.hblank_in();
        assert!(v.vbe_effective);
        assert_eq!(v.erasing.as_ref().unwrap().area.buffer, 0);
        v.advance(2).unwrap();
        assert_eq!(v.erasing.as_ref().unwrap().words_left, 58811);
        // Registers may change while accepted work is in flight; they cannot
        // replace the captured color, bounds or physical destination.
        for (o, n) in [(6, 0x5678), (8, 0x0402), (10, 0x0803)] {
            reg(&mut v, o, n);
        }
        v.hblank_in(); // later raster interrupts never restart an accepted job
        assert_eq!(v.erasing.as_ref().unwrap().words_left, 58811);
        v.advance(100).unwrap();
        assert!(v.erasing.is_none());
        let stride = if mode == 3 { 512 } else { 1024 };
        assert_eq!(
            &v.buffers[0][stride + 16..stride + 20],
            &[0x12, 0x34, 0x12, 0x34]
        );
        v.buffers[0][stride + 16..stride + 18].copy_from_slice(&[0xAB, 0xCD]);
        v.hblank_in();
        v.advance(100).unwrap();
        assert_eq!(&v.buffers[0][stride + 16..stride + 18], &[0xAB, 0xCD]);
        reg(&mut v, 2, 3);
        v.vblank_out(); // capture B; display physical1
        sample(&mut v); // no TVMR write: persistent enable
        v.advance(100).unwrap();
        assert_eq!(
            &v.buffers[1][2 * stride + 32..2 * stride + 36],
            &[0x56, 0x78, 0x56, 0x78]
        );
        v.vblank_out(); // no repeated manual swap
        assert_eq!(v.draw, 0);
        reg(&mut v, 0, mode); // §4.2 immediate post-VBlank-out disable
        sample(&mut v);
        assert!(!v.vbe_effective);
        // Rotation's automatic erase remains independent of written VBE.
        assert_eq!(v.erasing.is_some(), mode & 2 != 0);
        assert_eq!(v.read16(REGISTERS + 0x16).unwrap() & 15, mode);
        assert!(!v.running);
        assert_eq!(v.edsr, 0);
    }
}
#[test]
fn prohibited_window_writes_are_deterministic_without_claiming_hardware_results() {
    let mut v = Vdp1::default();
    reg(&mut v, 6, 0x1234);
    reg(&mut v, 10, 0x0200);
    reg(&mut v, 2, 3);
    v.vblank_out();
    sample(&mut v); // sampled disabled
    reg(&mut v, 0, 8);
    reg(&mut v, 6, 0x5678);
    v.hblank_in();
    v.advance(10000).unwrap();
    assert!(v.erasing.is_none());
    assert_eq!(v.display_pixel(0, 0), 0);
}
