//! Browser-independent Saturn functional subset with independent SH-2 processors.
pub mod backup;
pub mod bus;
pub mod cache;
pub mod cd;
mod cd_audio;
pub mod cpu;
pub mod devices;
pub mod disc;
mod hle;
pub mod m68k;
pub mod media;
pub mod onchip;
pub mod scu;
pub mod smpc;
pub mod sound;
pub mod timing;
pub mod vdp1;
pub mod vdp2;
#[cfg(target_arch = "wasm32")]
mod wasm;

use bus::{Bus, BusError, HIGH_WRAM, Memory, Processor, WRAM_SIZE};
use cpu::{Cpu, CpuError, CpuState};
use std::fmt;

pub const MAX_RUN_STEPS: u32 = 10_000;
/// Fixed peripheral-clock quantum per interpreter turn. This is a deterministic
/// functional interleave, NOT measured SH-2 instruction timing or bus arbitration.
pub const CLOCKS_PER_STEP: u32 = 8;
pub const FIXTURE_PC: u32 = HIGH_WRAM;
// Original homebrew: MOV #5,R0; ADD #-1,R0; MOV R0,R1; BRA +1;
// ADD #3,R1 (slot); MOV #99,R1 (skipped); NOP; BRA -2; NOP (slot).
pub const FIXTURE: [u16; 9] = [
    0xE005, 0x70FF, 0x6103, 0xA001, 0x7103, 0xE163, 0x0009, 0xAFFE, 0x0009,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    HlePrototype,
    ExternalBios,
    HleDisc,
}

pub enum Boot<'a> {
    HlePrototype,
    ExternalBios(&'a [u8]),
    HleDisc(&'a [u8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineError {
    Bus(BusError),
    Cpu(CpuError),
    SlaveCpu(CpuError),
    StepLimit(u32),
    Disc(disc::DiscError),
    Video(String),
    Scu(scu::ScuError),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => error.fmt(f),
            Self::Cpu(error) => error.fmt(f),
            Self::SlaveCpu(error) => write!(f, "Slave SH-2: {error}"),
            Self::Disc(error) => error.fmt(f),
            Self::Video(error) => f.write_str(error),
            Self::Scu(error) => error.fmt(f),
            Self::StepLimit(n) => write!(f, "Run count {n} is outside 1..={MAX_RUN_STEPS}"),
        }
    }
}
impl std::error::Error for MachineError {}
impl From<BusError> for MachineError {
    fn from(e: BusError) -> Self {
        Self::Bus(e)
    }
}
impl From<CpuError> for MachineError {
    fn from(e: CpuError) -> Self {
        Self::Cpu(e)
    }
}

#[derive(Default)]
pub struct Machine {
    cpu: Cpu,
    slave: Cpu,
    bus: Bus,
    mode: Option<BootMode>,
    clocks: u64,
    nmi_pending: bool,
    dma_errors: [bool; 2],
    fault: Option<MachineError>,
}

