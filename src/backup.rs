//! Internal battery-backed RAM, separate from volatile machine reset state.
//!
//! SCU User's Manual ST-097 §1.2, figs. 1.3/1.5 defines the 64 KiB window;
//! SMPC User's Manual ST-169 §2.3 (SYSRES) specifies 256 Kbit retention.
//! Only odd-address byte accesses are supported here. Unconnected byte lanes,
//! wider accesses and undocumented aliases remain explicit unsupported accesses.
use crate::bus::BusError;

pub const BACKUP_RAM: u32 = 0x0018_0000;
pub const BACKUP_SIZE: usize = 32 * 1024;
pub const BACKUP_WINDOW: u32 = 64 * 1024;

pub struct BackupRam {
    bytes: Vec<u8>,
}

impl Default for BackupRam {
    fn default() -> Self {
        // Unformatted storage; do not synthesize a Sega backup filesystem.
        Self {
            bytes: vec![0; BACKUP_SIZE],
        }
    }
}

impl BackupRam {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Packed physical bytes in ascending odd-address order, without a header.
    /// Validate before changing any byte so a rejected import is atomic.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        if bytes.len() != BACKUP_SIZE {
            return Err(BusError::BackupLength {
                actual: bytes.len(),
            });
        }
        self.bytes.copy_from_slice(bytes);
        Ok(())
    }

    fn offset(address: u32) -> Result<usize, BusError> {
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_WINDOW).contains(&address) && address & 1 != 0 {
            Ok(((address - BACKUP_RAM) / 2) as usize)
        } else {
            Err(BusError::UnsupportedDevice { address, width: 1 })
        }
    }

    pub fn read8(&self, address: u32) -> Result<u8, BusError> {
        Ok(self.bytes[Self::offset(address)?])
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.bytes[Self::offset(address)?] = value;
        Ok(())
    }
}
