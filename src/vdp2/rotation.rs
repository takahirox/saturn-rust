//! Independent fixed-point rotation equations from ST-058-R2 §6.1, p.147;
//! table layout/widths: §6.3 figs.6.2/6.3; coefficient modes: §6.4 fig.6.7.
//! The renderer samples a frame snapshot. Scanline reloads/bank-cycle timing
//! remain separate work; unsupported enabled controls are rejected explicitly.
use super::{Layer, Vdp2};

#[derive(Clone, Copy)]
struct Coefficient {
    transparent: bool,
    value: i64,
    mode: u16,
    line_color: Option<u16>,
}

fn signed(value: u32, bits: u32) -> i64 {
    i64::from((value << (32 - bits)) as i32 >> (32 - bits))
}
fn multiply(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}
impl Vdp2 {
    pub(super) fn validate_rotation(&self) -> Result<(), String> {
        if self.word(0x20) & 0x30 == 0 {
            return Ok(());
        }
        let rbg1 = self.word(0x20) & 0x20 != 0;
        if rbg1 && self.word(0x20) & 0x10 == 0 {
            return Err("VDP2 RBG1 requires RBG0 to be enabled".into());
        }
        if rbg1
            && (self.word(0xB0) & 3 != 0
                || self.word(0xE) & 0x200 == 0
                || (self.word(0x28) >> 4) & 7 > 4)
        {
            return Err(
                "VDP2 RBG1 requires mode 0, split B banks and a supported cell format".into(),
            );
        }
        let ctl = self.word(0x2A) >> 8;
        if ctl >> 4 & 7 > 4 || self.word(0xB2) != 0 {
            return Err("Unsupported VDP2 rotation reload or color format".into());
        }
        self.validate_special(4, ctl >> 4 & 7)?;
        if rbg1 {
            self.validate_special(0, self.word(0x28) >> 4 & 7)?;
        }
        for parameter in 0..2 {
            let size = self.word(0x3A) >> (8 + parameter * 4) & 3;
            let over = self.word(0x3A) >> (10 + parameter * 4) & 3;
            let bitmap = ctl & 2 != 0 && !(rbg1 && parameter == 1);
            if size == 2 || (bitmap && over == 1) {
                return Err("Reserved VDP2 rotation plane size or bitmap screen-over mode".into());
            }
        }
        if self.word(0xB0) & 3 == 2 && self.word(0xB4) & 1 == 0 {
            return Err("VDP2 rotation mode 2 requires coefficient table A".into());
        }
        Ok(())
    }
    fn rotation_table(&self, parameter: usize) -> usize {
        // RPTA bit 6 is fixed to 0/1 for A/B, not part of the base.
        (self.table_address(0xBC) & !0x80) | (parameter * 0x80)
    }
    fn coefficient(&self, parameter: usize, x: usize, y: usize) -> Option<Coefficient> {
        let control = self.word(0xB4) >> (parameter * 8);
        if control & 1 == 0 {
            return None;
        }
        let at = self.rotation_table(parameter);
        let index = (i64::from(self.vlong(at + 0x54) & 0xFFFFFFC0)
            + signed(self.vlong(at + 0x58) & 0x03FFFFC0, 26) * y as i64
            + signed(self.vlong(at + 0x5C) & 0x03FFFFC0, 26) * x as i64)
            >> 16;
        let mode = control >> 2 & 3;
        let short = control & 2 != 0;
        let bank = usize::from(self.word(0xB6) >> (parameter * 8) & 7);
        let address = (bank * 0x10000)
            .wrapping_add(index as usize)
            .wrapping_mul(if short { 2 } else { 4 });
        // ST-058 §6.4 pp.163/167: CRKTE uses the latter 2 KiB of CRAM,
        // ignoring KTAOF and all but ten word-address bits. Both word sizes
        // wrap within that half; parameter and image tables still use VRAM.
        let read = |address| {
            if self.word(0xE) & 0x8000 != 0 {
                let at = 0x800 | (address & 0x7FE);
                u16::from_be_bytes([self.cram[at], self.cram[at + 1]])
            } else {
                self.vword(address)
            }
        };
        let (transparent, value, line_color) = if short {
            let raw = read(address);
            // Scale: signed Q4.10 -> Q16; Xp: signed Q12.2 -> Q16.
            (
                raw & 0x8000 != 0,
                signed(u32::from(raw), 15) << if mode == 3 { 14 } else { 6 },
                None,
            )
        } else {
            let raw = u32::from(read(address)) << 16 | u32::from(read(address.wrapping_add(2)));
            (
                raw & 0x80000000 != 0,
                signed(raw, 24) << if mode == 3 { 8 } else { 0 },
                (control & 0x10 != 0).then_some((raw >> 24) as u16 & 0x7F),
            )
        };
        Some(Coefficient {
            transparent,
            value,
            mode,
            line_color,
        })
    }
    /// Compute map coordinates in Q16 throughout; discard fractional bits only
    /// after matrix/perspective/translation operations. No host floating point.
    fn rotation_coordinates(
        &self,
        parameter: usize,
        x: usize,
        y: usize,
        coefficient: Option<Coefficient>,
    ) -> (i64, i64) {
        let at = self.rotation_table(parameter);
        let start = |offset| signed(self.vlong(at + offset) & 0x1FFFFFC0, 29);
        let delta = |offset| signed(self.vlong(at + offset) & 0x7FFC0, 19);
        let matrix = |offset| signed(self.vlong(at + offset) & 0xFFFC0, 20);
        let point = |offset| signed(u32::from(self.vword(at + offset)), 14) << 16;
        let shift = |offset| signed(self.vlong(at + offset) & 0x3FFFFFC0, 30);
        let a = matrix(0x1C);
        let b = matrix(0x20);
        let c = matrix(0x24);
        let d = matrix(0x28);
        let e = matrix(0x2C);
        let f = matrix(0x30);
        let px = point(0x34);
        let py = point(0x36);
        let pz = point(0x38);
        let cx = point(0x3C);
        let cy = point(0x3E);
        let cz = point(0x40);
        let sx = start(0) + delta(0xC) * y as i64 + delta(0x14) * x as i64 - px;
        let sy = start(4) + delta(0x10) * y as i64 + delta(0x18) * x as i64 - py;
        let sz = start(8) - pz;
        let mut xp =
            multiply(a, px - cx) + multiply(b, py - cy) + multiply(c, pz - cz) + cx + shift(0x44);
        let yp =
            multiply(d, px - cx) + multiply(e, py - cy) + multiply(f, pz - cz) + cy + shift(0x48);
        let mut kx = signed(self.vlong(at + 0x4C), 24);
        let mut ky = signed(self.vlong(at + 0x50), 24);
        if let Some(Coefficient { value, mode, .. }) = coefficient {
            match mode {
                0 => {
                    kx = value;
                    ky = value;
                }
                1 => kx = value,
                2 => ky = value,
                _ => xp = value,
            }
        }
        (
            (multiply(kx, multiply(a, sx) + multiply(b, sy) + multiply(c, sz)) + xp) >> 16,
            (multiply(ky, multiply(d, sx) + multiply(e, sy) + multiply(f, sz)) + yp) >> 16,
        )
    }
    pub(super) fn rotation(&self, x: usize, y: usize, sprite_window: bool) -> Option<Layer> {
        self.rotation_screen(false, x, y, sprite_window)
    }
    pub(super) fn rotation1(&self, x: usize, y: usize, sprite_window: bool) -> Option<Layer> {
        self.rotation_screen(true, x, y, sprite_window)
    }
    fn rotation_screen(
        &self,
        rbg1: bool,
        mut x: usize,
        y: usize,
        sprite_window: bool,
    ) -> Option<Layer> {
        let id = if rbg1 { 0 } else { 4 };
        if self.word(0x20) & if rbg1 { 0x20 } else { 0x10 } == 0
            || !self.window(
                self.word(if rbg1 { 0xD0 } else { 0xD4 }),
                x,
                y,
                sprite_window,
            )
        {
            return None;
        }
        if self.word(0x22) & (1 << id) != 0 {
            x -= x % (usize::from(self.word(0x22) >> 8 & 15) + 1);
        }
        let mode = self.word(0xB0) & 3;
        let ca = self.coefficient(0, x, y);
        let parameter = if rbg1 {
            1
        } else {
            match mode {
                0 => 0,
                1 => 1,
                2 => usize::from(ca.is_some_and(|c| c.transparent)),
                _ => usize::from(!self.window(self.word(0xD6), x, y, sprite_window)),
            }
        };
        let coefficient = if parameter == 0 {
            ca
        } else {
            self.coefficient(1, x, y)
        };
        // ST-058 §6.4 p.166: the coefficient MSB is ignored for RBG1.
        if !rbg1 && coefficient.is_some_and(|c| c.transparent) && !(mode == 2 && parameter == 0) {
            return None;
        }
        let (sx, sy) = self.rotation_coordinates(parameter, x, y, coefficient);
        let ctl = if rbg1 {
            self.word(0x28)
        } else {
            self.word(0x2A) >> 8
        };
        let format = ctl >> 4 & 7;
        let plane = self.word(0x3A) >> (8 + parameter * 4) & 3;
        let bitmap = !rbg1 && ctl & 2 != 0;
        let width = if bitmap {
            512
        } else if plane == 0 {
            2048
        } else {
            4096
        };
        let height = if bitmap {
            if ctl & 4 == 0 { 256 } else { 512 }
        } else if plane == 3 {
            4096
        } else {
            2048
        };
        let outside = sx < 0 || sy < 0 || sx >= width || sy >= height;
        let over = self.word(0x3A) >> (10 + parameter * 4) & 3;
        if (over == 2 && outside)
            || (over == 3 && !(0..512).contains(&sx))
            || (over == 3 && !(0..512).contains(&sy))
        {
            return None;
        }
        let sx = sx.rem_euclid(width) as usize;
        let sy = sy.rem_euclid(height) as usize;
        let (base, pixel, palette, special) = if bitmap {
            (
                usize::from(self.word(0x3E) >> (parameter * 4) & 7) * 0x20000,
                sy * 512 + sx,
                usize::from(self.word(0x2E) & 7) * 256,
                self.word(0x2E) >> 4 & 3,
            )
        } else {
            self.tile_map(
                id,
                ctl,
                format,
                sx,
                sy,
                Some(parameter),
                outside && over == 1,
            )?
        };
        let pixel = self.color(
            format,
            base,
            pixel,
            usize::from(self.word(if rbg1 { 0xE4 } else { 0xE6 }) & 7) * 256,
            palette,
            self.word(0x20) & (0x100 << id) != 0,
        )?;
        let mut layer = self.scroll_layer(id, format, pixel, special)?;
        // §6.4 fig.6.6: mode 2 and dual rotation use A's line data
        // even when the displayed image uses parameter B.
        layer.coefficient_color = if mode == 2 || self.word(0x20) & 0x20 != 0 {
            ca
        } else {
            coefficient
        }
        .and_then(|c| c.line_color);
        Some(layer)
    }
}
