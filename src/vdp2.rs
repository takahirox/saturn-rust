//! Original software-visible VDP2 renderer; Sega ST-058-R2 §§3–6,8–13.
//! Normal tile/bitmap sampling and VDP1 composition are independent of guest identity.
use crate::{
    bus::BusError,
    devices::{HEIGHT, VDP2, WIDTH},
    vdp1::Vdp1,
};

pub const CRAM: u32 = 0x05F0_0000;
pub struct Vdp2 {
    registers: [u8; 0x120],
    pub vram: Vec<u8>,
    pub cram: Vec<u8>,
    pub vblank: bool,
    pub hblank: bool,
}
impl Default for Vdp2 {
    fn default() -> Self {
        Self {
            registers: [0; 0x120],
            vram: vec![0; 0x80000],
            cram: vec![0; 0x1000],
            vblank: false,
            hblank: false,
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Layer {
    rgb: [u8; 3],
    priority: u16,
    ratio: u16,
    calculate: bool,
    id: usize,
}
fn rgb555(color: u16) -> [u8; 3] {
    std::array::from_fn(|channel| {
        let v = (color >> (channel * 5) & 31) as u8;
        v << 3 | v >> 2
    })
}
impl Vdp2 {
    pub(crate) fn cram_offset(&self, offset: usize) -> usize {
        offset
            & if self.word(0xE) & 0x3000 == 0 {
                0x7FF
            } else {
                0xFFF
            }
    }
    fn supported(offset: usize) -> bool {
        matches!(offset,
            0 | 4 | 0xE | 0x10 | 0x12 | 0x20 | 0x22 | 0x28 | 0x2A | 0x2C | 0x30..=0x36 | 0x3A | 0x3C | 0x40..=0x4E |
            0x70..=0x96 | 0xAC | 0xAE | 0xC0..=0xD2 | 0xD4 | 0xD6 | 0xE0 |
            0xE4 | 0xE6 | 0xEC | 0xF0..=0xFA | 0x100..=0x10A | 0x110..=0x11E
        ) && offset & 1 == 0
    }
    fn word(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.registers[offset], self.registers[offset + 1]])
    }
    fn long(&self, offset: usize) -> u32 {
        u32::from(self.word(offset)) << 16 | u32::from(self.word(offset + 2))
    }
    fn vword(&self, offset: usize) -> u16 {
        let offset = offset & 0x7FFFE;
        u16::from_be_bytes([self.vram[offset], self.vram[offset + 1]])
    }
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        let offset = (address - VDP2) as usize;
        if offset == 4 {
            return Ok(u16::from(self.vblank) << 3 | u16::from(self.hblank) << 2);
        }
        if !Self::supported(offset) {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        Ok(self.word(offset))
    }
    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        let offset = (address - VDP2) as usize;
        if offset == 4 || !Self::supported(offset) {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        self.registers[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        Ok(())
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, String> {
        self.render(None)
    }
    pub fn composite(&self, sprites: &Vdp1) -> Result<Vec<u8>, String> {
        if !sprites.normal_scanout() {
            return Err("Unsupported VDP1 scanout mode in VDP2 compositor".into());
        }
        self.render(Some(sprites))
    }
    fn validate(&self) -> Result<(), String> {
        let unsupported = self.word(0) != 0x8000
            || self.word(0x20) & !0xF0F != 0
            || self.word(0xE) & !0x3000 != 0
            || self.word(0xE) >> 12 == 3
            || self.word(0xEC) & !0x14F != 0
            || self.word(0xE0) & 0x10 != 0
            || self.word(0xD0) & 0x2020 != 0
            || self.word(0xD2) & 0x2020 != 0
            || self.word(0xD4) & 0x2020 != 0
            || self.word(0xD6) & 0x2020 != 0;
        if unsupported {
            return Err("Unsupported VDP2 display, banking, layer or effects configuration".into());
        }
        for layer in 0..4 {
            if self.word(0x20) & (1 << layer) == 0 {
                continue;
            }
            let (ctl, format) = self.character_control(layer);
            if format
                > if layer == 0 {
                    4
                } else if layer == 1 {
                    3
                } else {
                    1
                }
            {
                return Err("Unsupported VDP2 character color format".into());
            }
            if layer < 2 && ctl & 2 != 0 {
                if self.word(0x2C) >> (layer * 8) & 0x30 != 0 {
                    return Err("Unsupported VDP2 bitmap special priority/color calculation".into());
                }
            } else if self.word(0x3A) >> (layer * 2) & 3 == 2 {
                return Err("Reserved VDP2 plane size".into());
            }
        }
        // Palette sprite types 0/1 and mixed RGB are currently implemented.
        if self.word(0xE0) & 15 > 1 {
            return Err("Unsupported VDP2 sprite type".into());
        }
        Ok(())
    }
    fn palette(&self, index: usize) -> [u8; 3] {
        let mode = self.word(0xE) >> 12 & 3;
        if mode == 2 {
            let at = (index & 1023) * 4;
            [self.cram[at + 3], self.cram[at + 2], self.cram[at + 1]]
        } else {
            let at = (index & if mode == 0 { 1023 } else { 2047 }) * 2;
            rgb555(u16::from_be_bytes([self.cram[at], self.cram[at + 1]]))
        }
    }
    fn offset(&self, mut rgb: [u8; 3], id: usize) -> [u8; 3] {
        if self.word(0x110) & (1 << id) != 0 {
            let base = if self.word(0x112) & (1 << id) == 0 {
                0x114
            } else {
                0x11A
            };
            for (ch, v) in rgb.iter_mut().enumerate() {
                let delta = (self.word(base + ch * 2) << 7) as i16 >> 7;
                *v = (i16::from(*v) + delta).clamp(0, 255) as u8;
            }
        }
        rgb
    }
    fn window(&self, control: u16, x: usize, y: usize) -> bool {
        let mut any = false;
        let and = control & 0x80 != 0;
        let mut result = and;
        for i in 0..2 {
            if control & (2 << (i * 2)) == 0 {
                continue;
            }
            any = true;
            let base = 0xC0 + i * 8;
            let inside = x >= usize::from(self.word(base) >> 1)
                && x <= usize::from(self.word(base + 4) >> 1)
                && y >= usize::from(self.word(base + 2))
                && y <= usize::from(self.word(base + 6));
            let valid = inside != (control & (1 << (i * 2)) != 0);
            if and {
                result &= valid;
            } else {
                result |= valid;
            }
        }
        !any || result
    }
    fn character_control(&self, layer: usize) -> (u16, u16) {
        if layer < 2 {
            let ctl = self.word(0x28) >> (layer * 8);
            (ctl, ctl >> 4 & if layer == 0 { 7 } else { 3 })
        } else {
            let ctl = self.word(0x2A) >> ((layer - 2) * 4);
            (ctl & 1, ctl >> 1 & 1)
        }
    }
    /// ST-058 §§4.6–4.8, figs.4.9/4.10, table4.8. A page covers 512²
    /// pixels regardless of character/name size. Plane selection precedes page,
    /// then character and cell addressing; character storage is always 32-byte units.
    fn tile(
        &self,
        layer: usize,
        ctl: u16,
        format: u16,
        x: usize,
        y: usize,
    ) -> (usize, usize, usize) {
        let size = if ctl & 1 == 0 { 8 } else { 16 };
        let pnc = self.word(0x30 + layer * 2);
        let one = pnc & 0x8000 != 0;
        let name_bytes = if one { 2 } else { 4 };
        let columns = 512 / size;
        let page_bytes = columns * columns * name_bytes;
        let plane_size = self.word(0x3A) >> (layer * 2) & 3;
        let pages_x = if plane_size == 0 { 1 } else { 2 };
        let pages_y = if plane_size == 3 { 2 } else { 1 };
        let plane_x = 512 * pages_x;
        let plane_y = 512 * pages_y;
        let plane = ((x / plane_x) & 1) | (((y / plane_y) & 1) << 1);
        let map = self.word(0x40 + layer * 4 + (plane / 2) * 2) >> ((plane & 1) * 8) & 63;
        let bank = self.word(0x3C) >> (layer * 4) & 7;
        let selection = usize::from((bank << 6) | map);
        let base = ((selection * page_bytes) & 0x7FFFF) & !(page_bytes * pages_x * pages_y - 1);
        let px = x % plane_x;
        let py = y % plane_y;
        let page = py / 512 * pages_x + px / 512;
        let name_index = (py % 512 / size) * columns + px % 512 / size;
        let at = base + page * page_bytes + name_index * name_bytes;
        let name = self.vword(at);
        let (character, palette, flip) = if one {
            let aux = pnc & 0x4000 != 0;
            let number = usize::from(name & if aux { 0xFFF } else { 0x3FF });
            let character = if size == 8 {
                number | (usize::from(pnc & if aux { 0x1C } else { 0x1F }) << 10)
            } else {
                number << 2
                    | (usize::from(pnc & if aux { 0x10 } else { 0x1C }) << 10)
                    | usize::from(pnc & 3)
            };
            let palette = if format == 0 {
                (usize::from(name >> 12) * 16) | (usize::from(pnc & 0xE0) << 3)
            } else {
                usize::from(name & 0x7000) >> 4
            };
            (character, palette, if aux { 0 } else { name >> 10 & 3 })
        } else {
            (
                usize::from(self.vword(at + 2) & 0x7FFF),
                usize::from(name & 0x7F) * 16,
                name >> 14 & 3,
            )
        };
        let mut cx = x % size;
        let mut cy = y % size;
        if flip & 1 != 0 {
            cx = size - 1 - cx;
        }
        if flip & 2 != 0 {
            cy = size - 1 - cy;
        }
        let cell = (cy / 8) * 2 + cx / 8;
        let cell_bytes = [32, 64, 128, 128, 256][usize::from(format)];
        (
            (character * 32 + cell * cell_bytes) & 0x7FFFF,
            (cy & 7) * 8 + (cx & 7),
            palette,
        )
    }
    fn background(&self, layer: usize, mut x: usize, mut y: usize) -> Option<Layer> {
        if self.word(0x20) & (1 << layer) == 0
            || !self.window(self.word(0xD0 + (layer / 2) * 2) >> ((layer & 1) * 8), x, y)
        {
            return None;
        }
        let priority = self.word(0xF8 + (layer / 2) * 2) >> ((layer & 1) * 8) & 7;
        if priority == 0 {
            return None;
        }
        if self.word(0x22) & (1 << layer) != 0 {
            let mx = usize::from(self.word(0x22) >> 8 & 15) + 1;
            let my = usize::from(self.word(0x22) >> 12) + 1;
            x -= x % mx;
            y -= y % my;
        }
        let (ctl, format) = self.character_control(layer);
        let (sx, sy) = if layer < 2 {
            let scroll = 0x70 + layer * 0x10;
            (
                ((u64::from(self.long(scroll)) + x as u64 * u64::from(self.long(scroll + 8))) >> 16)
                    as usize,
                ((u64::from(self.long(scroll + 4)) + y as u64 * u64::from(self.long(scroll + 12)))
                    >> 16) as usize,
            )
        } else {
            (
                x + usize::from(self.word(0x90 + (layer - 2) * 4) & 0x7FF),
                y + usize::from(self.word(0x92 + (layer - 2) * 4) & 0x7FF),
            )
        };
        let (base, pixel, palette) = if layer < 2 && ctl & 2 != 0 {
            let width = if ctl & 8 == 0 { 512 } else { 1024 };
            let height = if ctl & 4 == 0 { 256 } else { 512 };
            (
                usize::from(self.word(0x3C) >> (layer * 4) & 7) * 0x20000,
                (sy % height) * width + sx % width,
                usize::from(self.word(0x2C) >> (layer * 8) & 7) * 256,
            )
        } else {
            self.tile(layer, ctl, format, sx, sy)
        };
        let opaque = self.word(0x20) & (0x100 << layer) != 0;
        let offset = usize::from(self.word(0xE4) >> (layer * 4) & 7) * 256;
        let rgb = match format {
            0 => {
                let byte = self.vram[(base + pixel / 2) & 0x7FFFF];
                let index = usize::from(if pixel & 1 == 0 { byte >> 4 } else { byte & 15 });
                if index == 0 && !opaque {
                    return None;
                }
                self.palette(offset + palette + index)
            }
            1 => {
                let index = usize::from(self.vram[(base + pixel) & 0x7FFFF]);
                if index == 0 && !opaque {
                    return None;
                }
                self.palette(offset + (palette & 0x700) + index)
            }
            2 => {
                let index = usize::from(self.vword(base + pixel * 2) & 2047);
                if index == 0 && !opaque {
                    return None;
                }
                self.palette(offset + index)
            }
            3 => {
                let color = self.vword(base + pixel * 2);
                if color & 0x8000 == 0 && !opaque {
                    return None;
                }
                rgb555(color)
            }
            _ => {
                let a = self.vword(base + pixel * 4);
                let b = self.vword(base + pixel * 4 + 2);
                if a & 0x8000 == 0 && !opaque {
                    return None;
                }
                [b as u8, (b >> 8) as u8, a as u8]
            }
        };
        Some(Layer {
            rgb,
            priority,
            ratio: self.word(0x108 + (layer / 2) * 2) >> ((layer & 1) * 8) & 31,
            calculate: self.word(0xEC) & (1 << layer) != 0,
            id: layer,
        })
    }
    fn sprite(&self, sprites: &Vdp1, x: usize, y: usize) -> Result<Option<Layer>, String> {
        if !self.window(self.word(0xD4) >> 8, x, y) {
            return Ok(None);
        }
        let data = sprites.display_pixel(x, y);
        if data == 0 {
            return Ok(None);
        }
        let rgb_mode = self.word(0xE0) & 0x20 != 0 && data & 0x8000 != 0;
        let (selector, ratio, rgb) = if rgb_mode {
            (0, 0, rgb555(data))
        } else {
            let (p, r) = if self.word(0xE0) & 15 == 0 {
                (data >> 14, data >> 11 & 7)
            } else {
                (data >> 13 & 7, data >> 11 & 3)
            };
            (
                usize::from(p),
                usize::from(r),
                self.palette(
                    usize::from(data & 2047) + usize::from(self.word(0xE6) >> 4 & 7) * 256,
                ),
            )
        };
        let priority = self.word(0xF0 + selector / 2 * 2) >> (selector % 2 * 8) & 7;
        if priority == 0 {
            return Ok(None);
        }
        let condition = self.word(0xE0) >> 12 & 3;
        if !rgb_mode && data & 2047 == 2046 {
            return Err("Unsupported VDP2 normal shadow sprite".into());
        }
        if condition == 3 && !rgb_mode && self.word(0xEC) & 0x40 != 0 {
            return Err("Unsupported VDP2 palette sprite color-MSB calculation".into());
        }
        let number = self.word(0xE0) >> 8 & 7;
        let calculate = match condition {
            0 => priority <= number,
            1 => priority == number,
            2 => priority >= number,
            _ => data & 0x8000 != 0,
        };
        Ok(Some(Layer {
            rgb,
            priority,
            ratio: self.word(0x100 + ratio / 2 * 2) >> (ratio % 2 * 8) & 31,
            calculate: calculate && self.word(0xEC) & 0x40 != 0,
            id: 6,
        }))
    }
    fn render(&self, sprites: Option<&Vdp1>) -> Result<Vec<u8>, String> {
        let mut pixels = vec![0; WIDTH * HEIGHT * 4];
        if self.word(0) & 0x8000 == 0 {
            for p in pixels.as_chunks_mut::<4>().0 {
                p[3] = 255;
            }
            return Ok(pixels);
        }
        self.validate()?;
        for y in 0..HEIGHT {
            let back = self.vword(
                ((self.long(0xAC) & 0x7FFFF) as usize
                    + if self.word(0xAC) & 0x8000 != 0 { y } else { 0 })
                    * 2,
            );
            for x in 0..WIDTH {
                let mut top = Layer {
                    rgb: rgb555(back),
                    id: 5,
                    ..Layer::default()
                };
                let mut second = top;
                for layer in [
                    self.background(3, x, y),
                    self.background(2, x, y),
                    self.background(1, x, y),
                    self.background(0, x, y),
                    sprites.map(|s| self.sprite(s, x, y)).transpose()?.flatten(),
                ]
                .into_iter()
                .flatten()
                {
                    if layer.priority >= top.priority {
                        second = top;
                        top = layer;
                    } else if layer.priority >= second.priority {
                        second = layer;
                    }
                }
                let mut rgb = top.rgb;
                if top.calculate && self.window(self.word(0xD6) >> 8, x, y) {
                    for (ch, value) in rgb.iter_mut().enumerate() {
                        *value = if self.word(0xEC) & 0x100 != 0 {
                            u16::from(*value)
                                .saturating_add(u16::from(second.rgb[ch]))
                                .min(255) as u8
                        } else {
                            ((u16::from(*value) * (31 - top.ratio)
                                + u16::from(second.rgb[ch]) * (top.ratio + 1))
                                >> 5) as u8
                        };
                    }
                }
                let p = &mut pixels[(y * WIDTH + x) * 4..][..4];
                // ST-058 §13.1: offset follows color calculation and uses
                // the top screen's enable/select, never the lower screen's.
                rgb = self.offset(rgb, top.id);
                p[..3].copy_from_slice(&rgb);
                p[3] = 255;
            }
        }
        Ok(pixels)
    }
}
