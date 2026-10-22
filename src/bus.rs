//! Checked partial Saturn map; devices and cache-through aliases are documented separately.
use crate::backup::{BACKUP_RAM, BACKUP_SELECT, BackupRam};
use crate::cache::{CCR, Cache};
use crate::devices::{SMPC, Smpc, VDP2, VRAM, Vdp2};
use crate::onchip::OnChip;
use crate::scu::{SCU, Scu, ScuError};
use crate::sound::{SCSP, SOUND_RAM, Sound};
use crate::timing::{Raster, RasterEvent, SystemClock};
use crate::vdp1::{self, Vdp1};
use std::cell::{Cell, RefCell};
use std::fmt;
#[cfg(test)]
mod dmac_tests;
mod transactions;

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
    scheduled: bool,
}

/// SCU DMA/DSP bus-master transactions have physical pins, rather than the
/// CPU-local cache/peripheral view. Both UBCs can monitor these cycles.
#[cfg(test)]
struct ExternalBus<'a>(&'a mut Bus);
#[cfg(test)]
impl ExternalBus<'_> {
    fn observe(&self, address: u32, width: usize, value: u32, write: bool) {
        self.0.release_external_bus();
        for chip in &self.0.onchip {
            chip.observe_access(Bus::physical(address), width, value, write, false, true);
        }
    }
}
#[cfg(test)]
impl Memory for ExternalBus<'_> {
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        let value = self.0.read8(address)?;
        self.observe(address, 1, u32::from(value), false);
        Ok(value)
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        let value = self.0.read16(address)?;
        self.observe(address, 2, u32::from(value), false);
        Ok(value)
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        let value = self.0.read32(address)?;
        self.observe(address, 4, value, false);
        Ok(value)
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        self.0.write8(address, value)?;
        self.observe(address, 1, u32::from(value), true);
        Ok(())
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        self.0.write16(address, value)?;
        self.observe(address, 2, u32::from(value), true);
        Ok(())
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        self.0.write32(address, value)?;
        self.observe(address, 4, value, true);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusError {
    /// Internal scheduler suspension; never an architectural address error.
    Pending,
    Unaligned {
        address: u32,
        width: usize,
    },
    Unmapped {
        address: u32,
        width: usize,
    },
    ReadOnly {
        address: u32,
        width: usize,
    },
    BiosLength {
        actual: usize,
    },
    BackupLength {
        actual: usize,
    },
    UnsupportedDevice {
        address: u32,
        width: usize,
    },
}

impl fmt::Display for BusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Bus transaction is pending"),
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
    /// Diagnostic provenance for a deferred SH-2 write. This does not make
    /// the BSC buffer part of the architectural exception mechanism.
    fn sh2_write_context(&self, _pc: u32, _vector: Option<u8>) {}
    /// Discard the sequential instruction word on a taken control transfer.
    /// SH7604 §§7.3.1/7.11.2: the external fetch is a longword, including
    /// entry at 4n+2. This register is independent of the data cache.
    fn sh2_discard_prefetch(&self) {}
    /// SH programming manual §7.5: a WB/EX interlock precedes the
    /// dependent instruction's operand cycles. Clocked adapters suspend the
    /// CPU locally; an architectural memory fixture has no clock domain.
    fn sh2_pipeline_wait(&mut self, _clocks: u32) -> Result<(), BusError> {
        Ok(())
    }
    /// MC68000UM §5.5: RESET holds the external device signal for 124 clocks.
    /// A clocked sound adapter suspends here until the pulse completes; an
    /// architectural-only memory fixture has no external clock domain.
    fn m68k_reset_output(&mut self) -> Result<(), BusError> {
        Ok(())
    }
    /// MC68000UM §§5.1/6.3.2–4: the standard sound board autovectors.
    /// Other architectural fixtures may return a device vector or BERR
    /// (spurious vector24). Pending defers stacking until IACK completes.
    fn m68k_interrupt_acknowledge(&mut self, level: u8) -> Result<u8, BusError> {
        Ok(24 + level)
    }
    /// SH7604 §9.3.4 single-address DMA selects a peripheral by its DACK pin,
    /// not by SAR/DAR. A board adapter supplies these electrical data transfers.
    /// Unconnected/unsupported endpoints never become a second memory access.
    fn dack_read(
        &mut self,
        channel: usize,
        width: usize,
        _active_high: bool,
    ) -> Result<u32, BusError> {
        Err(BusError::UnsupportedDevice {
            address: 0xFFFF_FF8C + channel as u32 * 16,
            width,
        })
    }
    fn dack_write(
        &mut self,
        channel: usize,
        width: usize,
        _active_high: bool,
        _value: u32,
    ) -> Result<(), BusError> {
        Err(BusError::UnsupportedDevice {
            address: 0xFFFF_FF8C + channel as u32 * 16,
            width,
        })
    }
    fn enter_sleep(&mut self) {}
    /// SH7604 §6.3.2: native-mode before/after execution comparison.
    /// SH7000-compatible comparisons belong to the adapter's real IF cycle.
    fn instruction_break(&self, _pc: u32, _after: bool) -> bool {
        false
    }
    fn break_pending(&self) -> bool {
        false
    }
    fn acknowledge_break(&self) {}
    fn read8(&self, address: u32) -> Result<u8, BusError>;
    fn read16(&self, address: u32) -> Result<u16, BusError>;
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        self.read16(address)
    }
    fn tas_read8(&self, address: u32) -> Result<u8, BusError> {
        self.read8(address)
    }
    fn read32(&self, address: u32) -> Result<u32, BusError>;
    /// H §9.1.1: the four reads of a 16-byte DMAC unit form one burst.
    fn dma_read32(&self, address: u32, _first: bool, _last: bool) -> Result<u32, BusError> {
        self.read32(address)
    }
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

    /// H §4.8.2: an unaligned VBR raises an address error during exception
    /// entry. As with invalid stacking, complete the original entry before
    /// dispatching that error. Use exact-address big-endian bytes as the
    /// deterministic invalid-cycle data policy, without rounding the VBR.
    fn exception_read32(&self, address: u32) -> Result<u32, BusError> {
        if address & 3 == 0 {
            return self.read32(address);
        }
        let mut bytes = [0; 4];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = self.read8(address.wrapping_add(i as u32))?;
        }
        Ok(u32::from_be_bytes(bytes))
    }
}

pub struct Bus {
    transactions: transactions::Transactions,
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
    cpu_dma_errors: [bool; 2],
    cpu_bus_clocks: [Cell<u32>; 2],
    system_clock: SystemClock,
    pub(crate) backup: BackupRam,
    onchip: [OnChip; 2],
    cache: [RefCell<Cache>; 2],
    instruction_word: [Cell<Option<(u32, u16)>>; 2],
    external_owner: Cell<Option<Processor>>,
    pub(crate) slave_enabled: bool,
}

impl Default for Bus {
    fn default() -> Self {
        Self {
            transactions: transactions::Transactions::default(),
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
            cpu_dma_errors: [false; 2],
            cpu_bus_clocks: [Cell::new(0), Cell::new(0)],
            system_clock: SystemClock::default(),
            backup: BackupRam::default(),
            onchip: std::array::from_fn(|i| {
                let mut chip = OnChip::default();
                chip.set_slave(i == 1);
                chip
            }),
            cache: std::array::from_fn(|_| RefCell::new(Cache::default())),
            instruction_word: std::array::from_fn(|_| Cell::new(None)),
            external_owner: Cell::new(None),
            slave_enabled: false,
        }
    }
}

#[cfg(test)]
mod cpu_dma_clock_tests {
    use super::*;
    #[test]
    fn onchip_dma_progresses_on_clock_deadlines_without_cpu_issue_and_is_chunk_invariant() {
        let mut reference = None;
        for chunk in [1, 3, 7, 64] {
            let mut bus = Bus::default();
            for i in 0..3 {
                bus.write32(HIGH_WRAM + 0x100 + i * 4, 0x12340000 + i)
                    .unwrap();
            }
            for (offset, value) in [
                (0, HIGH_WRAM + 0x100),
                (4, HIGH_WRAM + 0x200),
                (8, 3),
                (12, 0x5A05),
                (0x30, 1),
            ] {
                bus.onchip[0]
                    .write32(crate::onchip::DMAC + offset, value)
                    .unwrap();
            }
            // Reset BSC: eight-state 32-bit read/write plus two turnaround
            // states (WCR IW3=2), for eighteen states per first unit.
            bus.advance_clocks(17).unwrap();
            assert_eq!(bus.read32(HIGH_WRAM + 0x200), Ok(0));
            bus.advance_clocks(1).unwrap();
            assert_eq!(bus.read32(HIGH_WRAM + 0x200), Ok(0x12340000));
            assert_eq!(bus.onchip[0].read32(crate::onchip::DMAC + 8), Ok(2));
            let mut remaining = 128;
            while remaining != 0 {
                let delta = remaining.min(chunk);
                bus.advance_clocks(delta).unwrap();
                remaining -= delta;
            }
            let trace = (
                (0..3)
                    .map(|i| bus.read32(HIGH_WRAM + 0x200 + i * 4).unwrap())
                    .collect::<Vec<_>>(),
                bus.onchip[0].read32(crate::onchip::DMAC + 8).unwrap(),
                bus.onchip[0].read32(crate::onchip::DMAC + 12).unwrap(),
                bus.take_dma_errors(),
            );
            assert_eq!(trace.0, [0x12340000, 0x12340001, 0x12340002]);
            assert_eq!(trace.1, 0);
            assert_ne!(trace.2 & 2, 0);
            assert_eq!(trace.3, [false; 2]);
            if let Some(ref previous) = reference {
                assert_eq!(&trace, previous);
            }
            reference = Some(trace);
        }
    }
}

