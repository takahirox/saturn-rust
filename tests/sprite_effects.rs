//! ST-058 §§8.1/9.1/9.2/14.1, original guest register/framebuffer writes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/sprite-effects.hex")
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for w in words {
            bytes[at..at + 2].copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    bytes
}
fn patch(bytes: &mut [u8], old: u32, new: u32) {
    let offsets: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(i, v)| (v == old.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 1);
    bytes[offsets[0]..offsets[0] + 4].copy_from_slice(&new.to_be_bytes());
}
fn pixels(machine: &mut Machine, rom: &[u8]) -> Vec<u8> {
    machine.reset(Boot::ExternalBios(rom)).unwrap();
    machine.run_steps(3000).unwrap();
    machine.advance_frame().unwrap();
    assert_eq!(machine.bus().read32(0x0600B000).unwrap(), 0x600D);
    machine.framebuffer().unwrap()[..32].to_vec()
}
#[test]
fn actual_guest_composes_sprite_window_shadow_and_palette_msb_blends() {
    let rom = rom();
    let mut m = Machine::new();
    let expected = [
        255, 0, 0, 255, 0, 127, 127, 255, 0, 0, 127, 255, 127, 127, 0, 255, 0, 0, 255, 255, 255, 0,
        0, 255, 127, 0, 0, 255, 0, 0, 255, 255,
    ];
    assert_eq!(pixels(&mut m, &rom), expected);
    assert_eq!(pixels(&mut m, &rom), expected);
    let mut no_window = rom.clone();
    patch(&mut no_window, 0x3013, 0x3003);
    assert_eq!(
        pixels(&mut m, &no_window),
        [
            0, 0, 255, 255, 0, 127, 127, 255, 0, 0, 127, 255, 0, 63, 63, 255, 0, 0, 255, 255, 0, 0,
            255, 255, 0, 0, 127, 255, 0, 0, 255, 255
        ]
    );
    let mut no_calculate = rom.clone();
    patch(&mut no_calculate, 0x83E0, 0x03E0);
    let f = pixels(&mut m, &no_calculate);
    assert_eq!(&f[4..8], &[0, 255, 0, 255]);
    assert_eq!(&f[12..16], &[0, 255, 0, 255]);
}
