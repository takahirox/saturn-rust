//! ST-058 §§4.1/6.2/6.4: original guest dual rotation and changed-B-table pixels.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    devices::WIDTH,
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/rbg1.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            bytes[at..at + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    bytes
}
#[test]
fn guest_dual_rotation_and_mutated_parameter_b_have_objective_pixels_and_reset_trace() {
    for (my, expected) in [
        (15u32, [[0, 255, 0], [0, 0, 255], [0, 0, 255], [0, 255, 0]]),
        (7, [[0, 0, 255], [255, 0, 0], [0, 255, 0], [255, 0, 0]]),
    ] {
        let mut rom = rom();
        // The original guest load literal is used only for B's My. Mutate that
        // source operand, not VRAM/registers from the host.
        let literal = 0x000F0000u32.to_be_bytes();
        let offsets: Vec<_> = rom
            .windows(4)
            .enumerate()
            .filter_map(|(i, w)| (w == literal).then_some(i))
            .collect();
        assert_eq!(offsets.len(), 1);
        rom[offsets[0]..offsets[0] + 4].copy_from_slice(&(my << 16u32).to_be_bytes());
        let mut m = Machine::new();
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.run_steps(10000).unwrap();
            assert_eq!(m.bus().read32(0x0600A100).unwrap(), 0x600D);
            let pixels = m.framebuffer().unwrap();
            for ((x, y), rgb) in [(0, 0), (8, 0), (0, 8), (8, 8)].into_iter().zip(expected) {
                assert_eq!(
                    &pixels[(y * WIDTH + x) * 4..][..3],
                    &rgb,
                    "My={my},x={x},y={y}"
                );
            }
            assert_eq!(&pixels[16 * 4..16 * 4 + 4], &[255, 0, 0, 255]);
            if let Some(before) = &previous {
                assert_eq!(&pixels, before);
            }
            previous = Some(pixels);
        }
    }
}
