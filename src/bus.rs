//! Checked partial Saturn map; devices and cache-through aliases are documented separately.
use crate::backup::{BACKUP_RAM, BACKUP_WINDOW, BackupRam};
use crate::devices::{SMPC, Smpc, VDP2, VRAM, Vdp2};
use crate::onchip::OnChip;
use std::fmt;

pub const BIOS_SIZE: usize = 512 * 1024;
pub const WRAM_SIZE: usize = 1024 * 1024;
pub const LOW_WRAM: u32 = 0x0020_0000;
pub const HIGH_WRAM: u32 = 0x0600_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Processor {
    Master = 0,
    Slave = 1,
}

/// A CPU-local view of shared external memory and independent on-chip devices.
pub struct CpuBus<'a> {
    bus: &'a mut Bus,
    processor: Processor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    Unaligned { address: u32, width: usize },
    Unmapped { address: u32, width: usize },
    ReadOnly { address: u32, width: usize },
    BiosLength { actual: usize },
    BackupLength { actual: usize },
    UnsupportedDevice { address: u32, width: usize },
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
            Self::UnsupportedDevice { address, width } => write!(
                f,
                "Unsupported Saturn device/register: {width}-byte access at 0x{address:08X}; this build supports only the documented homebrew hardware subset"
            ),
            Self::BiosLength { actual } => write!(
                f,
                "Select a raw 512 KiB BIOS ({BIOS_SIZE} bytes); received {actual} bytes"
            ),
            Self::BackupLength { actual } => write!(
                f,
                "Select a packed 32 KiB backup RAM file (32768 bytes); received {actual} bytes"
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
    pub(crate) vdp2: Vdp2,
    smpc: Smpc,
    pub(crate) backup: BackupRam,
    onchip: [OnChip; 2],
    pub(crate) slave_enabled: bool,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: None,
            low: vec![0; WRAM_SIZE],
            high: vec![0; WRAM_SIZE],
            vdp2: Vdp2::default(),
            smpc: Smpc::default(),
            backup: BackupRam::default(),
            onchip: std::array::from_fn(|_| OnChip::default()),
            slave_enabled: false,
        }
    }
}

