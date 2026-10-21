//! Original VDP1 command processor. Behavioral source: Sega ST-013-R3 §§2–7.
//! Commands advance on emulated clocks, never on framebuffer inspection. Pixel
//! coverage uses integer ruled quadrilaterals; subpixel hardware edge rounding,
//! high-speed shrink and interlaced/rotated scanout remain separate work.
use crate::bus::BusError;

pub const VRAM: u32 = 0x05C0_0000;
pub const FRAMEBUFFER: u32 = 0x05C8_0000;
pub const REGISTERS: u32 = 0x05D0_0000;
pub const VRAM_SIZE: usize = 0x80000;
pub const BUFFER_SIZE: usize = 0x40000;
const COMMAND_CLOCKS: u32 = 32;

#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}
impl Point {
    fn lerp(self, b: Self, n: i32, d: i32) -> Self {
        Self {
            x: lerp(self.x, b.x, n, d),
            y: lerp(self.y, b.y, n, d),
        }
    }
    fn distance(self, b: Self) -> i32 {
        (self.x - b.x).abs().max((self.y - b.y).abs())
    }
}
fn lerp(a: i32, b: i32, n: i32, d: i32) -> i32 {
    if d == 0 { a } else { a + (b - a) * n / d }
}

struct Drawing {
    command: [u16; 16],
    points: [Point; 4],
    shade: [[i32; 3]; 4],
    row: i32,
    rows: i32,
}

