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
/// Maximum idle polling interval. Issued SH-2 instructions use their own
/// execution states and CPU-local bus waits, never this interval.
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

#[cfg(test)]
mod scheduling_tests {
    use super::*;

    fn cached_pair(master: &[u16], slave: &[u16]) -> Machine {
        let mut m = Machine::new();
        for (base, words) in [(HIGH_WRAM, master), (HIGH_WRAM + 0x100, slave)] {
            for (i, word) in words.iter().enumerate() {
                m.bus.write16(base + i as u32 * 2, *word).unwrap();
            }
        }
        m.bus.set_slave_enabled(true);
        for (processor, address) in [
            (Processor::Master, HIGH_WRAM),
            (Processor::Slave, HIGH_WRAM + 0x100),
        ] {
            let mut memory = m.bus.for_cpu(processor);
            memory.write32(0xFFFFFFE8, 0xA55A0000).unwrap();
            memory.write8(cache::CCR, 0x11).unwrap();
            memory.fetch16(address).unwrap();
            m.bus.cpu_delay(processor, 0);
        }
        m.bus.advance_clocks(32).unwrap();
        m.clocks = 32;
        m.cpu_ready = [32; 2];
        m.cpu = Cpu::start(HIGH_WRAM, HIGH_WRAM + 0x1000);
        m.slave = Cpu::start(HIGH_WRAM + 0x100, HIGH_WRAM + 0x2000);
        m
    }

    #[test]
    fn different_opcode_deadlines_allow_the_other_cpu_to_issue_independently() {
        // P table7.2: NOP=1, BRA=2 with one NOP slot=1.
        let mut m = cached_pair(&[9; 8], &[0xAFFE, 9]);
        m.step().unwrap();
        assert_eq!(
            (
                m.clocks(),
                m.cpu.state().instructions,
                m.slave.state().instructions
            ),
            (33, 1, 1)
        );
        m.step().unwrap();
        assert_eq!(
            (
                m.clocks(),
                m.cpu.state().instructions,
                m.slave.state().instructions
            ),
            (34, 2, 1)
        );
        m.step().unwrap();
        assert_eq!(
            (
                m.clocks(),
                m.cpu.state().instructions,
                m.slave.state().instructions
            ),
            (35, 3, 2)
        );
        assert_eq!(m.slave.state().pc, HIGH_WRAM + 0x100);
    }

    #[test]
    fn one_cpu_external_waits_do_not_charge_the_other_cpus_deadline() {
        let mut m = cached_pair(&[0x60F2, 9, 9], &[9; 8]); // MOV.L @R15,R0
        m.cpu = Cpu::start(HIGH_WRAM, (HIGH_WRAM + 0x2000) | 0x20000000);
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A0080)
            .unwrap();
        m.step().unwrap();
        assert_eq!(m.cpu_ready, [u64::MAX, 33]); // data phase has not completed
        assert_eq!(m.cpu.state().instructions, 0);
        for _ in 0..3 {
            m.step().unwrap();
        }
        assert_eq!(
            (
                m.clocks(),
                m.cpu.state().instructions,
                m.slave.state().instructions
            ),
            (36, 0, 4)
        );
        m.step().unwrap();
        assert_eq!(
            (m.cpu.state().instructions, m.slave.state().instructions),
            (1, 5)
        );
        m.reset(Boot::HlePrototype).unwrap();
        assert_eq!(m.cpu_ready, [0; 2]);
        m.step().unwrap();
        assert_eq!((m.clocks(), m.cpu.state().instructions), (8, 1));
    }
    #[test]
    fn stalled_cpu_write_is_invisible_to_peer_until_its_data_phase_and_reset_cancels_it() {
        let mut m = cached_pair(&[0xE02A, 0x2F02, 9, 9], &[9; 8]);
        let shared = HIGH_WRAM + 0x2000;
        m.cpu = Cpu::start(HIGH_WRAM, shared | 0x20000000);
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A0080)
            .unwrap();
        m.step().unwrap(); // cached MOV #42,R0
        assert_eq!(m.clocks(), 33);
        for expected_clock in 34..=36 {
            m.step().unwrap();
            assert_eq!(m.clocks(), expected_clock);
            assert_eq!(m.bus.read32(shared), Ok(0));
            assert_eq!(m.cpu.state().instructions, 1);
        }
        m.step().unwrap(); // t37: write data is now visible, before retirement
        assert_eq!(m.bus.read32(shared), Ok(42));
        assert_eq!(m.cpu.state().instructions, 1);
        m.step().unwrap();
        assert_eq!(m.cpu.state().instructions, 2);

        // Cancel a second write between grant and publication. Preserved RAM
        // in SYSRES must not receive an old instruction's delayed write.
        let mut m = cached_pair(&[0xE02A, 0x2F02, 9, 9], &[9; 8]);
        m.cpu = Cpu::start(HIGH_WRAM, shared | 0x20000000);
        m.step().unwrap();
        m.step().unwrap();
        assert_eq!(m.bus.read32(shared), Ok(0));
        m.bus.system_reset();
        m.bus.advance_clocks(100).unwrap();
        assert_eq!(m.bus.read32(shared), Ok(0));
    }
    #[test]
    fn long_dma_ownership_yields_without_faulting_or_publishing_cpu_work_early() {
        let mut m = Machine::new();
        m.reset(Boot::HlePrototype).unwrap();
        for (offset, value) in [
            (0, HIGH_WRAM + 0x20000),
            (4, HIGH_WRAM + 0x40000),
            (8, 16384),
            (12, 0x102),
            (20, 7),
            (16, 0x101),
        ] {
            m.bus.write32(scu::SCU + offset, value).unwrap();
        }
        m.step().unwrap();
        assert!(m.clocks() > 1000);
        assert_eq!(m.cpu_state().instructions, 0);
        assert_eq!(m.fault(), None);
        for _ in 0..64 {
            m.step().unwrap();
            if m.cpu_state().instructions != 0 {
                break;
            }
        }
        assert_eq!(m.bus.read32(scu::SCU + 0xA4).unwrap() & 0x800, 0x800);
        assert!(m.cpu_state().instructions > 0);
        assert_eq!(m.fault(), None);
    }
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

