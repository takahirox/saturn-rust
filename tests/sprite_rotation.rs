//! Original SH-2 guest draws VDP1 polygons and changes A-table/RPRCTL on beam time.
//! ST-013 §§1.2/4.1, ST-058 §6.3 pp.152/158–159. MIT fixtures, no firmware.
use saturn_rust::{Boot, Machine, bus::Memory};
fn rom(text: &str) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn run(m: &mut Machine, image: &[u8]) {
    m.reset(Boot::ExternalBios(image)).unwrap();
    for _ in 0..500 {
        m.run_steps(500).unwrap();
        if m.bus().read32(0x0600de18) == Ok(0x600d) {
            return;
        }
    }
    panic!("guest never completed: {:?}", m.cpu_state());
}
fn expected(x: usize, y: usize, changed: bool) -> [u8; 4] {
    // Independent polygon coordinates, §6.3 p.159. Requests on rows127/191
    // affect 128/192 once; the CURRENT vertical deltas then accumulate.
    let (sx, sy) = if y < 128 {
        (255 - y, if changed { 0 } else { x })
    } else if y < 192 {
        (256 - y, if changed { 0 } else { x })
    } else {
        (y - 192, 128 + y - 192 + if changed { 0 } else { x })
    };
    if sx >= 256 || sy >= 256 {
        return [0, 0, 0, 255];
    }
    match (sx < 128, sy < 128) {
        (true, true) => [255, 0, 0, 255],
        (false, true) => [0, 255, 0, 255],
        (true, false) => [0, 0, 255, 255],
        (false, false) => [255; 4],
    }
}
#[test]
fn guest_rotation_depths_reload_strobes_and_historical_pixels_survive_reset_and_mutation() {
    for (source, trace) in [
        (
            include_str!("fixtures/sprite-rotation16.hex"),
            include_str!("fixtures/sprite-rotation16.tsv"),
        ),
        (
            include_str!("fixtures/sprite-rotation8.hex"),
            include_str!("fixtures/sprite-rotation8.tsv"),
        ),
    ] {
        let mut image = rom(source);
        let expected_records: Vec<_> = trace
            .lines()
            .map(|l| u32::from_str_radix(l, 16).unwrap())
            .collect();
        let mut m = Machine::new();
        for changed in [false, false, true] {
            if changed {
                image[0x104..0x108].fill(0);
            }
            run(&mut m, &image);
            let actual: Vec<_> = (0..7)
                .map(|i| m.bus().read32(0x0600de00 + i * 4).unwrap())
                .collect();
            assert_eq!(actual, expected_records);
            let f = m.framebuffer().unwrap();
            for y in 0..224 {
                for x in 0..320 {
                    assert_eq!(
                        &f[(y * 320 + x) * 4..][..4],
                        expected(x, y, changed),
                        "({x},{y}) changed={changed}"
                    );
                }
            }
            assert_eq!(m.bus().read32(0x05e10000), Ok(511 << 16));
            m.advance_frame().unwrap();
            assert_eq!(
                m.framebuffer().unwrap(),
                f,
                "VBlank retains the completed field"
            );
            m.advance_frame().unwrap();
            assert!(
                m.framebuffer()
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [0, 0, 0, 255])
            );
        }
    }
}
