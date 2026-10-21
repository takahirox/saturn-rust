//! ST-058-R2 §9.1 fig.9.1/table9.1: all sixteen palette bit layouts.
//! Shadow is evaluated after color calculation/offset (§14.1).
use super::{Layer, Vdp2, rgb555};
use crate::vdp1::Vdp1;

#[derive(Default)]
pub(super) struct Sprite {
    pub layer: Option<Layer>,
    pub shadow_priority: Option<u16>,
    pub shade_sprite: bool,
}

// (priority shift/mask, calculation shift/mask, dot-color mask).
// Types C–F deliberately share the selector and palette-address bits.
fn fields(kind: u16, data: u16) -> (usize, usize, u16, u16) {
    let (ps, pm, cs, cm, mask) = match kind {
        0 => (14, 3, 11, 7, 0x7FF),
        1 => (13, 7, 11, 3, 0x7FF),
        2 => (14, 1, 11, 7, 0x7FF),
        3 => (13, 3, 11, 3, 0x7FF),
        4 => (13, 3, 10, 7, 0x3FF),
        5 => (12, 7, 11, 1, 0x7FF),
        6 => (12, 7, 10, 3, 0x3FF),
        7 => (12, 7, 9, 7, 0x1FF),
        8 => (7, 1, 0, 0, 0x7F),
        9 => (7, 1, 6, 1, 0x3F),
        10 => (6, 3, 0, 0, 0x3F),
        11 => (0, 0, 6, 3, 0x3F),
        12 => (7, 1, 0, 0, 0xFF),
        13 => (7, 1, 6, 1, 0xFF),
        14 => (6, 3, 0, 0, 0xFF),
        _ => (0, 0, 6, 3, 0xFF),
    };
    (
        usize::from((data >> ps) & pm),
        usize::from((data >> cs) & cm),
        data & mask,
        mask,
    )
}

impl Vdp2 {
    pub(super) fn sprite_window(&self, sprites: Option<&Vdp1>, x: usize, y: usize) -> bool {
        self.word(0xE0) & 0x30 == 0x10
            && (2..=7).contains(&(self.word(0xE0) & 15))
            && sprites.is_some_and(|s| s.display_pixel(x, y) & 0x8000 != 0)
    }
    fn palette_msb(&self, index: usize) -> bool {
        let mode = self.word(0xE) >> 12 & 3;
        let at = if mode == 2 {
            (index & 1023) * 4
        } else {
            (index & if mode == 0 { 1023 } else { 2047 }) * 2
        };
        self.cram[at] & 0x80 != 0
    }
    pub(super) fn sprite(&self, sprites: &Vdp1, x: usize, y: usize) -> Sprite {
        if !self.window(self.word(0xD4) >> 8, x, y, false) {
            return Sprite::default();
        }
        let data = sprites.display_pixel(x, y);
        if data == 0 {
            return Sprite::default();
        }
        let kind = self.word(0xE0) & 15;
        let rgb_mode = self.word(0xE0) & 0x20 != 0 && data & 0x8000 != 0;
        let (selector, ratio, color, mask) = if rgb_mode {
            (0, 0, 0, 0)
        } else {
            fields(kind, data)
        };
        let priority = self.word(0xF0 + selector / 2 * 2) >> (selector % 2 * 8) & 7;
        if priority == 0 {
            return Sprite::default();
        }
        let msb_shadow = !rgb_mode
            && (2..=7).contains(&kind)
            && data & 0x8000 != 0
            && self.word(0xE0) & 0x10 == 0;
        let normal_shadow = !rgb_mode && color == mask - 1;
        if normal_shadow || (msb_shadow && data & 0x7FFF == 0) {
            return Sprite {
                shadow_priority: (normal_shadow || self.word(0xE2) & 0x100 != 0)
                    .then_some(priority),
                ..Sprite::default()
            };
        }
        // With sprite window enabled, a lone SD bit is a window, not a color.
        if !rgb_mode && self.word(0xE0) & 0x10 != 0 && data == 0x8000 {
            return Sprite::default();
        }
        let index = usize::from(color) + usize::from(self.word(0xE6) >> 4 & 7) * 256;
        let rgb = if rgb_mode {
            rgb555(data)
        } else {
            self.palette(index)
        };
        let number = self.word(0xE0) >> 8 & 7;
        let calculate = match self.word(0xE0) >> 12 & 3 {
            0 => priority <= number,
            1 => priority == number,
            2 => priority >= number,
            _ => rgb_mode || self.palette_msb(index),
        };
        Sprite {
            layer: Some(Layer {
                rgb,
                priority,
                ratio: self.word(0x100 + ratio / 2 * 2) >> (ratio % 2 * 8) & 31,
                calculate: calculate && self.word(0xEC) & 0x40 != 0,
                id: 6,
            }),
            shade_sprite: msb_shadow,
            shadow_priority: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn figure_nine_one_literal_fields_include_shared_color_bits() {
        // Distinct expected bit fields transcribed from the figure, not from
        // the decoder table. High unused bits of 8-bit types are ignored.
        for (kind, data, expected) in [
            (0, 0xD801, (3, 3, 1, 0x7FF)),
            (1, 0xD801, (6, 3, 1, 0x7FF)),
            (2, 0xD801, (1, 3, 1, 0x7FF)),
            (3, 0xD801, (2, 3, 1, 0x7FF)),
            (4, 0xD401, (2, 5, 1, 0x3FF)),
            (5, 0xD801, (5, 1, 1, 0x7FF)),
            (6, 0xDC01, (5, 3, 1, 0x3FF)),
            (7, 0xDE01, (5, 7, 1, 0x1FF)),
            (8, 0xFF81, (1, 0, 1, 0x7F)),
            (9, 0xFFC1, (1, 1, 1, 0x3F)),
            (10, 0xFF81, (2, 0, 1, 0x3F)),
            (11, 0xFFC1, (0, 3, 1, 0x3F)),
            (12, 0xFF81, (1, 0, 0x81, 0xFF)),
            (13, 0xFFC1, (1, 1, 0xC1, 0xFF)),
            (14, 0xFF81, (2, 0, 0x81, 0xFF)),
            (15, 0xFFC1, (0, 3, 0xC1, 0xFF)),
        ] {
            assert_eq!(fields(kind, data), expected, "type {kind:X}");
        }
    }
}
