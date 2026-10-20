//! Browser-independent Saturn vertical slice; only the master SH-2 executes.
pub mod bus;
pub mod cpu;
pub mod devices;
pub mod disc;
#[cfg(target_arch = "wasm32")]
mod wasm;

use bus::{Bus, BusError, HIGH_WRAM, Memory, WRAM_SIZE};
use cpu::{Cpu, CpuError, CpuState};
use std::fmt;

pub const MAX_RUN_STEPS: u32 = 10_000;
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
    StepLimit(u32),
    Disc(disc::DiscError),
    Video(String),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bus(error) => error.fmt(f),
            Self::Cpu(error) => error.fmt(f),
            Self::Disc(error) => error.fmt(f),
            Self::Video(error) => f.write_str(error),
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
    bus: Bus,
    mode: Option<BootMode>,
    frames: u64,
}

impl Machine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cpu_state(&self) -> &CpuState {
        self.cpu.state()
    }
    pub fn mode(&self) -> Option<BootMode> {
        self.mode
    }
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Cold reset clears both RAM banks, prior ROM, counters, delay state and faults.
    /// A rejected BIOS leaves ExternalBios selected and the CPU stopped in Reset.
    pub fn reset(&mut self, boot: Boot<'_>) -> Result<(), MachineError> {
        self.frames = 0;
        self.cpu = Cpu::default();
        self.bus = Bus::default();
        match boot {
            Boot::ExternalBios(bytes) => {
                self.mode = Some(BootMode::ExternalBios);
                self.bus.load_bios(bytes)?;
                self.cpu = Cpu::power_on(&self.bus)?;
            }
            Boot::HleDisc(image) => {
                self.mode = Some(BootMode::HleDisc);
                let boot = disc::parse(image).map_err(MachineError::Disc)?;
                for (i, byte) in boot.ip.iter().enumerate() {
                    self.bus.write8(disc::IP_ADDRESS + i as u32, *byte)?;
                }
                for (i, byte) in boot.executable.iter().enumerate() {
                    self.bus.write8(boot.load_address + i as u32, *byte)?;
                }
                self.cpu = Cpu::start(disc::AIP_ENTRY, boot.stack);
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

    pub fn set_buttons(&mut self, buttons: u16) {
        self.bus.set_buttons(buttons);
    }

    pub fn frames(&self) -> u64 {
        self.frames
    }

    pub fn framebuffer(&self) -> Result<Vec<u8>, MachineError> {
        self.bus.vdp2.framebuffer().map_err(MachineError::Video)
    }

    /// Deterministic instruction budgets, not cycle-accurate Saturn timing.
    /// One active phase followed by one blank phase. Guest polls TVSTAT.
    /// No game-specific PC, state, opcode or memory interception.
    pub fn advance_frame(&mut self) -> Result<(), MachineError> {
        self.bus.vdp2.vblank = false;
        for _ in 0..60_000 {
            self.step()?;
        }
        self.bus.vdp2.vblank = true;
        for _ in 0..20_000 {
            self.step()?;
        }
        self.bus.vdp2.vblank = false;
        self.frames += 1;
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), MachineError> {
        self.cpu.step(&mut self.bus)?;
        Ok(())
    }

    /// Stops immediately at the first error. Count is retired instructions, not cycles.
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