impl Bus {
    fn release_external_bus(&self) {
        self.external_owner.set(None);
        for chip in &self.onchip {
            chip.release_external_bus();
        }
    }
    fn claim_external_bus(&self, processor: Processor) {
        if self.external_owner.get() != Some(processor) {
            // H §7.9: arbitration supplies the empty cycle; an old owner's
            // read must not create a second turnaround on reacquisition.
            self.release_external_bus();
            self.external_owner.set(Some(processor));
        }
    }
    pub fn for_cpu(&mut self, processor: Processor) -> CpuBus<'_> {
        CpuBus {
            bus: self,
            processor,
            cached: true,
            scheduled: false,
        }
    }

    pub(crate) fn interrupt(&self, processor: Processor) -> Option<(u8, u8)> {
        let internal = self.onchip[processor as usize].interrupt();
        let external = if processor == Processor::Master {
            self.scu
                .interrupt()
                .map(|(level, vector)| (level, self.onchip[0].external_vector(level, vector)))
        } else {
            None
        };
        match (internal, external) {
            // H table 5.4: UBC precedes IRL15; all other internal sources
            // follow an equal-priority external request.
            (Some(i), Some(e)) => Some(if i == (15, 12) || i.0 > e.0 { i } else { e }),
            (i, e) => i.or(e),
        }
    }

    pub(crate) fn acknowledge_interrupt(&mut self, processor: Processor, irq: (u8, u8)) {
        if irq == (15, 12) && self.onchip[processor as usize].interrupt() == Some(irq) {
            self.onchip[processor as usize].acknowledge_break();
            return;
        }
        if processor == Processor::Master
            && let Some((level, vector)) = self.scu.interrupt()
            && irq == (level, self.onchip[0].external_vector(level, vector))
        {
            // Acknowledge the actual SCU source, even in SH-2 auto-vector mode.
            self.scu.acknowledge(vector);
        }
    }
    #[cfg(test)]
    pub(crate) fn advance_scu_dma(&mut self) -> Result<(), ScuError> {
        if !self.scu.dma_pending() {
            return Ok(());
        }
        let mut scu = std::mem::take(&mut self.scu);
        let result = scu.advance_dma(&mut ExternalBus(self));
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
            self.cancel_cpu_transaction(Processor::Slave);
            self.cancel_dmac_transaction(Processor::Slave);
        }
        if enabled != self.slave_enabled {
            self.cpu_dma_errors[1] = false;
            self.cpu_bus_clocks[1].set(0);
        }
        if !enabled {
            self.onchip[1] = OnChip::default();
            self.onchip[1].set_slave(true);
            self.cache[1].get_mut().reset_control();
        }
        self.slave_enabled = enabled;
    }

    #[cfg(test)]
    pub(crate) fn take_wait_clocks(&self) -> u32 {
        let clocks = self.onchip.iter().map(OnChip::take_wait_clocks).sum();
        if clocks == 0 {
            0
        } else {
            self.system_clock.deadline(clocks)
        }
    }
    pub(crate) fn cpu_delay(&self, processor: Processor, states: u32) -> u32 {
        let waits = self.onchip[processor as usize].take_wait_clocks();
        let external = self.cpu_bus_clocks[processor as usize].replace(0);
        self.system_clock
            .deadline(states.max(external).saturating_add(waits))
            .max(1)
    }
    pub(crate) fn take_dma_errors(&mut self) -> [bool; 2] {
        std::mem::take(&mut self.cpu_dma_errors)
    }
    pub(crate) fn cpu_reset_active(&self, processor: Processor) -> bool {
        self.onchip[processor as usize].reset_active()
    }
    pub(crate) fn take_cpu_reset(
        &mut self,
        processor: Processor,
    ) -> Option<crate::onchip::ResetRequest> {
        if !self.reset_drain_complete(processor) {
            return None;
        }
        let chip = &mut self.onchip[processor as usize];
        let reset = chip.take_reset_request()?;
        chip.finish_watchdog_reset();
        self.cancel_cpu_transaction(processor);
        self.cancel_dmac_transaction(processor);
        self.cache[processor as usize].get_mut().reset_control();
        Some(reset)
    }
    pub(crate) fn signal_nmi(&mut self, processor: Processor) {
        self.onchip[processor as usize].signal_nmi();
    }
    pub(crate) fn nmi_ready(&self, processor: Processor) -> bool {
        self.onchip[processor as usize].nmi_ready()
    }
    #[cfg(test)]
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
            scheduled: false,
        };
        let acknowledged = dma.transfer(&mut memory, requests);
        if let Some(source) = acknowledged {
            self.onchip[index].acknowledge_serial_dma(source);
        }
        let error = std::mem::take(&mut dma.address_error);
        self.onchip[index].dma = dma;
        error
    }
    fn flush_video(&mut self) {
        if self.raster.line < self.raster.display_lines() {
            let end = self.raster.visible_pixels(self.vdp2.dimensions().0);
            self.vdp2
                .scan_to(&self.vdp1, usize::from(self.raster.line), end);
        }
    }
    fn main_deadline(&self) -> u32 {
        self.onchip[0]
            .clocks_to_event()
            .min(self.bus_main_deadline())
            .min(self.onchip[1].clocks_to_event())
            .min(self.raster.clocks_to_event())
            .min(self.scu.clocks_to_event())
            .min(self.vdp1.clocks_to_event())
    }

    fn advance_fixed_clocks(&mut self, clocks: u32) -> Result<Option<u8>, ScuError> {
        self.cd.advance(clocks);
        if self.cd.interrupt() {
            self.scu.signal(16);
        }
        self.sound.advance_with_cd(clocks, &mut self.cd)?;
        if self.sound.scsp.main_interrupt() {
            self.scu.signal(6);
        }
        let command = self.smpc.advance_clocks(clocks);
        if self.smpc.take_irq() {
            self.scu.signal(7);
        }
        Ok(command)
    }

    fn advance_main_clocks(
        &mut self,
        mut remaining: u32,
        reference_clocks: u32,
    ) -> Result<Option<u8>, ScuError> {
        if remaining == 0 {
            return self.advance_fixed_clocks(reference_clocks);
        }
        let mut reference_clocks = Some(reference_clocks);
        let mut command = None;
        while remaining != 0 {
            self.grant_bus_transaction();
            let delta = remaining.min(self.main_deadline());
            self.onchip[0].advance_clocks(delta);
            if self.slave_enabled {
                self.onchip[1].advance_clocks(delta);
            }
            self.scu.advance_clocks(delta)?;
            let (vram_busy, framebuffer_busy) = self.vdp1_bus_ownership();
            if self
                .vdp1
                .advance_with_bus(delta, vram_busy, framebuffer_busy)?
            {
                self.scu.signal(13);
            }
            // Keep the established boundary order: main peripherals, fixed
            // domains, blanking edges, DMA. A PLL conversion can deliver two
            // main ticks at one reference boundary; fixed time advances once.
            if let Some(clocks) = reference_clocks.take() {
                command = self.advance_fixed_clocks(clocks)?;
            }
            match self.raster.advance(delta) {
                Some(RasterEvent::HBlankIn) => {
                    if self.raster.line < self.raster.display_lines() {
                        self.vdp2
                            .scanline(&self.vdp1, usize::from(self.raster.line));
                        self.vdp1.erase_display_line(
                            usize::from(self.raster.line),
                            self.vdp2.dimensions().0,
                        );
                    }
                    self.vdp2.hblank = true;
                    self.scu.hblank_in();
                }
                Some(RasterEvent::VBlankIn) => {
                    self.vdp1.vblank_in(self.raster.line_clocks());
                    self.vdp2.finish_field();
                    self.vdp2.hblank = false;
                    self.vdp2.vblank = true;
                    self.smpc.vblank_in();
                    self.scu.signal(0);
                }
                Some(RasterEvent::VBlankOut) => {
                    self.vdp2.hblank = false;
                    self.vdp2.vblank = false;
                    self.scu.vblank_out();
                    let active = self.system_clock.deadline(self.raster.active_clocks());
                    self.smpc.vblank_out_after(active);
                    self.vdp1.vblank_out();
                    self.vdp2.scan_field(self.raster.odd());
                    self.vdp2.begin_scanline(0);
                }
                Some(RasterEvent::LineStart) => {
                    self.vdp1.blank_line_start();
                    self.vdp2.hblank = false;
                    if self.raster.line < self.raster.display_lines() {
                        self.vdp2.begin_scanline(usize::from(self.raster.line));
                    }
                }
                None => (),
            }
            let (h, v) = self.raster.counters();
            self.vdp2.counters(h, v);
            self.vdp2.scan_field(self.raster.odd());
            // Complete the granted data phase before contenders at this edge.
            self.advance_bus_transaction(delta)?;
            for processor in [Processor::Master, Processor::Slave] {
                if self.cpu_reset_active(processor) {
                    self.hold_cpu_reset(processor);
                }
            }
            remaining -= delta;
        }
        Ok(command)
    }

    /// SMPC §§2.3.9–10: PLL change resets the listed LSIs, not work RAM,
    /// the master architectural state, CD block or battery-backed storage.
    fn change_system_clock(&mut self, fast: bool) {
        self.system_clock.select(fast);
        self.raster.select_clock(fast);
        self.vdp1 = Vdp1::default();
        self.vdp2 = Vdp2::default();
        self.vdp2.pal = self.raster.pal;
        self.raster.configure(0);
        self.scu = Scu::default();
        self.cancel_scu_transaction();
        self.sound = Sound::default();
        self.set_slave_enabled(false);
    }

    /// Advance attached hardware in reference clocks without issuing CPU
    /// instructions. Machine coordinates command completions with CPU state.
    pub fn advance_clocks(&mut self, clocks: u32) -> Result<Option<u8>, ScuError> {
        let mut remaining = clocks;
        let mut command = None;
        while remaining != 0 {
            let delta = remaining
                .min(self.system_clock.deadline(self.main_deadline()).max(1))
                .min(self.cd.clocks_to_event())
                .min(self.smpc.clocks_to_event())
                .min(self.sound.clocks_to_event());
            let main_clocks = self.system_clock.advance(delta);
            if let Some(completed) = self.advance_main_clocks(main_clocks, delta)? {
                if matches!(completed, 0x0E | 0x0F) {
                    self.change_system_clock(completed == 0x0E);
                }
                command = Some(completed);
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
    /// Physical video standard strap, independent of software region flags.
    pub fn set_pal(&mut self, pal: bool) {
        self.raster.pal = pal;
        self.vdp2.pal = pal;
        self.system_clock.set_pal(pal);
    }
    pub fn set_region(&mut self, region: crate::smpc::Region) {
        self.smpc.set_region(region);
    }
    pub fn set_analog(&mut self, port: usize, axes: Option<[u8; 3]>) {
        self.smpc.set_analog(port, axes);
    }
    pub fn set_3d_pad(&mut self, port: usize, axes: Option<[u8; 4]>) {
        self.smpc.set_3d_pad(port, axes);
    }

    pub(crate) fn set_peripheral_th(&mut self, port: usize, high: bool) {
        if self.smpc.set_th(port, high) {
            self.vdp2.external_latch();
            self.scu.signal(8);
        }
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
        let pal = self.raster.pal;
        let backup = std::mem::take(&mut self.backup);
        let mut smpc = std::mem::take(&mut self.smpc);
        smpc.reset_volatile();
        *self = Self {
            backup,
            smpc,
            ..Self::default()
        };
        self.set_pal(pal);
    }

    /// ST-169 §2.3.8: SYSRES restarts the installed ROM, clears volatile
    /// hardware/memory and retains the physical disc and battery-backed state.
    pub(crate) fn system_reset(&mut self) {
        let bios = self.bios.take();
        let media = self.cd.media().cloned();
        let pal = self.raster.pal;
        let backup = std::mem::take(&mut self.backup);
        let mut smpc = std::mem::take(&mut self.smpc);
        smpc.system_reset();
        *self = Self {
            bios,
            backup,
            smpc,
            ..Self::default()
        };
        self.set_pal(pal);
        if let Some(media) = media {
            self.cd.mount(media);
        }
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

    /// Unpopulated CPU/A-bus selects on the standard console (ST-097 §1.2,
    /// fig1.3). With no responding device, use pulled-up data pins. Writes
    /// still occupy bus cycles but cannot change storage. This deterministic
    /// policy is confined to absent devices; implemented-chip register holes
    /// continue to report unsupported behavior instead of silently succeeding.
    fn unconnected(address: u32) -> bool {
        (0x0040_0000..0x0100_0000).contains(&address)
            || (0x0100_0004..0x0180_0000).contains(&address)
            || (0x0180_0004..0x0580_0000).contains(&address)
            || (0x0590_0000..0x05A0_0000).contains(&address)
    }

    fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        let address = Self::physical(address);
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        if Self::unconnected(address)
            || (0x0100_0000..0x0100_0004).contains(&address)
            || (0x0180_0000..0x0180_0004).contains(&address)
        {
            return Ok([255; N]);
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
        if (SOUND_RAM..SCSP).contains(&address) {
            let offset = (address - SOUND_RAM) as usize;
            let mut bytes = [0; N];
            for (i, byte) in bytes.iter_mut().enumerate() {
                *byte = self.sound.read_ram8(offset + i);
            }
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
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_SELECT).contains(&address) {
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
        if (vdp1::FRAMEBUFFER..vdp1::REGISTERS + 0x18).contains(&address)
            || (VRAM..VDP2 + 0x120).contains(&address)
        {
            self.flush_video();
        }
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        if Self::unconnected(address) {
            return Ok(());
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
        if (0x0100_0000..0x0100_0004).contains(&address)
            || (0x0180_0000..0x0180_0004).contains(&address)
        {
            if N != 2 && N != 4 {
                return Err(BusError::UnsupportedDevice { address, width: N });
            }
            let target = usize::from(address < 0x0180_0000);
            if target == 0 || self.slave_enabled {
                self.onchip[target].capture_strobe();
            }
            return Ok(());
        }
        if (SOUND_RAM..SCSP).contains(&address) {
            let offset = (address - SOUND_RAM) as usize;
            for (i, byte) in bytes.into_iter().enumerate() {
                self.sound.write_ram8(offset + i, byte);
            }
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
        if (BACKUP_RAM..BACKUP_RAM + BACKUP_SELECT).contains(&address) {
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
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                let target = address + i as u32 * 2;
                Vdp2::validate_value(target, u16::from_be_bytes(*pair))?;
            }
            for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
                let target = address + i as u32 * 2;
                let value = u16::from_be_bytes([pair[0], pair[1]]);
                self.vdp2.write16(target, value)?;
                if target == VDP2 {
                    self.raster.configure(value);
                }
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
    fn external_read<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        self.external_read_group(address, true, true, false)
    }
    fn external_read_group<const N: usize>(
        &self,
        address: u32,
        first: bool,
        last: bool,
        locked: bool,
    ) -> Result<[u8; N], BusError> {
        let chip = &self.bus.onchip[self.processor as usize];
        let bus_width = chip.external_bus_width(Bus::physical(address));
        let little = chip.external_little_endian(Bus::physical(address));
        // H §7.3.2 figs.7.5/7.6: addresses/byte strobes do not change.
        // Reverse bytes within the operand; a longword on a word-wide bus
        // transfers its low word first. Byte operands are unchanged.
        let width = bus_width.min(N);
        let mut result = [0; N];
        for (i, bytes) in result.chunks_mut(width).enumerate() {
            let at = address + (i * width) as u32;
            let first = first && i == 0;
            let last = last && (i + 1) * width == N;
            if self.scheduled {
                let value = self.scheduled_transfer(transactions::Transfer {
                    address: at,
                    width,
                    write: None,
                    first,
                    last,
                    unit_end: address + N as u32,
                    locked,
                })?;
                bytes.copy_from_slice(&value.to_be_bytes()[4 - width..]);
            } else {
                match width {
                    1 => bytes.copy_from_slice(&self.bus.read::<1>(at)?),
                    2 => bytes.copy_from_slice(&self.bus.read::<2>(at)?),
                    _ => bytes.copy_from_slice(&self.bus.read::<4>(at)?),
                }
                let value = bytes.iter().fold(0u32, |v, b| v << 8 | u32::from(*b));
                self.observe_external(at, width, value, false, first, last);
            }
        }
        if little {
            result.reverse();
        }
        Ok(result)
    }
    fn external_write<const N: usize>(
        &mut self,
        address: u32,
        mut bytes: [u8; N],
    ) -> Result<(), BusError> {
        let chip = &self.bus.onchip[self.processor as usize];
        let bus_width = chip.external_bus_width(Bus::physical(address));
        let little = chip.external_little_endian(Bus::physical(address));
        let width = bus_width.min(N);
        if little {
            bytes.reverse();
        }
        // TAS keeps the external bus until its write completes. Ordinary CPU
        // and DMAC writes use the SH7604 BSC's single-entry buffer (§7.11.2).
        if self.scheduled && !self.buffered_write_locked() {
            return self.buffered_external_write(address, bytes);
        }
        for (i, bytes) in bytes.chunks(width).enumerate() {
            let at = address + (i * width) as u32;
            let value = bytes.iter().fold(0u32, |v, b| v << 8 | u32::from(*b));
            if self.scheduled {
                self.scheduled_external(
                    at,
                    width,
                    Some(value),
                    i == 0,
                    (i + 1) * width == N,
                    false,
                )?;
                continue;
            }
            let result = match width {
                1 => self.bus.write(at, [bytes[0]]),
                2 => self.bus.write(at, [bytes[0], bytes[1]]),
                _ => self.bus.write(at, [bytes[0], bytes[1], bytes[2], bytes[3]]),
            };
            // The ROM has no write-enable pin. Keep the checked host loader's
            // ReadOnly error, but a real CPU write completes without changing
            // ROM or manufacturing an SH-2 bus exception.
            match result {
                Ok(()) | Err(BusError::ReadOnly { .. }) => (),
                Err(error) => return Err(error),
            }
            let value = bytes.iter().fold(0u32, |v, b| v << 8 | u32::from(*b));
            self.observe_external(at, width, value, true, i == 0, (i + 1) * width == N);
        }
        Ok(())
    }
    fn observe(&self, address: u32, width: usize, value: u32, write: bool) {
        let index = self.processor as usize;
        self.bus.onchip[index].observe_access(address, width, value, write, self.cached, false);
    }
    fn observe_external(
        &self,
        address: u32,
        width: usize,
        value: u32,
        write: bool,
        first: bool,
        last: bool,
    ) {
        self.reserve_external(address, width, write, first, last);
        self.observe_external_data(address, width, value, write);
    }
    fn reserve_external(&self, address: u32, width: usize, write: bool, first: bool, last: bool) {
        if (0xFFFF8000..0xFFFFC000).contains(&address) && write && width == 2 {
            let i = self.processor as usize;
            let states = self.bus.onchip[i]
                .sdram_mode_states(address)
                .expect("validated MRS");
            self.bus.claim_external_bus(self.processor);
            self.bus.cpu_bus_clocks[i].set(self.bus.cpu_bus_clocks[i].get().saturating_add(states));
        } else if address < 0x4000_0000 {
            let shared = self.bus.onchip[self.processor as usize]
                .shared_external_access(Bus::physical(address));
            if shared {
                self.bus.claim_external_bus(self.processor);
            }
            // H fig.7.7: ordinary external beats occupy T1/T2 even with no
            // software waits. Keep those clocks separate from Tw and internal
            // execution states; cache hits and on-chip accesses do not enter.
            let clocks = &self.bus.cpu_bus_clocks[self.processor as usize];
            let base = self.bus.onchip[self.processor as usize].external_access(
                // Address decoding may mirror the RAM backing store, but
                // BSC row/bank comparators see the actual A26-A0 pins.
                address & 0x1FFF_FFFF,
                write,
                first,
                last,
            );
            clocks.set(clocks.get().saturating_add(base));
            self.bus.onchip[self.processor as usize].add_external_wait_clocks(
                self.bus
                    .scu
                    .abus_access(address & 0x1FFF_FFFF, width, write),
            );
            let physical = Bus::physical(address);
            if (VRAM..VRAM + 0x80000).contains(&physical) {
                let slots = self.bus.vdp2.cpu_access_slots(physical);
                self.bus.onchip[self.processor as usize].add_external_wait_clocks(
                    self.bus.raster.vram_wait_clocks(
                        slots,
                        self.bus.onchip[self.processor as usize].pending_wait_clocks(),
                    ),
                );
            }
        }
    }
    fn observe_external_data(&self, address: u32, width: usize, value: u32, write: bool) {
        if address < 0x4000_0000
            && self.bus.onchip[self.processor as usize]
                .shared_external_access(Bus::physical(address))
        {
            self.bus.onchip[1 - self.processor as usize].observe_access(
                Bus::physical(address),
                width,
                value,
                write,
                false,
                true,
            );
        }
    }
    fn read<const N: usize>(&self, address: u32, instruction: bool) -> Result<[u8; N], BusError> {
        if !address.is_multiple_of(N as u32) {
            return Err(BusError::Unaligned { address, width: N });
        }
        if self.cached {
            let mut cache = self.bus.cache[self.processor as usize].borrow_mut();
            if let Some(bytes) = cache.array_read(address)? {
                return Ok(bytes);
            }
            let mut index = 0;
            if let Some(bytes) = cache.read(address, instruction, |beat| {
                index += 1;
                self.external_read_group::<4>(beat, index == 1, index == 4, false)
            })? {
                return Ok(bytes);
            }
        }
        self.external_read::<N>(address)
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
        self.external_write(address, bytes)?;
        if self.cached {
            self.bus.cache[index].get_mut().written(address, bytes);
        }
        Ok(())
    }
}
impl Memory for CpuBus<'_> {
    fn sh2_discard_prefetch(&self) {
        self.bus.instruction_word[self.processor as usize].set(None);
    }
    fn break_pending(&self) -> bool {
        self.bus.onchip[self.processor as usize].interrupt() == Some((15, 12))
    }
    fn enter_sleep(&mut self) {
        self.bus.onchip[self.processor as usize].enter_sleep();
    }
    fn instruction_break(&self, pc: u32, after: bool) -> bool {
        self.bus.onchip[self.processor as usize].instruction_break(pc, after)
    }
    fn acknowledge_break(&self) {
        self.bus.onchip[self.processor as usize].acknowledge_break();
    }
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address == CCR && self.cached {
            return Ok(self.bus.cache[self.processor as usize].borrow().control());
        }
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 1 });
            }
            if self.scheduled {
                return self
                    .scheduled_external(address, 1, None, true, true, false)
                    .map(|v| v as u8);
            }
            let value = self.bus.onchip[self.processor as usize].read8(address)?;
            self.observe(address, 1, u32::from(value), false);
            return Ok(value);
        }
        let value = self.read::<1>(address, false)?[0];
        self.observe(address, 1, u32::from(value), false);
        Ok(value)
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 2 });
            }
            if self.scheduled {
                return self
                    .scheduled_external(address, 2, None, true, true, false)
                    .map(|v| v as u16);
            }
            let value = self.bus.onchip[self.processor as usize].read16(address)?;
            self.observe(address, 2, u32::from(value), false);
            return Ok(value);
        }
        let value = u16::from_be_bytes(self.read(address, false)?);
        self.observe(address, 2, u32::from(value), false);
        Ok(value)
    }
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        let retained = &self.bus.instruction_word[self.processor as usize];
        if let Some((next, word)) = retained.get()
            && next == address
        {
            retained.set(None);
            return Ok(word);
        }
        let words: [u8; 4] = self.read(address & !3, true)?;
        self.bus.onchip[self.processor as usize].instruction_fetch(address & !3);
        if address & 2 == 0 {
            retained.set(Some((
                address + 2,
                u16::from_be_bytes([words[2], words[3]]),
            )));
            Ok(u16::from_be_bytes([words[0], words[1]]))
        } else {
            retained.set(None);
            Ok(u16::from_be_bytes([words[2], words[3]]))
        }
    }
    fn tas_read8(&self, address: u32) -> Result<u8, BusError> {
        let value = self.external_read_group::<1>(address, true, true, true)?[0];
        self.observe(address, 1, u32::from(value), false);
        Ok(value)
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 4 });
            }
            if self.scheduled {
                return self.scheduled_external(address, 4, None, true, true, false);
            }
            let value = self.bus.onchip[self.processor as usize].read32(address)?;
            self.observe(address, 4, value, false);
            return Ok(value);
        }
        let value = u32::from_be_bytes(self.read(address, false)?);
        self.observe(address, 4, value, false);
        Ok(value)
    }
    fn dma_read32(&self, address: u32, first: bool, last: bool) -> Result<u32, BusError> {
        if address >= 0xFFFF_FE00 {
            return self.read32(address);
        }
        let value = u32::from_be_bytes(self.external_read_group(address, first, last, false)?);
        self.observe(address, 4, value, false);
        Ok(value)
    }
    fn write8(&mut self, address: u32, value: u8) -> Result<(), BusError> {
        if address == CCR && self.cached {
            self.bus.cache[self.processor as usize]
                .get_mut()
                .set_control(value);
            return Ok(());
        }
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 1 });
            }
            if self.scheduled {
                return self
                    .scheduled_external(address, 1, Some(u32::from(value)), true, true, false)
                    .map(|_| ());
            }
            self.bus.onchip[self.processor as usize].write8(address, value)?;
            self.observe(address, 1, u32::from(value), true);
            return Ok(());
        }
        self.write(address, [value])?;
        self.observe(address, 1, u32::from(value), true);
        Ok(())
    }
    fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if (0xFFFF8000..0xFFFFC000).contains(&address) {
            let i = self.processor as usize;
            let states = self.bus.onchip[i].sdram_mode_states(address)?;
            if self.scheduled {
                self.scheduled_external(address, 2, Some(u32::from(value)), true, true, false)?;
            } else {
                self.bus.onchip[i].write_sdram_mode(address)?;
                self.bus.cpu_bus_clocks[i]
                    .set(self.bus.cpu_bus_clocks[i].get().saturating_add(states));
            }
            self.observe(address, 2, u32::from(value), true);
            return Ok(());
        }
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 2 });
            }
            if self.scheduled {
                return self
                    .scheduled_external(address, 2, Some(u32::from(value)), true, true, false)
                    .map(|_| ());
            }
            self.bus.onchip[self.processor as usize].write16(address, value)?;
            self.observe(address, 2, u32::from(value), true);
            return Ok(());
        }
        self.write(address, value.to_be_bytes())?;
        self.observe(address, 2, u32::from(value), true);
        Ok(())
    }
    fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        if address >= 0xFFFF_FE00 {
            if !self.cached && address >= 0xFFFF_FF40 {
                return Err(BusError::UnsupportedDevice { address, width: 4 });
            }
            if self.scheduled {
                return self
                    .scheduled_external(address, 4, Some(value), true, true, false)
                    .map(|_| ());
            }
            self.bus.onchip[self.processor as usize].write32(address, value)?;
            self.observe(address, 4, value, true);
            return Ok(());
        }
        self.write(address, value.to_be_bytes())?;
        self.observe(address, 4, value, true);
        Ok(())
    }
}

