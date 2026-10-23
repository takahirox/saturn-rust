//! Original software-visible VDP2 renderer; Sega ST-058-R2 §§3–6,8–13.
//! Normal tile/bitmap sampling and VDP1 composition are independent of guest identity.
use crate::{bus::BusError, devices::VDP2, vdp1::Vdp1};
use std::cell::{Cell, RefCell};

mod access;
mod fetch;
mod rotation;
mod sprite;
mod tables;

pub const CRAM: u32 = 0x05F0_0000;
pub struct Vdp2 {
    registers: [u8; 0x120],
    pub vram: Vec<u8>,
    pub cram: Vec<u8>,
    pub vblank: bool,
    pub hblank: bool,
    live_counters: (u16, u16),
    pub(crate) pal: bool,
    odd: bool,
    latched_counters: Cell<(u16, u16)>,
    external_latched: Cell<bool>,
    pub(crate) scanning: Vec<u8>,
    completed: Option<Result<Vec<u8>, String>>,
    scan_error: Option<String>,
    scan_size: (usize, usize),
    completed_size: (usize, usize),
    rotation_read: rotation::RotationRead,
    scan_x: usize,
    scan_history: [[u8; 3]; 2],
    // Static debugger decoding remains useful for inspecting an unfinished
    // register setup. Only beam-driven output consumes display bus grants.
    beam_rendering: bool,
    clocked: bool,
    line_tables: tables::LineTables,
    pub(crate) display_fetch: RefCell<fetch::DisplayFetch>,
}
impl Default for Vdp2 {
    fn default() -> Self {
        Self {
            registers: [0; 0x120],
            vram: vec![0; 0x80000],
            cram: vec![0; 0x1000],
            vblank: false,
            hblank: false,
            live_counters: (0, 0),
            pal: false,
            odd: true,
            latched_counters: Cell::new((0, 0)),
            external_latched: Cell::new(false),
            scanning: Vec::new(),
            completed: None,
            scan_error: None,
            scan_size: (0, 0),
            completed_size: (0, 0),
            rotation_read: rotation::RotationRead::default(),
            scan_x: 0,
            scan_history: [[0; 3]; 2],
            beam_rendering: false,
            clocked: false,
            line_tables: tables::LineTables::default(),
            display_fetch: RefCell::default(),
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
    coefficient_color: Option<u16>,
    palette: bool,
}
struct Pixel {
    rgb: [u8; 3],
    code: u16,
    color_msb: bool,
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
            0 | 2 | 4 | 8 | 0xA | 0xE | 0x10..=0x1E | 0x20..=0x2E | 0x30..=0x6E |
            0x70..=0xBE | 0xC0..=0xDE | 0xE0 | 0xE2 |
            0xE4..=0xFC | 0x100..=0x11E
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
    fn vlong(&self, offset: usize) -> u32 {
        u32::from(self.vword(offset)) << 16 | u32::from(self.vword(offset + 2))
    }
    // Table pointers hold word addresses with bit 0 ignored: ST-058 §§5.3/8.1.
    fn table_address(&self, register: usize) -> usize {
        ((self.long(register) & 0x7FFFE) << 1) as usize & 0x7FFFF
    }
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        let offset = (address - VDP2) as usize;
        if offset == 4 {
            // ST-058 §2.5: board TV standard and current scan field.
            return Ok(u16::from(self.external_latched.replace(false)) << 9
                | u16::from(self.vblank || self.word(0) & 0x8000 == 0) << 3
                | u16::from(self.hblank) << 2
                | u16::from(self.odd || self.word(0) & 0xC0 == 0) << 1
                | u16::from(self.pal));
        }
        match offset {
            2 if self.word(2) & 0x200 == 0 => self.latched_counters.set(self.live_counters),
            8 => return Ok(self.latched_counters.get().0),
            0xA => return Ok(self.latched_counters.get().1),
            _ => (),
        }
        if !Self::supported(offset) {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        Ok(self.word(offset))
    }
    pub(crate) fn scan_field(&mut self, odd: bool) {
        self.odd = odd;
    }
    pub(crate) fn counters(&mut self, horizontal: u16, vertical: u16) {
        self.live_counters = (horizontal & 0x3FF, vertical & 0x3FF);
    }
    pub(crate) fn external_latch(&self) {
        if self.word(2) & 0x200 != 0 {
            self.latched_counters.set(self.live_counters);
            self.external_latched.set(true);
        }
    }
    pub(crate) fn validate_write(address: u32) -> Result<(), BusError> {
        let offset = (address - VDP2) as usize;
        if matches!(offset, 4 | 8 | 0xA) || !Self::supported(offset) {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        Ok(())
    }
    pub(crate) fn validate_value(address: u32, value: u16) -> Result<(), BusError> {
        Self::validate_write(address)?;
        if (VDP2 + 0x10..=VDP2 + 0x1E).contains(&address)
            && (0..4).any(|n| (8..12).contains(&(value >> (n * 4) & 15)))
        {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        Ok(())
    }
    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        Self::validate_value(address, value)?;
        let offset = (address - VDP2) as usize;
        self.registers[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        if offset <= 0xA6 {
            self.reconfigure_display();
        }
        Ok(())
    }
    pub fn framebuffer(&self) -> Result<Vec<u8>, String> {
        self.render(None)
    }
    pub fn composite(&self, sprites: &Vdp1) -> Result<Vec<u8>, String> {
        self.render(Some(sprites))
    }
    /// The debugger may inspect current VRAM before the first field completes.
    /// Once scanout has produced a field, presentation uses its historical
    /// pixels, never a retroactive render using later registers or VRAM.
    pub fn scanout(&self, sprites: &Vdp1) -> Result<Vec<u8>, String> {
        self.completed
            .clone()
            .unwrap_or_else(|| self.composite(sprites))
    }
    pub(crate) fn begin_scanline(&mut self, line: usize) {
        if self.display_fetch.borrow().prepared_line != Some(line) {
            *self.display_fetch.get_mut() = fetch::DisplayFetch::default();
        }
        self.scan_x = 0;
        self.scan_history = [[0; 3]; 2];
        let y = if self.word(0) & 0xC0 == 0xC0 {
            line * 2 + usize::from(!self.odd)
        } else {
            line
        };
        self.read_line_tables(y);
        self.beam_rendering = true;
        self.read_rotation_line(y, line == 0);
        self.beam_rendering = false;
        self.rotation_read.line = None;
    }
    pub(crate) fn scanline(&mut self, sprites: &Vdp1, line: usize) {
        self.scan_to(sprites, line, self.dimensions().0);
    }
    /// Sample only dots already passed by the beam. Bus writes flush this
    /// prefix before changing VRAM/CRAM/registers. Normal-screen operands
    /// were acquired by the clock; palette/effect state remains live.
    /// Horizontal gradation history survives successive flushes.
    pub(crate) fn scan_to(&mut self, sprites: &Vdp1, line: usize, end: usize) {
        let size = self.dimensions();
        if size != self.scan_size {
            self.scan_size = size;
            self.scan_x = 0;
            self.scan_history = [[0; 3]; 2];
            self.scanning = vec![0; size.0 * size.1 * 4];
            for pixel in self.scanning.as_chunks_mut::<4>().0 {
                pixel[3] = 255;
            }
        }
        let y = if self.word(0) & 0xC0 == 0xC0 {
            line * 2 + usize::from(!self.odd)
        } else {
            line
        };
        if y >= size.1 {
            return;
        }
        let first = self.scan_x;
        let end = end.min(size.0);
        if end <= first {
            return;
        }
        // Table reload is a line-start event. Writes during active display
        // must not move this line's origin before it is sampled at HBlank.
        // The initial reset field, or a density change inside a line, has no
        // matching line-start latch and initializes here.
        if !self.rotation_line_ready(y) || self.line_tables.line != Some(y) {
            self.read_line_tables(y);
            self.beam_rendering = true;
            self.read_rotation_line(y, line == 0);
            self.beam_rendering = false;
        }
        self.rotation_read.line = Some(y);
        self.beam_rendering = true;
        let mut history = self.scan_history;
        let pixels = self.render_region(Some(sprites), y, y + 1, first..end, &mut history);
        self.scan_history = history;
        self.scan_x = end;
        self.rotation_read.line = None;
        self.beam_rendering = false;
        match pixels {
            Ok(pixels) => self.scanning[(y * size.0 + first) * 4..(y * size.0 + end) * 4]
                .copy_from_slice(&pixels),
            Err(error) => self.scan_error = Some(error),
        }
    }
    pub(crate) fn finish_field(&mut self) {
        self.completed_size = self.scan_size;
        self.completed = Some(match self.scan_error.take() {
            Some(error) => Err(error),
            None => Ok(self.scanning.clone()),
        });
    }
    pub fn output_dimensions(&self) -> (usize, usize) {
        if self.completed.is_some() {
            self.completed_size
        } else {
            self.dimensions()
        }
    }
    /// ST-058 §2.4 tables2.1: double density doubles source lines; single
    /// density repeats the same image in alternating fields.
    pub fn dimensions(&self) -> (usize, usize) {
        let mode = self.word(0);
        let width = [320, 352, 640, 704][(mode & 3) as usize];
        let height = if mode & 4 != 0 {
            480
        } else {
            [224, 240, 256, 256][(mode >> 4 & 3) as usize] * if mode & 0xC0 == 0xC0 { 2 } else { 1 }
        };
        (width, height)
    }
    fn validate(&self) -> Result<(), String> {
        let mode = self.word(0);
        let unsupported = mode & !0x81F7 != 0
            || mode & 0xC0 == 0x40
            || (mode & 4 != 0 && mode & 0xC0 != 0)
            || (mode & 4 == 0 && mode & 0x30 == 0x30)
            || (mode & 4 == 0 && !self.pal && mode & 0x30 == 0x20)
            || self.word(0x20) & !0x1F3F != 0
            || self.word(0xE) & !0xB3FF != 0
            || self.word(0xE) >> 12 & 3 == 3
            || (self.word(0xE) & 0x8000 != 0 && self.word(0xE) & 0x3000 != 0x1000)
            || self.word(0xEC) & !0xF77F != 0;
        if unsupported {
            return Err("Unsupported VDP2 display, banking, layer or effects configuration".into());
        }
        if self.word(0xEC) & 0x8000 != 0
            && (self.word(0xE) & 0x3000 != 0 || self.gradation_screen().is_none())
        {
            return Err("Reserved VDP2 gradation screen or color RAM mode".into());
        }
        for layer in 0..4 {
            if self.word(0x20) & 0x20 != 0 || self.word(0x20) & (1 << layer) == 0 {
                continue;
            }
            let (ctl, format) = self.character_control(layer);
            if layer < 2 && self.word(0x98) >> (layer * 8) & 3 == 2 {
                return Err("Reserved VDP2 reduction display setting".into());
            }
            if mode & 6 != 0 && layer == 0 && format == 4 {
                return Err("VDP2 RGB888 NBG0 requires normal display mode".into());
            }
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
            self.validate_special(layer, format)?;
            if !(layer < 2 && ctl & 2 != 0) && self.word(0x3A) >> (layer * 2) & 3 == 2 {
                return Err("Reserved VDP2 plane size".into());
            }
        }
        self.validate_rotation()?;
        Ok(())
    }
    fn validate_special(&self, id: usize, format: u16) -> Result<(), String> {
        let priority = self.word(0xEA) >> (id * 2) & 3;
        let calculate = self.word(0xEE) >> (id * 2) & 3;
        if priority == 3 || (format >= 3 && (priority == 2 || calculate == 2)) {
            return Err(
                "Reserved VDP2 special priority/color calculation mode for this format".into(),
            );
        }
        Ok(())
    }
    /// ST-058 §§10.3/11.2/12.3: special flags are properties of the sampled
    /// character/bitmap. Dot codes use waveform low bits before palette offsets.
    fn scroll_layer(&self, id: usize, format: u16, pixel: Pixel, flags: u16) -> Option<Layer> {
        let priority_register = if id == 4 { 0xFC } else { 0xF8 + id / 2 * 2 };
        let ratio_register = if id == 4 { 0x10C } else { 0x108 + id / 2 * 2 };
        let shift = if id == 4 { 0 } else { id % 2 * 8 };
        let mut priority = self.word(priority_register) >> shift & 7;
        let code_select = usize::from(self.word(0x24) & (1 << id) != 0) * 8;
        let dot = self.word(0x26) >> code_select & (1 << ((pixel.code & 15) >> 1)) != 0;
        match self.word(0xEA) >> (id * 2) & 3 {
            1 => priority = (priority & !1) | ((flags >> 1) & 1),
            2 => priority = (priority & !1) | u16::from(flags & 2 != 0 && dot),
            _ => (),
        }
        if priority == 0 {
            return None;
        }
        let special = match self.word(0xEE) >> (id * 2) & 3 {
            1 => flags & 1 != 0,
            2 => flags & 1 != 0 && dot,
            3 => format >= 3 || pixel.color_msb,
            _ => true,
        };
        Some(Layer {
            rgb: pixel.rgb,
            priority,
            ratio: self.word(ratio_register) >> shift & 31,
            calculate: self.word(0xEC) & (1 << id) != 0 && special,
            id,
            coefficient_color: None,
            palette: format < 3,
        })
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
    fn window(&self, control: u16, x: usize, y: usize, sprite: bool) -> bool {
        let mut any = false;
        let and = control & 0x80 != 0;
        let mut result = and;
        for i in 0..2 {
            if control & (2 << (i * 2)) == 0 {
                continue;
            }
            any = true;
            let base = 0xC0 + i * 8;
            let (left, right) = if self.word(0xD8 + i * 4) & 0x8000 != 0 {
                self.window_data(i, y)
            } else {
                (self.word(base), self.word(base + 4))
            };
            // ST-058 table8.1: exclusive normal coordinates use H8..H0;
            // exclusive high resolution omits H0 and measures pairs of dots.
            let exclusive = self.word(0) & 4 != 0;
            let mask = if exclusive { 0x1FF } else { 0x3FF };
            let shift = u32::from(!exclusive && self.word(0) & 2 == 0);
            let window_x = if exclusive && self.word(0) & 2 != 0 {
                x / 2
            } else {
                x
            };
            let inside = window_x >= usize::from((left & mask) >> shift)
                && window_x <= usize::from((right & mask) >> shift)
                && y >= usize::from(self.word(base + 2))
                && y <= usize::from(self.word(base + 6));
            let valid = inside != (control & (1 << (i * 2)) != 0);
            if and {
                result &= valid;
            } else {
                result |= valid;
            }
        }
        if control & 0x20 != 0 {
            any = true;
            let valid = sprite != (control & 0x10 != 0);
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
    /// ST-058 §5.3, figs.5.4–5.8: packed enabled fields, relative X/Y,
    /// held X/zoom over the programmed line interval, and Y advancement only
    /// since the last table read. Vertical-cell entries are interleaved when both
    /// normal screens use them; mosaic takes precedence over cell scrolling.
    fn normal_coordinates(&self, layer: usize, x: usize, y: usize) -> Option<(usize, usize)> {
        let scroll = 0x70 + layer * 0x10;
        let control = self.word(0x9A) >> (layer * 8) & 0x3F;
        let interval = 1usize << (control >> 4);
        let fields = (control & 14).count_ones() as usize;
        let mut at = self.table_address(0xA0 + layer * 4) + y / interval * fields * 4;
        let mut sx = u64::from(self.long(scroll) & 0x7FFFF00);
        let mut sy = u64::from(self.long(scroll + 4) & 0x7FFFF00);
        let mut dx = self.long(scroll + 8) & 0x7FF00;
        if control & 2 != 0 {
            sx += u64::from(self.scroll_data(layer, 0, at)? & 0x7FFFF00);
            at += 4;
        }
        let dy = u64::from(self.long(scroll + 12) & 0x7FF00);
        if control & 4 != 0 {
            sy +=
                u64::from(self.scroll_data(layer, 1, at)? & 0x7FFFF00) + (y % interval) as u64 * dy;
            at += 4;
        } else {
            sy += y as u64 * dy;
        }
        if control & 8 != 0 {
            dx = self.scroll_data(layer, 2, at)? & 0x7FF00;
        }
        if control & 1 != 0 && self.word(0x22) & (1 << layer) == 0 {
            let both = self.word(0x9A) & 0x101 == 0x101;
            let cell_size = if self.word(0x28) >> (layer * 8) & 3 == 1 {
                16
            } else {
                8
            };
            let entry = x / cell_size * if both { 2 } else { 1 } + if both { layer } else { 0 };
            let address = self.table_address(0x9C) + entry * 4;
            if !self.cell_access(layer, address) {
                return None;
            }
            self.fetch_group((x / cell_size, 0));
            let value = u32::from(self.fetch_word(layer, 2, address)?) << 16
                | u32::from(self.fetch_word(layer, 2, address + 2)?);
            sy += u64::from(value & 0x7FFFF00);
        }
        Some((
            ((sx + x as u64 * u64::from(dx)) >> 16) as usize,
            (sy >> 16) as usize,
        ))
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
    ) -> Option<(usize, usize, usize, u16)> {
        self.tile_map(layer, ctl, format, x, y, None, false)
    }
    #[allow(clippy::too_many_arguments)]
    fn tile_map(
        &self,
        layer: usize,
        ctl: u16,
        format: u16,
        x: usize,
        y: usize,
        parameter: Option<usize>,
        over: bool,
    ) -> Option<(usize, usize, usize, u16)> {
        let size = if ctl & 1 == 0 { 8 } else { 16 };
        let pnc = self.word(0x30 + layer * 2);
        let one = pnc & 0x8000 != 0;
        let name_bytes = if one { 2 } else { 4 };
        let columns = 512 / size;
        let page_bytes = columns * columns * name_bytes;
        let plane_size = self.word(0x3A) >> parameter.map_or(layer * 2, |p| 8 + p * 4) & 3;
        let pages_x = if plane_size == 0 { 1 } else { 2 };
        let pages_y = if plane_size == 3 { 2 } else { 1 };
        let plane_x = 512 * pages_x;
        let plane_y = 512 * pages_y;
        let mask = if parameter.is_some() { 3 } else { 1 };
        let plane = ((x / plane_x) & mask) | (((y / plane_y) & mask) * (mask + 1));
        let map_register = parameter.map_or(0x40 + layer * 4, |p| 0x50 + p * 16);
        let map = self.word(map_register + (plane / 2) * 2) >> ((plane & 1) * 8) & 63;
        let bank = parameter.map_or_else(
            || self.word(0x3C) >> (layer * 4),
            |p| self.word(0x3E) >> (p * 4),
        ) & 7;
        let selection = usize::from((bank << 6) | map);
        let base = ((selection * page_bytes) & 0x7FFFF) & !(page_bytes * pages_x * pages_y - 1);
        let px = x % plane_x;
        let py = y % plane_y;
        let page = py / 512 * pages_x + px / 512;
        let name_index = (py % 512 / size) * columns + px % 512 / size;
        let at = base + page * page_bytes + name_index * name_bytes;
        // ST-058 §6.2: RBG1 names are read from B1; its characters from B0.
        // An address outside the selected bank cannot fetch display data.
        if !over && !self.name_access(layer, at, parameter.is_some()) {
            return None;
        }
        let name = if over {
            self.word(0xB8 + parameter.unwrap() * 2)
        } else {
            self.fetch_word(layer, 0, at)?
        };
        let (character, palette, flip) = if one || over {
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
                usize::from(self.fetch_word(layer, 0, at + 2)? & 0x7FFF),
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
        let base = (character * 32 + cell * cell_bytes) & 0x7FFFF;
        if !self.character_access(
            layer,
            base,
            format,
            (!over).then_some(at),
            parameter.is_some(),
        ) {
            return None;
        }
        let special = if one || over {
            pnc >> 8 & 3
        } else {
            name >> 12 & 3
        };
        Some((base, (cy & 7) * 8 + (cx & 7), palette, special))
    }
    fn background(&self, layer: usize, x: usize, y: usize, sprite_window: bool) -> Option<Layer> {
        if self.word(0x20) & 0x20 != 0
            || self.word(0x20) & (1 << layer) == 0
            || !self.normal_layer_available(layer)
            || !self.window(
                self.word(0xD0 + (layer / 2) * 2) >> ((layer & 1) * 8),
                x,
                y,
                sprite_window,
            )
        {
            return None;
        }
        let (base, pixel, palette, special, format) = self.normal_sample(layer, x, y)?;
        let opaque = self.word(0x20) & (0x100 << layer) != 0;
        let offset = usize::from(self.word(0xE4) >> (layer * 4) & 7) * 256;
        let pixel = self.color(layer, format, base, pixel, offset, palette, opaque)?;
        self.scroll_layer(layer, format, pixel, special)
    }
    // Address generation is shared by acquisition and scanout. It does not
    // depend on windows, priority, palette contents or framebuffer requests.
    fn normal_sample(
        &self,
        layer: usize,
        mut x: usize,
        mut y: usize,
    ) -> Option<fetch::NormalSample> {
        if let Some(sample) = self.scheduled_sample(layer) {
            return sample;
        }
        // ST-058 §2.1 p.13: exclusive high resolution alternates the two
        // normal screens. Guest increments of two and NBG1's +1 scroll
        // produce adjacent source dots, not two full-width overlaid layers.
        if self.word(0) & 6 == 6 {
            if layer != x & 1 {
                return None;
            }
            x /= 2;
        }
        if self.word(0x22) & (1 << layer) != 0 {
            let mx = usize::from(self.word(0x22) >> 8 & 15) + 1;
            let my = usize::from(self.word(0x22) >> 12) + 1;
            x -= x % mx;
            y -= y % my;
        }
        let (ctl, format) = self.character_control(layer);
        let (sx, sy) = if layer < 2 {
            self.normal_coordinates(layer, x, y)?
        } else {
            (
                x + usize::from(self.word(0x90 + (layer - 2) * 4) & 0x7FF),
                y + usize::from(self.word(0x92 + (layer - 2) * 4) & 0x7FF),
            )
        };
        self.fetch_group((sx / 8, sy));
        let (base, pixel, palette, special) = if layer < 2 && ctl & 2 != 0 {
            let width = if ctl & 8 == 0 { 512 } else { 1024 };
            let height = if ctl & 4 == 0 { 256 } else { 512 };
            (
                usize::from(self.word(0x3C) >> (layer * 4) & 7) * 0x20000,
                (sy % height) * width + sx % width,
                usize::from(self.word(0x2C) >> (layer * 8) & 7) * 256,
                self.word(0x2C) >> (layer * 8 + 4) & 3,
            )
        } else {
            self.tile(layer, ctl, format, sx, sy)?
        };
        if layer < 2 && ctl & 2 != 0 {
            let address = Self::dot_address(base, pixel, format);
            if !self.character_access(layer, address, format, None, false) {
                return None;
            }
        }
        Some((base, pixel, palette, special, format))
    }
    /// ST-058 §4.5 p.60: the color decoder shares resources across layers.
    /// Raising NBG0/1's color depth disables the documented companion layer,
    /// regardless of its priority or whether that first layer is transparent.
    fn normal_layer_available(&self, layer: usize) -> bool {
        let n0 = self.character_control(0).1;
        let n1 = self.character_control(1).1;
        match layer {
            1 => n0 != 4,
            2 => n0 < 2,
            3 => n0 != 4 && n1 < 2,
            _ => true,
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn color(
        &self,
        layer: usize,
        format: u16,
        base: usize,
        pixel: usize,
        offset: usize,
        palette: usize,
        opaque: bool,
    ) -> Option<Pixel> {
        let (rgb, code, color_msb) = match format {
            0 => {
                let byte = self.fetch_byte(layer, base + pixel / 2)?;
                let index = usize::from(if pixel & 1 == 0 { byte >> 4 } else { byte & 15 });
                if index == 0 && !opaque {
                    return None;
                }
                let address = offset + palette + index;
                (
                    self.palette(address),
                    index as u16,
                    self.palette_msb(address),
                )
            }
            1 => {
                let index = usize::from(self.fetch_byte(layer, base + pixel)?);
                if index == 0 && !opaque {
                    return None;
                }
                let address = offset + (palette & 0x700) + index;
                (
                    self.palette(address),
                    index as u16,
                    self.palette_msb(address),
                )
            }
            2 => {
                let index = usize::from(self.fetch_word(layer, 1, base + pixel * 2)? & 2047);
                if index == 0 && !opaque {
                    return None;
                }
                (
                    self.palette(offset + index),
                    index as u16,
                    self.palette_msb(offset + index),
                )
            }
            3 => {
                let color = self.fetch_word(layer, 1, base + pixel * 2)?;
                if color & 0x8000 == 0 && !opaque {
                    return None;
                }
                (rgb555(color), 0, true)
            }
            _ => {
                let a = self.fetch_word(layer, 1, base + pixel * 4)?;
                let b = self.fetch_word(layer, 1, base + pixel * 4 + 2)?;
                if a & 0x8000 == 0 && !opaque {
                    return None;
                }
                ([b as u8, (b >> 8) as u8, a as u8], 0, true)
            }
        };
        Some(Pixel {
            rgb,
            code,
            color_msb,
        })
    }
    fn dot_address(base: usize, pixel: usize, format: u16) -> usize {
        base + match format {
            0 => pixel / 2,
            1 => pixel,
            2 | 3 => pixel * 2,
            _ => pixel * 4,
        }
    }
    fn render(&self, sprites: Option<&Vdp1>) -> Result<Vec<u8>, String> {
        self.render_lines(sprites, 0, self.dimensions().1)
    }
    fn render_lines(
        &self,
        sprites: Option<&Vdp1>,
        first: usize,
        end: usize,
    ) -> Result<Vec<u8>, String> {
        self.render_region(
            sprites,
            first,
            end,
            0..self.dimensions().0,
            &mut [[0; 3]; 2],
        )
    }
    fn render_region(
        &self,
        sprites: Option<&Vdp1>,
        first: usize,
        end: usize,
        columns: std::ops::Range<usize>,
        history: &mut [[u8; 3]; 2],
    ) -> Result<Vec<u8>, String> {
        let (_, height) = self.dimensions();
        let width = columns.len();
        let mut pixels = vec![0; width * (end - first) * 4];
        if self.word(0) & 0x8000 == 0 && self.word(0) & 0x100 == 0 {
            *history = if width == 1 {
                [history[1], [0; 3]]
            } else {
                [[0; 3]; 2]
            };
            for p in pixels.as_chunks_mut::<4>().0 {
                p[3] = 255;
            }
            return Ok(pixels);
        }
        self.validate()?;
        let gradation = self.gradation_screen();
        for y in first..end {
            // The manuals do not define the two pre-display pipeline samples.
            // Use black at the left border, reset every scanline; never carry
            // pixels from the previous line or the previous rendered frame.
            if columns.start == 0 {
                *history = [[0u8; 3]; 2];
            }
            let back = if self.beam_rendering {
                self.line_tables.back
            } else {
                self.vword(self.line_address(
                    0xAC,
                    if self.word(0) & 0x8000 == 0 {
                        height - 1
                    } else {
                        y
                    },
                ))
            };
            for x in columns.clone() {
                self.display_fetch.borrow_mut().pixel = x;
                if self.word(0) & 0x8000 == 0 {
                    pixels[((y - first) * width + x - columns.start) * 4..][..4]
                        .copy_from_slice(&[rgb555(back)[0], rgb555(back)[1], rgb555(back)[2], 255]);
                    continue;
                }
                let sprite_window = self.sprite_window(sprites, x, y);
                let sprite = sprites.map(|s| self.sprite(s, x, y)).unwrap_or_default();
                let back = Layer {
                    rgb: rgb555(back),
                    id: 5,
                    ratio: self.word(0x10E) >> 8 & 31,
                    ..Layer::default()
                };
                let mut images = [back; 4];
                let mut gradation_rgb = [0; 3];
                for mut layer in [
                    self.background(3, x, y, sprite_window),
                    self.background(2, x, y, sprite_window),
                    self.background(1, x, y, sprite_window),
                    self.background(0, x, y, sprite_window),
                    self.rotation1(x, y, sprite_window),
                    self.rotation(x, y, sprite_window),
                    sprite.layer,
                ]
                .into_iter()
                .flatten()
                {
                    if Some(layer.id) == gradation {
                        gradation_rgb = layer.rgb;
                    }
                    for image in &mut images {
                        if layer.priority >= image.priority {
                            std::mem::swap(image, &mut layer);
                        }
                    }
                }
                let [top, mut second, mut third, mut fourth] = images;
                // ST-058 §12.2/fig.12.4: the selected screen's horizontal
                // samples are collected even when it is hidden by other
                // screens. Only a top/second selected image replaces second;
                // top, its enable, ratio and offset selection remain intact.
                if gradation.is_some_and(|id| top.id == id || second.id == id) {
                    second.rgb = std::array::from_fn(|ch| {
                        ((u16::from(history[0][ch])
                            + u16::from(history[1][ch])
                            + 2 * u16::from(gradation_rgb[ch]))
                            >> 2) as u8
                    });
                    if Some(top.id) == gradation {
                        second.ratio = top.ratio;
                    }
                }
                *history = [history[1], gradation_rgb];
                let mut rgb = top.rgb;
                // ST-058 §§7.1/11.3: LNCL is inserted immediately below
                // an enabled top screen. It never appears by itself or takes
                // part in priority selection. Sprite enable uses bit 5, while
                // the compositor reserves id 5 for the back screen.
                let line_bit = if top.id == 6 { 5 } else { top.id };
                let inserted =
                    gradation.is_none() && top.id != 5 && self.word(0xE8) & (1 << line_bit) != 0;
                if inserted {
                    let mut index = if self.beam_rendering {
                        self.line_tables.color
                    } else {
                        self.vword(self.line_address(0xA8, y))
                    } & 0x7FF;
                    if let Some(low) = top.coefficient_color {
                        index = (index & 0x780) | low;
                    }
                    fourth = third;
                    third = second;
                    second = Layer {
                        rgb: self.palette(usize::from(index)),
                        ratio: self.word(0x10E) & 31,
                        id: 7,
                        palette: true,
                        ..Layer::default()
                    };
                }
                if top.calculate && self.window(self.word(0xD6) >> 8, x, y, sprite_window) {
                    if gradation.is_none() && self.word(0xEC) & 0x400 != 0 {
                        second.rgb = self.extended_color(second, third, fourth, inserted);
                    }
                    // §12.1: CCRTMD chooses the ratio of the second image;
                    // calculation enable always belongs to the top image.
                    let ratio = if self.word(0xEC) & 0x200 != 0 {
                        second.ratio
                    } else {
                        top.ratio
                    };
                    for (ch, value) in rgb.iter_mut().enumerate() {
                        *value = if self.word(0xEC) & 0x100 != 0 {
                            u16::from(*value)
                                .saturating_add(u16::from(second.rgb[ch]))
                                .min(255) as u8
                        } else {
                            ((u16::from(*value) * (31 - ratio)
                                + u16::from(second.rgb[ch]) * (ratio + 1))
                                >> 5) as u8
                        };
                    }
                }
                let p = &mut pixels[((y - first) * width + x - columns.start) * 4..][..4];
                // ST-058 §13.1: offset follows color calculation and uses
                // the top screen's enable/select, never the lower screen's.
                rgb = self.offset(rgb, top.id);
                if (sprite.shade_sprite && top.id == 6)
                    || (sprite.shadow_priority.is_some_and(|p| p >= top.priority)
                        && self.word(0xE2) & (1 << top.id) != 0)
                {
                    rgb = rgb.map(|v| v >> 1);
                }
                p[..3].copy_from_slice(&rgb);
                p[3] = 255;
            }
        }
        Ok(pixels)
    }

    /// ST-058 §12.2 CCCTL BOKEN/BOKN; RBG1 shares NBG0's screen ID.
    fn gradation_screen(&self) -> Option<usize> {
        if self.word(0xEC) & 0x8000 == 0 {
            return None;
        }
        match self.word(0xEC) >> 12 & 7 {
            0 => Some(6), // VDP1 sprite screen
            1 => Some(4), // RBG0
            2 => Some(0), // NBG0 / RBG1
            4 => Some(1), // NBG1 / EXBG
            5 => Some(2), // NBG2
            6 => Some(3), // NBG3
            _ => None,
        }
    }

    /// ST-058 §12.1 fig.12.3/table12.2: lower-screen enables are the
    /// unqualified CCCTL bits; SFCCMD conditions apply only to the top image.
    fn extended_color(
        &self,
        second: Layer,
        third: Layer,
        fourth: Layer,
        inserted: bool,
    ) -> [u8; 3] {
        let enabled = |layer: Layer| {
            layer.id != 5 && self.word(0xEC) & (1 << if layer.id == 7 { 5 } else { layer.id }) != 0
        };
        let mode0 = self.word(0xE) & 0x3000 == 0;
        let weights = if !enabled(second) || (!mode0 && third.palette) {
            [4, 0, 0]
        } else if inserted && enabled(third) && (mode0 || !fourth.palette) {
            [2, 1, 1]
        } else {
            [2, 2, 0]
        };
        // The 1997 Sega HTML §12.1/table12.2 corrects the older PDF's mode0
        // 2:1:0 entry to 2:1:1, also agreeing with fig.12.3's fourth input.
        std::array::from_fn(|ch| {
            ((u16::from(second.rgb[ch]) * weights[0]
                + u16::from(third.rgb[ch]) * weights[1]
                + u16::from(fourth.rgb[ch]) * weights[2])
                >> 2) as u8
        })
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn display_validation_covers_standard_tv_reserved_and_monitor_encodings() {
        let mut v = Vdp2::default();
        for (mode, pal, valid) in [
            (0x8000, false, true),
            (0x8012, false, true),
            (0x8023, true, true),
            (0x80A2, true, true),
            (0x80E3, true, true),
            (0x8004, false, true),
            (0x8007, false, true),
            (0x8020, false, false),
            (0x8030, true, false),
            (0x8040, false, false),
            (0x8084, false, false),
            (0x8200, false, false),
        ] {
            v.pal = pal;
            v.write16(VDP2, mode).unwrap();
            assert_eq!(v.framebuffer().is_ok(), valid, "{mode:X},pal={pal}");
        }
    }
    #[test]
    fn blank_border_uses_lowermost_back_color_and_hires_windows_keep_dot_precision() {
        let mut v = Vdp2::default();
        v.write16(VDP2 + 0xAC, 0x8003).unwrap();
        v.vram[0x60000..0x60002].copy_from_slice(&0x801fu16.to_be_bytes());
        v.vram[0x601BE..0x601C0].copy_from_slice(&0xfc00u16.to_be_bytes()); // line223
        v.write16(VDP2, 0x8100).unwrap();
        assert_eq!(&v.framebuffer().unwrap()[..4], [255, 0, 0, 255]);
        v.write16(VDP2, 0x100).unwrap();
        assert!(
            v.framebuffer()
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 255, 255])
        );
        v.write16(VDP2, 0).unwrap();
        assert!(
            v.framebuffer()
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 0, 0, 255])
        );
        for (mode, left, right) in [
            (0x8000, 2, 4),
            (0x8002, 4, 8),
            (0x8004, 4, 8),
            (0x8005, 4, 8),
            (0x8006, 8, 17),
            (0x8007, 8, 17),
        ] {
            v.write16(VDP2, mode).unwrap();
            v.write16(VDP2 + 0xC0, 4).unwrap();
            v.write16(VDP2 + 0xC4, 8).unwrap();
            v.write16(VDP2 + 0xC6, 223).unwrap();
            assert!(!v.window(2, left - 1, 0, false));
            assert!(v.window(2, left, 0, false));
            assert!(v.window(2, right, 0, false));
            assert!(!v.window(2, right + 1, 0, false));
        }
    }
}
