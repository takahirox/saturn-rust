//! Authored pixel vectors from ST-058-R2 §§6.4/7.1/11.3/12.1.
//! No reference core, sampled frame or guest-specific renderer is used.
use saturn_rust::{devices::VDP2, vdp2::Vdp2};

fn reg(v: &mut Vdp2, off: u32, value: u16) {
    v.write16(VDP2 + off, value).unwrap();
}
fn pointer(v: &mut Vdp2, off: u32, value: u32) {
    reg(v, off, (value >> 16) as u16);
    reg(v, off + 2, value as u16);
}
fn word(v: &mut Vdp2, at: usize, value: u16) {
    v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn long(v: &mut Vdp2, at: usize, value: u32) {
    v.vram[at..at + 4].copy_from_slice(&value.to_be_bytes());
}
fn color(v: &mut Vdp2, index: usize, value: u16) {
    v.cram[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
}
fn pixel(f: &[u8], x: usize, y: usize) -> &[u8] {
    &f[(y * 320 + x) * 4..][..4]
}
fn normal() -> Vdp2 {
    let mut v = Vdp2::default();
    for (off, value) in [
        (0, 0x8000),
        (0x20, 0x101),
        (0x28, 0x32),
        (0xF8, 4),
        (0x78, 1),
        (0x7C, 1),
        (0xEC, 1),
        (0x108, 15),
    ] {
        reg(&mut v, off, value);
    }
    pointer(&mut v, 0xAC, 0x38000);
    word(&mut v, 0x70000, 0x7C00); // Blue back; red NBG0 above it.
    word(&mut v, 0, 0x801F);
    word(&mut v, 1024, 0x801F);
    v
}
#[test]
fn line_insertion_is_below_only_the_enabled_top_and_obeys_color_window_and_offset() {
    let mut v = normal();
    color(&mut v, 1, 0x3E0); // Green line, different from blue back.
    // Odd table word address is valid; high reserved data bits are ignored.
    pointer(&mut v, 0xA8, 0x30001);
    word(&mut v, 0x60002, 0xF801);
    reg(&mut v, 0xE8, 1);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 127, 0, 255]);
    reg(&mut v, 0xE8, 2); // NBG1 insertion cannot affect NBG0 top.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 0, 127, 255]);
    reg(&mut v, 0xE8, 1);
    reg(&mut v, 0xEC, 0); // Line is never a separately visible screen.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [255, 0, 0, 255]);
    reg(&mut v, 0xEC, 1);
    reg(&mut v, 0xD6, 0x300); // Calculation outside rectangle, (0,0) is inside.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [255, 0, 0, 255]);
    reg(&mut v, 0xD6, 0);
    reg(&mut v, 0x110, 1);
    reg(&mut v, 0x114, 10);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [137, 127, 0, 255]);
    reg(&mut v, 0x20, 0);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [0, 0, 255, 255]);
}
#[test]
fn single_per_line_tables_wrap_vram_and_palette_modes_ignore_only_documented_address_bits() {
    let mut v = normal();
    reg(&mut v, 0xE8, 1);
    pointer(&mut v, 0xA8, 0x8003FFFF); // Per line, starts at last word of VRAM.
    word(&mut v, 0x7FFFE, 0x401); // Mode0 ignores color address bit10.
    word(&mut v, 0, 2); // Also NBG0 first pixel; inspect y1 only after this write.
    color(&mut v, 1, 0x3E0);
    color(&mut v, 2, 0x7C00);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 1), [127, 0, 127, 255]);
    pointer(&mut v, 0xA8, 0x3FFFF); // Single color uses same green on both lines.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 1), [127, 127, 0, 255]);
    reg(&mut v, 0xE, 0x1000);
    color(&mut v, 0x401, 0x7FFF); // Mode1 preserves address bit10.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 1), [255, 127, 127, 255]);
    reg(&mut v, 0xE, 0x2000);
    v.cram[4..8].copy_from_slice(&[0, 0x80, 0x40, 0x20]); // B,G,R
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 1), [143, 32, 64, 255]);
}
#[test]
fn second_image_ratios_apply_to_normal_line_and_back_and_addition_ignores_ratios() {
    let mut v = normal();
    reg(&mut v, 0xEC, 0x201); // Ratio comes from second (blue back), not red top.
    reg(&mut v, 0x10E, 7 << 8);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [191, 0, 63, 255]);
    reg(&mut v, 0x20, 0x303);
    reg(&mut v, 0x28, 0x3232);
    reg(&mut v, 0x3C, 0x20); // NBG1 bitmap at 40000.
    reg(&mut v, 0x88, 1);
    reg(&mut v, 0x8C, 1);
    reg(&mut v, 0xF8, 0x304);
    reg(&mut v, 0x108, 0x1F0F); // NBG1 second ratio31 -> all green.
    word(&mut v, 0x40000, 0x83E0);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [0, 255, 0, 255]);
    reg(&mut v, 0xE8, 1);
    color(&mut v, 1, 0x7C00);
    pointer(&mut v, 0xA8, 0x30000);
    word(&mut v, 0x60000, 1);
    reg(&mut v, 0x10E, 15);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 0, 127, 255]);
    reg(&mut v, 0xEC, 0x301); // Additive mode ignores both ratio registers.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [255, 0, 255, 255]);
    reg(&mut v, 0xEC, 0x200); // Second's enable must not turn on calculation.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [255, 0, 0, 255]);
}
#[test]
fn sprite_top_line_enable_uses_bit_five_and_sprite_second_supplies_its_own_ratio() {
    use saturn_rust::vdp1::Vdp1;
    let mut v = normal();
    let mut sprites = Vdp1::default();
    sprites.draw_buffer_mut()[..2].copy_from_slice(&0x83E0u16.to_be_bytes());
    sprites.vblank_out();
    reg(&mut v, 0xE0, 0x520); // RGB selector0; calculate at priority <=5.
    reg(&mut v, 0xF0, 5);
    reg(&mut v, 0xEC, 0x40);
    reg(&mut v, 0x100, 15);
    color(&mut v, 1, 0x7C00);
    pointer(&mut v, 0xA8, 0x30000);
    word(&mut v, 0x60000, 1);
    reg(&mut v, 0xE8, 0x20);
    assert_eq!(
        pixel(&v.composite(&sprites).unwrap(), 0, 0),
        [0, 127, 127, 255]
    );
    reg(&mut v, 0xE8, 0x10); // RBG0 bit cannot select sprite insertion.
    assert_eq!(
        pixel(&v.composite(&sprites).unwrap(), 0, 0),
        [127, 127, 0, 255]
    );
    reg(&mut v, 0xF0, 3); // Red normal top, green sprite second.
    reg(&mut v, 0xEC, 0x201);
    reg(&mut v, 0x100, 7);
    assert_eq!(
        pixel(&v.composite(&sprites).unwrap(), 0, 0),
        [191, 63, 0, 255]
    );
}