#[cfg(test)]
mod scu_timing_tests {
    use super::*;
    #[test]
    fn cs2_endian_follows_manual_data_pins_and_byte_strobes_at_both_bus_widths() {
        // H figs.7.5/7.6. Expected physical bytes and operands are literals;
        // a roundtrip alone would miss a symmetric wrong-lane implementation.
        for select in [2, 3] {
            let mut b = Bus::default();
            b.write32(SOUND_RAM, 0x11223344).unwrap();
            b.for_cpu(Processor::Master)
                .write32(0xFFFF_FFE4, 0xA55A_00CC | select << 4)
                .unwrap();
            b.for_cpu(Processor::Master)
                .write32(0xFFFF_FFE0, 0xA55A_13F0)
                .unwrap();
            let address = SOUND_RAM | 0x20000000;
            assert_eq!(b.for_cpu(Processor::Master).read32(address), Ok(0x44332211));
            assert_eq!(b.for_cpu(Processor::Master).read16(address), Ok(0x2211));
            assert_eq!(b.for_cpu(Processor::Master).read16(address + 2), Ok(0x4433));
            for (i, byte) in [0x11, 0x22, 0x33, 0x44].into_iter().enumerate() {
                assert_eq!(
                    b.for_cpu(Processor::Master).read8(address + i as u32),
                    Ok(byte)
                );
            }
            b.for_cpu(Processor::Master)
                .write32(address, 0xA1B2C3D4)
                .unwrap();
            assert_eq!(b.read32(SOUND_RAM), Ok(0xD4C3B2A1));
            b.for_cpu(Processor::Master)
                .write16(address, 0x1234)
                .unwrap();
            assert_eq!(b.read32(SOUND_RAM), Ok(0x3412B2A1));
            b.for_cpu(Processor::Master).write8(address, 0x56).unwrap();
            assert_eq!(b.read32(SOUND_RAM), Ok(0x5612B2A1));
            // Per-CPU selection does not reconfigure the other CPU or SCSP RAM.
            assert_eq!(b.for_cpu(Processor::Slave).read32(address), Ok(0x5612B2A1));
            b.write32(HIGH_WRAM, 0x11223344).unwrap();
            assert_eq!(
                b.for_cpu(Processor::Master).read32(HIGH_WRAM | 0x20000000),
                Ok(0x11223344)
            );
        }
    }
    #[test]
    fn narrow_bsc_buses_split_data_refills_and_writes_into_actual_ordered_external_beats() {
        for (select, width, waits, refill_waits) in [(1, 1, 24, 96), (2, 2, 12, 48), (3, 4, 6, 24)]
        {
            let mut b = Bus::default();
            b.write32(HIGH_WRAM, 0x12345678).unwrap();
            b.for_cpu(Processor::Master)
                .write32(0xFFFF_FFE4, 0xA55A_003C | select << 6)
                .unwrap();
            assert_eq!(
                b.for_cpu(Processor::Master).read32(HIGH_WRAM | 0x20000000),
                Ok(0x12345678)
            );
            assert_eq!(b.take_wait_clocks(), waits);
            b.for_cpu(Processor::Master)
                .write32(HIGH_WRAM, 0xABCDEF12)
                .unwrap();
            assert_eq!(b.read32(HIGH_WRAM), Ok(0xABCDEF12));
            assert_eq!(b.take_wait_clocks(), waits + 2); // Read->write turnaround.
            b.for_cpu(Processor::Master).write8(CCR, 0x11).unwrap();
            b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap();
            assert_eq!(b.take_wait_clocks(), refill_waits);
            b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap();
            assert_eq!(b.take_wait_clocks(), 0);
            b.for_cpu(Processor::Master)
                .read16(HIGH_WRAM | 0x20000000)
                .unwrap();
            assert_eq!(b.take_wait_clocks(), if width == 1 { 12 } else { 6 });
            b.for_cpu(Processor::Master)
                .read8(HIGH_WRAM | 0x20000000)
                .unwrap();
            assert_eq!(b.take_wait_clocks(), 6);
            // A sequential external break at byte/word 0 then the next beat
            // proves splitting reaches the other CPU's physical bus monitor.
            if width < 4 {
                let chip = &mut b.onchip[1];
                chip.write32(0xFFFF_FF40, HIGH_WRAM).unwrap();
                chip.write32(0xFFFF_FF60, HIGH_WRAM + width).unwrap();
                chip.write16(0xFFFF_FF48, 0xA4).unwrap();
                chip.write16(0xFFFF_FF68, 0xA4).unwrap();
                chip.write16(0xFFFF_FF78, 0x3010).unwrap();
                chip.read16(0xFFFF_FF78).unwrap();
                b.for_cpu(Processor::Master)
                    .read32(HIGH_WRAM | 0x20000000)
                    .unwrap();
                assert_eq!(b.onchip[1].interrupt(), Some((15, 12)));
            }
        }
    }
    #[test]
    fn bsc_waits_apply_to_external_beats_and_tas_but_never_cache_hits_or_arrays() {
        let mut b = Bus::default();
        b.write32(HIGH_WRAM, 0x12345678).unwrap();
        b.for_cpu(Processor::Master).write8(CCR, 0x11).unwrap();
        assert_eq!(
            b.for_cpu(Processor::Master).read32(HIGH_WRAM),
            Ok(0x12345678)
        );
        assert_eq!(b.take_wait_clocks(), 24); // Four refill beats, six Tw each.
        b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap();
        b.for_cpu(Processor::Master).read32(0xC000_0000).unwrap();
        assert_eq!(b.take_wait_clocks(), 0);
        b.for_cpu(Processor::Master).tas_read8(HIGH_WRAM).unwrap();
        assert_eq!(b.take_wait_clocks(), 6);
        b.for_cpu(Processor::Master)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 6);
        b.for_cpu(Processor::Master).write32(HIGH_WRAM, 0).unwrap();
        assert_eq!(b.take_wait_clocks(), 8); // External write + read->write turnaround.
        b.for_cpu(Processor::Master)
            .write32(0xFFFF_FFE8, 0xA55A_AA40)
            .unwrap();
        b.for_cpu(Processor::Master)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 1);
        b.for_cpu(Processor::Slave)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 6); // Independent slave configuration.
        b.reset_volatile();
        b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap();
        assert_eq!(b.take_wait_clocks(), 6);
    }
    #[test]
    fn bsc_turnaround_tracks_physical_cpu_beats_cache_hits_and_empty_bus_clocks() {
        let mut b = Bus::default();
        b.for_cpu(Processor::Master)
            .write32(0xFFFF_FFE8, 0xA55A_2100)
            .unwrap();
        b.for_cpu(Processor::Slave)
            .write32(0xFFFF_FFE8, 0xA55A_0000)
            .unwrap();
        // Master CS2 read -> CS3 read uses IW2=2, not destination IW3=0.
        b.for_cpu(Processor::Master)
            .read16(SOUND_RAM | 0x20000000)
            .unwrap();
        b.for_cpu(Processor::Master)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 2);
        // An immediate read->write still takes one idle even with IW3=0.
        b.for_cpu(Processor::Master)
            .write32(HIGH_WRAM | 0x20000000, 0x12345678)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 1);
        b.for_cpu(Processor::Master)
            .read16(SOUND_RAM | 0x20000000)
            .unwrap();
        // The other CPU's first read has no preceding transaction, and the
        // ownership change ends the master's prior read (H §7.9 arbitration).
        b.for_cpu(Processor::Slave)
            .read16(SOUND_RAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 0);
        b.for_cpu(Processor::Master)
            .write32(HIGH_WRAM, 0x12345678)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 0);
        b.advance_clocks(100).unwrap();
        b.for_cpu(Processor::Master).write8(CCR, 0x11).unwrap();
        b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap(); // Refill.
        assert_eq!(b.take_wait_clocks(), 0);
        b.advance_clocks(100).unwrap();
        b.for_cpu(Processor::Master)
            .read16(SOUND_RAM | 0x20000000)
            .unwrap();
        b.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap(); // Cache hit.
        b.for_cpu(Processor::Master).read32(0xC0000000).unwrap(); // Cache array.
        // Neither hit changes the preceding CS2 read. Its IW2 still applies
        // to the actual write-through transaction, even on a cache hit.
        b.for_cpu(Processor::Master)
            .write32(HIGH_WRAM, 0xABCDEF12)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 2);
        assert_eq!(b.read32(HIGH_WRAM), Ok(0xABCDEF12));
        b.advance_clocks(100).unwrap();
        b.for_cpu(Processor::Master)
            .read16(SOUND_RAM | 0x20000000)
            .unwrap();
        b.advance_clocks(3).unwrap(); // T1/T2 plus one empty clock.
        b.for_cpu(Processor::Master)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(b.take_wait_clocks(), 1);
    }
    #[test]
    fn smpc_input_edges_latch_raster_counters_and_raise_pad_irqs_with_independent_enables() {
        // ST-169 §1.2/table1.5; ST-058 §2.5/tables2.3–2.4; SCU table2.1.
        let mut b = Bus::default();
        b.advance_clocks(1708 + 40).unwrap();
        assert_eq!(b.read16(VDP2 + 2), Ok(0)); // software latch
        assert_eq!(b.read32(VDP2 + 8), Ok(0x00140001));
        b.advance_clocks(80).unwrap();
        assert_eq!(b.read32(VDP2 + 8), Ok(0x00140001)); // held until another latch
        b.read16(VDP2 + 2).unwrap();
        assert_eq!(b.read32(VDP2 + 8), Ok(0x003C0001));
        b.write16(VDP2 + 2, 0x200).unwrap();
        b.write8(SMPC + 0x7D, 3).unwrap();
        b.write8(SMPC + 0x7F, 3).unwrap();
        b.advance_clocks(40).unwrap();
        b.set_peripheral_th(0, false);
        assert_eq!(b.read32(VDP2 + 8), Ok(0x00500001));
        assert_eq!(b.read16(VDP2 + 4), Ok(0x202));
        assert_eq!(b.read16(VDP2 + 4), Ok(2)); // read clears EXLTFG only
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x100, 0x100);
        b.scu.acknowledge(0x48);
        b.advance_clocks(40).unwrap();
        b.set_peripheral_th(0, false); // no second edge
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x100, 0);
        assert_eq!(b.read32(VDP2 + 8), Ok(0x00500001));
        b.write8(SMPC + 0x7B, 0x40).unwrap();
        b.set_peripheral_th(1, false); // output TH is not a latch input
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x100, 0);
        b.set_peripheral_th(1, true);
        b.write8(SMPC + 0x7B, 0).unwrap();
        b.write16(VDP2 + 2, 0).unwrap();
        b.set_peripheral_th(1, false); // PAD IRQ does not require VDP2 EXLTEN
        assert_eq!(b.read32(SCU + 0xA4).unwrap() & 0x100, 0x100);
        assert_eq!(b.read16(VDP2 + 4), Ok(2));
        // The physical TH input is visible on PDR even with all bits input.
        assert_eq!(b.read8(SMPC + 0x77).unwrap() & 0x40, 0);
        b.reset_volatile();
        assert_eq!(b.read32(VDP2 + 8), Ok(0));
        assert_eq!(b.read16(VDP2 + 4), Ok(2));
    }
    #[test]
    fn cache_refill_wrap_order_is_observable_by_the_other_cpus_sequential_ubc() {
        // H §8.4.1 fig.8.4 + §6.3: a read of offset8 refills C,0,4,8.
        // An external A=C then B=0 sequential break must fire. Ascending
        // line reads would observe B before A and fail to request the break.
        let mut bus = Bus::default();
        for i in 0..4 {
            bus.write32(HIGH_WRAM + i * 4, i + 10).unwrap();
        }
        let observer = &mut bus.onchip[1];
        observer.write32(0xFFFF_FF40, HIGH_WRAM + 12).unwrap();
        observer.write32(0xFFFF_FF60, HIGH_WRAM).unwrap();
        observer.write16(0xFFFF_FF48, 0xA4).unwrap();
        observer.write16(0xFFFF_FF68, 0xA4).unwrap();
        observer.write16(0xFFFF_FF78, 0x3010).unwrap();
        observer.read16(0xFFFF_FF78).unwrap(); // UBC configuration barrier
        bus.for_cpu(Processor::Master).write8(CCR, 0x11).unwrap();
        assert_eq!(bus.for_cpu(Processor::Master).read32(HIGH_WRAM + 8), Ok(12));
        assert_eq!(bus.onchip[1].interrupt(), Some((15, 12)));
        assert_eq!(bus.onchip[1].read16(0xFFFF_FF78), Ok(0x7050));
        bus.onchip[1].acknowledge_break();
        bus.onchip[1].write16(0xFFFF_FF78, 0x3010).unwrap();
        bus.onchip[1].read16(0xFFFF_FF78).unwrap();
        bus.for_cpu(Processor::Master)
            .read32(HIGH_WRAM + 8)
            .unwrap();
        assert_eq!(bus.onchip[1].interrupt(), None); // hit: no external beats
    }
    #[test]
    fn user_breaks_distinguish_cached_cpu_accesses_from_external_bus_transactions() {
        let mut bus = Bus::default();
        // H §6.3.4: external masters have no operand-size qualification.
        for index in 0..2 {
            bus.onchip[index].write32(0xFFFF_FF40, HIGH_WRAM).unwrap();
            bus.onchip[index]
                .write16(0xFFFF_FF48, if index == 0 { 0x67 } else { 0xA5 })
                .unwrap();
            bus.onchip[index].write16(0xFFFF_FF78, 0x3000).unwrap();
            bus.onchip[index].read16(0xFFFF_FF78).unwrap();
        }
        bus.write32(HIGH_WRAM, 0x12345678).unwrap();
        bus.for_cpu(Processor::Master).write8(CCR, 1).unwrap();
        assert_eq!(
            bus.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap(),
            0x12345678
        );
        assert_eq!(bus.onchip[0].interrupt(), Some((15, 12))); // CPU long read
        assert_eq!(bus.onchip[1].interrupt(), Some((15, 12))); // actual cache fill
        for chip in &bus.onchip {
            chip.acknowledge_break();
        }
        bus.for_cpu(Processor::Master).read32(HIGH_WRAM).unwrap();
        assert_eq!(bus.onchip[0].interrupt(), Some((15, 12)));
        assert_eq!(bus.onchip[1].interrupt(), None); // cached data has no external cycle
        bus.for_cpu(Processor::Master)
            .read32(HIGH_WRAM | 0x20000000)
            .unwrap();
        assert_eq!(bus.onchip[1].interrupt(), Some((15, 12)));
        bus.scu.write32(SCU + 0xA0, 0xBFFE).unwrap();
        bus.scu.signal(0); // VBlank-in asserts external level 15.
        assert_eq!(bus.interrupt(Processor::Master), Some((15, 12)));
        bus.acknowledge_interrupt(Processor::Master, (15, 12));
        assert_eq!(bus.interrupt(Processor::Master), Some((15, 71)));
    }
    #[test]
    fn external_scu_bus_and_local_dmac_have_separate_user_break_sources() {
        let mut bus = Bus::default();
        for chip in &mut bus.onchip {
            chip.write32(0xFFFF_FF60, HIGH_WRAM + 0x100).unwrap();
            chip.write16(0xFFFF_FF68, 0xA8).unwrap(); // peripheral/data/write
            chip.write16(0xFFFF_FF78, 0x3000).unwrap();
            chip.read16(0xFFFF_FF78).unwrap();
        }
        bus.write32(HIGH_WRAM, 0x12345678).unwrap();
        for (offset, value) in [
            (0, HIGH_WRAM),
            (4, HIGH_WRAM + 0x100),
            (8, 4),
            (12, 0x102),
            (20, 7),
            (16, 0x101),
        ] {
            bus.write32(SCU + offset, value).unwrap();
        }
        for _ in 0..4 {
            bus.advance_scu_dma().unwrap();
        }
        assert_eq!(bus.read32(HIGH_WRAM + 0x100).unwrap(), 0x12345678);
        for chip in &bus.onchip {
            assert_eq!(chip.interrupt(), Some((15, 12)));
            assert_eq!(chip.read16(0xFFFF_FF78).unwrap(), 0x3040);
        }
        // H §6.3.7: DMAC cannot program UBC registers as ordinary memory.
        let dma_view = CpuBus {
            bus: &mut bus,
            processor: Processor::Master,
            cached: false,
            scheduled: false,
        };
        assert!(dma_view.read32(0xFFFF_FF40).is_err());
    }
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
            assert_eq!(b.interrupt(Processor::Master), Some((2, 65)));
            assert_eq!(b.read16(vdp1::REGISTERS + 0x10).unwrap(), 2);
            // Setup2 + RAM read8 + read-to-write turnaround2 + write8.
            b.advance_clocks(19).unwrap();
            assert_eq!(b.read32(VRAM).unwrap(), 0);
            b.advance_clocks(1).unwrap();
            assert_eq!(b.read32(VRAM).unwrap(), 0x12340000);
            b.advance_clocks(8).unwrap();
            assert_eq!(b.read32(VRAM).unwrap(), 0x12345678);
            b.acknowledge_interrupt(Processor::Master, (2, 65));
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
    fn dsp_b_bus_halfwords_progress_on_separate_deadlines_and_reset_cancels_the_tail() {
        for chunk in [1, 3, 8, 17] {
            let mut b = Bus::default();
            b.write32(SCU + 0x88, 0).unwrap();
            b.write32(SCU + 0x8c, 0x11223344).unwrap();
            for word in [0x9d700400, 0xc0009001, 0xf8000000] {
                b.write32(SCU + 0x84, word).unwrap();
            }
            b.write32(SCU + 0x80, 0x18000).unwrap();
            let mut elapsed = 0;
            for (deadline, value, pending) in [
                (11, 0, true),
                (12, 0x11220000, true),
                (19, 0x11220000, true),
                (20, 0x11223344, false),
            ] {
                while elapsed < deadline {
                    let clocks = chunk.min(deadline - elapsed);
                    b.advance_clocks(clocks).unwrap();
                    elapsed += clocks;
                }
                assert_eq!(b.read32(vdp1::VRAM + 0x1000).unwrap(), value);
                assert_eq!(b.read32(SCU + 0x80).unwrap() & 0x800000 != 0, pending);
            }
            // Start again, reset after the first half, and prove the old low
            // half is not delivered into freshly reset video memory.
            b.write32(SCU + 0x88, 1).unwrap();
            b.write32(SCU + 0x8c, 0xdeadbeef).unwrap();
            b.write32(SCU + 0x80, 0x18000).unwrap();
            b.advance_clocks(12).unwrap();
            assert_eq!(b.read32(vdp1::VRAM + 0x1000).unwrap(), 0xdead3344);
            b.reset_volatile();
            b.advance_clocks(32).unwrap();
            assert_eq!(b.read32(vdp1::VRAM + 0x1000).unwrap(), 0);
            assert_eq!(b.read32(SCU + 0x80).unwrap() & 0x810000, 0);
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

#[cfg(test)]
mod system_clock_tests {
    use super::*;

    #[test]
    fn changed_pll_clocks_frt_and_raster_but_not_the_sample_rate() {
        for fast in [false, true] {
            for chunk in [3, 8, 607, 268741] {
                let mut bus = Bus::default();
                bus.change_system_clock(fast);
                let mut left = 268741; // 10 ms in the independent reference domain
                while left != 0 {
                    let n = chunk.min(left);
                    bus.advance_clocks(n).unwrap();
                    left -= n;
                }
                let counter = u16::from_be_bytes([
                    bus.read8(0xFFFF_FE12).unwrap(),
                    bus.read8(0xFFFF_FE13).unwrap(),
                ]);
                assert_eq!(counter, if fast { 35795 } else { 33592 });
                assert_eq!(bus.sound.scsp.generated, 441);
                assert_eq!(bus.raster.line, 157);
            }
        }
    }

    #[test]
    fn clock_reset_cancels_scu_work_and_slave_without_discarding_master_or_cd_domains() {
        let mut bus = Bus::default();
        bus.set_slave_enabled(true);
        bus.write8(CCR, 1).unwrap();
        bus.onchip[0].write8(0xFFFF_FE12, 0x34).unwrap();
        bus.onchip[0].write8(0xFFFF_FE13, 0x56).unwrap();
        bus.scu.write32(SCU + 0xA0, 0).unwrap();
        bus.scu.signal(11);
        assert!(bus.interrupt(Processor::Master).is_some());
        bus.write16(crate::cd::REGISTERS + 12, 0x56).unwrap();
        bus.cd
            .mount(crate::media::Media::iso(&vec![0x69; 2048]).unwrap());
        bus.write32(HIGH_WRAM, 0x12345678).unwrap();
        bus.write32(LOW_WRAM, 0x2468ACE0).unwrap();
        bus.write8(BACKUP_RAM + 1, 0xA5).unwrap();
        bus.vdp2.vram[0] = 0x69;
        bus.vdp1.vram[0] = 0x42;
        bus.sound.ram[0] = 0x53;
        bus.advance_clocks(crate::timing::CLOCKS_PER_FIELD as u32)
            .unwrap();
        let fields = bus.raster.fields;
        bus.change_system_clock(true);
        assert_eq!(bus.read8(CCR), Ok(1));
        assert_eq!(bus.read8(0xFFFF_FE12), Ok(0x0F)); // FRC retained and clocked
        assert_eq!(bus.read32(HIGH_WRAM), Ok(0x12345678));
        assert_eq!(bus.read32(LOW_WRAM), Ok(0x2468ACE0));
        assert_eq!(bus.read8(BACKUP_RAM + 1), Ok(0xA5));
        assert_eq!(bus.read16(crate::cd::REGISTERS + 12), Ok(0x56));
        assert_eq!(
            bus.cd.media().unwrap().cooked_sector(150).unwrap(),
            [0x69; 2048]
        );
        assert!(!bus.slave_enabled);
        assert_eq!(bus.cache[1].borrow().control(), 0);
        bus.scu.signal(11);
        assert_eq!(bus.scu.interrupt(), None); // reset masks DMA IRQ
        bus.scu.write32(SCU + 0xA0, 0).unwrap();
        assert_eq!(bus.scu.interrupt(), Some((5, 0x4B)));
        bus.scu.acknowledge(0x4B);
        assert_eq!(bus.vdp1.vram[0], 0);
        assert_eq!(bus.vdp2.vram[0], 0);
        assert_eq!(bus.sound.ram[0], 0);
        assert_eq!(bus.raster.fields, fields);
        assert_eq!(bus.raster.line, 0);
        assert_eq!(bus.raster.clocks_to_event(), 1408);
        assert_eq!(bus.interrupt(Processor::Master), None);
        bus.advance_clocks(1321).unwrap();
        assert!(!bus.vdp2.hblank);
        bus.advance_clocks(1).unwrap();
        assert!(bus.vdp2.hblank);
    }
}

#[cfg(test)]
mod pal_fixed_domain_tests {
    use super::*;
    #[test]
    fn pal_reference_ticks_without_main_edges_still_produce_every_audio_sample() {
        for chunk in [1, 3, 73, 1708, 10000] {
            let mut b = Bus::default();
            b.set_pal(true);
            let mut reference = 0u64;
            while reference < 100_000 {
                let n = chunk.min((100_000 - reference) as u32);
                b.advance_clocks(n).unwrap();
                reference += u64::from(n);
                assert_eq!(b.sound.scsp.generated, reference * 44_100 / 26_874_100);
            }
            b.reset_volatile();
            assert!(b.raster.pal);
            assert_eq!(b.read16(VDP2 + 4).unwrap() & 1, 1);
            b.system_reset();
            assert!(b.raster.pal);
            assert_eq!(b.read16(VDP2 + 4).unwrap() & 1, 1);
        }
    }
}

#[cfg(test)]
mod vram_grant_tests;

#[cfg(test)]
mod bsc_memory_tests {
    use super::*;
    #[test]
    fn cache_line_is_one_sdram_burst_and_hits_issue_no_external_cycles_on_either_cpu() {
        // H §§7.5.3/7.5.4, fig8.4: four longword refill callbacks
        // constitute one burst, not four single uncached reads.
        for processor in [Processor::Master, Processor::Slave] {
            for (mcr, expected) in [(0x40, 7), (0, 11)] {
                let mut b = Bus::default();
                let mut view = b.for_cpu(processor);
                view.write32(0xFFFFFFE8, 0xA55A0000).unwrap();
                view.write32(0xFFFFFFE0, 0xA55A0401).unwrap();
                view.write32(0xFFFFFFEC, 0xA55A0000 | mcr).unwrap();
                b.write32(HIGH_WRAM, 0x12345678).unwrap();
                assert_eq!(
                    b.for_cpu(processor).read32(HIGH_WRAM + 0x20000000),
                    Ok(0x12345678)
                );
                assert_eq!(b.cpu_delay(processor, 1), expected);
                b.for_cpu(processor).write8(CCR, 0x11).unwrap();
                assert_eq!(b.for_cpu(processor).read32(HIGH_WRAM), Ok(0x12345678));
                assert_eq!(b.cpu_delay(processor, 1), expected);
                assert_eq!(b.for_cpu(processor).read32(HIGH_WRAM), Ok(0x12345678));
                assert_eq!(b.cpu_delay(processor, 1), 1);
            }
        }
    }
    #[test]
    fn sdram_bank_comparators_see_actual_pins_before_work_ram_mirror_decode() {
        let mut b = Bus::default();
        let p = Processor::Master;
        let mut view = b.for_cpu(p);
        view.write32(0xFFFFFFE8, 0xA55A0000).unwrap();
        view.write32(0xFFFFFFE0, 0xA55A0401).unwrap();
        view.write32(0xFFFFFFEC, 0xA55A0240).unwrap();
        for (at, value, clocks) in [
            (0x26000000, 0x11111111, 2),
            (0x26000004, 0x22222222, 1),
            (0x26200000, 0x33333333, 2),
            (0x26000008, 0x44444444, 1),
            (0x26000400, 0x55555555, 3),
        ] {
            b.for_cpu(p).write32(at, value).unwrap();
            assert_eq!(b.cpu_delay(p, 1), clocks, "{at:X}");
        }
        assert_eq!(b.read32(HIGH_WRAM), Ok(0x33333333)); // same actual RAM chip
        b.onchip[0].reset_from_watchdog(crate::onchip::ResetRequest::Manual);
        assert_eq!(b.for_cpu(p).read32(0xFFFFFFEC), Ok(0x240));
        b.for_cpu(p).write32(0x26000404, 0x66666666).unwrap();
        assert_eq!(b.cpu_delay(p, 1), 1); // H §7.11.1: retained mode and open row
    }
    #[test]
    fn indirect_dma_table_reads_have_separate_deadlines_and_reset_cancels_a_partial_descriptor() {
        // ST-210 #25 and ST-097 §3.2: descriptor count/write/read each
        // owns a grant. No source/destination transaction precedes them.
        for chunk in [1, 3, 8, 17] {
            let mut b = Bus::default();
            for (i, v) in [4, HIGH_WRAM + 0x200, 0x80000000 | (HIGH_WRAM + 0x100)]
                .into_iter()
                .enumerate()
            {
                b.write32(HIGH_WRAM + i as u32 * 4, v).unwrap();
            }
            b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
            for (off, value) in [(4, HIGH_WRAM), (12, 0x102), (20, 0x01000107), (16, 0x101)] {
                b.write32(SCU + off, value).unwrap();
            }
            let mut elapsed = 0;
            // One SCU setup clock (two main), five eight-main-clock data
            // phases, plus IW3's two read-to-write turnaround clocks.
            for deadline in [2, 10, 18, 26, 34, 43, 44] {
                while elapsed < deadline {
                    let n = chunk.min(deadline - elapsed);
                    b.advance_clocks(n).unwrap();
                    elapsed += n;
                }
                assert_eq!(
                    b.read32(HIGH_WRAM + 0x200).unwrap(),
                    if deadline == 44 { 0x12345678 } else { 0 }
                );
                assert_eq!(
                    b.scu.read32(SCU + 0xA4).unwrap() & 0x800,
                    if deadline == 44 { 0x800 } else { 0 }
                );
            }
            b.write32(SCU + 4, HIGH_WRAM).unwrap();
            b.write32(SCU + 16, 0x101).unwrap();
            b.advance_clocks(16).unwrap(); // one descriptor word latched
            b.reset_volatile();
            b.advance_clocks(100).unwrap();
            assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0));
            assert_eq!(b.scu.read32(SCU + 0xA4).unwrap(), 0);
        }
    }
}