impl Machine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cpu_state(&self) -> &CpuState {
        self.cpu.state()
    }
    pub fn slave_state(&self) -> &CpuState {
        self.slave.state()
    }
    pub fn clocks(&self) -> u64 {
        self.clocks
    }
    pub fn fault(&self) -> Option<&MachineError> {
        self.fault.as_ref()
    }
    pub fn reset_in_progress(&self) -> bool {
        self.bus.cpu_reset_active(Processor::Master)
    }
    pub fn can_wake_master(&self) -> bool {
        self.nmi_pending || self.dma_errors[0] || self.bus.can_wake_master()
    }
    pub fn mode(&self) -> Option<BootMode> {
        self.mode
    }
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Cold reset clears volatile RAM, prior ROM, counters, delay state and faults.
    /// Internal battery-backed RAM survives all boot selections and failed boots.
    /// A rejected BIOS leaves ExternalBios selected and the CPU stopped in Reset.
    pub fn reset(&mut self, boot: Boot<'_>) -> Result<(), MachineError> {
        self.clocks = 0;
        self.nmi_pending = false;
        self.dma_errors = [false; 2];
        self.fault = None;
        self.cpu = Cpu::default();
        self.slave = Cpu::default();
        self.bus.reset_volatile();
        match boot {
            Boot::ExternalBios(bytes) => {
                self.mode = Some(BootMode::ExternalBios);
                self.bus.load_bios(bytes)?;
                self.cpu = Cpu::power_on(&self.bus)?;
            }
            Boot::HleDisc(image) => {
                self.mode = Some(BootMode::HleDisc);
                self.boot_disc(image)?;
            }
            Boot::HlePrototype => {
                self.mode = Some(BootMode::HlePrototype);
                for (i, opcode) in FIXTURE.iter().enumerate() {
                    self.bus.write16(FIXTURE_PC + (i as u32) * 2, *opcode)?;
                }
                self.cpu = Cpu::start(FIXTURE_PC, HIGH_WRAM + WRAM_SIZE as u32);
            }
        }
        Ok(())
    }

    /// HLE bootstrap shared by cold startup and guest SYSRES. Hardware reset
    /// has already selected the reset domain; this only reloads IP/executable.
    fn boot_disc(&mut self, image: &[u8]) -> Result<(), MachineError> {
        let boot = disc::parse(image).map_err(MachineError::Disc)?;
        for (i, byte) in boot.ip.iter().enumerate() {
            self.bus.write8(disc::IP_ADDRESS + i as u32, *byte)?;
        }
        for (i, byte) in boot.executable.iter().enumerate() {
            self.bus.write8(boot.load_address + i as u32, *byte)?;
        }
        // Original HLE reset and system routines are never installed in BIOS
        // mode. Master and slave execute the same real reset trampoline.
        hle::install(&mut self.bus)?;
        self.cpu = Cpu::start(0x100, boot.stack);
        self.bus
            .cd
            .mount(media::Media::iso(image).map_err(MachineError::Disc)?);
        Ok(())
    }

    /// Boot caller-resolved ISO/BIN/CUE media through the same independent HLE.
    /// No file reference is opened by the emulator. Invalid input stops the old
    /// machine while preserving battery RAM, as with the single-file loader.
    pub fn reset_media(&mut self, media: media::Media) -> Result<(), MachineError> {
        let image = match media.boot_iso() {
            Ok(image) => image,
            Err(error) => {
                let _ = self.reset(Boot::HleDisc(&[]));
                return Err(MachineError::Disc(error));
            }
        };
        self.reset(Boot::HleDisc(&image))?;
        self.bus.cd.mount(media);
        Ok(())
    }

    /// Attach local media to an external BIOS without replacing its CPU/reset
    /// state or invoking HLE. Also useful for synthetic boot ROM acceptance.
    pub fn mount_media(&mut self, media: media::Media) {
        self.bus.cd.mount(media);
    }

    pub fn sound_state(&self) -> &m68k::State {
        &self.bus.sound.cpu.state
    }
    pub fn drain_audio(&mut self, frames: usize) -> Vec<i16> {
        self.bus.sound.scsp.drain(frames)
    }
    pub fn audio_generated(&self) -> u64 {
        self.bus.sound.scsp.generated
    }
    pub fn audio_dropped(&self) -> u64 {
        self.bus.sound.scsp.dropped
    }

    pub fn set_reset_button(&mut self, pressed: bool) {
        if self.bus.set_reset_button(pressed) {
            self.nmi_pending = true;
            self.bus.signal_nmi(Processor::Master);
        }
    }
    pub fn set_analog(&mut self, port: usize, axes: Option<[u8; 3]>) {
        self.bus.set_analog(port, axes);
    }
    pub fn set_3d_pad(&mut self, port: usize, axes: Option<[u8; 4]>) {
        self.bus.set_3d_pad(port, axes);
    }

    /// Peripheral port electrical TH input; it never writes guest RAM or CPU state.
    pub fn set_peripheral_th(&mut self, port: usize, high: bool) {
        self.bus.set_peripheral_th(port, high);
    }
    pub fn set_buttons(&mut self, buttons: u16) {
        self.bus.set_buttons(buttons);
    }

    pub fn set_port2_buttons(&mut self, buttons: u16) {
        self.bus.set_port2_buttons(buttons);
    }

    pub fn backup_ram(&self) -> &[u8] {
        self.bus.backup.bytes()
    }

    pub fn import_backup_ram(&mut self, bytes: &[u8]) -> Result<(), MachineError> {
        self.bus.backup.import(bytes)?;
        Ok(())
    }

    pub fn frames(&self) -> u64 {
        self.bus.raster.fields
    }

    pub fn framebuffer(&self) -> Result<Vec<u8>, MachineError> {
        self.bus
            .vdp2
            .composite(&self.bus.vdp1)
            .map_err(MachineError::Video)
    }

    pub fn video_dimensions(&self) -> (usize, usize) {
        self.bus.vdp2.dimensions()
    }

    /// Advance to the next raster field deadline. Peripherals and video events
    /// progress during SLEEP and DIVU stalls; execution stays bounded for the UI.
    /// CPU issue still uses a fixed quantum; this does not claim opcode/bus timing.
    pub fn advance_frame(&mut self) -> Result<(), MachineError> {
        let target = self.frames() + 1;
        for _ in 0..100_000 {
            self.step()?;
            if self.frames() >= target {
                return Ok(());
            }
        }
        Err(MachineError::StepLimit(100_000))
    }

    pub fn step(&mut self) -> Result<(), MachineError> {
        if let Some(error) = &self.fault {
            return Err(error.clone());
        }
        let result = self.step_inner();
        if let Err(error) = &result {
            self.fault = Some(error.clone());
        }
        result
    }

    fn step_inner(&mut self) -> Result<(), MachineError> {
        // Fixed ordering: master boundary, slave boundary, peripheral clocks,
        // then SMPC completion. Shared writes are visible to the next CPU turn.
        if self.bus.cpu_reset_active(Processor::Master) {
            self.cpu = Cpu::default();
        } else {
            let nmi_ready = self.bus.nmi_ready(Processor::Master);
            let mut memory = self.bus.for_cpu(Processor::Master);
            let dma_accepted =
                self.dma_errors[0] && self.cpu.accept_dma_address_error(&mut memory)?;
            if dma_accepted {
                self.dma_errors[0] = false;
            }
            let nmi_accepted = !dma_accepted
                && self.nmi_pending
                && nmi_ready
                && self.cpu.accept_nmi(&mut memory)?;
            if nmi_accepted {
                self.nmi_pending = false;
            }
            let irq = self.bus.interrupt(Processor::Master);
            let mut memory = self.bus.for_cpu(Processor::Master);
            let irq_accepted = !dma_accepted
                && !nmi_accepted
                && if let Some((level, vector)) = irq {
                    self.cpu.accept_interrupt(&mut memory, level, vector)?
                } else {
                    false
                };
            if irq_accepted {
                self.bus
                    .acknowledge_interrupt(Processor::Master, irq.unwrap());
            }
            let mut memory = self.bus.for_cpu(Processor::Master);
            if !dma_accepted && !nmi_accepted && !irq_accepted {
                self.cpu.step(&mut memory)?;
            }
        }
        if self.bus.slave_enabled {
            if self.bus.cpu_reset_active(Processor::Slave) {
                self.slave = Cpu::default();
            } else {
                let irq = self.bus.interrupt(Processor::Slave);
                let mut memory = self.bus.for_cpu(Processor::Slave);
                let dma_accepted = self.dma_errors[1]
                    && self
                        .slave
                        .accept_dma_address_error(&mut memory)
                        .map_err(MachineError::SlaveCpu)?;
                if dma_accepted {
                    self.dma_errors[1] = false;
                }
                let accepted = dma_accepted
                    || if let Some((level, vector)) = irq {
                        self.slave
                            .accept_interrupt(&mut memory, level, vector)
                            .map_err(MachineError::SlaveCpu)?
                    } else {
                        false
                    };
                if !accepted {
                    self.slave
                        .step(&mut memory)
                        .map_err(MachineError::SlaveCpu)?;
                }
            }
        }
        self.dma_errors[0] |= self.bus.advance_dma(Processor::Master);
        if self.bus.slave_enabled {
            self.dma_errors[1] |= self.bus.advance_dma(Processor::Slave);
        }
        let elapsed = CLOCKS_PER_STEP.saturating_add(self.bus.take_wait_clocks());
        self.clocks += u64::from(elapsed);
        match self
            .bus
            .advance_clocks(elapsed)
            .map_err(MachineError::Scu)?
        {
            Some(0x02) if !self.bus.slave_enabled => {
                self.slave = Cpu::power_on(&self.bus)
                    .map_err(|source| MachineError::SlaveCpu(CpuError::Bus { pc: 0, source }))?;
                self.bus.set_slave_enabled(true);
            }
            Some(0x03) => {
                self.bus.set_slave_enabled(false);
                self.slave = Cpu::default();
            }
            Some(0x06) => self.bus.sound.set_enabled(true)?,
            Some(0x07) => self.bus.sound.set_enabled(false)?,
            Some(0x08) => self.bus.cd.set_enabled(true),
            Some(0x09) => self.bus.cd.set_enabled(false),
            Some(0x0D) => {
                let power_clear = if self.mode == Some(BootMode::HleDisc) {
                    Some([
                        self.bus.read32(hle::POWER_CLEAR)?,
                        self.bus.read32(hle::POWER_CLEAR + 4)?,
                    ])
                } else {
                    None
                };
                self.bus.system_reset();
                self.cpu = Cpu::default();
                self.slave = Cpu::default();
                match self.mode {
                    Some(BootMode::HleDisc) => {
                        if let Some(media) = self.bus.cd.media().cloned() {
                            let image = media.boot_iso().map_err(MachineError::Disc)?;
                            self.boot_disc(&image)?;
                            self.bus.cd.mount(media);
                            if let Some(words) = power_clear {
                                self.bus.write32(hle::POWER_CLEAR, words[0])?;
                                self.bus.write32(hle::POWER_CLEAR + 4, words[1])?;
                            }
                        }
                    }
                    Some(BootMode::HlePrototype) => {
                        for (i, opcode) in FIXTURE.iter().enumerate() {
                            self.bus.write16(FIXTURE_PC + i as u32 * 2, *opcode)?;
                        }
                        self.cpu = Cpu::start(FIXTURE_PC, HIGH_WRAM + WRAM_SIZE as u32);
                    }
                    _ => self.cpu = Cpu::power_on(&self.bus)?,
                }
                self.nmi_pending = false;
                self.dma_errors = [false; 2];
            }
            Some(0x0E | 0x0F) => {
                self.slave = Cpu::default();
                self.dma_errors[1] = false;
            }
            Some(0x18) => {
                self.nmi_pending = true;
                self.bus.signal_nmi(Processor::Master);
            }
            _ => (),
        }
        for processor in [Processor::Master, Processor::Slave] {
            if processor == Processor::Slave && !self.bus.slave_enabled {
                continue;
            }
            if let Some(reset) = self.bus.take_cpu_reset(processor) {
                let cpu = match reset {
                    onchip::ResetRequest::PowerOn => Cpu::power_on(&self.bus),
                    onchip::ResetRequest::Manual => Cpu::manual_reset(&self.bus),
                }?;
                match processor {
                    Processor::Master => {
                        self.cpu = cpu;
                        self.nmi_pending = false;
                    }
                    Processor::Slave => self.slave = cpu,
                }
                self.dma_errors[processor as usize] = false;
            }
        }
        Ok(())
    }

    /// Bounded CPU steps, stopping at the first host error. Exception entry and
    /// sleeping steps may retire no instructions; this is not a cycle budget.
    pub fn run_steps(&mut self, n: u32) -> Result<(), MachineError> {
        if !(1..=MAX_RUN_STEPS).contains(&n) {
            return Err(MachineError::StepLimit(n));
        }
        for _ in 0..n {
            self.step()?;
        }
        Ok(())
    }
}
