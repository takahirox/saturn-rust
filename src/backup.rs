//! Internal battery-backed RAM, separate from volatile machine reset state.
//!
//! SCU User's Manual ST-097 §1.2, figs. 1.3/1.5 defines the 64 KiB window;
//! SMPC User's Manual ST-169 §2.3 (SYSRES) specifies 256 Kbit retention.
//! D7–D0 are connected on odd byte lanes. Even lanes read pulled-up ones and
//! writes have no strobe. Bus word/long accesses decompose into these lanes.
use crate::bus::BusError;

pub const BACKUP_RAM: u32 = 0x0018_0000;
pub const BACKUP_SIZE: usize = 32 * 1024;
pub const BACKUP_WINDOW: u32 = 64 * 1024;
/// The CPU-board select spans 00180000–001FFFFF. Only A15–A1 reach
/// the 32 KiB chip; the remaining addresses select the same byte lanes.
pub const BACKUP_SELECT: u32 = 512 * 1024;

pub struct BackupRam {
    bytes: Vec<u8>,
    revision: u32,
}

impl Default for BackupRam {
    fn default() -> Self {
        // Unformatted storage; do not synthesize a Sega backup filesystem.
        Self::new(BACKUP_SIZE)
    }
}

impl BackupRam {
    pub(crate) fn new(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
            revision: 0,
        }
    }

    pub(crate) fn revision(&self) -> u32 {
        self.revision
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Packed physical bytes in ascending odd-address order, without a header.
    /// Validate before changing any byte so a rejected import is atomic.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        if bytes.len() != self.bytes.len() {
            return Err(if self.bytes.len() == BACKUP_SIZE {
                BusError::BackupLength {
                    actual: bytes.len(),
                }
            } else {
                BusError::CartridgeBackupLength {
                    actual: bytes.len(),
                }
            });
        }
        if self.bytes != bytes {
            self.bytes.copy_from_slice(bytes);
            self.revision = self.revision.wrapping_add(1);
        }
        Ok(())
    }

    pub(crate) fn write_packed(&mut self, offset: usize, value: u8) {
        if self.bytes[offset] != value {
            self.bytes[offset] = value;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn offset(address: u32) -> Result<usize, BusError> {
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_SELECT).contains(&address) {
            Ok((((address - BACKUP_RAM) & (BACKUP_WINDOW - 1)) / 2) as usize)
        } else {
            Err(BusError::UnsupportedDevice { address, width: 1 })
        }
    }

    pub fn read8(&self, address: u32) -> Result<u8, BusError> {
        let offset = Self::offset(address)?;
        Ok(if address & 1 == 0 {
            255
        } else {
            self.bytes[offset]
        })
    }

    pub fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        let offset = Self::offset(address)?;
        if address & 1 != 0 {
            self.write_packed(offset, value);
        }
        Ok(())
    }
}