#[cfg(test)]
mod raster_write_tests {
    use super::*;
    fn finish(bus: &mut Bus) -> Vec<u8> {
        while !bus.vdp2.vblank {
            bus.advance_clocks(bus.raster.clocks_to_event()).unwrap();
        }
        bus.vdp2.scanout(&bus.vdp1).unwrap()
    }
    #[test]
    fn line_start_back_color_fetch_is_held_until_the_next_line_at_every_density() {
        // ST-058 §§2.1/2.4/7.2. No instruction timing assumptions: the bus
        // writes take place at literal main-clock boundaries.
        for (mode, width, period) in [
            (0x8000, 320, 1280),
            (0x8002, 640, 1280),
            (0x8004, 320, 640),
            (0x8006, 640, 640),
        ] {
            for split in [1, 17, 63, 127] {
                let mut b = Bus::default();
                b.write16(VDP2, mode).unwrap();
                b.write32(VDP2 + 0xAC, 0x00030000).unwrap();
                b.write16(VRAM + 0x60000, 0x801F).unwrap();
                b.advance_clocks(split * period / width).unwrap();
                b.write16(VRAM + 0x60000, 0x83E0).unwrap();
                let frame = finish(&mut b);
                for x in 0..width {
                    assert_eq!(
                        &frame[x as usize * 4..][..4],
                        [255, 0, 0, 255],
                        "mode={mode:X}, split={split}, x={x}"
                    );
                }
                assert_eq!(&frame[width as usize * 4..][..4], [0, 255, 0, 255]);
            }
        }
    }
    #[test]
    fn palette_vram_and_color_offset_changes_cannot_repaint_the_scanned_prefix() {
        // ST-058 §§3.4/4.2/13.1: independent live palette/RAM/offset changes.
        let mut b = Bus::default();
        for (offset, value) in [
            (0, 0x8000),
            (0x10, 0x44FF),
            (0x12, 0xEEEE),
            (0x20, 0x101),
            (0x28, 0x12),
            (0x78, 1),
            (0x7C, 1),
            (0xF8, 1),
        ] {
            b.write16(VDP2 + offset, value).unwrap();
        }
        for x in 0..320 {
            b.write8(VRAM + x, 1).unwrap();
        }
        b.write16(crate::vdp2::CRAM + 2, 0x801F).unwrap();
        b.advance_clocks(256).unwrap(); // first 64 dots
        b.write16(crate::vdp2::CRAM + 2, 0x83E0).unwrap();
        b.advance_clocks(256).unwrap();
        b.write16(VDP2 + 0x110, 1).unwrap();
        b.write16(VDP2 + 0x118, 31).unwrap(); // +blue on NBG0
        b.advance_clocks(256).unwrap();
        for x in 192..320 {
            b.write8(VRAM + x, 0).unwrap();
        }
        let frame = finish(&mut b);
        for x in 0..320 {
            let expected = match x {
                0..64 => [255, 0, 0, 255],
                64..128 => [0, 255, 0, 255],
                128..192 => [0, 255, 31, 255],
                _ => [0, 0, 31, 255],
            };
            assert_eq!(&frame[x * 4..][..4], expected, "x={x}");
        }
    }
}