#[derive(Clone, Copy)]
struct CpuBoundary {
    dma: bool,
    nmi: bool,
    irq: Option<(u8, u8)>,
    started: u64,
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
    cpu_ready: [u64; 2],
    cpu_boundary: [Option<CpuBoundary>; 2],
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
        self.cpu_ready = [0; 2];
        self.cpu_boundary = [None; 2];
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
        hle::install(&mut self.bus, boot.aip_entry, boot.slave_stack)?;
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
    pub fn set_pal(&mut self, pal: bool) {
        self.bus.set_pal(pal);
    }
    pub fn set_region(&mut self, region: smpc::Region) {
        self.bus.set_region(region);
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
            .scanout(&self.bus.vdp1)
            .map_err(MachineError::Video)
    }

    pub fn video_dimensions(&self) -> (usize, usize) {
        self.bus.vdp2.output_dimensions()
    }

    /// Advance to the next raster field deadline. Peripherals and video events
    /// progress during SLEEP and DIVU stalls; execution stays bounded for the UI.
    /// CPU issue follows independent opcode deadlines and deferred bus data
    /// phases. External CPU and SCU cycles share arbitration; this bound keeps
    /// the caller responsive while detailed chip pipelines remain independent.
    pub fn advance_frame(&mut self) -> Result<(), MachineError> {
        let target = self.frames() + 1;
        for _ in 0..1_000_000 {
            self.step()?;
            if self.frames() >= target {
                return Ok(());
            }
        }
        Err(MachineError::StepLimit(1_000_000))
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

    fn issue_cpu(&mut self, processor: Processor) -> Result<bool, MachineError> {
        let i = processor as usize;
        let slave = processor == Processor::Slave;
        if self.bus.cpu_reset_active(processor) {
            self.bus.cancel_cpu_transaction(processor);
            self.cpu_boundary[i] = None;
            if slave {
                self.slave = Cpu::default();
            } else {
                self.cpu = Cpu::default();
            }
            self.cpu_ready[i] = self.clocks + u64::from(CLOCKS_PER_STEP);
            return Ok(true);
        }
        if self.cpu_ready[i] > self.clocks {
            return Ok(false);
        }
        let boundary = *self.cpu_boundary[i].get_or_insert_with(|| CpuBoundary {
            dma: self.dma_errors[i],
            nmi: !slave && self.nmi_pending && self.bus.nmi_ready(processor),
            irq: self.bus.interrupt(processor),
            started: self.clocks,
        });
        // Keep the architectural state at its previous boundary until all bus
        // cycles have completed. The journal reuses their sampled data and
        // effects while the original interpreter resumes through this clone.
        let mut cpu = if slave {
            self.slave.clone()
        } else {
            self.cpu.clone()
        };
        let result = (|| -> Result<(bool, bool, bool), CpuError> {
            let mut memory = self.bus.replay_cpu(processor);
            let dma = boundary.dma && cpu.accept_dma_address_error(&mut memory)?;
            let nmi = !dma && boundary.nmi && cpu.accept_nmi(&mut memory)?;
            let irq = !dma
                && !nmi
                && if let Some((level, vector)) = boundary.irq {
                    cpu.accept_interrupt(&mut memory, level, vector)?
                } else {
                    false
                };
            if !dma && !nmi && !irq {
                cpu.step(&mut memory)?;
            }
            Ok((dma, nmi, irq))
        })();
        match result {
            Err(
                CpuError::Bus {
                    source: BusError::Pending,
                    ..
                }
                | CpuError::ExceptionEntry {
                    source: BusError::Pending,
                    ..
                },
            ) => {
                self.cpu_ready[i] = u64::MAX;
                Ok(false)
            }
            Err(error) => {
                self.bus.cancel_cpu_transaction(processor);
                if slave {
                    self.slave = cpu;
                } else {
                    self.cpu = cpu;
                }
                Err(if slave {
                    MachineError::SlaveCpu(error)
                } else {
                    MachineError::Cpu(error)
                })
            }
            Ok((dma, nmi, irq)) => {
                if dma {
                    self.dma_errors[i] = false;
                }
                if nmi {
                    self.nmi_pending = false;
                }
                if irq {
                    self.bus
                        .acknowledge_interrupt(processor, boundary.irq.unwrap());
                }
                let states = cpu.last_clocks();
                // External data phases have already consumed their clocks.
                // Independent internal states can overlap them, never precede
                // their completion. On-chip wait states remain CPU-local.
                self.cpu_ready[i] = (boundary.started
                    + u64::from(self.bus.cpu_delay(processor, states)))
                .max(self.clocks + u64::from(if states == 0 { CLOCKS_PER_STEP } else { 0 }));
                if slave {
                    self.slave = cpu;
                } else {
                    self.cpu = cpu;
                }
                self.cpu_boundary[i] = None;
                self.bus.finish_cpu_transaction(processor);
                Ok(true)
            }
        }
    }

    fn step_inner(&mut self) -> Result<(), MachineError> {
        // Complete an architectural boundary when possible. Long bus-master
        // bursts can keep both CPUs waiting; yield after bounded hardware work
        // without manufacturing a CPU fault or freezing the browser caller.
        for _ in 0..1024 {
            if self.step_hardware_boundary()? {
                return Ok(());
            }
        }
        Ok(())
    }

    fn step_hardware_boundary(&mut self) -> Result<bool, MachineError> {
        let mut committed = self.issue_cpu(Processor::Master)?;
        if self.bus.slave_enabled {
            committed |= self.issue_cpu(Processor::Slave)?;
        }
        self.bus.grant_bus_transaction();
        let next = if self.bus.slave_enabled {
            self.cpu_ready[0].min(self.cpu_ready[1])
        } else {
            self.cpu_ready[0]
        };
        if committed && next <= self.clocks {
            return Ok(true);
        }
        let elapsed = next
            .saturating_sub(self.clocks)
            .min(u64::from(self.bus.bus_transaction_deadline()))
            .max(1)
            .min(u64::from(CLOCKS_PER_STEP)) as u32;
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
                self.cpu_ready[1] = self.clocks;
                self.cpu_boundary[1] = None;
            }
            Some(0x03) => {
                self.bus.set_slave_enabled(false);
                self.slave = Cpu::default();
                self.cpu_boundary[1] = None;
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
                self.cpu_ready = [self.clocks; 2];
                self.cpu_boundary = [None; 2];
            }
            Some(0x0E | 0x0F) => {
                self.slave = Cpu::default();
                self.dma_errors[1] = false;
                self.cpu_ready = [self.clocks; 2];
                self.cpu_boundary = [None; 2];
            }
            Some(0x18) => {
                self.nmi_pending = true;
                self.bus.signal_nmi(Processor::Master);
            }
            _ => (),
        }
        let errors = self.bus.take_dma_errors();
        for (pending, error) in self.dma_errors.iter_mut().zip(errors) {
            *pending |= error;
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
                self.cpu_ready[processor as usize] = self.clocks;
                self.cpu_boundary[processor as usize] = None;
            }
        }
        for processor in [Processor::Master, Processor::Slave] {
            if self.bus.take_cpu_transaction_completion(processor) {
                self.cpu_ready[processor as usize] = self.clocks;
            }
        }
        Ok(committed)
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
