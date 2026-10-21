//! Checked partial Saturn map; devices and cache-through aliases are documented separately.
use crate::backup::{BACKUP_RAM, BACKUP_WINDOW, BackupRam};
use crate::cache::{CCR, Cache};
use crate::devices::{SMPC, Smpc, VDP2, VRAM, Vdp2};
use crate::onchip::OnChip;
use crate::scu::{SCU, Scu, ScuError};
use crate::sound::{SCSP, SOUND_RAM, SOUND_RAM_SIZE, Sound};
use crate::timing::{Raster, RasterEvent};
use crate::vdp1::{self, Vdp1};
use std::cell::RefCell;
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
    cached: bool,
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
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        self.read16(address)
    }
    fn tas_read8(&self, address: u32) -> Result<u8, BusError> {
        self.read8(address)
    }
    fn read32(&self, address: u32) -> Result<u32, BusError>;
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError>;
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError>;
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError>;

    /// SH7604 §4.8.3: exception stacking issues a bus cycle even with an
    /// unaligned SP. The physical write data is undefined. This functional model
    /// writes the big-endian bytes at the exact SP address, without rounding it.
    fn exception_write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        if address & 3 == 0 {
            return self.write32(address, value);
        }
        for (i, byte) in value.to_be_bytes().into_iter().enumerate() {
            self.write8(address.wrapping_add(i as u32), byte)?;
        }
        Ok(())
    }
}

pub struct Bus {
    bios: Option<Vec<u8>>,
    low: Vec<u8>,
    high: Vec<u8>,
    pub(crate) vdp2: Vdp2,
    pub(crate) vdp1: Vdp1,
    pub sound: Sound,
    pub cd: crate::cd::CdBlock,
    smpc: Smpc,
    pub(crate) scu: Scu,
    pub(crate) raster: Raster,
    scu_dma_phase: u32,
    pub(crate) backup: BackupRam,
    onchip: [OnChip; 2],
    cache: [RefCell<Cache>; 2],
    pub(crate) slave_enabled: bool,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            bios: None,
            low: vec![0; WRAM_SIZE],
            high: vec![0; WRAM_SIZE],
            vdp2: Vdp2::default(),
            vdp1: Vdp1::default(),
            sound: Sound::default(),
            cd: crate::cd::CdBlock::default(),
            smpc: Smpc::default(),
            scu: Scu::default(),
            raster: Raster::default(),
            scu_dma_phase: 0,
            backup: BackupRam::default(),
            onchip: std::array::from_fn(|i| {
                let mut chip = OnChip::default();
                chip.set_slave(i == 1);
                chip
            }),
            cache: std::array::from_fn(|_| RefCell::new(Cache::default())),
            slave_enabled: false,
        }
    }
}