impl Bus {
    pub fn for_cpu(&mut self, processor: Processor) -> CpuBus<'_> {
        CpuBus {
            bus: self,
            processor,
        }
    }

    pub(crate) fn interrupt(&self, processor: Processor) -> Option<(u8, u8)> {
        self.onchip[processor as usize].interrupt()
    }

    pub(crate) fn can_wake_master(&self) -> bool {
        self.slave_enabled || self.smpc.busy() || self.onchip[0].can_request_interrupt()
    }

    pub(crate) fn set_slave_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.onchip[1] = OnChip::default();
        }
        self.slave_enabled = enabled;
    }

    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Option<u8> {
        self.onchip[0].advance_clocks(clocks);
        if self.slave_enabled {
            self.onchip[1].advance_clocks(clocks);
        }
        self.smpc.advance_clocks(clocks)
    }
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

    pub fn set_buttons(&mut self, buttons: u16) {
        self.smpc.set_buttons(buttons);
    }

    pub fn set_port2_buttons(&mut self, buttons: u16) {
        self.smpc.set_port2_buttons(buttons);
    }

    /// Reset volatile hardware while retaining battery-backed bytes, including
    /// when a subsequent boot image is rejected.
    pub(crate) fn reset_volatile(&mut self) {
        let backup = std::mem::take(&mut self.backup);
        *self = Self {
            backup,
            ..Self::default()
        };
    }

    // SH7604 §2.3: P1 cache-through accesses share the external physical bus.
    fn physical(address: u32) -> u32 {
        if (0x2000_0000..0x4000_0000).contains(&address) {
            address - 0x2000_0000
        } else {
            address
        }
    }

    fn region(address: u32, width: usize) -> Result<(u8, usize), BusError> {
        if !address.is_multiple_of(width as u32) {
            return Err(BusError::Unaligned { address, width });
        }
        for (id, base, size) in [
            (0, 0, BIOS_SIZE),
            (1, LOW_WRAM, WRAM_SIZE),
            (2, HIGH_WRAM, WRAM_SIZE),
            (3, VRAM, 512 * 1024),
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
        let address = Self::physical(address);
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_WINDOW).contains(&address) {
            if N != 1 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            bytes[0] = self.backup.read8(address)?;
            return Ok(bytes);
        }
        if (SMPC..SMPC + 0x80).contains(&address) {
            if N != 1 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            bytes[0] = self.smpc.read(address)?;
            return Ok(bytes);
        }
        if (VDP2..VDP2 + 0x120).contains(&address) {
            if N != 2 && N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            for (i, pair) in bytes.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                pair.copy_from_slice(&self.vdp2.read16(address + i as u32 * 2)?.to_be_bytes());
            }
            return Ok(bytes);
        }
        let (id, offset) = Self::region(address, N)?;
        let memory = match id {
            0 => self
                .bios
                .as_ref()
                .ok_or(BusError::Unmapped { address, width: N })?,
            1 => &self.low,
            2 => &self.high,
            _ => &self.vdp2.vram,
        };
        let mut bytes = [0; N];
        bytes.copy_from_slice(&memory[offset..offset + N]);
        Ok(bytes)
    }

    fn write<const N: usize>(&mut self, address: u32, bytes: [u8; N]) -> Result<(), BusError> {
        let address = Self::physical(address);
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        // ST-202 §1.2: word strobes signal slave at 21000000 and master
        // at 21800000. Both cacheable and cache-through aliases share the wire.
        if matches!(address, 0x0100_0000 | 0x0180_0000) {
            if N != 2 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let target = usize::from(address == 0x0100_0000);
            if target == 0 || self.slave_enabled {
                self.onchip[target].capture_strobe();
            }
            return Ok(());
        }
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_WINDOW).contains(&address) {
            if N != 1 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            return self.backup.write8(address, bytes[0]);
        }
        if (SMPC..SMPC + 0x80).contains(&address) {
            if N != 1 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            return self.smpc.write(address, bytes[0]);
        }
        if (VDP2..VDP2 + 0x120).contains(&address) {
            if N != 2 && N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            // Validate the entire access before mutating either halfword.
            for i in (0..N).step_by(2) {
                let target = address + i as u32;
                if target == VDP2 + 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                self.vdp2.read16(target)?;
            }
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                self.vdp2.write16(
                    address + i as u32 * 2,
                    u16::from_be_bytes([pair[0], pair[1]]),
                )?;
            }
            return Ok(());
        }
        let (id, offset) = Self::region(address, N)?;
        let memory = match id {
            0 if self.bios.is_some() => return Err(BusError::ReadOnly { address, width: N }),
            0 => return Err(BusError::Unmapped { address, width: N }),
            1 => &mut self.low,
            2 => &mut self.high,
            _ => &mut self.vdp2.vram,
        };
        memory[offset..offset + N].copy_from_slice(&bytes);
        Ok(())
    }
}

impl Memory for Bus {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].read8(address);
        }
        Ok(self.read::<1>(address)?[0])
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].read16(address);
        }
        Ok(u16::from_be_bytes(self.read(address)?))
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        Ok(u32::from_be_bytes(self.read(address)?))
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].write8(address, value);
        }
        self.write(address, [value])
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].write16(address, value);
        }
        self.write(address, value.to_be_bytes())
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.write(address, value.to_be_bytes())
    }
}

impl Memory for CpuBus<'_> {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address >= 0xFFFF_FE00 {
            self.bus.onchip[self.processor as usize].read8(address)
        } else {
            self.bus.read8(address)
        }
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address >= 0xFFFF_FE00 {
            self.bus.onchip[self.processor as usize].read16(address)
        } else {
            self.bus.read16(address)
        }
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        self.bus.read32(address)
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            self.bus.onchip[self.processor as usize].write8(address, value)
        } else {
            self.bus.write8(address, value)
        }
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            self.bus.onchip[self.processor as usize].write16(address, value)
        } else {
            self.bus.write16(address, value)
        }
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.bus.write32(address, value)
    }
}
