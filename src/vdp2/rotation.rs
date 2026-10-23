//! Independent fixed-point rotation equations from ST-058-R2 §6.1, p.147;
//! table layout/widths: §6.3 figs.6.2/6.3; coefficient modes: §6.4 fig.6.7.
//! RPRCTL one-shot start/coefficient requests: §6.3 pp.157–159.
//! Sprite readout uses A's start/deltas, not its matrix or coefficients (p.159).
use super::{Layer, Vdp2};
use crate::vdp1::Vdp1;

#[derive(Clone, Copy)]
pub(super) struct RotationSample {
    pub image: super::fetch::NormalSample,
    pub coefficient_color: Option<u16>,
}

#[derive(Default)]
pub(super) struct RotationRead {
    pub line: Option<usize>,
    previous: Option<usize>,
    starts: [[i64; 3]; 2],
    coefficients: [Option<Coefficient>; 2],
}

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
    pub(super) fn rotation_line_ready(&self, y: usize) -> bool {
        self.rotation_read.previous == Some(y)
    }
    /// Capture each line's start values. With RxSTRE clear the previous
    /// value advances by the current delta; changing table RAM alone cannot
    /// reload it. Each of Xst, Yst and KAst has an independent one-shot request.
    pub(super) fn read_rotation_line(&mut self, y: usize, first: bool) {
        let distance = self
            .rotation_read
            .previous
            .map_or(y, |old| y.saturating_sub(old));
        for parameter in 0..2 {
            let control = self.word(0xB2) >> (parameter * 8);
            for (i, offset) in [0, 4, 0x54].into_iter().enumerate() {
                let raw = self.rotation_long(parameter, offset);
                let start = if i == 2 {
                    i64::from(raw & 0xFFFFFFC0)
                } else {
                    signed(raw & 0x1FFFFFC0, 29)
                };
                let step = self.rotation_delta(parameter, i);
                let value = if first || self.rotation_read.previous.is_none() {
                    start + step * y as i64
                } else if control & (1 << i) != 0 {
                    start
                } else {
                    self.rotation_read.starts[parameter][i] + step * distance as i64
                };
                // The accumulators have the table's documented width.
                self.rotation_read.starts[parameter][i] = if i == 2 {
                    i64::from(value as u32)
                } else {
                    signed(value as u32, 29)
                };
            }
        }
        // ST-058-R2 §6.3 pp.157–158: each request clears simultaneously
        // with the next parameter read (including field initialization).
        // Only raster reads enter here; writes and debugger renders do not.
        self.registers[0xB2] &= !7;
        self.registers[0xB3] &= !7;
        self.rotation_read.previous = Some(y);
        self.rotation_read.line = Some(y);
        for parameter in 0..2 {
            self.rotation_read.coefficients[parameter] =
                if self.rotation_long(parameter, 0x5C) & 0x03FFFFC0 == 0 {
                    self.read_coefficient(parameter, 0, y)
                } else {
                    None
                };
        }
    }
    fn rotation_delta(&self, parameter: usize, index: usize) -> i64 {
        let offset = [0xC, 0x10, 0x58][index];
        let bits = if index == 2 { 26 } else { 19 };
        signed(
            self.rotation_long(parameter, offset) & ((1 << bits) - 64),
            bits,
        )
    }
    fn rotation_start(&self, parameter: usize, index: usize, y: usize) -> i64 {
        if self.rotation_read.line == Some(y) {
            return self.rotation_read.starts[parameter][index];
        }
        // Debugger renders preview static table operands. A pending request
        // previews the table start; otherwise use the nominal row delta.
        // Neither path participates in the raster latch or consumes requests.
        let raw = self.rotation_long(parameter, [0, 4, 0x54][index]);
        let start = if index == 2 {
            i64::from(raw & 0xFFFFFFC0)
        } else {
            signed(raw & 0x1FFFFFC0, 29)
        };
        start
            + if self.word(0xB2) >> (parameter * 8) & (1 << index) == 0 {
                self.rotation_delta(parameter, index) * y as i64
            } else {
                0
            }
    }
    pub(super) fn sprite_data(&self, sprites: &Vdp1, x: usize, y: usize) -> u16 {
        if !sprites.rotated_scanout() {
            // Each double-density field reads one 256-line buffer. VDP2's
            // historical scanout retains the opposite field's output rows.
            let row = if self.word(0) & 0xC0 == 0xC0 {
                y / 2
            } else {
                y
            };
            return sprites.display_pixel(x, row);
        }
        // ST-058 p.159: line starts are signed Q10.9 and horizontal
        // increments signed Q2.9. Discard seven low Q16 bits BEFORE adding
        // each horizontal increment. VDP1 §1.2: no framebuffer wrapping.
        let coordinate = |index, offset| {
            let start = signed((self.rotation_start(0, index, y) >> 7) as u32, 20);
            let delta = signed(self.rotation_long(0, offset) >> 7, 12);
            signed((start + delta * x as i64) as u32, 20) >> 9
        };
        let sx = coordinate(0, 0x14);
        let sy = coordinate(1, 0x18);
        if sx < 0 || sy < 0 {
            0
        } else {
            sprites.display_pixel(sx as usize, sy as usize)
        }
    }
    pub(super) fn validate_rotation(&self) -> Result<(), String> {
        if self.word(0xB2) & !0x707 != 0 {
            return Err("Reserved VDP2 rotation parameter reload bits".into());
        }
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
        if ctl >> 4 & 7 > 4 {
            return Err("Unsupported VDP2 rotation color format".into());
        }
        // §4.5 N0CHCN/R0CHCN tables: RGB888 is prohibited in high
        // resolution for both rotation images (RBG1 reuses NBG0 controls).
        if self.word(0) & 6 != 0 && (ctl >> 4 & 7 == 4 || (rbg1 && self.word(0x28) >> 4 & 7 == 4)) {
            return Err("VDP2 RGB888 rotation requires normal display mode".into());
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
    pub(super) fn rotation_table(&self, parameter: usize) -> usize {
        // RPTA bit 6 is fixed to 0/1 for A/B, not part of the base.
        (self.table_address(0xBC) & !0x80) | (parameter * 0x80)
    }
    fn coefficient(&self, parameter: usize, x: usize, y: usize) -> Option<Coefficient> {
        if self.beam_rendering && self.rotation_long(parameter, 0x5C) & 0x03FFFFC0 == 0 {
            return self.rotation_read.coefficients[parameter];
        }
        self.read_coefficient(parameter, x, y)
    }
    fn read_coefficient(&self, parameter: usize, x: usize, y: usize) -> Option<Coefficient> {
        let control = self.word(0xB4) >> (parameter * 8);
        if control & 1 == 0 {
            return None;
        }
        let index = (self.rotation_start(parameter, 2, y)
            + signed(self.rotation_long(parameter, 0x5C) & 0x03FFFFC0, 26) * x as i64)
            >> 16;
        let mode = control >> 2 & 3;
        let short = control & 2 != 0;
        let bank = usize::from(self.word(0xB6) >> (parameter * 8) & 7);
        let address = (bank * 0x10000)
            .wrapping_add(index as usize)
            .wrapping_mul(if short { 2 } else { 4 });
        // §6.2/6.4: per-line coefficients can come from any bank. Per-dot
        // coefficients need a dedicated rotation coefficient bank, or CRAM.
        if self.beam_rendering
            && self.word(0xE) & 0x8000 == 0
            && self.rotation_long(parameter, 0x5C) & 0x03FFFFC0 != 0
            && !self.rotation_access(4, address, 1)
        {
            return Some(Coefficient {
                transparent: true,
                value: 0,
                mode,
                line_color: None,
            });
        }
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
        let start = |offset| signed(self.rotation_long(parameter, offset) & 0x1FFFFFC0, 29);
        let delta = |offset| signed(self.rotation_long(parameter, offset) & 0x7FFC0, 19);
        let matrix = |offset| signed(self.rotation_long(parameter, offset) & 0xFFFC0, 20);
        let point = |offset| signed(u32::from(self.rotation_word(parameter, offset)), 14) << 16;
        let shift = |offset| signed(self.rotation_long(parameter, offset) & 0x3FFFFFC0, 30);
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
        let sx = self.rotation_start(parameter, 0, y) + delta(0x14) * x as i64 - px;
        let sy = self.rotation_start(parameter, 1, y) + delta(0x18) * x as i64 - py;
        let sz = start(8) - pz;
        let mut xp =
            multiply(a, px - cx) + multiply(b, py - cy) + multiply(c, pz - cz) + cx + shift(0x44);
        let yp =
            multiply(d, px - cx) + multiply(e, py - cy) + multiply(f, pz - cz) + cy + shift(0x48);
        let mut kx = signed(self.rotation_long(parameter, 0x4C), 24);
        let mut ky = signed(self.rotation_long(parameter, 0x50), 24);
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
        self.display_fetch.borrow().rotation.set(true);
        let result = self.rotation_screen(false, x, y, sprite_window);
        self.display_fetch.borrow().rotation.set(false);
        result
    }
    pub(super) fn rotation1(&self, x: usize, y: usize, sprite_window: bool) -> Option<Layer> {
        self.display_fetch.borrow().rotation.set(true);
        let result = self.rotation_screen(true, x, y, sprite_window);
        self.display_fetch.borrow().rotation.set(false);
        result
    }
    fn rotation_screen(
        &self,
        rbg1: bool,
        x: usize,
        y: usize,
        sprite_window: bool,
    ) -> Option<Layer> {
        let id = if rbg1 { 0 } else { 4 };
        if self.word(0) & 4 != 0
            || self.word(0x20) & if rbg1 { 0x20 } else { 0x10 } == 0
            || !self.window(
                self.word(if rbg1 { 0xD0 } else { 0xD4 }),
                x,
                y,
                sprite_window,
            )
        {
            return None;
        }
        let sample = if let Some(sample) = self.scheduled_rotation_sample(rbg1) {
            sample?
        } else {
            self.rotation_sample(rbg1, x, y, sprite_window)?
        };
        let (base, pixel, palette, special, format) = sample.image;
        let pixel = self.color(
            id,
            format,
            base,
            pixel,
            usize::from(self.word(if rbg1 { 0xE4 } else { 0xE6 }) & 7) * 256,
            palette,
            self.word(0x20) & (0x100 << id) != 0,
        )?;
        let mut layer = self.scroll_layer(id, format, pixel, special)?;
        layer.coefficient_color = sample.coefficient_color;
        Some(layer)
    }
    pub(super) fn rotation_sample(
        &self,
        rbg1: bool,
        mut x: usize,
        y: usize,
        sprite_window: bool,
    ) -> Option<RotationSample> {
        let id = if rbg1 { 0 } else { 4 };
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
        if bitmap && !self.rotation_access(id, Self::dot_address(base, pixel, format), 3) {
            return None;
        }
        Some(RotationSample {
            image: (base, pixel, palette, special, format),
            coefficient_color: if mode == 2 || self.word(0x20) & 0x20 != 0 {
                ca
            } else {
                coefficient
            }
            .and_then(|c| c.line_color),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::VDP2;

    fn put(v: &mut Vdp2, offset: usize, value: u32) {
        v.vram[0x10000 + offset..0x10004 + offset].copy_from_slice(&value.to_be_bytes());
    }
    #[test]
    fn parameter_reload_strobes_are_independent_and_accumulate_current_deltas() {
        // ST-058 §6.3 pp.152/158: each start loads on the first row;
        // later RxSTRE bits independently choose reload or accumulation.
        for parameter in 0..2 {
            for enabled in 0..8 {
                let mut v = Vdp2::default();
                v.write16(VDP2 + 0xBE, 0x8000).unwrap();
                let base = parameter * 0x80;
                for (off, value) in [
                    (0, 10 << 16),
                    (4, 20 << 16),
                    (0x54, 30 << 16),
                    (0xC, 1 << 16),
                    (0x10, 0xFFFF0000),
                    (0x58, 2 << 16),
                ] {
                    put(&mut v, base + off, value);
                }
                v.read_rotation_line(0, true);
                assert_eq!(
                    v.rotation_read.starts[parameter],
                    [10 << 16, 20 << 16, 30 << 16]
                );
                // First line is loaded even when the other parameter's bits are set.
                v.rotation_read.line = None;
                v.write16(VDP2 + 0xB2, enabled << (parameter * 8)).unwrap();
                for (off, value) in [(0, 100 << 16), (4, 200 << 16), (0x54, 300 << 16)] {
                    put(&mut v, base + off, value);
                }
                v.read_rotation_line(1, false);
                assert_eq!(
                    v.rotation_read.starts[parameter],
                    [
                        if enabled & 1 != 0 {
                            100 << 16
                        } else {
                            11 << 16
                        },
                        if enabled & 2 != 0 {
                            200 << 16
                        } else {
                            19 << 16
                        },
                        if enabled & 4 != 0 {
                            300 << 16
                        } else {
                            32 << 16
                        },
                    ]
                );
                v.rotation_read.line = None;
                // Delta changes apply at the next line, without reloading starts.
                put(&mut v, base + 0xC, 2 << 16);
                let old = v.rotation_read.starts[parameter];
                v.read_rotation_line(3, false);
                assert_eq!(
                    v.rotation_read.starts[parameter],
                    [old[0] + (4 << 16), old[1] - (2 << 16), old[2] + (4 << 16)]
                );
                // A new field must discard all accumulated values.
                v.read_rotation_line(0, true);
                assert_eq!(
                    v.rotation_read.starts[parameter],
                    [100 << 16, 200 << 16, 300 << 16]
                );
            }
        }
    }
    #[test]
    fn single_x_request_10_100_101_including_initial_scanout_fallback() {
        let mut v = Vdp2::default();
        v.write16(VDP2 + 0xBE, 0x8000).unwrap();
        put(&mut v, 0, 10 << 16);
        put(&mut v, 0xC, 1 << 16);
        v.write16(VDP2 + 0xB2, 1).unwrap();
        v.framebuffer().unwrap();
        assert_eq!(v.word(0xB2), 1); // debugger inspection is not a read event
        let sprites = Vdp1::default();
        v.scan_to(&sprites, 0, 16); // initial field has no line-start event
        assert_eq!(v.rotation_read.starts[0][0], 10 << 16);
        put(&mut v, 0, 100 << 16);
        v.write16(VDP2 + 0xB2, 1).unwrap();
        v.framebuffer().unwrap();
        v.scan_to(&sprites, 0, 32);
        v.scan_to(&sprites, 0, 32);
        assert_eq!(v.rotation_read.starts[0][0], 10 << 16);
        assert_eq!(v.word(0xB2), 1);
        v.begin_scanline(1);
        assert_eq!(v.rotation_read.starts[0][0], 100 << 16);
        v.begin_scanline(2);
        assert_eq!(v.rotation_read.starts[0][0], 101 << 16);
    }

    #[test]
    fn one_shot_requests_accumulate_mutate_and_rearm_all_six_starts() {
        // Independent §6.3 sequence oracle: no production math helpers.
        // The A-X vector includes the literal 10 -> 100 -> 101 distinction.
        for a in 0..8u16 {
            for b in 0..8u16 {
                let mut v = Vdp2::default();
                v.write16(VDP2 + 0xBE, 0x8000).unwrap();
                for (parameter, base) in [0, 0x80].into_iter().enumerate() {
                    for (off, value) in [(0, 10), (4, 20), (0x54, 30)] {
                        put(&mut v, base + off, value << 16);
                    }
                    for (i, off) in [0xC, 0x10, 0x58].into_iter().enumerate() {
                        let delta = [[1i32, 2, 3], [-1, 3, -2]][parameter][i];
                        put(&mut v, base + off, (delta as u32) << 16);
                    }
                }
                v.begin_scanline(0);
                assert_eq!(v.rotation_read.starts, [[10 << 16, 20 << 16, 30 << 16]; 2]);
                v.write16(VDP2 + 0xB2, a | b << 8).unwrap();
                for base in [0, 0x80] {
                    for (off, value) in [(0, 100), (4, 200), (0x54, 300)] {
                        put(&mut v, base + off, value << 16);
                    }
                }
                // Neither a write nor static debugger rendering consumes a request.
                let before = v.rotation_read.starts;
                v.framebuffer().unwrap();
                assert_eq!(v.rotation_read.starts, before);
                v.begin_scanline(1);
                let mut expected = [[0; 3]; 2];
                for (parameter, mask) in [a, b].into_iter().enumerate() {
                    for i in 0..3 {
                        expected[parameter][i] = if mask & (1 << i) != 0 {
                            [100, 200, 300][i] << 16
                        } else {
                            [[11, 22, 33], [9, 23, 28]][parameter][i] << 16
                        };
                    }
                }
                assert_eq!(v.rotation_read.starts, expected);
                v.begin_scanline(2);
                for (parameter, row) in expected.iter_mut().enumerate() {
                    for (i, value) in row.iter_mut().enumerate() {
                        *value += [[1, 2, 3], [-1, 3, -2]][parameter][i] << 16;
                    }
                }
                assert_eq!(v.rotation_read.starts, expected, "a={a}, b={b}");
                for (parameter, base) in [0, 0x80].into_iter().enumerate() {
                    for (off, value) in [(0, 400), (4, 500), (0x54, 600)] {
                        put(&mut v, base + off, value << 16);
                    }
                    for (i, off) in [0xC, 0x10, 0x58].into_iter().enumerate() {
                        let delta = [[-1i32, 3, -2], [2, -2, 1]][parameter][i];
                        put(&mut v, base + off, (delta as u32) << 16);
                    }
                }
                for line in 3..=5 {
                    v.begin_scanline(line);
                    for (parameter, row) in expected.iter_mut().enumerate() {
                        for (i, value) in row.iter_mut().enumerate() {
                            *value += [[-1, 3, -2], [2, -2, 1]][parameter][i] << 16;
                        }
                    }
                    assert_eq!(v.rotation_read.starts, expected);
                }
                // Arm after the read; repeatedly sampling this same row must
                // leave the new request pending for the NEXT line.
                v.write16(VDP2 + 0xB2, a | b << 8).unwrap();
                let sprites = Vdp1::default();
                v.scan_to(&sprites, 5, 16);
                v.framebuffer().unwrap();
                v.scan_to(&sprites, 5, 32);
                assert_eq!(v.rotation_read.starts, expected);
                v.begin_scanline(6);
                for (parameter, mask) in [a, b].into_iter().enumerate() {
                    for i in 0..3 {
                        expected[parameter][i] = if mask & (1 << i) != 0 {
                            [400, 500, 600][i] << 16
                        } else {
                            expected[parameter][i] + ([[-1, 3, -2], [2, -2, 1]][parameter][i] << 16)
                        };
                    }
                }
                assert_eq!(v.rotation_read.starts, expected);
                v.begin_scanline(7);
                for (parameter, row) in expected.iter_mut().enumerate() {
                    for (i, value) in row.iter_mut().enumerate() {
                        *value += [[-1, 3, -2], [2, -2, 1]][parameter][i] << 16;
                    }
                }
                assert_eq!(v.rotation_read.starts, expected);
            }
        }
    }

    #[test]
    fn requests_on_first_line_fields_signed_wrap_and_reset() {
        for odd in [true, false] {
            let mut v = Vdp2::default();
            v.write16(VDP2, 0x80C0).unwrap();
            v.write16(VDP2 + 0xBE, 0x8000).unwrap();
            v.scan_field(odd);
            for base in [0, 0x80] {
                for (off, value) in [
                    (0, 0x0FFFFFC0),
                    (4, 0x10000000),
                    (0x54, 0xFFFFFFC0),
                    (0xC, 64),
                    (0x10, 0xFFFFFFC0),
                    (0x58, 64),
                ] {
                    put(&mut v, base + off, value);
                }
            }
            v.write16(VDP2 + 0xB2, 0x707).unwrap();
            v.begin_scanline(0);
            let first = if odd {
                [0x0FFFFFC0, -0x10000000, 0xFFFFFFC0]
            } else {
                [-0x10000000, 0x0FFFFFC0, 0]
            };
            assert_eq!(v.rotation_read.starts, [first; 2]);
            // Mutate every table start. A first-line request was consumed too.
            for base in [0, 0x80] {
                for off in [0, 4, 0x54] {
                    put(&mut v, base + off, 0);
                }
            }
            v.begin_scanline(1);
            let next = if odd {
                [-0x0FFFFFC0, 0x0FFFFF80, 64]
            } else {
                [-0x0FFFFF80, 0x0FFFFF40, 128]
            };
            assert_eq!(v.rotation_read.starts, [next; 2]);
            // The next field unconditionally initializes, including even y=1.
            v.begin_scanline(0);
            assert_eq!(
                v.rotation_read.starts,
                [if odd { [0; 3] } else { [64, -64, 64] }; 2]
            );
            v.write16(VDP2 + 0xB2, 0x707).unwrap();
            v = Vdp2::default();
            assert_eq!(v.word(0xB2), 0);
            assert_eq!(v.rotation_read.previous, None);
            assert_eq!(v.rotation_read.starts, [[0; 3]; 2]);
        }
    }

    #[test]
    fn scheduled_requests_respect_line_read_edges_on_both_sh2_adapters() {
        use crate::bus::{Bus, BusError, Memory, Processor};
        fn advance(b: &mut Bus, clocks: u32, chunk: u32, inspect: bool) {
            let mut left = clocks;
            while left != 0 {
                let n = left.min(chunk);
                b.advance_clocks(n).unwrap();
                left -= n;
                if inspect {
                    b.vdp2.framebuffer().unwrap();
                    b.vdp2.scanout(&b.vdp1).unwrap();
                }
            }
        }
        for processor in [Processor::Master, Processor::Slave] {
            // Establish the adapter's actual buffered physical write duration;
            // acceptance is not register publication. This probe is outside any
            // raster edge, and observes private state, not guest readback.
            let mut probe = Bus::default();
            probe.set_slave_enabled(true);
            assert_eq!(
                probe
                    .replay_cpu(processor)
                    .write16(VDP2 | 0x200000B2, 0x707),
                Err(BusError::Pending)
            );
            let mut latency = 0;
            while probe.vdp2.word(0xB2) == 0 {
                probe.advance_clocks(1).unwrap();
                latency += 1;
                assert!(latency < 64);
            }
            for parameter in 0..2 {
                for edge in [-1i32, 0, 1] {
                    for chunk in [1, 3, 17, 10000] {
                        for inspect in [false, true] {
                            let mut b = Bus::default();
                            b.set_slave_enabled(true);
                            for (at, value) in [
                                (0, 0x8000),
                                (0xE, 3),
                                (0x20, 0x10),
                                (0x2A, 0x3200),
                                (0xFC, 4),
                                (0xBE, 0x8000),
                                (0xB0, parameter as u16),
                            ] {
                                b.vdp2.write16(VDP2 + at, value).unwrap();
                            }
                            for y in 0..6 {
                                for x in 0..256 {
                                    let color = 0x8000u16 | (x as u16 & 31) | (y as u16) << 5;
                                    let at = (y * 512 + x) * 2;
                                    b.vdp2.vram[at..at + 2].copy_from_slice(&color.to_be_bytes());
                                }
                            }
                            for (p, x, k) in [(0, 10, 30), (1, 20, 60)] {
                                for (off, value) in [
                                    (0, x),
                                    (0x54, k),
                                    (0xC, 1),
                                    (0x10, 1),
                                    (0x58, 1),
                                    (0x14, 1),
                                    (0x1C, 1),
                                    (0x2C, 1),
                                    (0x4C, 1),
                                    (0x50, 1),
                                ] {
                                    put(&mut b.vdp2, p * 0x80 + off, value << 16);
                                }
                            }
                            b.vdp2.begin_scanline(0);
                            for (p, x, y, k) in [(0, 100, 2, 40), (1, 200, 3, 80)] {
                                for (off, value) in [(0, x), (4, y), (0x54, k)] {
                                    put(&mut b.vdp2, p * 0x80 + off, value << 16);
                                }
                            }
                            let period = b.raster.line_clocks();
                            let issue = (period as i32 + edge) as u32 - latency;
                            // Inspection is deliberately interleaved only near
                            // the boundary; doing it once per clock for a whole
                            // field would repeatedly allocate full debug images.
                            advance(&mut b, issue - 2, chunk, false);
                            advance(&mut b, 2, chunk, inspect);
                            assert_eq!(
                                b.replay_cpu(processor).write16(VDP2 | 0x200000B2, 0x707),
                                Err(BusError::Pending)
                            );
                            advance(&mut b, latency + 2, chunk, inspect);
                            let now = issue + latency + 2;
                            let mut expected = if edge < 0 {
                                [[100, 2, 40], [200, 3, 80]]
                            } else {
                                [[11, 1, 31], [21, 1, 61]]
                            };
                            let mut rows = vec![expected[parameter]];
                            assert_eq!(
                                b.vdp2.rotation_read.starts,
                                expected.map(|r| r.map(|n| n << 16))
                            );
                            let mut time = now;
                            for line in 2..=3 {
                                let deadline = period * line + 2;
                                advance(&mut b, deadline - time - 2, chunk, false);
                                advance(&mut b, 2, chunk, inspect);
                                time = deadline;
                                expected = if line == 2 && edge >= 0 {
                                    [[100, 2, 40], [200, 3, 80]]
                                } else {
                                    expected.map(|r| r.map(|n| n + 1))
                                };
                                rows.push(expected[parameter]);
                                assert_eq!(
                                    b.vdp2.rotation_read.starts,
                                    expected.map(|r| r.map(|n| n << 16)),
                                    "{processor:?} edge={edge} chunk={chunk} inspect={inspect}"
                                );
                            }
                            advance(&mut b, period * 4 - time, chunk, false);
                            for (i, [sx, sy, _]) in rows.into_iter().enumerate() {
                                let red = (sx & 31) as u8;
                                let green = sy as u8;
                                assert_eq!(
                                    &b.vdp2.scanning[((i + 1) * 320) * 4..][..4],
                                    [red << 3 | red >> 2, green << 3 | green >> 2, 0, 255]
                                );
                            }
                            // A queued or already accepted request must not
                            // survive system reset and later change a new field.
                            b.finish_cpu_transaction(processor);
                            assert_eq!(
                                b.replay_cpu(processor).write16(VDP2 | 0x200000B2, 0x707),
                                Err(BusError::Pending)
                            );
                            if inspect {
                                b.advance_clocks(1).unwrap();
                            }
                            b.system_reset();
                            b.vdp2.write16(VDP2 + 0xBE, 0x8000).unwrap();
                            for base in [0, 0x80] {
                                put(&mut b.vdp2, base, 10 << 16);
                                put(&mut b.vdp2, base + 0xC, 1 << 16);
                            }
                            b.vdp2.begin_scanline(0);
                            for base in [0, 0x80] {
                                put(&mut b.vdp2, base, 100 << 16);
                            }
                            advance(&mut b, 10000, chunk, false);
                            assert_eq!(b.raster.line, 5);
                            assert_eq!(b.vdp2.word(0xB2), 0);
                            assert_eq!(b.vdp2.rotation_read.starts, [[15 << 16, 0, 0]; 2]);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn coefficient_reload_changes_executed_table_address_and_reset_clears_latches() {
        let mut v = Vdp2::default();
        v.write16(VDP2 + 0xBE, 0x8000).unwrap();
        v.write16(VDP2 + 0xB4, 3).unwrap(); // 16-bit Q4.10 coefficients, A
        put(&mut v, 0x54, 0x20000);
        put(&mut v, 0x58, 0x10000);
        v.vram[4..12].copy_from_slice(&[0x04, 0, 0x08, 0, 0x0C, 0, 0x10, 0]);
        v.read_rotation_line(0, true);
        assert_eq!(v.coefficient(0, 0, 0).unwrap().value, 1 << 16);
        put(&mut v, 0x54, 0x50000);
        v.read_rotation_line(1, false);
        assert_eq!(v.coefficient(0, 0, 1).unwrap().value, 2 << 16);
        v.write16(VDP2 + 0xB2, 4).unwrap();
        v.read_rotation_line(2, false);
        assert_eq!(v.coefficient(0, 0, 2).unwrap().value, 4 << 16);
        v = Vdp2::default();
        assert_eq!(v.rotation_read.previous, None);
        assert_eq!(v.rotation_read.line, None);
        assert_eq!(v.rotation_read.starts, [[0; 3]; 2]);
    }
    #[test]
    fn line_coefficients_are_captured_but_dot_coefficients_require_their_own_bank() {
        let mut v = Vdp2::default();
        for (at, value) in [
            (0, 0x8000),
            (0x20, 0x10),
            (0xE, 0x300),
            (0xBC, 1),
            (0xB4, 3),
            (0xB6, 2),
        ] {
            v.write16(VDP2 + at, value).unwrap();
        }
        v.vram[0x40000..0x40002].copy_from_slice(&0x0400u16.to_be_bytes());
        v.begin_scanline(0);
        v.beam_rendering = true;
        v.vram[0x40000..0x40002].copy_from_slice(&0x0800u16.to_be_bytes());
        assert_eq!(v.coefficient(0, 5, 0).unwrap().value, 1 << 16);
        v.beam_rendering = false;
        v.begin_scanline(1);
        v.beam_rendering = true;
        assert_eq!(v.coefficient(0, 5, 1).unwrap().value, 2 << 16);
        // Change the guest rotation table to select per-dot coefficients.
        v.vram[0x2005C..0x20060].copy_from_slice(&0x10000u32.to_be_bytes());
        v.vram[0x40004..0x40006].copy_from_slice(&0x0C00u16.to_be_bytes());
        v.beam_rendering = false;
        v.begin_scanline(2);
        v.beam_rendering = true;
        assert!(v.coefficient(0, 2, 2).unwrap().transparent); // no coefficient port in B0
        v.write16(VDP2 + 0xE, 0x310).unwrap();
        let coefficient = v.coefficient(0, 2, 2).unwrap();
        assert!(!coefficient.transparent);
        assert_eq!(coefficient.value, 3 << 16);
        v.vram[0x40004..0x40006].copy_from_slice(&0x1000u16.to_be_bytes());
        assert_eq!(v.coefficient(0, 2, 2).unwrap().value, 4 << 16); // actual per-dot read
    }
}
