use super::*;
#[test]
fn display_erase_rows_preserve_every_byte_clip_and_front_patch() {
    let mut used = 0;
    for width in [320usize, 512, 1024] {
        for bytes in [1usize, 2] {
            for color in [0u16, 0xA55A, 0x7F7F, 0x1234] {
                for display_width in [0usize, 319, width] {
                    for buffer in [0, 1] {
                        let make = || {
                            let mut v = Vdp1::default();
                            for (buffer_index, data) in v.buffers.iter_mut().enumerate() {
                                for (i, byte) in data.iter_mut().enumerate() {
                                    *byte = (i.wrapping_mul(37) + buffer_index * 17) as u8;
                                }
                            }
                            v.draw = usize::from(color & 1 != 0);
                            v.enable_display_journal(true);
                            v.display_erase = Some(EraseArea {
                                buffer,
                                width,
                                bytes,
                                color,
                                left: if color == 0 {
                                    0
                                } else if bytes == 1 {
                                    7
                                } else {
                                    8
                                },
                                right: width - 8,
                                top: 1,
                                bottom: 3,
                            });
                            v
                        };
                        let mut actual = make();
                        let mut reference = make();
                        actual.enable_display_erase_rows(true);
                        reference.enable_display_erase_rows(false);
                        let mut expected = actual.buffers.clone();
                        for line in 0..4 {
                            actual.erase_display_line(line, display_width);
                            reference.erase_display_line(line, display_width);
                            if (1..3).contains(&line) {
                                let left = if color == 0 {
                                    0
                                } else if bytes == 1 {
                                    7
                                } else {
                                    8
                                };
                                let end =
                                    (width - 8).min(display_width + if bytes == 2 { 4 } else { 8 });
                                for x in left..end {
                                    let at = (line * width + x) * bytes;
                                    if bytes == 2 {
                                        expected[buffer][at..at + 2]
                                            .copy_from_slice(&color.to_be_bytes());
                                    } else {
                                        expected[buffer][at] = color.to_be_bytes()[x & 1];
                                    }
                                }
                            }
                            assert_eq!(actual.buffers[0], reference.buffers[0]);
                            assert_eq!(
                                actual.buffers, expected,
                                "width={width}, bytes={bytes}, color={color:X}, display={display_width}, line={line}"
                            );
                            assert_eq!(actual.buffers[1], reference.buffers[1]);
                            let a = actual.take_display_journal();
                            let b = reference.take_display_journal();
                            assert_eq!(a.len(), b.len());
                            for (a, b) in a.iter().zip(&b) {
                                assert_eq!(a.offset, b.offset);
                                assert_eq!(a.bytes, b.bytes);
                            }
                        }
                        used += actual.display_erase_row_counts()[0];
                    }
                }
            }
        }
    }
    assert!(used > 0);
}
