//! ST-013 §§1.2/4.1, ST-058 §6.3 p.159: independent sprite readout vectors.
use saturn_rust::{
    devices::VDP2,
    vdp1::{REGISTERS, Vdp1},
    vdp2::Vdp2,
};

fn reg(v: &mut Vdp2, offset: u32, value: u16) {
    v.write16(VDP2 + offset, value).unwrap();
}
fn table(v: &mut Vdp2, offset: usize, value: u32) {
    v.vram[0x10000 + offset..0x10004 + offset].copy_from_slice(&value.to_be_bytes());
}
fn setup(mode: u16) -> (Vdp1, Vdp2) {
    let mut s = Vdp1::default();
    s.write16(REGISTERS, mode).unwrap();
    s.write16(REGISTERS + 2, 3).unwrap();
    let mut v = Vdp2::default();
    reg(&mut v, 0, 0x8000);
    reg(&mut v, 0xBC, 0);
    reg(&mut v, 0xBE, 0x8040); // bit6 ignored, A base = 10000
    reg(&mut v, 0xF0, 3);
    reg(&mut v, 0xE0, if mode == 2 { 0x20 } else { 8 });
    for (i, color) in [0, 0x801F_u16, 0x83E0, 0xFC00, 0xFFFF]
        .into_iter()
        .enumerate()
    {
        v.cram[i * 2..i * 2 + 2].copy_from_slice(&color.to_be_bytes());
    }
    // Four distinct source corners, including the last framebuffer row.
    let (width, height, bytes) = if mode == 2 {
        (512, 256, 2)
    } else {
        (512, 512, 1)
    };
    for (x, y, color) in [(0, 0, 1), (1, 0, 2), (0, 1, 3), (511, height - 1, 4)] {
        let at = (y * width + x) * bytes;
        if bytes == 1 {
            s.draw_buffer_mut()[at] = color;
        } else {
            let rgb = [0, 0x801F_u16, 0x83E0, 0xFC00, 0xFFFF][usize::from(color)];
            s.draw_buffer_mut()[at..at + 2].copy_from_slice(&rgb.to_be_bytes());
        }
    }
    s.vblank_out();
    (s, v)
}
fn pixel(s: &Vdp1, v: &Vdp2, x: usize, y: usize) -> [u8; 4] {
    let f = v.composite(s).unwrap();
    f[(y * 320 + x) * 4..(y * 320 + x + 1) * 4]
        .try_into()
        .unwrap()
}
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255; 4];
const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn rotated_16_and_8_read_a_start_and_deltas_without_matrix_coefficients_or_wrap() {
    for mode in [2, 3] {
        let (s, mut v) = setup(mode);
        table(&mut v, 0x14, 1 << 16);
        table(&mut v, 0x10, 1 << 16);
        // B table and A matrix/scales must not control sprite coordinates.
        table(&mut v, 0x80, 511 << 16);
        table(&mut v, 0x1C, 0xFFFF0000);
        table(&mut v, 0x4C, 7 << 16);
        reg(&mut v, 0xB0, 1);
        reg(&mut v, 0xB4, 3);
        assert_eq!(pixel(&s, &v, 0, 0), RED);
        assert_eq!(pixel(&s, &v, 1, 0), GREEN);
        assert_eq!(pixel(&s, &v, 0, 1), BLUE);
        // Quarter turn: (source x, source y) = (screen y, 1-screen x).
        table(&mut v, 0x14, 0);
        table(&mut v, 0x18, 0xFFFF0000);
        table(&mut v, 0xC, 1 << 16);
        table(&mut v, 0x10, 0);
        table(&mut v, 4, 1 << 16);
        assert_eq!(pixel(&s, &v, 0, 0), BLUE);
        assert_eq!(pixel(&s, &v, 1, 0), RED);
        assert_eq!(pixel(&s, &v, 1, 1), GREEN);
        assert_eq!(pixel(&s, &v, 2, 0), BLACK);
        table(&mut v, 0, 511 << 16);
        table(&mut v, 4, if mode == 2 { 255 << 16 } else { 511 << 16 });
        assert_eq!(pixel(&s, &v, 0, 0), WHITE);
        table(&mut v, 0, 512 << 16);
        assert_eq!(pixel(&s, &v, 0, 0), BLACK);
        table(&mut v, 0, 0);
        table(&mut v, 4, if mode == 2 { 256 << 16 } else { 512 << 16 });
        assert_eq!(pixel(&s, &v, 0, 0), BLACK);
        table(&mut v, 4, 0);
        table(&mut v, 0, 0xFFFF0000);
        assert_eq!(pixel(&s, &v, 0, 0), BLACK);
    }
}
#[test]
fn sprite_link_quantizes_before_dot_accumulation_and_applies_shared_window_coordinates() {
    let (mut s, mut v) = setup(2);
    table(&mut v, 0x14, 0x00008040); // .500976... -> .5, never carry the discarded bit.
    table(&mut v, 0, 0x0000FF80); // .998046875
    assert_eq!(pixel(&s, &v, 0, 0), RED);
    assert_eq!(pixel(&s, &v, 2, 0), GREEN); // floor(.998046875 + 2*.5) = 1
    table(&mut v, 0, 0);
    table(&mut v, 0x14, 8 << 16); // Q2.9 signed delta wraps to zero.
    assert_eq!(pixel(&s, &v, 1, 0), RED);
    table(&mut v, 0x14, 0xFFFE0000); // -2, negative coordinates transparent
    assert_eq!(pixel(&s, &v, 1, 0), BLACK);
    // Sprite windows use the rotated word too, not the original screen word.
    s.write16(REGISTERS + 2, 3).unwrap();
    s.draw_buffer_mut()[2..4].copy_from_slice(&0x8000_u16.to_be_bytes());
    s.vblank_out();
    reg(&mut v, 0xE0, 0x12);
    table(&mut v, 0, 1 << 16);
    table(&mut v, 0x14, 0);
    // Enable NBG0 with sprite-window-inside and a green bitmap.
    reg(&mut v, 0xAE, 0x7800);
    reg(&mut v, 0x20, 0x101);
    reg(&mut v, 0x28, 0x32);
    reg(&mut v, 0xF8, 3);
    reg(&mut v, 0xD0, 0x20);
    v.vram[..2].copy_from_slice(&0x83E0_u16.to_be_bytes());
    assert_eq!(pixel(&s, &v, 0, 0), GREEN);
    table(&mut v, 0, 2 << 16);
    assert_eq!(pixel(&s, &v, 0, 0), BLACK);
}
