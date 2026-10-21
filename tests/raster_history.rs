//! ST-058 §§2.5/7.2/13.1: historical pixels from guest VRAM/register writes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};

#[test]
fn guest_scanline_writes_survive_later_vram_and_color_offset_changes() {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/raster-history.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..400 {
            m.run_steps(500).unwrap();
            if m.bus().read32(0x0600D900) == Ok(0x600D) {
                break;
            }
        }
        assert_eq!(m.bus().read32(0x0600D900), Ok(0x600D));
        assert_eq!(m.bus().read16(0x05E60000), Ok(0x8000));
        let frame = m.framebuffer().unwrap();
        for y in 0..224 {
            let rgb = match y {
                0..=63 => [255, 0, 0, 255],
                64..=127 => [0, 255, 0, 255],
                128..=191 => [0, 0, 255, 255],
                _ => [255; 4],
            };
            for x in [0, 159, 319] {
                assert_eq!(&frame[(y * 320 + x) * 4..][..4], rgb, "({x},{y})");
            }
        }
        // The frame remains unchanged during VBlank, then the next field
        // uses the new black back color plus its red offset.
        m.advance_frame().unwrap();
        assert_eq!(m.framebuffer().unwrap(), frame);
        m.advance_frame().unwrap();
        assert!(
            m.framebuffer()
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [31, 0, 0, 255])
        );
    }
}
