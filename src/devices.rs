//! Original minimal VDP2 and SMPC devices; manual references: docs/architecture.md.
use crate::bus::BusError;

pub const VRAM: u32 = 0x05E0_0000;
pub const VDP2: u32 = 0x05F8_0000;
pub const SMPC: u32 = 0x0010_0000;
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 224;

pub use crate::smpc::Smpc;

pub struct Vdp2 {
    registers: [u8; 0x120],
    pub vram: Vec<u8>,
    pub vblank: bool,
    pub hblank: bool,
}

impl Default for Vdp2 {
    fn default() -> Self {
        Self {
            registers: [0; 0x120],
            vram: vec![0; 512 * 1024],
            vblank: false,
            hblank: false,
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
            // ST-058 §2.2: HBLANK bit 2, VBLANK bit 3.
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
        for command in [0, 8, 9, 0x0D, 0x0E, 0x0F, 0xFF] {
            assert!(smpc.write(SMPC + 0x1F, command).is_err());
            assert_eq!(smpc.read(SMPC + 0x5F).unwrap(), 0);
            assert_eq!(smpc.read(SMPC + 0x63).unwrap(), 1);
            assert_eq!(smpc.advance_clocks(1000), None);
        }
        assert!(smpc.read(SMPC + 0x1F).is_err()); // Write-only COMREG.
        assert!(smpc.write(SMPC + 0x5F, 0).is_err()); // Read-only OREG31.
    }
}