pub struct Vdp1 {
    pub vram: Vec<u8>,
    buffers: [Vec<u8>; 2],
    draw: usize,
    tvmr: u16,
    fbcr: u16,
    fb_request: bool,
    ptmr: u16,
    erase: [u16; 3],
    edsr: u16,
    lopr: u16,
    copr: u16,
    pc: usize,
    return_address: Option<usize>,
    running: bool,
    phase: u32,
    drawing: Option<Drawing>,
    local: Point,
    system: Point,
    user: [Point; 2],
}
impl Default for Vdp1 {
    fn default() -> Self {
        Self {
            vram: vec![0; VRAM_SIZE],
            buffers: std::array::from_fn(|_| vec![0; BUFFER_SIZE]),
            draw: 0,
            tvmr: 0,
            fbcr: 0,
            fb_request: false,
            ptmr: 0,
            erase: [0; 3],
            edsr: 0,
            lopr: 0,
            copr: 0,
            pc: 0,
            return_address: None,
            running: false,
            phase: 0,
            drawing: None,
            local: Point::default(),
            system: Point::default(),
            user: [Point::default(); 2],
        }
    }
}
impl Vdp1 {
    pub fn draw_buffer(&self) -> &[u8] {
        &self.buffers[self.draw]
    }
    pub fn draw_buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffers[self.draw]
    }
    pub fn busy(&self) -> bool {
        self.running
    }
    pub(crate) fn normal_scanout(&self) -> bool {
        self.tvmr & 6 == 0
    }
    fn geometry(&self) -> (usize, usize, usize) {
        match self.tvmr & 7 {
            1 => (1024, 256, 1),
            3 => (512, 512, 1),
            4 => (512, 256, 2),
            _ => (512, 256, 2),
        }
    }
    pub fn display_pixel(&self, x: usize, y: usize) -> u16 {
        let (width, height, bytes) = self.geometry();
        if x >= width || y >= height {
            return 0;
        }
        let offset = (y * width + x) * bytes;
        let buffer = &self.buffers[self.draw ^ 1];
        if bytes == 1 {
            u16::from(buffer[offset])
        } else {
            u16::from_be_bytes([buffer[offset], buffer[offset + 1]])
        }
    }
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        match address - REGISTERS {
            0x10 => Ok(self.edsr),
            0x12 => Ok(self.lopr),
            0x14 => Ok(self.copr),
            // MODR: version 1, latched mode bits (ST-013 §4.9).
            0x16 => Ok(0x1000 | (self.ptmr & 2) << 7 | (self.fbcr & 0x1E) << 3 | self.tvmr & 0xF),
            _ => Err(BusError::UnsupportedDevice { address, width: 2 }),
        }
    }
    pub fn validate_write(address: u32, value: u16) -> Result<(), BusError> {
        let valid = match address - REGISTERS {
            // Explicitly reject scanout/interlace modes not yet integrated.
            0 => value & !0xF == 0 && matches!(value & 7, 0..=3),
            2 => value & !3 == 0,
            4 => value <= 2,
            6 | 8 | 10 => true,
            12 => value == 0,
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(BusError::UnsupportedDevice { address, width: 2 })
        }
    }
    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        Self::validate_write(address, value)?;
        match address - REGISTERS {
            0 => self.tvmr = value,
            2 => {
                self.fbcr = value;
                self.fb_request = true;
            }
            4 => {
                self.ptmr = value;
                if value == 1 {
                    self.start(true);
                }
            }
            offset @ (6 | 8 | 10) => self.erase[(offset as usize - 6) / 2] = value,
            12 => {
                self.running = false;
                self.drawing = None;
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn start(&mut self, capture_end: bool) {
        self.edsr = if capture_end {
            (self.edsr >> 1) & 1
        } else {
            self.edsr & 1
        };
        self.pc = 0;
        self.drawing = None;
        self.copr = 0;
        self.return_address = None;
        self.running = true;
        self.phase = 0;
    }
    /// Field-change latch; CPU sees only the buffer currently being drawn.
    pub fn vblank_out(&mut self) {
        let change = self.fbcr & 2 == 0 || self.fb_request && self.fbcr & 1 != 0;
        if change {
            self.draw ^= 1;
            self.lopr = self.copr;
            self.edsr = (self.edsr >> 1) & 1;
        }
        if self.fbcr & 2 == 0 || self.tvmr & 8 != 0 || self.fb_request && self.fbcr & 1 == 0 {
            self.erase_buffer(if change { self.draw } else { self.draw ^ 1 });
        }
        self.fb_request = false;
        if change && self.ptmr == 2 {
            self.start(false);
        }
    }
    fn erase_buffer(&mut self, buffer: usize) {
        let (width, height, bytes) = self.geometry();
        let [color, left, right] = self.erase;
        let x1 = usize::from(left >> 9 & 63) * 16 / bytes;
        let x2 = (usize::from(right >> 9 & 127) * 16 / bytes).min(width);
        let y1 = usize::from(left & 511).min(height);
        let y2 = (usize::from(right & 511) + 1).min(height);
        for y in y1..y2 {
            for x in x1..x2 {
                let at = (y * width + x) * bytes;
                if bytes == 2 {
                    self.buffers[buffer][at..at + 2].copy_from_slice(&color.to_be_bytes());
                } else {
                    self.buffers[buffer][at] = color.to_be_bytes()[x & 1];
                }
            }
        }
    }
    pub fn clocks_to_event(&self) -> u32 {
        if self.running {
            COMMAND_CLOCKS - self.phase
        } else {
            u32::MAX
        }
    }
    /// One command fetch or raster row per functional deadline. Cyclic command lists remain busy;
    /// they never turn into unbounded host loops or falsely set CEF.
    /// The caller splits advancement at `clocks_to_event`, as the Bus scheduler does.
    pub fn advance(&mut self, clocks: u32) -> Result<bool, BusError> {
        if !self.running {
            return Ok(false);
        }
        assert!(clocks <= self.clocks_to_event());
        self.phase += clocks;
        if self.phase < COMMAND_CLOCKS {
            return Ok(false);
        }
        self.phase -= COMMAND_CLOCKS;
        if let Some(mut draw) = self.drawing.take() {
            self.draw_row(&draw);
            draw.row += 1;
            if draw.row <= draw.rows {
                self.drawing = Some(draw);
            }
            return Ok(false);
        }
        self.command()
    }
    fn word(&self, offset: usize) -> u16 {
        let offset = offset & (VRAM_SIZE - 1);
        u16::from_be_bytes([self.vram[offset], self.vram[(offset + 1) & (VRAM_SIZE - 1)]])
    }
    fn command(&mut self) -> Result<bool, BusError> {
        if self.pc + 32 > VRAM_SIZE || self.pc & 31 != 0 {
            return self.invalid();
        }
        let c: [u16; 16] = std::array::from_fn(|i| self.word(self.pc + i * 2));
        self.copr = (self.pc / 8) as u16;
        if c[0] & 0x8000 != 0 {
            self.edsr |= 2;
            self.running = false;
            return Ok(true);
        }
        if c[0] & 0x4000 == 0 {
            match c[0] & 15 {
                0..=2 | 4..=6 => self.draw_command(&c)?,
                8 => self.user = [Self::point(&c, 6), Self::point(&c, 10)],
                9 => self.system = Self::point(&c, 10),
                10 => self.local = Self::point(&c, 6),
                _ => return self.invalid(),
            }
        }
        let next = self.pc + 32;
        self.pc = match c[0] >> 12 & 3 {
            0 => next,
            1 => usize::from(c[1]) * 8,
            2 => {
                if self.return_address.is_some() {
                    return self.invalid();
                }
                self.return_address = Some(next);
                usize::from(c[1]) * 8
            }
            _ => self.return_address.take().unwrap_or(next),
        };
        Ok(false)
    }
    fn invalid<T>(&self) -> Result<T, BusError> {
        Err(BusError::UnsupportedDevice {
            address: VRAM + self.pc as u32,
            width: 32,
        })
    }
    fn point(c: &[u16; 16], index: usize) -> Point {
        Point {
            x: i32::from((c[index] << 3) as i16 >> 3),
            y: i32::from((c[index + 1] << 3) as i16 >> 3),
        }
    }
    fn draw_command(&mut self, c: &[u16; 16]) -> Result<(), BusError> {
        let mode = c[2];
        if mode & 7 == 5 || mode >> 3 & 7 > 5 || mode & 0x1000 != 0 {
            return self.invalid();
        }
        let kind = c[0] & 15;
        let textured = kind <= 2;
        let width = i32::from(c[5] >> 8 & 63) * 8;
        let height = i32::from(c[5] & 255);
        let mut p = [
            Self::point(c, 6),
            Self::point(c, 8),
            Self::point(c, 10),
            Self::point(c, 12),
        ];
        if textured && (width == 0 || height == 0) {
            return Ok(());
        }
        if kind == 0 {
            p[2] = Point {
                x: p[0].x + width - 1,
                y: p[0].y + height - 1,
            };
        }
        if kind == 1 && c[0] >> 8 & 15 != 0 {
            let zoom = c[0] >> 8 & 15;
            if !matches!(zoom, 5..=7 | 9..=11 | 13..=15) || p[1].x < 0 || p[1].y < 0 {
                return self.invalid();
            }
            p[0].x -= p[1].x * (i32::from(zoom & 3) - 1) / 2;
            p[0].y -= p[1].y * (i32::from(zoom >> 2) - 1) / 2;
            p[2] = Point {
                x: p[0].x + p[1].x,
                y: p[0].y + p[1].y,
            };
        }
        if kind <= 1 {
            p[1] = Point {
                x: p[2].x,
                y: p[0].y,
            };
            p[3] = Point {
                x: p[0].x,
                y: p[2].y,
            };
        }
        for point in &mut p {
            point.x += self.local.x;
            point.y += self.local.y;
        }
        let shade: [[i32; 3]; 4] = std::array::from_fn(|i| {
            let v = self.word(usize::from(c[14]) * 8 + i * 2);
            std::array::from_fn(|ch| i32::from(v >> (ch * 5) & 31) - 16)
        });
        let rows = match kind {
            6 => 0,
            5 => 3,
            _ => p[0].distance(p[3]).max(p[1].distance(p[2])),
        };
        self.drawing = Some(Drawing {
            command: *c,
            points: p,
            shade,
            row: 0,
            rows,
        });
        Ok(())
    }
    fn draw_row(&mut self, draw: &Drawing) {
        let Drawing {
            command: c,
            points: p,
            shade,
            row,
            rows,
        } = draw;
        let kind = c[0] & 15;
        if kind == 6 {
            self.line(c, p[0], p[1], (0, 0), (0, 0), shade[0], shade[1], false);
        } else if kind == 5 {
            let a = *row as usize;
            let b = (a + 1) % 4;
            self.line(c, p[a], p[b], (0, 0), (0, 0), shade[a], shade[b], false);
        } else {
            let a = p[0].lerp(p[3], *row, *rows);
            let b = p[1].lerp(p[2], *row, *rows);
            let sa = std::array::from_fn(|ch| lerp(shade[0][ch], shade[3][ch], *row, *rows));
            let sb = std::array::from_fn(|ch| lerp(shade[1][ch], shade[2][ch], *row, *rows));
            let v = lerp(0, i32::from(c[5] & 255) - 1, *row, *rows);
            self.line(
                c,
                a,
                b,
                (0, v),
                (i32::from(c[5] >> 8 & 63) * 8 - 1, v),
                sa,
                sb,
                kind <= 2,
            );
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn line(
        &mut self,
        c: &[u16; 16],
        a: Point,
        b: Point,
        uv0: (i32, i32),
        uv1: (i32, i32),
        shade0: [i32; 3],
        shade1: [i32; 3],
        textured: bool,
    ) {
        let steps = a.distance(b);
        let mut end_codes = 0;
        for n in 0..=steps {
            let point = a.lerp(b, n, steps);
            let mut color = c[3];
            if textured {
                let w = usize::from(c[5] >> 8 & 63) * 8;
                let h = usize::from(c[5] & 255);
                let mut u = lerp(uv0.0, uv1.0, n, steps) as usize;
                let mut v = lerp(uv0.1, uv1.1, n, steps) as usize;
                if c[0] & 0x10 != 0 {
                    u = w - 1 - u;
                }
                if c[0] & 0x20 != 0 {
                    v = h - 1 - v;
                }
                let pixel = v * w + u;
                let base = usize::from(c[4]) * 8;
                let mode = c[2] >> 3 & 7;
                let (raw, end) = match mode {
                    0 | 1 => {
                        let byte = self.vram[(base + pixel / 2) & (VRAM_SIZE - 1)];
                        (
                            u16::from(if pixel & 1 == 0 { byte >> 4 } else { byte & 15 }),
                            15,
                        )
                    }
                    2..=4 => (u16::from(self.vram[(base + pixel) & (VRAM_SIZE - 1)]), 255),
                    _ => (self.word(base + pixel * 2), 0x7FFF),
                };
                if raw == end && c[2] & 0x80 == 0 {
                    end_codes += 1;
                    if end_codes == 2 {
                        break;
                    }
                    continue;
                }
                let raw = if (2..=4).contains(&mode) {
                    raw & ((1 << (mode + 4)) - 1)
                } else {
                    raw
                };
                if raw == 0 && c[2] & 0x40 == 0 {
                    continue;
                }
                color = match mode {
                    0 => c[3] & !15 | raw,
                    1 => self.word(usize::from(c[3]) * 8 + usize::from(raw) * 2),
                    2..=4 => c[3] & !((1 << (mode + 4)) - 1) | raw,
                    _ => raw,
                };
            }
            let shade = std::array::from_fn(|ch| lerp(shade0[ch], shade1[ch], n, steps));
            self.plot(point, color, c[2], shade);
        }
    }
    fn plot(&mut self, p: Point, color: u16, mode: u16, shade: [i32; 3]) {
        let (width, height, bytes) = self.geometry();
        if p.x < 0
            || p.y < 0
            || p.x > self.system.x
            || p.y > self.system.y
            || p.x as usize >= width
            || p.y as usize >= height
        {
            return;
        }
        if mode & 0x400 != 0 {
            let inside = p.x >= self.user[0].x
                && p.x <= self.user[1].x
                && p.y >= self.user[0].y
                && p.y <= self.user[1].y;
            if inside == (mode & 0x200 != 0) {
                return;
            }
        }
        if mode & 0x100 != 0 && (p.x ^ p.y) & 1 != 0 {
            return;
        }
        let at = (p.y as usize * width + p.x as usize) * bytes;
        if bytes == 1 {
            self.buffers[self.draw][at] = color as u8;
            return;
        }
        let old =
            u16::from_be_bytes([self.buffers[self.draw][at], self.buffers[self.draw][at + 1]]);
        let mut out = color;
        if mode & 0x8000 != 0 {
            out = old | 0x8000;
        } else if mode & 7 == 1 {
            if old & 0x8000 == 0 {
                return;
            }
            out = (old & 0x7BDE) >> 1 | 0x8000;
        } else if mode & 7 != 0 {
            out = color & 0x8000;
            for (ch, correction) in shade.iter().enumerate() {
                let mut component = i32::from(color >> (ch * 5) & 31);
                if mode & 4 != 0 {
                    component = (component + correction).clamp(0, 31);
                }
                if mode & 3 == 2 {
                    component >>= 1;
                }
                if mode & 3 == 3 && old & 0x8000 != 0 {
                    component = (component + i32::from(old >> (ch * 5) & 31)) >> 1;
                }
                out |= (component as u16) << (ch * 5);
            }
        }
        self.buffers[self.draw][at..at + 2].copy_from_slice(&out.to_be_bytes());
    }
}