impl Bus {
    pub fn for_cpu(&mut self, processor: Processor) -> CpuBus<'_> {
        CpuBus {
            bus: self,
            processor,
            cached: true,
        }
    }

    pub(crate) fn interrupt(&self, processor: Processor) -> Option<(u8, u8)> {
        let internal = self.onchip[processor as usize].interrupt();
        let external = if processor == Processor::Master {
            self.scu.interrupt()
        } else {
            None
        };
        match (internal, external) {
            (Some(i), Some(e)) => Some(if i.0 > e.0 { i } else { e }),
            (i, e) => i.or(e),
        }
    }

    pub(crate) fn acknowledge_interrupt(&mut self, processor: Processor, irq: (u8, u8)) {
        if processor == Processor::Master && self.scu.interrupt() == Some(irq) {
            self.scu.acknowledge(irq.1);
        }
    }
    pub(crate) fn advance_scu_dma(&mut self) -> Result<(), ScuError> {
        if !self.scu.dma_pending() {
            return Ok(());
        }
        let mut scu = std::mem::take(&mut self.scu);
        let result = scu.advance_dma(self);
        self.scu = scu;
        result
    }
    pub(crate) fn can_wake_master(&self) -> bool {
        self.slave_enabled
            || self.vdp1.busy()
            || self.sound.enabled
            || self.cd.active()
            || self.smpc.busy()
            || self.onchip[0].can_request_interrupt()
            || self.scu.can_wake()
    }

    pub(crate) fn set_slave_enabled(&mut self, enabled: bool) {
        if !enabled {
            self.onchip[1] = OnChip::default();
            self.onchip[1].set_slave(true);
            self.cache[1].get_mut().reset_control();
        }
        self.slave_enabled = enabled;
    }

    pub(crate) fn take_wait_clocks(&self) -> u32 {
        self.onchip.iter().map(OnChip::take_wait_clocks).sum()
    }
    pub(crate) fn cpu_reset_active(&self, processor: Processor) -> bool {
        self.onchip[processor as usize].reset_active()
    }
    pub(crate) fn take_cpu_reset(
        &mut self,
        processor: Processor,
    ) -> Option<crate::onchip::ResetRequest> {
        let chip = &mut self.onchip[processor as usize];
        let reset = chip.take_reset_request()?;
        chip.reset_from_watchdog(reset);
        self.cache[processor as usize].get_mut().reset_control();
        Some(reset)
    }
    pub(crate) fn signal_nmi(&mut self, processor: Processor) {
        self.onchip[processor as usize].signal_nmi();
    }
    pub(crate) fn advance_dma(&mut self, processor: Processor) -> bool {
        let index = processor as usize;
        if self.onchip[index].reset_active() || self.onchip[index].dma_stopped() {
            return false;
        }
        let requests = self.onchip[index].serial_dma_requests();
        let mut dma = std::mem::take(&mut self.onchip[index].dma);
        let mut memory = CpuBus {
            bus: self,
            processor,
            cached: false,
        };
        let acknowledged = dma.transfer(&mut memory, requests);
        if let Some(source) = acknowledged {
            self.onchip[index].acknowledge_serial_dma(source);
        }
        let error = std::mem::take(&mut dma.address_error);
        self.onchip[index].dma = dma;
        error
    }
    pub(crate) fn advance_clocks(&mut self, clocks: u32) -> Result<Option<u8>, ScuError> {
        let mut remaining = clocks;
        let mut command = None;
        while remaining != 0 {
            let delta = remaining
                .min(self.raster.clocks_to_event())
                .min(self.scu.clocks_to_event())
                .min(self.vdp1.clocks_to_event())
                .min(self.cd.clocks_to_event())
                .min(self.sound.clocks_to_sample())
                .min(if self.scu.dma_pending() {
                    8 - self.scu_dma_phase
                } else {
                    u32::MAX
                });
            self.onchip[0].advance_clocks(delta);
            if self.slave_enabled {
                self.onchip[1].advance_clocks(delta);
            }
            self.scu.advance_clocks(delta)?;
            if self.vdp1.advance(delta)? {
                self.scu.signal(13);
            }
            self.cd.advance(delta);
            if self.cd.interrupt() {
                self.scu.signal(16);
            }
            self.sound.advance_with_cd(delta, &mut self.cd)?;
            if self.sound.scsp.main_interrupt() {
                self.scu.signal(6);
            }
            command = self.smpc.advance_clocks(delta).or(command);
            if self.smpc.take_irq() {
                self.scu.signal(7);
            }
            match self.raster.advance(delta) {
                Some(RasterEvent::HBlankIn) => {
                    self.vdp2.hblank = true;
                    self.scu.hblank_in();
                }
                Some(RasterEvent::VBlankIn) => {
                    self.vdp2.hblank = false;
                    self.vdp2.vblank = true;
                    self.smpc.vblank_in();
                    self.scu.signal(0);
                }
                Some(RasterEvent::VBlankOut) => {
                    self.vdp2.hblank = false;
                    self.vdp2.vblank = false;
                    self.scu.vblank_out();
                    self.smpc.vblank_out();
                    self.vdp1.vblank_out();
                }
                Some(RasterEvent::LineStart) => self.vdp2.hblank = false,
                None => (),
            }
            self.scu_dma_phase = (self.scu_dma_phase + delta) % 8;
            if self.scu_dma_phase == 0 {
                self.advance_scu_dma()?;
            }
            remaining -= delta;
        }
        Ok(command)
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

    /// Completed character from an external SCI adapter. No implicit loopback.
    pub fn supply_serial_character(
        &mut self,
        processor: Processor,
        byte: u8,
        errors: u8,
        multiprocessor: bool,
    ) {
        self.onchip[processor as usize].receive_serial(byte, errors, multiprocessor);
    }
    pub fn take_serial_output(&mut self, processor: Processor) -> Option<u8> {
        self.onchip[processor as usize].take_serial_output()
    }

    pub fn set_reset_button(&mut self, pressed: bool) -> bool {
        self.smpc.set_reset_button(pressed)
    }
    pub fn set_analog(&mut self, port: usize, axes: Option<[u8; 3]>) {
        self.smpc.set_analog(port, axes);
    }
    pub fn set_3d_pad(&mut self, port: usize, axes: Option<[u8; 4]>) {
        self.smpc.set_3d_pad(port, axes);
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
        let mut smpc = std::mem::take(&mut self.smpc);
        smpc.reset_volatile();
        *self = Self {
            backup,
            smpc,
            ..Self::default()
        };
    }

    // SH7604 §2.3: P1 cache-through accesses share the external physical bus.
    fn physical(address: u32) -> u32 {
        let address = if (0x2000_0000..0x4000_0000).contains(&address) {
            address - 0x2000_0000
        } else {
            address
        };
        // The high RAM chip decodes A19–A0 throughout CS3; A24–A20
        // do not select additional storage (Overview memory map / SCU §1.2).
        if address < 0x0010_0000 {
            address & 0x0007_FFFF
        } else if (SMPC..SMPC + 0x80000).contains(&address) {
            SMPC | (address & 0x7F)
        } else if (HIGH_WRAM..0x0800_0000).contains(&address) {
            HIGH_WRAM | (address & 0x000F_FFFF)
        } else if (LOW_WRAM..0x0040_0000).contains(&address) {
            LOW_WRAM | (address & 0x000F_FFFF)
        } else if (SOUND_RAM..SCSP).contains(&address) {
            SOUND_RAM | (address & 0x0007_FFFF)
        } else if (vdp1::VRAM..vdp1::FRAMEBUFFER).contains(&address) {
            vdp1::VRAM | (address & 0x0007_FFFF)
        } else if (vdp1::FRAMEBUFFER..vdp1::REGISTERS).contains(&address) {
            vdp1::FRAMEBUFFER | (address & 0x0003_FFFF)
        } else if (VRAM..crate::vdp2::CRAM).contains(&address) {
            VRAM | (address & 0x0007_FFFF)
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
        if (crate::cd::REGISTERS..crate::cd::REGISTERS + 0x2C).contains(&address)
            || (crate::cd::DATA..crate::cd::DATA + 4).contains(&address)
        {
            if N != 2 && !(N == 4 && address == crate::cd::DATA) {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            for pair in bytes.as_chunks_mut::<2>().0 {
                pair.copy_from_slice(&self.cd.read16(address)?.to_be_bytes());
            }
            return Ok(bytes);
        }
        if (SOUND_RAM..SOUND_RAM + SOUND_RAM_SIZE as u32).contains(&address) {
            let offset = (address - SOUND_RAM) as usize;
            let mut bytes = [0; N];
            bytes.copy_from_slice(&self.sound.ram[offset..offset + N]);
            return Ok(bytes);
        }
        for (base, memory) in [
            (vdp1::VRAM, self.vdp1.vram.as_slice()),
            (vdp1::FRAMEBUFFER, self.vdp1.draw_buffer()),
        ] {
            if let Some(offset) = address.checked_sub(base).map(|n| n as usize)
                && offset < memory.len()
                && N <= memory.len() - offset
            {
                let mut bytes = [0; N];
                bytes.copy_from_slice(&memory[offset..offset + N]);
                return Ok(bytes);
            }
        }
        if (crate::vdp2::CRAM..crate::vdp2::CRAM + 0x1000).contains(&address) {
            let offset = self
                .vdp2
                .cram_offset((address - crate::vdp2::CRAM) as usize);
            let mut bytes = [0; N];
            bytes.copy_from_slice(&self.vdp2.cram[offset..offset + N]);
            return Ok(bytes);
        }
        if (vdp1::REGISTERS..vdp1::REGISTERS + 0x18).contains(&address) {
            if N != 2 && N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            for (i, pair) in bytes.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                pair.copy_from_slice(&self.vdp1.read16(address + i as u32 * 2)?.to_be_bytes());
            }
            return Ok(bytes);
        }
        if (SCSP..SCSP + 0x1000).contains(&address) {
            let mut bytes = [0; N];
            if N == 1 {
                bytes[0] = self.sound.scsp.read8(address - SCSP)?;
            } else {
                for (i, pair) in bytes.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                    pair.copy_from_slice(
                        &self
                            .sound
                            .scsp
                            .read16(address - SCSP + i as u32 * 2)?
                            .to_be_bytes(),
                    );
                }
            }
            return Ok(bytes);
        }
        if (SCU..SCU + 0xD0).contains(&address) {
            if N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let mut bytes = [0; N];
            bytes.copy_from_slice(&self.scu.read32(address)?.to_be_bytes());
            return Ok(bytes);
        }
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_WINDOW).contains(&address) {
            let mut bytes = [0; N];
            for (i, byte) in bytes.iter_mut().enumerate() {
                *byte = self.backup.read8(address + i as u32)?;
            }
            return Ok(bytes);
        }
        if (SMPC..SMPC + 0x80).contains(&address) {
            let mut bytes = [0; N];
            for (i, byte) in bytes.iter_mut().enumerate() {
                let lane = address + i as u32;
                *byte = if lane & 1 == 0 {
                    255
                } else {
                    self.smpc.read(lane)?
                };
            }
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
        if (crate::cd::REGISTERS..crate::cd::REGISTERS + 0x2C).contains(&address)
            || (crate::cd::DATA..crate::cd::DATA + 4).contains(&address)
        {
            if N != 2 && !(N == 4 && address == crate::cd::DATA) {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            for pair in bytes.as_chunks::<2>().0 {
                self.cd.write16(address, u16::from_be_bytes(*pair))?;
            }
            return Ok(());
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
        if (SOUND_RAM..SOUND_RAM + SOUND_RAM_SIZE as u32).contains(&address) {
            let offset = (address - SOUND_RAM) as usize;
            self.sound.ram[offset..offset + N].copy_from_slice(&bytes);
            return Ok(());
        }
        if (vdp1::VRAM..vdp1::VRAM + vdp1::VRAM_SIZE as u32).contains(&address) {
            let offset = (address - vdp1::VRAM) as usize;
            self.vdp1.vram[offset..offset + N].copy_from_slice(&bytes);
            return Ok(());
        }
        if (crate::vdp2::CRAM..crate::vdp2::CRAM + 0x1000).contains(&address) {
            let offset = self
                .vdp2
                .cram_offset((address - crate::vdp2::CRAM) as usize);
            self.vdp2.cram[offset..offset + N].copy_from_slice(&bytes);
            return Ok(());
        }
        if (vdp1::FRAMEBUFFER..vdp1::FRAMEBUFFER + vdp1::BUFFER_SIZE as u32).contains(&address) {
            let offset = (address - vdp1::FRAMEBUFFER) as usize;
            self.vdp1.draw_buffer_mut()[offset..offset + N].copy_from_slice(&bytes);
            return Ok(());
        }
        if (vdp1::REGISTERS..vdp1::REGISTERS + 0x18).contains(&address) {
            if N != 2 && N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                Vdp1::validate_write(address + i as u32 * 2, u16::from_be_bytes(*pair))?;
            }
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                self.vdp1
                    .write16(address + i as u32 * 2, u16::from_be_bytes(*pair))?;
            }
            return Ok(());
        }
        if (SCSP..SCSP + 0x1000).contains(&address) {
            if N == 1 {
                return self.sound.scsp.write8(address - SCSP, bytes[0]);
            }
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                self.sound
                    .scsp
                    .write16(address - SCSP + i as u32 * 2, u16::from_be_bytes(*pair))?;
            }
            return Ok(());
        }
        if (SCU..SCU + 0xD0).contains(&address) {
            if N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            return self.scu.write32(
                address,
                u32::from_be_bytes(bytes.as_slice().try_into().unwrap()),
            );
        }
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_WINDOW).contains(&address) {
            for (i, byte) in bytes.into_iter().enumerate() {
                self.backup.write8(address + i as u32, byte)?;
            }
            return Ok(());
        }
        if (SMPC..SMPC + 0x80).contains(&address) {
            for (i, byte) in bytes.into_iter().enumerate() {
                let lane = address + i as u32;
                if lane & 1 != 0 {
                    self.smpc.write(lane, byte)?;
                }
            }
            return Ok(());
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
        if address == CCR {
            return Ok(self.cache[0].borrow().control());
        }
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
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].read32(address);
        }
        Ok(u32::from_be_bytes(self.read(address)?))
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address == CCR {
            self.cache[0].get_mut().set_control(value);
            return Ok(());
        }
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
        if address >= 0xFFFF_FE00 {
            return self.onchip[0].write32(address, value);
        }
        self.write(address, value.to_be_bytes())
    }
}

