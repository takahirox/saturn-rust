//! Original VDP1 command processor. Behavioral source: Sega ST-013-R3 §§2–7.
//! Commands advance on emulated clocks, never on framebuffer inspection. Pixel
//! coverage uses integer ruled quadrilaterals. Raster traversal retains its
//! position between clocks so CPU accesses, stops and frame changes can observe
//! partial drawing rather than an atomically completed raster row.
use crate::bus::BusError;

pub const VRAM: u32 = 0x05C0_0000;
pub const FRAMEBUFFER: u32 = 0x05C8_0000;
pub const REGISTERS: u32 = 0x05D0_0000;
pub const VRAM_SIZE: usize = 0x80000;
pub const BUFFER_SIZE: usize = 0x40000;
const COMMAND_CLOCKS: u32 = 32;
const COMMAND_WORD_CLOCKS: u32 = COMMAND_CLOCKS / 16;

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
    shade_read: usize,
    lookup: [u16; 16],
    lookup_read: usize,
    texture: Option<(usize, u16)>,
    row: i32,
    rows: i32,
    line: Option<RasterLine>,
    clip: Option<[Point; 2]>,
}
impl Drawing {
    fn tables_pending(&self) -> bool {
        self.shade_read < 4 || self.lookup_read < 16
    }
    fn texture_address(&self, control: u16) -> Option<usize> {
        if self.command[0] & 15 > 2 {
            return None;
        }
        let line;
        let current = match &self.line {
            Some(current) => current,
            None => {
                line = Vdp1::raster_line(self);
                &line
            }
        };
        if current.dot > current.last {
            return None;
        }
        let (pixel, _) = Vdp1::texture_coordinates(&self.command, current, control);
        let mode = self.command[2] >> 3 & 7;
        let offset = match mode {
            0 | 1 => pixel / 2,
            2..=4 => pixel,
            _ => pixel * 2,
        };
        Some((usize::from(self.command[4]) * 8 + offset) & (VRAM_SIZE - 2))
    }
    fn texture_pending(&self, control: u16) -> bool {
        self.texture_address(control)
            .is_some_and(|address| self.texture.is_none_or(|(old, _)| old != address))
    }
}

struct RasterLine {
    a: Point,
    b: Point,
    uv0: (i32, i32),
    uv1: (i32, i32),
    shade0: [i32; 3],
    shade1: [i32; 3],
    textured: bool,
    dot: i32,
    last: i32,
    end_codes: u8,
}

impl RasterLine {
    /// ST-013-R3 §6.3 p.83/fig.6.6: reject separated lines. Only
    /// horizontal/vertical lines may start at the clipping boundary; keep
    /// the original interpolation denominator and texture/shading endpoints.
    fn preclip(&mut self, [low, high]: [Point; 2]) {
        if low.x > high.x
            || low.y > high.y
            || self.a.x.max(self.b.x) < low.x
            || self.a.x.min(self.b.x) > high.x
            || self.a.y.max(self.b.y) < low.y
            || self.a.y.min(self.b.y) > high.y
        {
            self.last = -1;
            return;
        }
        if self.a.x != self.b.x && self.a.y != self.b.y {
            return;
        }
        let inside = |p: Point| p.x >= low.x && p.x <= high.x && p.y >= low.y && p.y <= high.y;
        // Starting at the inside endpoint avoids reading end codes from the
        // clipped side. UVs and Gouraud endpoints follow the inverted line.
        if !inside(self.a) && inside(self.b) {
            std::mem::swap(&mut self.a, &mut self.b);
            std::mem::swap(&mut self.uv0, &mut self.uv1);
            std::mem::swap(&mut self.shade0, &mut self.shade1);
        }
        let (a, b, low, high) = if self.a.y == self.b.y {
            (self.a.x, self.b.x, low.x, high.x)
        } else {
            (self.a.y, self.b.y, low.y, high.y)
        };
        if b >= a {
            self.dot = (low - a).max(0);
            self.last = self.last.min(high - a);
        } else {
            self.dot = (a - high).max(0);
            self.last = self.last.min(a - low);
        }
    }
}

