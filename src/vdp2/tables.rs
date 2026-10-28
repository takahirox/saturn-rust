//! ST-058-R2 §§5.3/6.1/7.1/8.1: line-based table reads. These
//! latches belong to the beam, independently of debugger snapshot rendering.
use super::Vdp2;

#[derive(Default)]
pub(super) struct LineTables {
    pub line: Option<usize>,
    scroll: [[Option<u32>; 3]; 2],
    scroll_group: [usize; 2],
    window: [Option<(u16, u16)>; 2],
    pub back: u16,
    pub color: u16,
    rotation: [[u32; 24]; 2],
}

impl Vdp2 {
    pub(super) fn read_line_tables(&mut self, y: usize) {
        let mut tables = LineTables {
            line: Some(y),
            ..LineTables::default()
        };
        for layer in 0..2 {
            let control = self.word(0x9A) >> (layer * 8) & 0x3F;
            let interval = 1usize << (control >> 4);
            let fields = (control & 14).count_ones() as usize;
            let image_y = if self.word(0x22) & (1 << layer) != 0 {
                let size = usize::from(self.word(0x22) >> 12) + 1;
                let row = self.mosaic_y(layer, y);
                row - row % size
            } else {
                y
            };
            tables.scroll_group[layer] = image_y / interval;
            if y != 0
                && self.line_tables.line.is_some()
                && self.line_tables.scroll_group[layer] == image_y / interval
            {
                tables.scroll[layer] = self.line_tables.scroll[layer];
                continue;
            }
            let mut at = self.table_address(0xA0 + layer * 4) + image_y / interval * fields * 4;
            for (field, bit) in [2, 4, 8].into_iter().enumerate() {
                if control & bit != 0 {
                    tables.scroll[layer][field] = Some(self.vlong(at));
                    at += 4;
                }
            }
        }
        for window in 0..2 {
            let reg = 0xD8 + window * 4;
            if self.word(reg) & 0x8000 != 0 {
                let at = self.table_address(reg) + y * 4;
                tables.window[window] = Some((self.vword(at), self.vword(at + 2)));
            }
        }
        for parameter in 0..2 {
            let at = self.rotation_table(parameter);
            for (i, value) in tables.rotation[parameter].iter_mut().enumerate() {
                *value = self.vlong(at + i * 4);
            }
        }
        tables.back = self.vword(self.line_address(
            0xAC,
            if self.word(0) & 0x8000 == 0 {
                self.dimensions().1 - 1
            } else {
                y
            },
        ));
        tables.color = self.vword(self.line_address(0xA8, y));
        self.line_tables = tables;
    }
    pub(super) fn line_address(&self, reg: usize, y: usize) -> usize {
        ((self.long(reg) & 0x7FFFF) as usize + if self.word(reg) & 0x8000 != 0 { y } else { 0 }) * 2
    }
    pub(super) fn scroll_data(&self, layer: usize, field: usize, address: usize) -> Option<u32> {
        if self.beam_rendering {
            self.line_tables.scroll[layer][field]
        } else {
            Some(self.vlong(address))
        }
    }
    pub(super) fn window_data(&self, window: usize, y: usize) -> (u16, u16) {
        if self.beam_rendering {
            self.line_tables.window[window].unwrap_or_else(|| {
                let base = 0xC0 + window * 8;
                (self.word(base), self.word(base + 4))
            })
        } else {
            let at = self.table_address(0xD8 + window * 4) + y * 4;
            (self.vword(at), self.vword(at + 2))
        }
    }
    pub(super) fn rotation_long(&self, parameter: usize, offset: usize) -> u32 {
        if self.beam_rendering {
            self.line_tables.rotation[parameter][offset / 4]
        } else {
            self.vlong(self.rotation_table(parameter) + offset)
        }
    }
    pub(super) fn rotation_word(&self, parameter: usize, offset: usize) -> u16 {
        if self.beam_rendering {
            (self.line_tables.rotation[parameter][offset / 4] >> ((2 - offset % 4) * 8)) as u16
        } else {
            self.vword(self.rotation_table(parameter) + offset)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{devices::VDP2, vdp1::Vdp1};
    fn reg(v: &mut Vdp2, at: u32, value: u16) {
        v.write16(VDP2 + at, value).unwrap();
    }
    fn long(v: &mut Vdp2, at: usize, value: u32) {
        v.vram[at..at + 4].copy_from_slice(&value.to_be_bytes());
    }
    fn word(v: &mut Vdp2, at: usize, value: u16) {
        v.vram[at..at + 2].copy_from_slice(&value.to_be_bytes());
    }
    #[test]
    fn line_scroll_window_and_back_color_are_held_while_palette_and_effects_remain_live() {
        let mut v = Vdp2::default();
        let s = Vdp1::default();
        for (at, value) in [
            (0, 0x8000),
            (0x20, 0x101),
            (0x28, 0x12),
            (0x10, 0x44FF),
            (0x12, 0xEEEE),
            (0x78, 1),
            (0xF8, 1),
            (0x9A, 2),
            (0xA0, 2),
            (0xD0, 2),
            (0xD8, 0x8002),
            (0xDA, 0x8000),
            (0xC6, 223),
            (0xAC, 3),
            (0xAE, 0x8000),
        ] {
            reg(&mut v, at, value);
        }
        v.vram[..32].copy_from_slice(&[1, 2].repeat(16));
        v.cram[2..6].copy_from_slice(&[0x80, 0x1F, 0x83, 0xE0]);
        for y in 0..2 {
            word(&mut v, 0x50000 + y * 4, 0);
            word(&mut v, 0x50002 + y * 4, 30);
        }
        word(&mut v, 0x70000, 0xFC00);
        v.begin_scanline(0);
        v.scan_to(&s, 0, 4);
        long(&mut v, 0x40000, 1 << 16);
        long(&mut v, 0x40004, 1 << 16);
        word(&mut v, 0x50002, 14);
        word(&mut v, 0x50006, 14);
        word(&mut v, 0x70000, 0xFFFF);
        v.scan_to(&s, 0, 32);
        let pixel = |v: &Vdp2, x: usize, y: usize| v.scanning[(y * 320 + x) * 4..][..4].to_vec();
        assert_eq!(pixel(&v, 8, 0), [255, 0, 0, 255]); // old scroll and window
        assert_eq!(pixel(&v, 20, 0), [0, 0, 255, 255]); // old back
        v.begin_scanline(1);
        v.scan_to(&s, 1, 4);
        assert_eq!(pixel(&v, 0, 1), [0, 255, 0, 255]); // newly fetched scroll
        v.cram[4..6].copy_from_slice(&0xFC00u16.to_be_bytes());
        reg(&mut v, 0x110, 1);
        reg(&mut v, 0x114, 31);
        // A new palette/effect value is live; table latches are independent.
        v.scan_to(&s, 1, 32);
        assert_eq!(pixel(&v, 4, 1), [31, 0, 255, 255]);
        assert_eq!(pixel(&v, 8, 1), [255; 4]); // newly fetched window and back
    }
    #[test]
    fn line_scroll_interval_retains_reads_and_rotation_captures_all_matrix_point_and_delta_words() {
        let mut v = Vdp2::default();
        for (at, value) in [(0, 0x8000), (0x9A, 0x12), (0xA0, 2), (0xBC, 3)] {
            reg(&mut v, at, value);
        }
        long(&mut v, 0x40000, 0x10000);
        long(&mut v, 0x40004, 0x20000);
        for parameter in 0..2 {
            for index in 0..24 {
                long(
                    &mut v,
                    0x60000 + parameter * 0x80 + index * 4,
                    (parameter * 100 + index) as u32,
                );
            }
        }
        v.begin_scanline(0);
        v.beam_rendering = true;
        long(&mut v, 0x40000, 0x30000);
        for parameter in 0..2 {
            for index in 0..24 {
                long(&mut v, 0x60000 + parameter * 0x80 + index * 4, 0xAABBCCDD);
                assert_eq!(
                    v.rotation_long(parameter, index * 4),
                    (parameter * 100 + index) as u32
                );
            }
        }
        assert_eq!(v.scroll_data(0, 0, 0), Some(0x10000));
        v.beam_rendering = false;
        v.begin_scanline(1);
        v.beam_rendering = true;
        assert_eq!(v.scroll_data(0, 0, 0), Some(0x10000)); // no extra read at line1
        assert_eq!(v.rotation_long(1, 0x34), 0xAABBCCDD);
        assert_eq!(v.rotation_word(1, 0x34), 0xAABB);
        assert_eq!(v.rotation_word(1, 0x36), 0xCCDD);
        v.beam_rendering = false;
        v.begin_scanline(2);
        v.beam_rendering = true;
        assert_eq!(v.scroll_data(0, 0, 0), Some(0x20000));
        // The first line of a new field always fetches again.
        v.beam_rendering = false;
        v.begin_scanline(0);
        v.beam_rendering = true;
        assert_eq!(v.scroll_data(0, 0, 0), Some(0x30000));
    }
    #[test]
    fn line_color_index_and_pointer_are_held_but_the_selected_palette_entry_remains_live() {
        let mut v = Vdp2::default();
        let s = Vdp1::default();
        for (at, value) in [
            (0, 0x8000),
            (0x20, 0x101),
            (0x28, 0x32),
            (0xF8, 1),
            (0x10, 0x4444),
            (0x12, 0xEEEE),
            (0xE8, 1),
            (0xEC, 1),
            (0x108, 15),
            (0xA8, 3),
            (0xAC, 3),
            (0xAE, 0x8000),
        ] {
            reg(&mut v, at, value);
        }
        word(&mut v, 0, 0x801F);
        word(&mut v, 0x60000, 1);
        word(&mut v, 0x60020, 3);
        v.cram[2..8].copy_from_slice(&[0x83, 0xE0, 0xFC, 0, 0xFF, 0xFF]);
        v.begin_scanline(0);
        v.scan_to(&s, 0, 1);
        word(&mut v, 0x60000, 2);
        reg(&mut v, 0xAA, 0x10);
        v.scan_to(&s, 0, 2);
        assert_eq!(&v.scanning[4..8], [127, 127, 0, 255]);
        v.cram[2..4].copy_from_slice(&0xFC00u16.to_be_bytes());
        v.scan_to(&s, 0, 3);
        assert_eq!(&v.scanning[8..12], [127, 0, 127, 255]);
        v.begin_scanline(1);
        v.scan_to(&s, 1, 1);
        assert_eq!(&v.scanning[320 * 4..][..4], [255, 127, 127, 255]);
    }
}