fn extended_layers() -> (Vdp2, saturn_rust::vdp1::Vdp1) {
    let mut v = normal();
    reg(&mut v, 0x20, 0x303);
    reg(&mut v, 0x28, 0x3232);
    reg(&mut v, 0x3C, 0x20);
    reg(&mut v, 0x88, 1);
    reg(&mut v, 0x8C, 1);
    reg(&mut v, 0xF8, 0x305); // Red NBG0 top5, blue NBG1 third3.
    word(&mut v, 0x40000, 0xFC00);
    reg(&mut v, 0xE0, 0x520);
    reg(&mut v, 0xF0, 4); // Green sprite second4.
    reg(&mut v, 0xEC, 0x443);
    let mut sprites = saturn_rust::vdp1::Vdp1::default();
    sprites.draw_buffer_mut()[..2].copy_from_slice(&0x83E0u16.to_be_bytes());
    sprites.vblank_out();
    color(&mut v, 1, 0x3E0);
    color(&mut v, 2, 0x7FFF);
    pointer(&mut v, 0xA8, 0x30000);
    word(&mut v, 0x60000, 2);
    (v, sprites)
}
#[test]
fn extended_three_image_blends_use_second_enable_and_third_format_for_each_cram_mode() {
    for mode in 0..3 {
        let (mut v, sprites) = extended_layers();
        reg(&mut v, 0xE, mode << 12);
        assert_eq!(
            pixel(&v.composite(&sprites).unwrap(), 0, 0),
            [127, 63, 63, 255]
        );
        reg(&mut v, 0xEC, 0x403); // Raw sprite enable0 inhibits extended blending.
        assert_eq!(
            pixel(&v.composite(&sprites).unwrap(), 0, 0),
            [127, 127, 0, 255]
        );
        reg(&mut v, 0xEC, 0x443);
        reg(&mut v, 0x28, 0x0232); // NBG1 third image switches to palette format.
        v.vram[0x40000] = 0x10;
        if mode == 2 {
            v.cram[4..8].copy_from_slice(&[0, 0xFF, 0, 0]);
        } else {
            color(&mut v, 1, 0x7C00);
        }
        let expected = if mode == 0 {
            [127, 63, 63, 255]
        } else {
            [127, 127, 0, 255]
        };
        assert_eq!(pixel(&v.composite(&sprites).unwrap(), 0, 0), expected);
    }
}
#[test]
fn extended_four_image_line_insertion_obeys_raw_enables_not_top_only_special_conditions() {
    for mode in [0, 1] {
        let (mut v, sprites) = extended_layers();
        reg(&mut v, 0xE, mode << 12);
        reg(&mut v, 0xE8, 1);
        reg(&mut v, 0xEC, 0x463); // Inserted white + green + blue, 2:1:1.
        reg(&mut v, 0xEE, 0x4); // Lower NBG1 character special flag0 is ignored.
        assert_eq!(
            pixel(&v.composite(&sprites).unwrap(), 0, 0),
            [191, 95, 95, 255]
        );
        reg(&mut v, 0xEC, 0x423); // Third's enable0 -> white/green 2:2.
        assert_eq!(
            pixel(&v.composite(&sprites).unwrap(), 0, 0),
            [191, 127, 63, 255]
        );
        reg(&mut v, 0xEC, 0x443); // Line enable0 -> unmodified white second.
        assert_eq!(
            pixel(&v.composite(&sprites).unwrap(), 0, 0),
            [255, 127, 127, 255]
        );
        reg(&mut v, 0xEC, 0x463);
        reg(&mut v, 0xE0, 0x500); // Third sprite palette green instead of RGB.
        // Install the same original sprite through the framebuffer bus, as palette1.
        let mut palette_sprites = saturn_rust::vdp1::Vdp1::default();
        palette_sprites.draw_buffer_mut()[..2].copy_from_slice(&1u16.to_be_bytes());
        palette_sprites.vblank_out();
        let expected = if mode == 0 {
            [191, 95, 95, 255]
        } else {
            [255, 127, 127, 255]
        };
        assert_eq!(
            pixel(&v.composite(&palette_sprites).unwrap(), 0, 0),
            expected
        );
    }
}
fn rotation() -> Vdp2 {
    let mut v = normal();
    reg(&mut v, 0x20, 0x110);
    reg(&mut v, 0x2A, 0x3200);
    reg(&mut v, 0xFC, 4);
    reg(&mut v, 0xEC, 0x10);
    reg(&mut v, 0x10C, 15);
    reg(&mut v, 0xE8, 0x10);
    pointer(&mut v, 0xBC, 0x20000);
    pointer(&mut v, 0xA8, 0x30000);
    word(&mut v, 0x60000, 0x180);
    for at in [0x40000, 0x40080] {
        for off in [0x10, 0x14, 0x1C, 0x2C, 0x4C, 0x50] {
            long(&mut v, at + off, 0x10000);
        }
        long(&mut v, at + 0x5C, 0x10000);
    }
    color(&mut v, 0x181, 0x7C00);
    color(&mut v, 0x182, 0x3E0);
    word(&mut v, 2, 0x83E0);
    word(&mut v, 4, 0xFC00);
    v
}
#[test]
fn coefficient_line_color_uses_high_seven_bits_and_modes_select_the_documented_parameter() {
    let mut v = rotation();
    reg(&mut v, 0xB4, 0x1111); // Both use two-word coefficients and line data.
    reg(&mut v, 0xB6, 0x100); // A at0, B at40000; move KAst to avoid image/table.
    long(&mut v, 0x40054, 0x20000000); // A byte8000.
    long(&mut v, 0x400D4, 0x20000000); // B byte48000.
    long(&mut v, 0x8000, 0x01010000);
    long(&mut v, 0x48000, 0x02010000);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 0, 127, 255]);
    reg(&mut v, 0xB0, 1);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 127, 0, 255]);
    reg(&mut v, 0xB0, 2);
    long(&mut v, 0x8000, 0x81010000); // A transparent selects B, line still A.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 0, 127, 255]);
    reg(&mut v, 0xB0, 3);
    reg(&mut v, 0xD6, 3); // Outside W0 selects B, whose line data must be used.
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [127, 127, 0, 255]);
    reg(&mut v, 0xB4, 0x1300); // B short ignores line enable.
    word(&mut v, 0x24000, 0x400);
    color(&mut v, 0x180, 0x7FFF);
    assert_eq!(pixel(&v.framebuffer().unwrap(), 0, 0), [255, 127, 127, 255]);
}
#[test]
fn cram_coefficients_wrap_in_upper_half_ignore_offset_and_preserve_signed_widths() {
    for short in [false, true] {
        let mut v = rotation();
        reg(&mut v, 0xE, 0x9000);
        reg(&mut v, 0xEC, 0); // Isolate coordinate coefficients, no blending.
        reg(&mut v, 0xB4, if short { 7 } else { 5 }); // kx only
        reg(&mut v, 0xB6, 7); // Has no effect in color RAM.
        // Select last aligned coefficient; the next dot wraps to CRAM800.
        long(&mut v, 0x40054, if short { 1023 << 16 } else { 511 << 16 });
        if short {
            color(&mut v, 0x7FF, 0x400);
            color(&mut v, 0x400, 0x800);
        } else {
            v.cram[0xFFC..0x1000].copy_from_slice(&0x00010000u32.to_be_bytes());
            v.cram[0x800..0x804].copy_from_slice(&0x00020000u32.to_be_bytes());
        }
        let f = v.framebuffer().unwrap();
        assert_eq!(pixel(&f, 0, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&f, 1, 0), [0, 0, 255, 255]); // kx2 addresses blue sample2.
        // Coefficient reads do not alias lower-half palette or VRAM.
        color(&mut v, 0, 0x7FFF);
        long(&mut v, 0x7F800, 0x80000000);
        assert_eq!(v.framebuffer().unwrap(), f);
        long(&mut v, 0x40044, 1 << 16); // Mx1, kx-1 maps screen x1 to red sample0.
        if short {
            color(&mut v, 0x400, 0x7C00);
        } else {
            v.cram[0x800..0x804].copy_from_slice(&0x00FF0000u32.to_be_bytes());
        }
        assert_eq!(pixel(&v.framebuffer().unwrap(), 1, 0), [255, 0, 0, 255]);
        reg(&mut v, 0xE, 0x8000); // CRKTE requires color mode1.
        assert!(v.framebuffer().is_err());
    }
}

