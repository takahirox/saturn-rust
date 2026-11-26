//! Saturn expansion slot (ST-097 §1.2 and Sega STN-47).
use crate::backup::BackupRam;

pub const POWER_MEMORY_SIZE: usize = 512 * 1024;
const ID: u32 = 0x04FF_FFFF;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Cartridge {
    #[default]
    None,
    Backup,
    Ram1MiB,
    Ram4MiB,
}

#[derive(Default)]
pub(crate) struct CartridgeSlot {
    kind: Cartridge,
    expansion: Vec<u8>,
    // Retain the detached cartridge's bytes. Only the selected cartridge
    // responds on the guest bus. An unused backup cartridge takes no RAM.
    backup: Option<BackupRam>,
}

impl CartridgeSlot {
    pub fn kind(&self) -> Cartridge {
        self.kind
    }

    pub fn insert(&mut self, kind: Cartridge) {
        self.kind = kind;
        self.expansion = match kind {
            Cartridge::Ram1MiB => vec![0; 1 << 20],
            Cartridge::Ram4MiB => vec![0; 4 << 20],
            _ => Vec::new(),
        };
        if kind == Cartridge::Backup {
            self.backup_mut();
        }
    }

    pub fn power_cycle(&mut self) {
        self.expansion.fill(0);
    }

    pub fn backup(&self) -> Option<&BackupRam> {
        self.backup.as_ref()
    }

    pub fn backup_mut(&mut self) -> &mut BackupRam {
        self.backup
            .get_or_insert_with(|| BackupRam::new(POWER_MEMORY_SIZE))
    }

    fn expansion_offset(&self, address: u32) -> Option<usize> {
        let offset = match self.kind {
            // Each 4 Mbit chip repeats once in its 1 MiB select.
            Cartridge::Ram1MiB => match address {
                0x0240_0000..=0x024F_FFFF => address & 0x7_FFFF,
                0x0260_0000..=0x026F_FFFF => (address & 0x7_FFFF) | 0x8_0000,
                _ => return None,
            },
            Cartridge::Ram4MiB if (0x0240_0000..0x0280_0000).contains(&address) => {
                address - 0x0240_0000
            }
            _ => return None,
        };
        Some(offset as usize)
    }

    fn backup_offset(&self, address: u32) -> Option<usize> {
        (self.kind == Cartridge::Backup
            && (0x0400_0000..=ID).contains(&address)
            && address & 1 != 0
            && address != ID)
            .then_some(((address >> 1) as usize) & (POWER_MEMORY_SIZE - 1))
    }

    pub fn read(&self, address: u32) -> u8 {
        if address == ID {
            return match self.kind {
                Cartridge::None => 0xFF,
                Cartridge::Backup => 0x21,
                Cartridge::Ram1MiB => 0x5A,
                Cartridge::Ram4MiB => 0x5C,
            };
        }
        if let Some(offset) = self.expansion_offset(address) {
            return self.expansion[offset];
        }
        if let Some(offset) = self.backup_offset(address) {
            return self.backup.as_ref().expect("inserted Power Memory").bytes()[offset];
        }
        0xFF
    }

    pub fn write(&mut self, address: u32, byte: u8) {
        if let Some(offset) = self.expansion_offset(address) {
            self.expansion[offset] = byte;
        } else if let Some(offset) = self.backup_offset(address) {
            self.backup_mut().write_packed(offset, byte);
        }
    }
}
