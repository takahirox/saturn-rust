//! Original minimal VDP2 and SMPC devices; manual references: docs/architecture.md.
use crate::bus::BusError;

pub const VRAM: u32 = 0x05E0_0000;
pub const VDP2: u32 = 0x05F8_0000;
pub const SMPC: u32 = 0x0010_0000;
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 224;

/// Host button mask: up, down, left, right, B, C, A, Start, Z, Y, X, R, L.
/// Only electrical input levels cross this boundary; no guest memory is written.
#[derive(Default)]
pub struct Smpc {
    buttons: u16,
    pdr: u8,
    ddr: u8,
    iosel: u8,
    exle: u8,
}

impl Smpc {
    pub fn set_buttons(&mut self, buttons: u16) {
        self.buttons = buttons & 0x1FFF;
    }

    // ST-169, §1.2 pp.7–8 and §3.4 table 3.36 p.97. Standard digital pad only.
    pub fn read(&self, address: u32) -> Result<u8, BusError> {
        match address - SMPC {
            0x75 if self.ddr == 0x60 && self.iosel & 1 != 0 && self.exle & 1 == 0 => {
                let nibble = match self.pdr & 0x60 {
                    0x60 => 0x04 | u8::from(self.buttons & (1 << 12) == 0) << 3,
                    0x20 => !self.buttons as u8 & 15,
                    0x40 => !(self.buttons >> 4) as u8 & 15,
                    _ => !(self.buttons >> 8) as u8 & 15,
                };
                Ok((self.pdr & 0x60) | 0x10 | nibble)
            }
            0x75 => Err(BusError::UnsupportedDevice { address, width: 1 }),
            // DDR/IOSEL/EXLE are write-only in hardware. Do not invent readback.
            _ => Err(BusError::UnsupportedDevice { address, width: 1 }),
        }
    }

    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        match address - SMPC {
            0x75 => self.pdr = value & 0x7F,
            0x79 => self.ddr = value & 0x7F,
            0x7D => self.iosel = value & 3,
            0x7F => self.exle = value & 3,
            _ => return Err(BusError::UnsupportedDevice { address, width: 1 }),
        }
        Ok(())
    }
}

pub struct Vdp2 {
    registers: [u8; 0x120],
    pub vram: Vec<u8>,
    pub vblank: bool,
}

impl Default for Vdp2 {
    fn default() -> Self {
        Self {
            registers: [0; 0x120],
            vram: vec![0; 512 * 1024],
            vblank: false,
        }
    }
}

impl Vdp2 {
    fn supported(offset: usize) -> bool {
        matches!(
            offset,
            0 | 4 | 0x10 | 0x12 | 0x20 | 0x28 | 0x3C | 0x78 | 0x7A | 0x7C | 0x7E | 0xF8
        )
    }

    fn word(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.registers[offset], self.registers[offset + 1]])
    }

    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        let offset = (address - VDP2) as usize;
        if offset == 4 {
            // ST-058 §2.2 screen status: VBLANK bit 3. No raster/HBlank model.
            return Ok(if self.vblank { 8 } else { 0 });
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
        let mut pixels = vec![0; WIDTH * HEIGHT * 4];
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        if self.word(0) & 0x8000 == 0 || self.word(0x20) & 1 == 0 {
            return Ok(pixels);
        }
        // ST-058 §§2,4–6,8,10: one non-scrolling NBG0 RGB555 bitmap.
        // All other display modes fail explicitly instead of becoming a fake game screen.
        if self.word(0) != 0x8000
            || self.word(0x20) != 0x0101
            || self.word(0x28) != 0x0032
            || self.word(0x3C) != 0
            || self.word(0x78) != 1
            || self.word(0x7A) != 0
            || self.word(0x7C) != 1
            || self.word(0x7E) != 0
            || self.word(0xF8) != 1
        {
            return Err("Unsupported VDP2 mode: requires 320×224 NBG0, 512×256 RGB555 bitmap at VRAM bank 0, no scroll, unit zoom, priority 1, transparency disabled".into());
        }
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let offset = (y * 512 + x) * 2;
                let color = u16::from_be_bytes([self.vram[offset], self.vram[offset + 1]]);
                let pixel = &mut pixels[(y * WIDTH + x) * 4..][..4];
                for (channel, out) in pixel[..3].iter_mut().enumerate() {
                    let c = ((color >> (channel * 5)) & 31) as u8;
                    *out = (c << 3) | (c >> 2);
                }
            }
        }
        Ok(pixels)
    }
}