fn guest_rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/line-color.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn original_sh2_guest_populates_and_mutates_real_coefficients_and_line_colors() {
    use saturn_rust::{Boot, Machine, bus::Memory};
    let mut m = Machine::new();
    let rom = guest_rom();
    for (old, new, expected) in [
        (
            0u32,
            0u32,
            [
                [127, 0, 127, 255],
                [127, 127, 0, 255],
                [0, 127, 127, 255],
                [0, 0, 0, 255],
                [127, 127, 0, 255],
                [0, 127, 127, 255],
                [127, 0, 127, 255],
                [0, 0, 0, 255],
            ],
        ),
        (
            0x02010000u32,
            0x02020000,
            [
                [127, 0, 127, 255],
                [127, 0, 127, 255],
                [0, 127, 127, 255],
                [0, 0, 0, 255],
                [127, 127, 0, 255],
                [0, 0, 255, 255],
                [127, 0, 127, 255],
                [0, 0, 0, 255],
            ],
        ),
        (
            0x01010000,
            0x03010000,
            [
                [127, 127, 0, 255],
                [127, 127, 0, 255],
                [0, 127, 127, 255],
                [0, 0, 0, 255],
                [255, 0, 0, 255],
                [0, 127, 127, 255],
                [127, 0, 127, 255],
                [0, 0, 0, 255],
            ],
        ),
        (
            0x00000210,
            0x00000630,
            [
                [127, 0, 63, 255],
                [63, 127, 0, 255],
                [0, 63, 127, 255],
                [0, 0, 0, 255],
                [127, 63, 0, 255],
                [0, 127, 63, 255],
                [63, 0, 127, 255],
                [0, 0, 0, 255],
            ],
        ),
    ] {
        let mut image = rom.clone();
        if old != 0 {
            let hits: Vec<_> = image
                .windows(4)
                .enumerate()
                .filter_map(|(at, b)| (b == old.to_be_bytes()).then_some(at))
                .collect();
            assert_eq!(hits.len(), 1);
            image[hits[0]..hits[0] + 4].copy_from_slice(&new.to_be_bytes());
        }
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            assert_eq!(m.bus().read32(0x0600A200).unwrap(), 0x600D);
            assert!(m.cpu_state().instructions > 100);
            let f = m.framebuffer().unwrap();
            for (i, color) in expected.iter().enumerate() {
                assert_eq!(
                    pixel(&f, i % 4, i / 4),
                    color,
                    "mutation {old:08X}, pixel {i}"
                );
            }
            if let Some(previous) = previous {
                assert_eq!(f, previous);
            }
            previous = Some(f);
        }
    }
}
