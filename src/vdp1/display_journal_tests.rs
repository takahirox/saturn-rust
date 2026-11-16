use super::*;

#[test]
fn display_journal_coalesces_front_erase_and_snapshots_new_front_after_swap() {
    let mut v = Vdp1::default();
    v.enable_display_journal(true);
    for (offset, value) in [(6, 0xABCD), (8, 0), (10, 0x200)] {
        v.write16(REGISTERS + offset, value).unwrap();
    }
    v.vblank_out();
    v.erase_display_line(0, 320);
    let patches = v.take_display_journal();
    assert_eq!(
        patches.len(),
        1,
        "contiguous erase uses one owned byte span"
    );
    assert_eq!(patches[0].offset, 0);
    assert_eq!(patches[0].bytes, [0xAB, 0xCD].repeat(8));
    assert_eq!(&v.display_surface().bytes[..16], patches[0].bytes);
    // CPU writes address the current draw buffer. The consumer needs these
    // bytes when it becomes the front, not at the time of this CPU write.
    v.draw_buffer_mut()[..2].copy_from_slice(&0x1234u16.to_be_bytes());
    assert!(v.take_display_journal().is_empty());
    v.vblank_out();
    assert_eq!(&v.display_surface().bytes[..2], &[0x12, 0x34]);
    assert!(v.take_display_journal().is_empty());
}

#[test]
fn display_journal_retains_pixel_completion_time_and_captured_buffer_across_swap() {
    let mut v = Vdp1 {
        pending_pixel: Some(PendingPixel {
            buffer: 0,
            address: 0,
            color: Some(0x801F),
            clocks: 5,
        }),
        running: true,
        ..Vdp1::default()
    };
    v.vblank_out(); // The accepted pixel's buffer is now displayed.
    v.enable_display_journal(true);
    v.advance(4).unwrap();
    assert_eq!(v.display_pixel(0, 0), 0);
    assert!(v.take_display_journal().is_empty());
    v.advance(1).unwrap();
    let patches = v.take_display_journal();
    assert_eq!(v.display_pixel(0, 0), 0x801F);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].offset, 0);
    assert_eq!(patches[0].bytes, [0x80, 0x1F]);
    // A subsequent write captured on the other buffer is not a front change.
    v.pending_pixel = Some(PendingPixel {
        buffer: v.draw,
        address: 0,
        color: Some(0x83E0),
        clocks: 1,
    });
    v.advance(1).unwrap();
    assert!(v.take_display_journal().is_empty());
    assert_eq!(v.display_pixel(0, 0), 0x801F);
    v.vblank_out();
    assert_eq!(v.display_pixel(0, 0), 0x83E0);
}