impl CpuBus<'_> {
    fn read<const N: usize>(&self, address: u32, instruction: bool) -> Result<[u8; N], BusError> {
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        if self.cached {
            let mut cache = self.bus.cache[self.processor as usize].borrow_mut();
            if let Some(bytes) = cache.array_read(address)? {
                return Ok(bytes);
            }
            if let Some(bytes) =
                cache.read(address, instruction, |line| self.bus.read::<16>(line))?
            {
                return Ok(bytes);
            }
        }
        self.bus.read(address)
    }
    fn write<const N: usize>(&mut self, address: u32, bytes: [u8; N]) -> Result<(), BusError> {
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        let index = self.processor as usize;
        if self.cached
            && self.bus.cache[index]
                .get_mut()
                .array_write(address, bytes)?
        {
            return Ok(());
        }
        self.bus.write(address, bytes)?;
        if self.cached {
            self.bus.cache[index].get_mut().written(address, bytes);
        }
        Ok(())
    }
}
impl Memory for CpuBus<'_> {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address == CCR {
            return Ok(self.bus.cache[self.processor as usize].borrow().control());
        }
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].read8(address);
        }
        Ok(self.read::<1>(address, false)?[0])
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].read16(address);
        }
        Ok(u16::from_be_bytes(self.read(address, false)?))
    }
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        Ok(u16::from_be_bytes(self.read(address, true)?))
    }
    fn tas_read8(&self, address: u32) -> Result<u8, BusError> {
        Ok(self.bus.read::<1>(address)?[0])
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].read32(address);
        }
        Ok(u32::from_be_bytes(self.read(address, false)?))
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address == CCR {
            self.bus.cache[self.processor as usize]
                .get_mut()
                .set_control(value);
            return Ok(());
        }
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].write8(address, value);
        }
        self.write(address, [value])
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].write16(address, value);
        }
        self.write(address, value.to_be_bytes())
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            return self.bus.onchip[self.processor as usize].write32(address, value);
        }
        self.write(address, value.to_be_bytes())
    }
}