#[cfg(test)]
mod abus_dma_tests {
    use super::*;
    #[test]
    fn a_bus_waits_delay_scu_data_publication_and_reset_cancels_the_next_beat() {
        // ST-097 §3.2 and H §7.4: setup2, CS0 read8+NW6, then
        // RAM write8+IW2. Later reads/writes repeat14+10. Publication is
        // at the END of each data phase (26/50), never before its waits.
        for chunk in [1, 3, 8, 17, 64] {
            let mut b = Bus::default();
            b.write32(SCU + 0xB0, 0x00300000).unwrap();
            for (off, value) in [
                (0, 0x02000000),
                (4, HIGH_WRAM),
                (8, 8),
                (12, 0x102),
                (20, 7),
                (16, 0x101),
            ] {
                b.write32(SCU + off, value).unwrap();
            }
            let mut elapsed = 0;
            for (deadline, first, second) in [
                (25, 0, 0),
                (26, u32::MAX, 0),
                (49, u32::MAX, 0),
                (50, u32::MAX, u32::MAX),
            ] {
                while elapsed < deadline {
                    let n = (deadline - elapsed).min(chunk);
                    b.advance_clocks(n).unwrap();
                    elapsed += n;
                }
                assert_eq!(
                    b.read32(HIGH_WRAM),
                    Ok(first),
                    "chunk={chunk},time={deadline}"
                );
                assert_eq!(b.read32(HIGH_WRAM + 4), Ok(second));
            }
            b.write32(SCU + 16, 0x101).unwrap();
            b.advance_clocks(8).unwrap();
            b.system_reset();
            b.write32(HIGH_WRAM, 0x12345678).unwrap();
            b.advance_clocks(100).unwrap();
            assert_eq!(b.read32(HIGH_WRAM), Ok(0x12345678));
        }
    }
}
