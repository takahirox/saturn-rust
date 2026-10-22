//! Independent ST-013-R3 §6.3 p.83/fig.6.6 line clipping vectors.
use super::*;

fn command(v: &mut Vdp1, at: usize, values: &[(usize, u16)]) {
    for &(offset, value) in values {
        v.vram[at + offset..at + offset + 2].copy_from_slice(&value.to_be_bytes());
    }
}
fn pixel(v: &Vdp1, x: usize, y: usize) -> u16 {
    let at = (y * 512 + x) * 2;
    u16::from_be_bytes(v.draw_buffer()[at..at + 2].try_into().unwrap())
}
fn finish(v: &mut Vdp1, chunk: u32) -> u32 {
    let mut time = 0;
    while v.busy() {
        let n = chunk.min(v.clocks_to_event());
        v.advance(n).unwrap();
        time += n;
        assert!(time < 10000);
    }
    time
}
fn line(a: (i16, i16), b: (i16, i16), disable: bool) -> Vdp1 {
    let mut v = Vdp1::default();
    command(&mut v, 0, &[(0, 9), (20, 10), (22, 10)]);
    command(
        &mut v,
        32,
        &[
            (0, 6),
            (4, 0xC0 | if disable { 0x800 } else { 0 }),
            (6, 0x801F),
            (12, a.0 as u16),
            (14, a.1 as u16),
            (16, b.0 as u16),
            (18, b.1 as u16),
        ],
    );
    command(&mut v, 64, &[(0, 0x8000)]);
    v.write16(REGISTERS + 2, 2).unwrap();
    v.write16(REGISTERS + 4, 1).unwrap();
    v
}

#[test]
fn extreme_axis_lines_keep_pixels_and_avoid_offscreen_memory_cycles() {
    for chunk in [1, 3, 17, 10000] {
        for (a, b) in [
            ((-4096, 5), (4095, 5)),
            ((4095, 5), (-4096, 5)),
            ((5, -4096), (5, 4095)),
            ((5, 4095), (5, -4096)),
        ] {
            let mut clipped = line(a, b, false);
            let mut walked = line(a, b, true);
            assert_eq!(finish(&mut clipped, chunk), 96 + 11);
            assert_eq!(finish(&mut walked, chunk), 96 + 8192);
            assert_eq!(clipped.draw_buffer(), walked.draw_buffer());
            assert_eq!(pixel(&clipped, 5, 5), 0x801F);
            assert_eq!(clipped.read16(REGISTERS + 0x10), Ok(2));
        }
    }
}

#[test]
fn separated_diagonals_and_empty_clip_skip_without_texture_or_framebuffer_ownership() {
    for (a, b) in [
        ((-7, -2), (-1, 11)),
        ((11, -2), (12, 11)),
        ((-2, -7), (11, -1)),
        ((-2, 11), (11, 12)),
    ] {
        let mut v = line(a, b, false);
        v.advance(64).unwrap();
        // An entirely rejected raster line has no framebuffer transaction.
        assert!(!v.advance_with_bus(1, true, true).unwrap());
        assert!(v.drawing.is_none());
        assert!(v.draw_buffer().iter().all(|&b| b == 0));
        assert_eq!(finish(&mut v, 1), 32);
    }
    let mut crossing = line((-5, 5), (15, 5), false);
    // Inside user area [3,7] intersects the system clip [0,10].
    crossing.user = [Point { x: 3, y: 0 }, Point { x: 7, y: 10 }];
    command(&mut crossing, 32, &[(4, 0x4C0)]);
    assert_eq!(finish(&mut crossing, 1), 96 + 5);
    for x in 0..11 {
        assert_eq!(
            pixel(&crossing, x, 5),
            if (3..=7).contains(&x) { 0x801F } else { 0 }
        );
    }
}

#[test]
fn inversion_starts_at_visible_endpoint_and_carries_texture_and_gouraud_direction() {
    // The hidden pair of end codes must not terminate the visible side when
    // pre-clipping can start at the inside endpoint. Pclp=1 reads that pair.
    for disable in [false, true] {
        let mut v = line((0, 0), (0, 0), disable);
        command(&mut v, 0, &[(20, 511), (22, 255)]);
        command(
            &mut v,
            32,
            &[
                (0, 0),
                (4, if disable { 0x800 } else { 0 }),
                (6, 0x8000),
                (8, 0x100),
                (10, 0x101),
                (12, (-4i16) as u16),
            ],
        );
        command(&mut v, 0x800, &[(0, 0xFF12), (2, 0x3456)]);
        finish(&mut v, 3);
        for x in 0..4 {
            assert_eq!(pixel(&v, x, 0), if disable { 0 } else { 0x8003 + x as u16 });
        }
    }
    let mut v = line((-4, 0), (3, 0), false);
    command(&mut v, 32, &[(4, 0xC4), (6, 0x8010), (28, 0x100)]);
    command(&mut v, 0x800, &[(0, 0x4200), (2, 0x421F)]);
    finish(&mut v, 1);
    // Inverted B→A interpolation retains the full seven-dot denominator.
    assert_eq!(
        (0..4).map(|x| pixel(&v, x, 0)).collect::<Vec<_>>(),
        [0x8012, 0x8017, 0x801B, 0x801F]
    );
}