struct PendingPixel {
    buffer: usize,
    address: usize,
    color: Option<u16>,
    clocks: u32,
}

#[derive(Clone, Copy)]
struct EraseArea {
    buffer: usize,
    width: usize,
    bytes: usize,
    color: u16,
    left: usize,
    right: usize,
    top: usize,
    bottom: usize,
}
struct Erasing {
    area: EraseArea,
    x: usize,
    y: usize,
    overhead: u32,
    budget: u32,
}

pub struct Vdp1 {
    pub vram: Vec<u8>,
    buffers: [Vec<u8>; 2],
    draw: usize,
    tvmr: u16,
    fbcr: u16,
    draw_control: u16,
    fb_request: bool,
    ptmr: u16,
    erase: [u16; 3],
    erase_latched: [u16; 3],
    edsr: u16,
    lopr: u16,
    copr: u16,
    pc: usize,
    return_address: Option<usize>,
    running: bool,
    phase: u32,
    command_words: [u16; 16],
    command_read: usize,
    stop_remaining: Option<u32>,
    vblank: bool,
    blank_erase_started: bool,
    blank_line_clocks: u32,
    erasing: Option<Erasing>,
    display_erase: Option<EraseArea>,
    drawing: Option<Drawing>,
    pending_pixel: Option<PendingPixel>,
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
            draw_control: 0,
            fb_request: false,
            ptmr: 0,
            erase: [0; 3],
            erase_latched: [0; 3],
            edsr: 0,
            lopr: 0,
            copr: 0,
            pc: 0,
            return_address: None,
            running: false,
            phase: 0,
            command_words: [0; 16],
            command_read: 0,
            stop_remaining: None,
            vblank: false,
            blank_erase_started: false,
            blank_line_clocks: 1708,
            erasing: None,
            display_erase: None,
            drawing: None,
            pending_pixel: None,
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
    pub(crate) fn rotated_scanout(&self) -> bool {
        self.tvmr & 2 != 0
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
            // MODR reports written settings, not internal latches (§4.9).
            0x16 => Ok(0x1000 | (self.ptmr & 2) << 7 | (self.fbcr & 0x1E) << 3 | self.tvmr & 0xF),
            _ => Err(BusError::UnsupportedDevice { address, width: 2 }),
        }
    }
    pub fn validate_write(address: u32, value: u16) -> Result<(), BusError> {
        let valid = match address - REGISTERS {
            // HDTV/exclusive monitors remain separate from standard TV modes.
            0 => value & !0xF == 0 && matches!(value & 7, 0..=3),
            2 => value & !0x1F == 0 && value & 0xC != 4,
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
                // ST-013 §4.5: ENDR terminates within about 30 clocks.
                // It neither fetches END nor raises the draw-end interrupt.
                if self.running {
                    self.stop_remaining = Some(30);
                }
            }
            _ => unreachable!(),
        }
        // Software is allowed to enable VBE just after VBlank-IN (§4.2).
        if self.vblank && self.tvmr & 8 != 0 && !self.blank_erase_started {
            self.start_blank_erase();
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
        self.pending_pixel = None;
        self.copr = 0;
        self.return_address = None;
        self.running = true;
        self.phase = 0;
        self.command_read = 0;
        self.stop_remaining = None;
    }
    /// Field-change latch; CPU sees only the buffer currently being drawn.
    pub fn vblank_out(&mut self) {
        // §4.2: erase is interrupted at VBlank-OUT, even if unfinished.
        self.vblank = false;
        self.blank_erase_started = false;
        self.erasing = None;
        let change = self.fbcr & 2 == 0 || self.fb_request && self.fbcr & 1 != 0;
        if change {
            self.draw ^= 1;
            // ST-013 §4, p.35/43/44: EOS/DIE/DIL apply to drawing
            // only after a framebuffer change, not on the CPU write.
            self.draw_control = self.fbcr & 0x1C;
            // ST-013-R3 ch.4: EWDR/EWLR/EWRR take effect only at
            // framebuffer switching, including manual erase/change mode.
            self.erase_latched = self.erase;
            self.lopr = self.copr;
            self.edsr = (self.edsr >> 1) & 1;
        }
        self.display_erase = if self.tvmr & 0xA == 0
            && (self.fbcr & 2 == 0 || self.fb_request && self.fbcr & 1 == 0)
        {
            Some(self.erase_area(self.draw ^ 1))
        } else {
            None
        };
        self.fb_request = false;
        if change && self.ptmr == 2 {
            self.start(false);
        }
    }
    fn erase_area(&self, buffer: usize) -> EraseArea {
        let (width, height, bytes) = self.geometry();
        let [color, left, right] = self.erase_latched;
        let x1 = usize::from(left >> 9 & 63) * 16 / bytes;
        let x2 = (usize::from(right >> 9 & 127) * 16 / bytes).min(width);
        let y1 = usize::from(left & 511).min(height);
        let y2 = (usize::from(right & 511) + 1).min(height);
        EraseArea {
            buffer,
            width,
            bytes,
            color,
            left: x1,
            right: x2,
            top: y1,
            bottom: y2,
        }
    }
    fn erase_dot(&mut self, area: EraseArea, x: usize, y: usize) {
        let at = (y * area.width + x) * area.bytes;
        if area.bytes == 2 {
            self.buffers[area.buffer][at..at + 2].copy_from_slice(&area.color.to_be_bytes());
        } else {
            self.buffers[area.buffer][at] = area.color.to_be_bytes()[x & 1];
        }
    }
    /// §4.4: display erase follows readout; it must not erase future lines.
    pub(crate) fn erase_display_line(&mut self, line: usize, display_width: usize) {
        if let Some(area) = self.display_erase
            && (area.top..area.bottom).contains(&line)
        {
            let trailing = if area.bytes == 2 { 4 } else { 8 };
            for x in area.left..area.right.min(display_width + trailing) {
                self.erase_dot(area, x, line);
            }
        }
    }
    pub(crate) fn vblank_in(&mut self, line_clocks: u32) {
        self.vblank = true;
        self.blank_erase_started = false;
        self.display_erase = None;
        self.blank_line_clocks = line_clocks;
        if self.tvmr & 0xA != 0 {
            self.start_blank_erase();
        }
    }
    fn start_blank_erase(&mut self) {
        self.blank_erase_started = true;
        let area = self.erase_area(self.draw ^ 1);
        if area.left < area.right && area.top < area.bottom {
            self.erasing = Some(Erasing {
                area,
                x: area.left,
                y: area.top,
                overhead: 200,
                budget: self.blank_line_clocks.saturating_sub(200),
            });
        }
    }
    pub(crate) fn blank_line_start(&mut self) {
        if let Some(erase) = &mut self.erasing {
            erase.overhead = 200;
            erase.budget = self.blank_line_clocks.saturating_sub(200);
        }
    }
    fn erase_deadline(&self) -> u32 {
        self.erasing.as_ref().map_or(u32::MAX, |erase| {
            if erase.overhead != 0 {
                erase.overhead
            } else if erase.budget != 0 {
                1
            } else {
                u32::MAX
            }
        })
    }
    fn advance_erase(&mut self, clocks: u32) {
        let Some(mut erase) = self.erasing.take() else {
            return;
        };
        if erase.overhead != 0 {
            erase.overhead -= clocks;
        } else if erase.budget != 0 && clocks != 0 {
            // §4.4: a 16-bit write erases one RGB pixel or two 8-bit pixels.
            for _ in 0..2 / erase.area.bytes {
                self.erase_dot(erase.area, erase.x, erase.y);
                erase.x += 1;
                if erase.x == erase.area.right {
                    erase.x = erase.area.left;
                    erase.y += 1;
                    if erase.y == erase.area.bottom {
                        return;
                    }
                }
            }
            erase.budget -= 1;
        }
        self.erasing = Some(erase);
    }
    pub fn clocks_to_event(&self) -> u32 {
        let draw = if self.running {
            let draw = if let Some(pixel) = &self.pending_pixel {
                pixel.clocks
            } else {
                match &self.drawing {
                    Some(draw)
                        if !draw.tables_pending() && !draw.texture_pending(self.draw_control) =>
                    {
                        1
                    }
                    _ => COMMAND_WORD_CLOCKS - self.phase,
                }
            };
            draw.min(self.stop_remaining.unwrap_or(u32::MAX))
        } else {
            u32::MAX
        };
        draw.min(self.erase_deadline())
    }
    /// ST-013 §§2/6.3: replacement draws one pixel per main clock;
    /// shadow/translucency take six. Table fetches use separate deadlines.
    /// Cyclic command lists remain busy and bounded.
    /// The caller splits advancement at `clocks_to_event`, as the Bus scheduler does.
    pub fn advance(&mut self, clocks: u32) -> Result<bool, BusError> {
        self.advance_with_bus(clocks, false, false)
    }
    /// ST-013 §2.1 pp.19–20: the system controller has priority over
    /// drawing on each local memory. The other framebuffer remains independent.
    pub(crate) fn advance_with_bus(
        &mut self,
        clocks: u32,
        vram_busy: bool,
        framebuffer_busy: bool,
    ) -> Result<bool, BusError> {
        let mut remaining = clocks;
        let mut ended = false;
        while remaining != 0 {
            let delta = remaining.min(self.clocks_to_event());
            ended |= self.advance_phase(delta, vram_busy, framebuffer_busy)?;
            remaining -= delta;
        }
        Ok(ended)
    }
    fn advance_phase(
        &mut self,
        clocks: u32,
        vram_busy: bool,
        framebuffer_busy: bool,
    ) -> Result<bool, BusError> {
        assert!(clocks <= self.clocks_to_event());
        self.advance_erase(clocks);
        if !self.running {
            return Ok(false);
        }
        if let Some(left) = self.stop_remaining {
            if clocks == left {
                self.running = false;
                self.drawing = None;
                self.pending_pixel = None;
                self.stop_remaining = None;
                return Ok(false);
            }
            self.stop_remaining = Some(left - clocks);
        }
        let tables_pending = self.drawing.as_ref().is_some_and(Drawing::tables_pending);
        let texture_pending = self.pending_pixel.is_none()
            && self
                .drawing
                .as_ref()
                .is_some_and(|draw| draw.texture_pending(self.draw_control));
        let rejected = framebuffer_busy
            && self.pending_pixel.is_none()
            && self.drawing.as_ref().is_some_and(|draw| {
                let line;
                let current = match &draw.line {
                    Some(current) => current,
                    None => {
                        line = Self::raster_line(draw);
                        &line
                    }
                };
                current.dot > current.last
            });
        let needs_vram =
            self.drawing.as_ref().is_none_or(|draw| {
                draw.tables_pending() || draw.texture_pending(self.draw_control)
            }) && self.pending_pixel.is_none();
        if (vram_busy && needs_vram)
            || (framebuffer_busy
                && self.drawing.is_some()
                && !tables_pending
                && !texture_pending
                && !rejected)
        {
            return Ok(false);
        }
        if let Some(mut pixel) = self.pending_pixel.take() {
            pixel.clocks -= clocks;
            if pixel.clocks == 0 {
                if let Some(color) = pixel.color {
                    self.buffers[pixel.buffer][pixel.address..pixel.address + 2]
                        .copy_from_slice(&color.to_be_bytes());
                }
                if self
                    .drawing
                    .as_ref()
                    .is_some_and(|draw| draw.row > draw.rows)
                {
                    self.drawing = None;
                }
            } else {
                self.pending_pixel = Some(pixel);
            }
            return Ok(false);
        }
        if let Some(mut draw) = self.drawing.take() {
            if draw.tables_pending() {
                self.phase += clocks;
                if self.phase == COMMAND_WORD_CLOCKS {
                    self.phase = 0;
                    // ST-013 §5.3 table5.2: A, B, C, D are independent
                    // VRAM words. A CPU grant pauses this fetch, without
                    // resampling earlier vertices or touching the framebuffer.
                    if draw.shade_read < 4 {
                        let v = self.word(usize::from(draw.command[14]) * 8 + draw.shade_read * 2);
                        draw.shade[draw.shade_read] =
                            std::array::from_fn(|ch| i32::from(v >> (ch * 5) & 31) - 16);
                        draw.shade_read += 1;
                    } else {
                        // ST-013 §3.1 step5 / §5.2: load the lookup table
                        // before character drawing. Retain all sixteen words,
                        // including entries not selected by the texture.
                        draw.lookup[draw.lookup_read] =
                            self.word(usize::from(draw.command[3]) * 8 + draw.lookup_read * 2);
                        draw.lookup_read += 1;
                    }
                }
                self.drawing = Some(draw);
                return Ok(false);
            }
            if clocks == 0 {
                self.drawing = Some(draw);
                return Ok(false);
            }
            if draw.texture_pending(self.draw_control) {
                self.phase += clocks;
                if self.phase == COMMAND_WORD_CLOCKS {
                    self.phase = 0;
                    let address = draw.texture_address(self.draw_control).unwrap();
                    draw.texture = Some((address, self.word(address)));
                }
                self.drawing = Some(draw);
                return Ok(false);
            }
            if draw.line.is_none() {
                draw.line = Some(Self::raster_line(&draw));
            }
            let line = draw.line.as_mut().unwrap();
            let done = self.draw_dot(
                &draw.command,
                &draw.lookup,
                draw.texture.map(|(_, word)| word),
                line,
            );
            if done {
                draw.line = None;
                draw.row += 1;
                draw.texture = None;
            }
            if draw.row <= draw.rows || self.pending_pixel.is_some() {
                self.drawing = Some(draw);
            }
            return Ok(false);
        }
        self.phase += clocks;
        if self.phase < COMMAND_WORD_CLOCKS {
            return Ok(false);
        }
        self.phase -= COMMAND_WORD_CLOCKS;
        if self.pc + 32 > VRAM_SIZE || self.pc & 31 != 0 {
            return self.invalid();
        }
        // §8.3 fig.8.3: +1E is dummy and is skipped during table fetch.
        // Retain the existing final decode phase without issuing a RAM read.
        if self.command_read != 15 {
            self.command_words[self.command_read] = self.word(self.pc + self.command_read * 2);
        }
        self.command_read += 1;
        if self.command_read != 16 {
            return Ok(false);
        }
        self.command_read = 0;
        self.command(self.command_words)
    }
    fn word(&self, offset: usize) -> u16 {
        let offset = offset & (VRAM_SIZE - 1);
        u16::from_be_bytes([self.vram[offset], self.vram[(offset + 1) & (VRAM_SIZE - 1)]])
    }
    fn command(&mut self, c: [u16; 16]) -> Result<bool, BusError> {
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
        // ST-013 §2: command-table reads wrap at the end of VRAM.
        let next = (self.pc + 32) & (VRAM_SIZE - 1);
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
        let kind = c[0] & 15;
        if mode & 7 == 5 || mode >> 3 & 7 > 5 || (mode & 0x1000 != 0 && !matches!(kind, 1 | 2)) {
            return self.invalid();
        }
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
        let rows = match kind {
            6 => 0,
            5 => 3,
            _ => p[0].distance(p[3]).max(p[1].distance(p[2])),
        };
        self.drawing = Some(Drawing {
            command: *c,
            points: p,
            shade: [[0; 3]; 4],
            shade_read: if mode & 4 != 0 { 0 } else { 4 },
            lookup: [0; 16],
            lookup_read: if textured && mode >> 3 & 7 == 1 {
                0
            } else {
                16
            },
            texture: None,
            row: 0,
            rows,
            line: None,
            clip: if mode & 0x800 == 0 {
                let (width, height, _) = self.geometry();
                let mut low = Point::default();
                let mut high = Point {
                    x: self.system.x.min(width as i32 - 1),
                    y: self
                        .system
                        .y
                        .min((height * if self.draw_control & 8 != 0 { 2 } else { 1 }) as i32 - 1),
                };
                if mode & 0x600 == 0x400 {
                    low.x = low.x.max(self.user[0].x);
                    low.y = low.y.max(self.user[0].y);
                    high.x = high.x.min(self.user[1].x);
                    high.y = high.y.min(self.user[1].y);
                }
                Some([low, high])
            } else {
                None
            },
        });
        Ok(())
    }
    fn raster_line(draw: &Drawing) -> RasterLine {
        let Drawing {
            command: c,
            points: p,
            shade,
            row,
            rows,
            ..
        } = draw;
        let kind = c[0] & 15;
        let (a, b, uv0, uv1, shade0, shade1) = if kind == 6 {
            (p[0], p[1], (0, 0), (0, 0), shade[0], shade[1])
        } else if kind == 5 {
            let a = *row as usize;
            let b = (a + 1) % 4;
            (p[a], p[b], (0, 0), (0, 0), shade[a], shade[b])
        } else {
            let a = p[0].lerp(p[3], *row, *rows);
            let b = p[1].lerp(p[2], *row, *rows);
            let sa = std::array::from_fn(|ch| lerp(shade[0][ch], shade[3][ch], *row, *rows));
            let sb = std::array::from_fn(|ch| lerp(shade[1][ch], shade[2][ch], *row, *rows));
            let v = lerp(0, i32::from(c[5] & 255) - 1, *row, *rows);
            (a, b, (0, v), (i32::from(c[5] >> 8 & 63) * 8 - 1, v), sa, sb)
        };
        let mut line = RasterLine {
            a,
            b,
            uv0,
            uv1,
            shade0,
            shade1,
            textured: kind <= 2,
            dot: 0,
            last: a.distance(b),
            end_codes: 0,
        };
        if let Some(clip) = draw.clip {
            line.preclip(clip);
        }
        line
    }
    fn texture_coordinates(c: &[u16; 16], line: &RasterLine, control: u16) -> (usize, bool) {
        let w = usize::from(c[5] >> 8 & 63) * 8;
        let h = usize::from(c[5] & 255);
        let steps = line.a.distance(line.b) as usize;
        // Pre-clipping can reverse traversal. Evaluate the original line's
        // sample in reverse order, retaining its full span and phase.
        let dot = if line.uv0.0 > line.uv1.0 {
            steps - line.dot as usize
        } else {
            line.dot as usize
        };
        let high_speed = c[2] & 0x1000 != 0;
        let filtered = high_speed && steps + 1 < w;
        let source = if filtered { w / 2 } else { w };
        // 1997 manual §6.3 fig.6.5: reduction selects source-bin centers;
        // expansion distributes repeats between the endpoints (half ties
        // retain the earlier sample). HSS first forms the half-width source,
        // then scales it, rather than quantizing ordinary samples to parity.
        // See docs/vdp1-sampling.md for literal rows and generalization limits.
        let mut u = if source > steps + 1 {
            (2 * dot + 1) * source / (2 * (steps + 1))
        } else if steps == 0 {
            0
        } else {
            (2 * dot * (source - 1) + steps - 1) / (2 * steps)
        };
        let mut v = lerp(line.uv0.1, line.uv1.1, line.dot, line.a.distance(line.b)) as usize;
        if c[0] & 0x10 != 0 {
            u = source - 1 - u;
        }
        if c[0] & 0x20 != 0 {
            v = h - 1 - v;
        }
        if filtered {
            u = 2 * u + usize::from(control & 0x10 != 0);
        }
        (v * w + u, high_speed)
    }
    fn draw_dot(
        &mut self,
        c: &[u16; 16],
        lookup: &[u16; 16],
        texture: Option<u16>,
        line: &mut RasterLine,
    ) -> bool {
        if line.dot > line.last {
            return true;
        }
        let RasterLine {
            a,
            b,
            shade0,
            shade1,
            textured,
            ..
        } = *line;
        let steps = a.distance(b);
        let n = line.dot;
        // The cursor advances even for transparent, clipped or mesh pixels.
        // Character data comes from the retained physical word; publication
        // uses the independent framebuffer port.
        (|| {
            let point = a.lerp(b, n, steps);
            let mut color = c[3];
            if textured {
                // ST-013 §6.3/fig.6.5: HSS samples only the selected
                // original X parity on reduced raster lines. Expansion retains
                // ordinary sampling. EOS refers to source coordinates even
                // when the command flips horizontally.
                let (pixel, high_speed) = Self::texture_coordinates(c, line, self.draw_control);
                let mode = c[2] >> 3 & 7;
                let word =
                    texture.expect("texture data must be fetched before the framebuffer phase");
                let (raw, end) = match mode {
                    0 | 1 => (word >> ((3 - pixel % 4) * 4) & 15, 15),
                    2..=4 => (word >> ((1 - pixel % 2) * 8) & 255, 255),
                    _ => (word, 0x7FFF),
                };
                // HSS ignores end codes at every magnification (§6.3).
                if raw == end && c[2] & 0x80 == 0 && !high_speed {
                    line.end_codes += 1;
                    if line.end_codes == 2 {
                        line.dot = line.last;
                    }
                    return;
                }
                let raw = if (2..=4).contains(&mode) {
                    raw & ((1 << (mode + 4)) - 1)
                } else {
                    raw
                };
                if raw == 0 && c[2] & 0x40 == 0 {
                    return;
                }
                color = match mode {
                    0 => c[3] & !15 | raw,
                    1 => lookup[usize::from(raw)],
                    2..=4 => c[3] & !((1 << (mode + 4)) - 1) | raw,
                    _ => raw,
                };
            }
            let shade = std::array::from_fn(|ch| lerp(shade0[ch], shade1[ch], n, steps));
            self.plot(point, color, c[2], shade);
        })();
        line.dot += 1;
        line.dot > line.last
    }
    fn plot(&mut self, p: Point, color: u16, mode: u16, shade: [i32; 3]) {
        let (width, height, bytes) = self.geometry();
        // ST-013 §4.2/fig.4.1: choose logical even/odd rows, then
        // pack them into the 256 physical framebuffer rows. CPU accesses and
        // erase Y coordinates continue to address physical rows.
        let interlace = self.draw_control & 8 != 0;
        if interlace && (p.y & 1) != i32::from(self.draw_control >> 2 & 1) {
            return;
        }
        let buffer_y = if interlace { p.y >> 1 } else { p.y };
        if p.x < 0
            || p.y < 0
            || p.x > self.system.x
            || p.y > self.system.y
            || p.x as usize >= width
            || buffer_y as usize >= height
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
        let at = (buffer_y as usize * width + p.x as usize) * bytes;
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
                self.pending_pixel = Some(PendingPixel {
                    buffer: self.draw,
                    address: at,
                    color: None,
                    clocks: 5,
                });
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
        if mode & 1 != 0 && mode & 0x8000 == 0 {
            // ST-013 §6.3: shadow and translucency take six times the
            // replacement time. Retain the sampled background and the
            // physical buffer through the five remaining clocks. Other
            // masters may access memory between read and publication.
            self.pending_pixel = Some(PendingPixel {
                buffer: self.draw,
                address: at,
                color: Some(out),
                clocks: 5,
            });
        } else {
            self.buffers[self.draw][at..at + 2].copy_from_slice(&out.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod erase_tests;
#[cfg(test)]
mod preclip_tests;
#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod sampling_tests;
#[cfg(test)]
mod table_tests;
