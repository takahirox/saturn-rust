//! Deliberately partial Saturn physical memory map. No aliases or MMIO yet.
use std::fmt;

pub const BIOS_SIZE: usize = 512 * 1024;
pub const WRAM_SIZE: usize = 1024 * 1024;
pub const LOW_WRAM: u32 = 0x0020_0000;
pub const HIGH_WRAM: u32 = 0x0600_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    Unaligned { address: u32, width: usize },
    Unmapped { address: u32, width: usize },
    ReadOnly { address: u32, width: usize },
    BiosLength { actual: usize },
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unaligned { address, width } => {
                write!(f, "Unaligned {width}-byte access at 0x{address:08X}")
            }
            Self::Unmapped { address, width } => {
                write!(f, "Unmapped {width}-byte access at 0x{address:08X}")
            }
            Self::ReadOnly { address, width } => {
                write!(f, "Read-only BIOS: {width}-byte write at 0x{address:08X}")
            }
            Self::BiosLength { actual } => write!(
                f,
                "Select a raw 512 KiB BIOS ({BIOS_SIZE} bytes); received {actual} bytes"
            ),
        }
    }
}

impl std::error::Error for BusError {}

/// The interpreter fetches through this boundary; it owns no memory pointers.
pub trait Memory {
    fn read8(&self, address: u32) -> Result<u8, BusError>;
    fn read16(&self, address: u32) -> Result<u16, BusError>;
    fn read32(&self, address: u32) -> Result<u32, BusError>;
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError>;
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError>;
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError>;
}

pub struct Bus {
    bios: Option<Vec<u8>>,
    low: Vec<u8>,
    high: Vec<u8>,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: None,
            low: vec![0; WRAM_SIZE],
            high: vec![0; WRAM_SIZE],
        }
    }
}

impl Bus {
    /// Replaces ROM atomically. Machine reset separately discards old ROM first.
    pub fn load_bios(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        if bytes.len() != BIOS_SIZE {
            return Err(BusError::BiosLength {
                actual: bytes.len(),
            });
        }
        self.bios = Some(bytes.to_vec());
        Ok(())
    }

    fn region(address: u32, width: usize) -> Result<(u8, usize), BusError> {
        if !address.is_multiple_of(width as u32) {
            return Err(BusError::Unaligned { address, width });
        }
        for (id, base, size) in [
            (0, 0, BIOS_SIZE),
            (1, LOW_WRAM, WRAM_SIZE),
            (2, HIGH_WRAM, WRAM_SIZE),
        ] {
            if let Some(offset) = address.checked_sub(base)
                && (offset as usize) < size
                && width <= size - offset as usize
            {
                return Ok((id, offset as usize));
            }
        }
        Err(BusError::Unmapped { address, width })
    }

    fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        let (id, offset) = Self::region(address, N)?;
        let memory = match id {
            0 => self
                .bios
                .as_ref()
                .ok_or(BusError::Unmapped { address, width: N })?,
            1 => &self.low,
            _ => &self.high,
        };
        let mut bytes = [0; N];
        bytes.copy_from_slice(&memory[offset..offset + N]);
        Ok(bytes)
    }

    fn write<const N: usize>(&mut self, address: u32, bytes: [u8; N]) -> Result<(), BusError> {
        let (id, offset) = Self::region(address, N)?;
        let memory = match id {
            0 if self.bios.is_some() => return Err(BusError::ReadOnly { address, width: N }),
            0 => return Err(BusError::Unmapped { address, width: N }),
            1 => &mut self.low,
            _ => &mut self.high,
        };
        memory[offset..offset + N].copy_from_slice(&bytes);
        Ok(())
    }
}

impl Memory for Bus {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        Ok(self.read::<1>(address)?[0])
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        Ok(u16::from_be_bytes(self.read(address)?))
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        Ok(u32::from_be_bytes(self.read(address)?))
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.write(address, [value])
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        self.write(address, value.to_be_bytes())
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.write(address, value.to_be_bytes())
    }
}
