//! SH7604 §4.3.1 Table 4.6 operand-cycle checks, independent of the Saturn map.
use crate::bus::{BusError, Memory};
use std::cell::Cell;

#[derive(Clone, Copy)]
enum Access {
    Data,
    PcRelative,
    Tas,
}

pub(super) struct Operands<'a, B> {
    pub inner: &'a mut B,
    pub address_error: Cell<bool>,
}

impl<'a, B: Memory> Operands<'a, B> {
    pub fn new(inner: &'a mut B) -> Self {
        Self {
            inner,
            address_error: Cell::new(false),
        }
    }

    fn invalid(&self, address: u32, width: u32, access: Access) -> bool {
        // Table 8.2 defines purge/address/data-array/I/O partitions by A31–A29.
        let partition = address >> 29;
        let invalid = !address.is_multiple_of(width)
            || (address >= 0xFFFF_FF00 && width == 1)
            || ((0xFFFF_FE00..0xFFFF_FF00).contains(&address) && width == 4)
            || match access {
                Access::Data => false,
                Access::PcRelative => matches!(partition, 2 | 3 | 7),
                Access::Tas => matches!(partition, 2 | 3 | 6 | 7),
            };
        if invalid {
            self.address_error.set(true);
        }
        invalid
    }

    fn read(&self, address: u32, width: u32, access: Access) -> Result<u32, BusError> {
        if self.invalid(address, width, access) {
            // Invalid-cycle data is not a useful architectural operand. Choose
            // deterministic zero and suppress the invalid device transaction;
            // this policy is documented/tested, not a physical bus-data claim.
            return Ok(0);
        }
        match width {
            1 => match access {
                Access::Tas => self.inner.tas_read8(address).map(u32::from),
                _ => self.inner.read8(address).map(u32::from),
            },
            2 => self.inner.read16(address).map(u32::from),
            _ => self.inner.read32(address),
        }
    }

    fn write(
        &mut self,
        address: u32,
        width: u32,
        value: u32,
        access: Access,
    ) -> Result<(), BusError> {
        // H table4.6 note1 / §7.5.8: MRS word writes do not raise an
        // alignment address error. A0 is not driven to the SDRAM mode pins.
        if width == 2 && (0xFFFF8000..0xFFFFC000).contains(&address) {
            return self.inner.write16(address, value as u16);
        }
        if self.invalid(address, width, access) {
            return Ok(());
        }
        match width {
            1 => self.inner.write8(address, value as u8),
            2 => self.inner.write16(address, value as u16),
            _ => self.inner.write32(address, value),
        }
    }

    pub fn pc_relative16(&self, address: u32) -> Result<u16, BusError> {
        self.read(address, 2, Access::PcRelative).map(|v| v as u16)
    }

    pub fn pc_relative32(&self, address: u32) -> Result<u32, BusError> {
        self.read(address, 4, Access::PcRelative)
    }

    pub fn tas_read(&self, address: u32) -> Result<u8, BusError> {
        self.read(address, 1, Access::Tas).map(|v| v as u8)
    }

    pub fn tas_write(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.write(address, 1, u32::from(value), Access::Tas)
    }
}

impl<B: Memory> Memory for Operands<'_, B> {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        self.read(address, 1, Access::Data).map(|v| v as u8)
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        self.read(address, 2, Access::Data).map(|v| v as u16)
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        self.read(address, 4, Access::Data)
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.write(address, 1, u32::from(value), Access::Data)
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        self.write(address, 2, u32::from(value), Access::Data)
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.write(address, 4, value, Access::Data)
    }
}