#[cfg(test)]
mod scu_timing_tests {
    use super::*;
    #[test]
    fn vdp1_end_interrupt_and_draw_end_dma_trigger_obey_clock_deadlines() {
        for chunk in [1, 3, 8, 17, 32] {
            let mut b = Bus::default();
            b.write32(HIGH_WRAM, 0x12345678).unwrap();
            b.write16(vdp1::VRAM, 0x8000).unwrap();
            for (off, value) in [
                (0, HIGH_WRAM),
                (4, VRAM),
                (8, 4),
                (12, 0x101),
                (20, 6),
                (16, 0x100),
            ] {
                b.write32(SCU + off, value).unwrap();
            }
            b.write32(SCU + 0xA0, 0x9FFF).unwrap();
            b.write16(vdp1::REGISTERS + 4, 1).unwrap();
            assert!(b.can_wake_master());
            let mut elapsed = 0;
            while elapsed < 31 {
                let n = chunk.min(31 - elapsed);
                b.advance_clocks(n).unwrap();
                elapsed += n;
            }
            assert_eq!(b.interrupt(Processor::Master), None);
            assert_eq!(b.read16(vdp1::REGISTERS + 0x10).unwrap(), 0);
            b.advance_clocks(1).unwrap();
            assert_eq!(b.interrupt(Processor::Master), Some((2, 0x4D)));
            assert_eq!(b.read16(vdp1::REGISTERS + 0x10).unwrap(), 2);
            b.advance_clocks(16).unwrap();
            assert_eq!(b.read32(VRAM).unwrap(), 0x12340000);
            b.advance_clocks(8).unwrap();
            assert_eq!(b.read32(VRAM).unwrap(), 0x12345678);
            b.acknowledge_interrupt(Processor::Master, (2, 0x4D));
            assert_eq!(b.interrupt(Processor::Master), None);
            b.reset_volatile();
            assert_eq!(b.read32(vdp1::VRAM).unwrap(), 0);
            assert_eq!(b.read32(vdp1::FRAMEBUFFER).unwrap(), 0);
            assert_eq!(b.read16(vdp1::REGISTERS + 0x10).unwrap(), 0);
            assert!(!b.can_wake_master());
        }
    }
    #[test]
    fn scu_dma_and_dsp_progress_by_hardware_clocks_independent_of_cpu_issue() {
        fn result(chunk: u32) -> (u32, u32, u64) {
            let mut b = Bus::default();
            b.write32(HIGH_WRAM, 0x12345678).unwrap();
            for (off, value) in [
                (0, HIGH_WRAM),
                (4, VRAM),
                (8, 4),
                (12, 0x101),
                (20, 7),
                (16, 0x101),
            ] {
                b.write32(SCU + off, value).unwrap();
            }
            b.write32(SCU + 0x80, 0x8000).unwrap();
            b.write32(SCU + 0x84, 0x8000002A).unwrap(); // MVI #42,MC0
            b.write32(SCU + 0x84, 0xF8000000).unwrap(); // ENDI
            b.write32(SCU + 0x80, 0x8000).unwrap();
            b.write32(SCU + 0x80, 0x10000).unwrap();
            let mut elapsed = 0;
            while elapsed < 100 {
                let n = chunk.min(100 - elapsed);
                b.advance_clocks(n).unwrap();
                elapsed += n;
            }
            b.write32(SCU + 0x88, 0).unwrap();
            assert_eq!(b.read32(SCU + 0x8C).unwrap(), 42);
            (
                b.read32(VRAM).unwrap(),
                b.read32(SCU + 0xA4).unwrap(),
                b.raster.fields,
            )
        }
        let expected = (0x12345678, (1 << 11) | (1 << 5), 0);
        for chunk in [1, 3, 8, 17, 100] {
            assert_eq!(result(chunk), expected);
        }
    }
    #[test]
    fn sleeping_master_can_wait_for_future_scu_timer_and_blanking_interrupts() {
        let mut b = Bus::default();
        assert!(!b.can_wake_master());
        b.write32(SCU + 0xA0, 0xBFF7).unwrap();
        b.write32(SCU + 0x98, 1).unwrap();
        assert!(b.can_wake_master());
        b.write32(SCU + 0x98, 0).unwrap();
        assert!(!b.can_wake_master());
        b.write32(SCU + 0xA0, 0xBFFE).unwrap();
        assert!(b.can_wake_master());
        b.reset_volatile();
        assert!(!b.can_wake_master());
        assert_eq!(b.raster.fields, 0);
        assert!(!b.vdp2.hblank);
        assert!(!b.vdp2.vblank);
    }
}
