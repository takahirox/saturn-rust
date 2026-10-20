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
    buttons: [u16; 2],
    pdr: [u8; 2],
    ddr: [u8; 2],
    iosel: u8,
    exle: u8,
    sf: bool,
    command: u8,
    pending: Option<(u8, u32)>,
}

impl Smpc {
    // ST-169 §2.3 pp.23/24/32: minimum 30 µs at the fixed NTSC 320
    // reference clock (26.8741 MHz). Internal firmware/task jitter is absent.
    pub const CPU_COMMAND_CLOCKS: u32 = 807;

    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Option<u8> {
        let (command, remaining) = self.pending?;
        if clocks < remaining {
            self.pending = Some((command, remaining - clocks));
            return None;
        }
        self.pending = None;
        self.sf = false;
        Some(command)
    }

    pub fn set_buttons(&mut self, buttons: u16) {
        self.buttons[0] = buttons & 0x1FFF;
    }

    pub fn set_port2_buttons(&mut self, buttons: u16) {
        self.buttons[1] = buttons & 0x1FFF;
    }

    // ST-169, §1.2 pp.7–8 and §3.4 table 3.36 p.97. Standard digital pad only.
    pub fn read(&self, address: u32) -> Result<u8, BusError> {
        match address - SMPC {
            0x5F => Ok(self.command),
            0x63 => Ok(u8::from(self.sf)),
            offset @ (0x75 | 0x77) => {
                let port = ((offset - 0x75) / 2) as usize;
                let bit = 1 << port;
                if self.ddr[port] != 0x60 || self.iosel & bit == 0 || self.exle & bit != 0 {
                    return Err(BusError::UnsupportedDevice { address, width: 1 });
                }
                let nibble = match self.pdr[port] & 0x60 {
                    0x60 => 0x04 | u8::from(self.buttons[port] & (1 << 12) == 0) << 3,
                    0x20 => !self.buttons[port] as u8 & 15,
                    0x40 => !(self.buttons[port] >> 4) as u8 & 15,
                    _ => !(self.buttons[port] >> 8) as u8 & 15,
                };
                Ok((self.pdr[port] & 0x60) | 0x10 | nibble)
            }
            // DDR/IOSEL/EXLE are write-only in hardware. Do not invent readback.
            _ => Err(BusError::UnsupportedDevice { address, width: 1 }),
        }
    }

    pub fn write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        match address - SMPC {
            0x63 if value == 1 => self.sf = true,
            0x1F if matches!(value, 0x02 | 0x03 | 0x18) && self.sf && self.pending.is_none() => {
                self.command = value;
                self.pending = Some((value, Self::CPU_COMMAND_CLOCKS));
            }
            0x75 => self.pdr[0] = value & 0x7F,
            0x77 => self.pdr[1] = value & 0x7F,
            0x79 => self.ddr[0] = value & 0x7F,
            0x7B => self.ddr[1] = value & 0x7F,
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

#[cfg(test)]
mod smpc_command_tests {
    use super::*;

    #[test]
    fn cpu_control_commands_require_sf_latch_output_and_complete_after_807_clocks() {
        for command in [2, 3, 0x18] {
            let mut smpc = Smpc::default();
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 0);
            assert!(smpc.write(SMPC + 0x1F, command).is_err());
            smpc.write(SMPC + 0x63, 1).unwrap();
            smpc.write(SMPC + 0x1F, command).unwrap();
            assert_eq!(smpc.read(SMPC + 0x5F).unwrap(), command);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert!(smpc.write(SMPC + 0x1F, 2).is_err()); // No overwrite while busy.
            assert!(smpc.write(SMPC + 0x63, 0).is_err()); // Guest cannot clear SF.
            assert_eq!(smpc.advance_clocks(806), None);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert_eq!(smpc.advance_clocks(1), Some(command));
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 0);
            assert_eq!(smpc.advance_clocks(1000), None); // No duplicate completion.
        }
    }

    #[test]
    fn unsupported_commands_and_register_width_contract_are_explicit() {
        let mut smpc = Smpc::default();
        smpc.write(SMPC + 0x63, 1).unwrap();
        for command in [0, 6, 7, 8, 9, 0x0D, 0x0E, 0x0F, 0x10, 0x19, 0x1A, 0xFF] {
            assert!(smpc.write(SMPC + 0x1F, command).is_err());
            assert_eq!(smpc.read(SMPC + 0x5F).unwrap(), 0);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert_eq!(smpc.advance_clocks(1000), None);
        }
        assert!(smpc.read(SMPC + 0x1F).is_err()); // Write-only COMREG.
        assert!(smpc.write(SMPC + 0x5F, 0).is_err()); // Read-only OREG31.
    }
}
