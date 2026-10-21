//! Independent fixed-point rotation equations from ST-058-R2 §6.1, p.147;
//! table layout/widths: §6.3 figs.6.2/6.3; coefficient modes: §6.4 fig.6.7.
//! The renderer samples a frame snapshot. Scanline reloads/bank-cycle timing
//! remain separate work; unsupported enabled controls are rejected explicitly.
use super::{Layer, Vdp2};

fn signed(value: u32, bits: u32) -> i64 {
    i64::from((value << (32 - bits)) as i32 >> (32 - bits))
}
fn multiply(a: i64, b: i64) -> i64 {
    (a * b) >> 16
}
impl Vdp2 {
    pub(super) fn validate_rotation(&self) -> Result<(), String> {
        if self.word(0x20) & 0x10 == 0 {
            return Ok(());
        }
        let ctl = self.word(0x2A) >> 8;
        if ctl >> 4 & 7 > 4
            || self.word(0x2E) & 0x30 != 0
            || self.word(0xB2) != 0
            || self.word(0xB4) & 0x1010 != 0
            || self.word(0xE) & 0x8000 != 0
        {
            return Err("Unsupported VDP2 rotation reload, line-color, CRAM coefficient or special bitmap control".into());
        }
        for parameter in 0..2 {
            let size = self.word(0x3A) >> (8 + parameter * 4) & 3;
            let over = self.word(0x3A) >> (10 + parameter * 4) & 3;
            if size == 2 || (ctl & 2 != 0 && over == 1) {
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
    fn coefficient(&self, parameter: usize, x: usize, y: usize) -> Option<(bool, i64, u16)> {
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
        let (transparent, value) = if short {
            let raw = self.vword(address);
            // Scale: signed Q4.10 -> Q16; Xp: signed Q12.2 -> Q16.
            (
                raw & 0x8000 != 0,
                signed(u32::from(raw), 15) << if mode == 3 { 14 } else { 6 },
            )
        } else {
            let raw = self.vlong(address);
            (
                raw & 0x80000000 != 0,
                signed(raw, 24) << if mode == 3 { 8 } else { 0 },
            )
        };
        Some((transparent, value, mode))
    }
    /// Compute map coordinates in Q16 throughout; discard fractional bits only
    /// after matrix/perspective/translation operations. No host floating point.
    fn rotation_coordinates(
        &self,
        parameter: usize,
        x: usize,
        y: usize,
        coefficient: Option<(bool, i64, u16)>,
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
        if let Some((_, value, mode)) = coefficient {
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
    pub(super) fn rotation(&self, mut x: usize, y: usize) -> Option<Layer> {
        if self.word(0x20) & 0x10 == 0 || !self.window(self.word(0xD4), x, y) {
            return None;
        }
        let priority = self.word(0xFC) & 7;
        if priority == 0 {
            return None;
        }
        if self.word(0x22) & 0x10 != 0 {
            x -= x % (usize::from(self.word(0x22) >> 8 & 15) + 1);
        }
        let mode = self.word(0xB0) & 3;
        let ca = self.coefficient(0, x, y);
        let parameter = match mode {
            0 => 0,
            1 => 1,
            2 => usize::from(ca.is_some_and(|c| c.0)),
            _ => usize::from(!self.window(self.word(0xD6), x, y)),
        };
        let coefficient = if parameter == 0 {
            ca
        } else {
            self.coefficient(1, x, y)
        };
        if coefficient.is_some_and(|c| c.0) && !(mode == 2 && parameter == 0) {
            return None;
        }
        let (sx, sy) = self.rotation_coordinates(parameter, x, y, coefficient);
        let ctl = self.word(0x2A) >> 8;
        let format = ctl >> 4 & 7;
        let plane = self.word(0x3A) >> (8 + parameter * 4) & 3;
        let bitmap = ctl & 2 != 0;
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
        let (base, pixel, palette) = if bitmap {
            (
                usize::from(self.word(0x3E) >> (parameter * 4) & 7) * 0x20000,
                sy * 512 + sx,
                usize::from(self.word(0x2E) & 7) * 256,
            )
        } else {
            self.tile_map(
                4,
                ctl,
                format,
                sx,
                sy,
                Some(parameter),
                outside && over == 1,
            )
        };
        let rgb = self.color(
            format,
            base,
            pixel,
            usize::from(self.word(0xE6) & 7) * 256,
            palette,
            self.word(0x20) & 0x1000 != 0,
        )?;
        Some(Layer {
            rgb,
            priority,
            ratio: self.word(0x10C) & 31,
            calculate: self.word(0xEC) & 0x10 != 0,
            id: 4,
        })
    }
}
