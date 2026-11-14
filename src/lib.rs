//! Browser-independent Saturn functional subset with independent SH-2 processors.
pub mod backup;
pub mod bus;
pub mod cache;
pub mod cd;
mod cd_audio;
mod cd_rw;
pub mod cpu;
pub mod devices;
pub mod disc;
mod hle;
#[cfg(test)]
mod load_scheduler_tests;
pub mod m68k;
pub mod media;
#[cfg(test)]
mod multiplier_scheduler_tests;
pub mod onchip;
#[cfg(test)]
mod reset_tests;
pub mod scu;
pub mod smpc;
pub mod sound;
pub mod timing;
pub mod vdp1;
pub mod vdp2;
#[cfg(target_arch = "wasm32")]
mod wasm;

use bus::{
    Bus, BusError, CachedBranch, CachedLoad, CachedStore, HIGH_WRAM, Memory, Processor, WRAM_SIZE,
};
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

    pub(super) fn cached_pair(master: &[u16], slave: &[u16]) -> Machine {
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
            // These stage tests begin with a resident instruction stream;
            // warm every line, including lookahead across the first boundary.
            let words = if processor == Processor::Master {
                master
            } else {
                slave
            };
            for offset in (0..words.len() * 2 + 4).step_by(16) {
                memory.fetch16(address + offset as u32).unwrap();
            }
            m.bus.cpu_delay(processor, 0);
        }
        m.bus.advance_clocks(32).unwrap();
        m.clocks = 32;
        m.cpu_ready = [32; 2];
        m.cpu = Cpu::start(HIGH_WRAM, HIGH_WRAM + 0x1000);
        m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
        m.slave = Cpu::start(HIGH_WRAM + 0x100, HIGH_WRAM + 0x2000);
        m.slave.prime_fetch(&m.bus.for_cpu(Processor::Slave));
        m
    }

    #[test]
    fn dual_cached_register_issue_matches_replay_during_external_access() {
        // The master starts an uncached load while the slave retires
        // register operations and branches at its own deadlines.
        let make = || {
            let mut m = cached_pair(
                &[0x60F2, 0xE101, 0x7101, 0x2108, 0xAFFD, 9],
                &[0xE20F, 0x7201, 0x6213, 0xAFFC, 9],
            );
            m.cpu = Cpu::start(HIGH_WRAM, (HIGH_WRAM + 0x2000) | 0x20000000);
            m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
            m
        };
        let mut reference = make();
        let mut optimized = make();
        let mut continued_branch = false;
        for _ in 0..512 {
            optimized.step_inner_with_idle(true).unwrap();
            reference.step_inner_with_idle(false).unwrap();
            assert_eq!(optimized.cpu, reference.cpu);
            assert_eq!(optimized.slave, reference.slave);
            assert_eq!(optimized.cpu_ready, reference.cpu_ready);
            assert_eq!(optimized.clocks(), reference.clocks());
            assert_eq!(optimized.bus.main_clocks, reference.bus.main_clocks);
            continued_branch |= optimized
                .cached_issue
                .iter()
                .any(|issue| matches!(issue, Some(CachedIssue::Branch(_))));
        }
        assert!(optimized.cached_dual_scalars > 100);
        assert!(continued_branch);
    }

    #[test]
    fn dual_cached_ftcsr_poll_samples_the_peer_strobe_on_the_internal_bus() {
        let make = || {
            let mut m = cached_pair(
                &[9, 9, 0x2F01, 0xAFFE, 9],
                &[0x60F0, 0xC880, 0x89FC, 0xAFFB, 9],
            );
            m.cpu = Cpu::start(HIGH_WRAM, 0x2100_0000);
            m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
            m.slave = Cpu::start(HIGH_WRAM + 0x100, onchip::FRT + 1);
            m.slave.prime_fetch(&m.bus.for_cpu(Processor::Slave));
            m
        };
        let mut reference = make();
        let mut optimized = make();
        let mut polling = false;
        let mut captured = false;
        for _ in 0..100 {
            reference.step_inner_with_idle(false).unwrap();
            optimized.step_inner_with_idle(true).unwrap();
            assert_eq!(optimized.clocks, reference.clocks);
            for (index, issue) in optimized.cached_issue.iter().enumerate() {
                // A private IF continuation may have combined several
                // internal wakes while its architectural state stays held.
                if !matches!(issue, Some(CachedIssue::Branch(_))) {
                    assert_eq!(optimized.cpu_ready[index], reference.cpu_ready[index]);
                }
            }
            assert_eq!(optimized.cpu, reference.cpu);
            assert_eq!(optimized.slave, reference.slave);
            polling |= matches!(optimized.cached_issue[1], Some(CachedIssue::Load(_)));
            captured |= optimized.slave.state().r[0] == 0xFFFF_FF80;
        }
        assert!(polling && captured);
    }

    #[test]
    fn aligned_slave_poll_batches_match_independent_retirement_and_capture() {
        for destination in [
            None,
            Some(HIGH_WRAM + 0x1000),
            Some(0x25E00000),
            Some(0x21000000),
        ] {
            let make = || {
                let master = if destination.is_some() {
                    [0x7001, 0x7101, 0x2F01, 0xAFFB, 9]
                } else {
                    [0x7001, 0x7101, 0x7201, 0xAFFB, 9]
                };
                let mut m = cached_pair(&master, &[0x60F0, 0xC880, 0x89FC, 0xAFFB, 9]);
                if let Some(address) = destination {
                    m.cpu = Cpu::start(HIGH_WRAM, address);
                    m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
                }
                m.slave = Cpu::start(HIGH_WRAM + 0x100, onchip::FRT + 1);
                m.slave.prime_fetch(&m.bus.for_cpu(Processor::Slave));
                m
            };
            let mut reference = make();
            let mut optimized = make();
            for field in 1..=2 {
                optimized.advance_frame().unwrap();
                while reference.frames() < field {
                    reference.step().unwrap();
                }
                assert_eq!(optimized.cpu, reference.cpu, "master {destination:?}");
                assert_eq!(optimized.slave, reference.slave, "slave {destination:?}");
                assert_eq!(optimized.clocks, reference.clocks);
                assert_eq!(optimized.cpu_ready, reference.cpu_ready);
                if destination.is_none() || destination == Some(HIGH_WRAM + 0x1000) {
                    assert!(optimized.cached_poll_periods > 0, "batch {destination:?}");
                    assert!(optimized.cached_poll_prefixes > 0, "prefix {destination:?}");
                }
                if destination != Some(0x21000000)
                    && let Some(address) = destination
                {
                    assert_eq!(optimized.bus.read16(address), reference.bus.read16(address));
                }
                reference.bus.write16(0x2100_0000, 0).unwrap();
                optimized.bus.write16(0x2100_0000, 0).unwrap();
            }
        }
    }

    #[test]
    fn slave_store_continuations_preserve_shared_ram_arbitration() {
        let make = || {
            let mut m = cached_pair(&[0x60F2, 0xAFFD, 9], &[0xE211, 0x2F22, 0x7201, 0xAFFC, 9]);
            m.slave = Cpu::start(HIGH_WRAM + 0x100, HIGH_WRAM + 0x1000);
            m.slave.prime_fetch(&m.bus.for_cpu(Processor::Slave));
            m
        };
        let (mut optimized, mut reference) = (make(), make());
        let mut continued = false;
        for field in 1..=2 {
            while optimized.frames() < field {
                optimized.step_inner_with_idle(true).unwrap();
                continued |= matches!(optimized.cached_issue[1], Some(CachedIssue::Store(_)));
            }
            while reference.frames() < field {
                reference.step().unwrap();
            }
            assert_eq!(optimized.cpu, reference.cpu);
            assert_eq!(optimized.slave, reference.slave);
            assert_eq!(optimized.cpu_ready, reference.cpu_ready);
            assert_eq!(optimized.clocks, reference.clocks);
            assert_eq!(
                optimized.bus.read32(HIGH_WRAM + 0x1000),
                reference.bus.read32(HIGH_WRAM + 0x1000)
            );
        }
        assert!(continued);
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
            (33, 1, 0)
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
        m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A0080)
            .unwrap();
        m.step().unwrap();
        assert_eq!(m.cpu_ready, [33, 33]); // EX lead-in; MA starts at 33
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
            (
                m.clocks(),
                m.cpu.state().instructions,
                m.slave.state().instructions
            ),
            (37, 0, 5)
        );
        m.step().unwrap();
        assert_eq!(
            (m.cpu.state().instructions, m.slave.state().instructions),
            (1, 6)
        );
        m.reset(Boot::HlePrototype).unwrap();
        assert_eq!(m.cpu_ready, [0; 2]);
        m.step().unwrap();
        // F=8: cold IF 0..8, ID/if 8..9, EX/IF 9..17.
        assert_eq!((m.clocks(), m.cpu.state().instructions), (17, 1));
    }
    #[test]
    fn buffered_cpu_write_retires_before_publication_and_reset_cancels_it() {
        let mut m = cached_pair(&[0xE02A, 0x2F02, 0x7101, 0x7101, 9], &[9; 8]);
        let shared = HIGH_WRAM + 0x2000;
        m.cpu = Cpu::start(HIGH_WRAM, shared | 0x20000000);
        m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
        m.bus
            .for_cpu(Processor::Master)
            .write32(0xFFFFFFE8, 0xA55A00C0)
            .unwrap();
        m.step().unwrap();
        assert_eq!(m.clocks(), 33);
        m.step_hardware_boundary().unwrap(); // store EX33, MA request at34
        assert_eq!(m.clocks(), 34);
        m.step_hardware_boundary().unwrap(); // acceptance ends at35
        assert_eq!(m.clocks(), 35);
        m.step_hardware_boundary().unwrap(); // writer retires independently
        assert_eq!(m.cpu_state().instructions, 2);
        assert_eq!(m.bus.read32(shared), Ok(0));
        for _ in 0..2 {
            m.step().unwrap();
        }
        assert!(m.cpu_state().r[1] >= 1);
        assert_eq!(m.bus.read32(shared), Ok(0));
        // SYSRES clears RAM and discards the buffered external cycle. A
        // canceled write must not repopulate RAM after the board reset.
        m.bus.system_reset();
        m.bus.advance_clocks(100).unwrap();
        assert_eq!(m.bus.read32(shared), Ok(0));
    }
    #[test]
    fn writeback_interlock_precedes_dependent_mmio_while_the_peer_keeps_running() {
        // MOV #42,R0; LDS R0,MACL; STS MACL,R0; MOV.B R0,@R15.
        // P §7.7.6 fig7.90: STS's WB conflicts with MOV's EX.
        let mut m = cached_pair(&[0xE02A, 0x401A, 0x001A, 0x2F00, 9], &[9; 8]);
        m.cpu = Cpu::start(HIGH_WRAM, 0xFFFFFE01); // SCI BRR
        m.cpu.prime_fetch(&m.bus.for_cpu(Processor::Master));
        // P §7.4.3/fig7.88: LDS at 4n+2 adds an IF/MA split.
        while m.cpu_state().instructions < 3 {
            m.step().unwrap();
        }
        assert_eq!(m.clocks(), 36);
        assert_eq!(m.cpu_state().instructions, 3);
        assert!(!m.issue_cpu(Processor::Master).unwrap());
        m.bus.grant_bus_transaction();
        assert_eq!(m.bus.for_cpu(Processor::Master).read8(0xFFFFFE01), Ok(255));
        // Only the local WB wait expires at t37. The dependent write has
        // not even requested the peripheral bus, and peer NOPs still retire.
        m.step_hardware_boundary().unwrap();
        assert_eq!(m.clocks(), 37);
        assert_eq!(m.cpu_state().instructions, 3);
        assert_eq!(m.slave_state().instructions, 5);
        assert_eq!(m.bus.for_cpu(Processor::Master).read8(0xFFFFFE01), Ok(255));
        m.step_hardware_boundary().unwrap();
        assert_eq!(m.clocks(), 38);
        assert_eq!(m.bus.for_cpu(Processor::Master).read8(0xFFFFFE01), Ok(255));
        // MOV EX37, MA request38, peripheral publication39.
        m.step_hardware_boundary().unwrap();
        assert_eq!(m.clocks(), 39);
        assert_eq!(m.bus.for_cpu(Processor::Master).read8(0xFFFFFE01), Ok(42));
        m.step_hardware_boundary().unwrap();
        assert_eq!(m.cpu_state().instructions, 4);
    }

    #[test]
    fn watchdog_reset_vectors_sample_at_separate_bus_completions_and_peer_runs() {
        for p in [Processor::Master, Processor::Slave] {
            for reset in [onchip::ResetRequest::PowerOn, onchip::ResetRequest::Manual] {
                let mut m = cached_pair(&[9; 8], &[9; 8]);
                let vector = if reset == onchip::ResetRequest::Manual {
                    8
                } else {
                    0
                };
                let mut rom = vec![0; bus::BIOS_SIZE];
                rom[vector..vector + 4].copy_from_slice(&0x06003000u32.to_be_bytes());
                rom[vector + 4..vector + 8].copy_from_slice(&0x06004000u32.to_be_bytes());
                m.bus.load_bios(&rom).unwrap();
                m.bus.for_cpu(p).write8(cache::CCR, 0).unwrap();
                let i = p as usize;
                if p == Processor::Master {
                    m.cpu = Cpu::default();
                } else {
                    m.slave = Cpu::default();
                }
                m.cpu_reset_vectors[i] = Some(reset);
                // Zero-wait controlled bus vector: PC high/low at34/36,
                // SP high/low at38/40. Mutate each unsampled successor.
                for (clock, pc, sp) in [
                    (34, 0x12345000u32, 0x06004000u32),
                    (36, 0xEEEE6000, 0x06007000),
                    (38, 0xFFFF9000, 0x11118000),
                    (40, 0x0000A000, 0x2222B000),
                ] {
                    while m.clocks < clock {
                        m.step_hardware_boundary().unwrap();
                    }
                    let cpu = if p == Processor::Master {
                        m.cpu_state()
                    } else {
                        m.slave_state()
                    };
                    assert_eq!(cpu, &CpuState::default());
                    rom[vector..vector + 4].copy_from_slice(&pc.to_be_bytes());
                    rom[vector + 4..vector + 8].copy_from_slice(&sp.to_be_bytes());
                    m.bus.load_bios(&rom).unwrap();
                }
                assert!(!m.issue_cpu(p).unwrap());
                let (cpu, peer) = if p == Processor::Master {
                    (m.cpu_state(), m.slave_state())
                } else {
                    (m.slave_state(), m.cpu_state())
                };
                assert_eq!(
                    (cpu.pc, cpu.r[15], cpu.instructions),
                    (0x06005000, 0x06008000, 0)
                );
                assert_eq!((cpu.vbr, cpu.sr), (0, 0xF0));
                assert_eq!(peer.instructions, 8);
                assert_eq!(m.cpu_reset_vectors[i], None);
            }
        }
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

#[derive(Clone, Copy, PartialEq, Eq)]
struct CpuBoundary {
    dma: bool,
    nmi: bool,
    irq: Option<bus::InterruptRequest>,
    started: u64,
}

// Keep continuations inline because these paths run at instruction frequency.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, PartialEq, Eq)]
enum CachedIssue {
    Branch(CachedBranch),
    Load(CachedLoad),
    Store(CachedStore),
}

#[derive(Default)]
pub struct Machine {
    private_poll: bus::private_poll::Context,
    #[cfg(test)]
    cached_private_phases: u64,
    #[cfg(test)]
    cached_retired: u64,
    #[cfg(test)]
    cached_dual_scalars: u64,
    #[cfg(test)]
    cached_poll_periods: u64,
    #[cfg(test)]
    cached_poll_prefixes: u64,
    #[cfg(test)]
    cached_poll_origins: Vec<u32>,
    cpu: Cpu,
    slave: Cpu,
    bus: Bus,
    mode: Option<BootMode>,
    clocks: u64,
    nmi_pending: bool,
    nmi_detected: Option<u64>,
    dma_errors: [bool; 2],
    cpu_ready: [u64; 2],
    cpu_boundary: [Option<CpuBoundary>; 2],
    cached_issue: [Option<CachedIssue>; 2],
    slave_poll_retry: u64,
    cpu_reset_vectors: [Option<onchip::ResetRequest>; 2],
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
    /// Explicit local diagnostic request. Bounded transaction state only; no
    /// firmware dump, filenames, logging, storage or extra guest bus reads.
    pub fn smpc_fault_diagnostics(&self) -> Option<String> {
        let (processor, error) = match self.fault.as_ref()? {
            MachineError::Cpu(error) => ("Master SH-2", error),
            MachineError::SlaveCpu(error) => ("Slave SH-2", error),
            _ => return None,
        };
        match error {
            CpuError::Bus {
                pc,
                source: BusError::Smpc(fault),
            }
            | CpuError::ExceptionEntry {
                pc,
                source: BusError::Smpc(fault),
                ..
            } => Some(format!("{processor} PC=0x{pc:08X} {fault:?}")),
            _ => None,
        }
    }

    /// Enable a fresh bounded local trace of completed legacy-DSTP cycles.
    /// Disabled by default and cleared by cold/system reset. No guest state,
    /// timing or bus reads are changed by observation.
    pub fn set_dstp_trace(&mut self, enabled: bool) {
        self.bus.dstp_trace = enabled.then(Vec::new);
    }
    pub fn dstp_trace(&self) -> Option<String> {
        self.bus.dstp_trace.as_ref().map(|trace| trace.join("\n"))
    }

    pub fn fault(&self) -> Option<&MachineError> {
        self.fault.as_ref()
    }
    pub fn reset_in_progress(&self) -> bool {
        self.bus.cpu_reset_active(Processor::Master) || self.cpu_reset_vectors[0].is_some()
    }
    pub fn can_wake_master(&self) -> bool {
        self.cpu_reset_vectors[0].is_some()
            || self.nmi_pending
            || self.dma_errors[0]
            || self.bus.can_wake_master()
    }
    pub fn mode(&self) -> Option<BootMode> {
        self.mode
    }
    pub fn bus(&self) -> &Bus {
        &self.bus
    }

    /// Debugger inspection uses passive CD register/FIFO access.
    pub fn debug_read16(&self, address: u32) -> Result<u16, bus::BusError> {
        self.bus.debug_read16(address)
    }
    pub fn debug_read32(&self, address: u32) -> Result<u32, bus::BusError> {
        self.bus.debug_read32(address)
    }

    /// Cold reset clears volatile RAM, prior ROM, counters, delay state and faults.
    /// Internal battery-backed RAM survives all boot selections and failed boots.
    /// A rejected BIOS leaves ExternalBios selected and the CPU stopped in Reset.
    /// External firmware resets retain the physical medium, but reset every CD
    /// command, selector, FIFO and interrupt. Use clear_media to change selection.
    pub fn reset(&mut self, boot: Boot<'_>) -> Result<(), MachineError> {
        let medium = if matches!(boot, Boot::ExternalBios(_)) {
            self.bus.cd.media().cloned()
        } else {
            None
        };
        self.clear_execution();
        match boot {
            Boot::ExternalBios(bytes) => {
                self.mode = Some(BootMode::ExternalBios);
                if let Some(medium) = medium {
                    self.bus.cd.mount(medium);
                }
                self.bus.load_bios(bytes)?;
                self.schedule_cpu_reset(Processor::Master, onchip::ResetRequest::PowerOn);
            }
            Boot::HleDisc(image) => {
                self.mode = Some(BootMode::HleDisc);
                let media = media::Media::iso(image).map_err(MachineError::Disc)?;
                self.boot_disc(&media)?;
                self.bus.cd.mount(media);
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

    fn clear_execution(&mut self) {
        self.clocks = 0;
        self.nmi_pending = false;
        self.nmi_detected = None;
        self.dma_errors = [false; 2];
        self.cpu_ready = [0; 2];
        self.cpu_boundary = [None; 2];
        self.cached_issue = [None, None];
        self.slave_poll_retry = 0;
        self.private_poll = bus::private_poll::Context::default();
        self.cpu_reset_vectors = [None; 2];
        self.fault = None;
        self.cpu = Cpu::default();
        self.slave = Cpu::default();
        self.bus.reset_volatile();
    }

    /// Restart the retained immutable medium without copying its stored bytes.
    pub fn reset_loaded_disc(&mut self) -> Result<(), MachineError> {
        let medium = self.bus.cd.media().cloned();
        match medium {
            Some(medium) => self.reset_media(medium),
            None => self.reset(Boot::HleDisc(&[])),
        }
    }

    /// HLE bootstrap shared by cold startup and guest SYSRES. Hardware reset
    /// has already selected the reset domain; this only reloads IP/executable.
    fn boot_disc(&mut self, medium: &media::Media) -> Result<(), MachineError> {
        let boot = disc::parse_sectors(medium).map_err(MachineError::Disc)?;
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
        Ok(())
    }

    /// Boot caller-resolved ISO/BIN/CUE media through the same independent HLE.
    /// No file reference is opened by the emulator. Invalid input stops the old
    /// machine while preserving battery RAM, as with the single-file loader.
    /// Structurally valid media remains mounted even if its HLE boot is rejected.
    pub fn reset_media(&mut self, media: media::Media) -> Result<(), MachineError> {
        self.clear_execution();
        self.mode = Some(BootMode::HleDisc);
        let result = self.boot_disc(&media);
        self.bus.cd.mount(media);
        result
    }

    /// Attach local media to an external BIOS without replacing its CPU/reset
    /// state or invoking HLE. Also useful for synthetic boot ROM acceptance.
    pub fn mount_media(&mut self, media: media::Media) {
        self.bus.cd.mount(media);
    }

    /// Discard a local media selection before cold reset. This host selection
    /// operation is separate from the guest's timed tray/drive commands.
    pub fn clear_media(&mut self) {
        self.bus.cd = cd::CdBlock::default();
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
            self.request_nmi();
        }
    }
    fn request_nmi(&mut self) {
        self.nmi_pending = true;
        self.nmi_detected.get_or_insert(self.bus.main_clocks);
        self.bus.signal_nmi(Processor::Master);
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
    pub fn set_3d_pad_digital(&mut self, port: usize, buttons: u16) {
        self.bus.set_3d_pad_digital(port, buttons);
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
        let mut steps = 0;
        while steps < 1_000_000 {
            let limit = (1_000_000 - steps).min(512);
            let mut count = if self.bus.slave_enabled
                && (((self.cpu_ready[1] != self.bus.main_clocks
                    || self.bus.main_clocks < self.slave_poll_retry)
                    && self.bus.main_clocks < self.private_poll.retry
                    && !self.private_poll.has_templates())
                    || self.cpu_boundary[0].is_some()
                    || self.cached_issue[0].is_some())
            {
                0
            } else {
                self.try_cached_batch(limit)?
            };
            if count == 0 {
                if let Some(error) = &self.fault {
                    return Err(error.clone());
                }
                count = self
                    .step_inner_with_retry(true, limit)
                    .inspect_err(|error| self.fault = Some(error.clone()))?;
            }
            steps += count.max(1);
            if self.frames() >= target {
                return Ok(());
            }
        }
        Err(MachineError::StepLimit(1_000_000))
    }

    fn try_cached_batch(&mut self, limit: u32) -> Result<u32, MachineError> {
        let retired = self.try_retired_poll_batch(limit)?;
        if retired != 0 {
            return Ok(retired);
        }
        self.try_private_poll_batch(limit)
    }
    fn try_retired_poll_batch(&mut self, limit: u32) -> Result<u32, MachineError> {
        if self.fault.is_some()
            || self.nmi_pending
            || self.dma_errors[0]
            || self.cpu_boundary[0].is_some()
            || self.cached_issue[0].is_some()
            || self.cpu_reset_vectors[0].is_some()
            || self.cpu_ready[0].saturating_sub(self.bus.main_clocks)
                > if self.bus.slave_enabled { 8 } else { 0 }
        {
            return Ok(0);
        }
        let origin = self.bus.main_clocks;
        let peer = if self.bus.slave_enabled {
            if origin < self.slave_poll_retry {
                return Ok(0);
            }
            if self.cpu_ready[1] != origin
                || self.cpu_boundary[1].is_some()
                || self.cached_issue[1].is_some()
                || self.cpu_reset_vectors[1].is_some()
                || self.dma_errors[1]
            {
                return Ok(0);
            }
            if !self.bus.cached_master_admissible(&self.cpu) {
                self.slave_poll_retry = origin + 64;
                return Ok(0);
            }
            let Some(cycle) = self.bus.prepare_poll_cycle(&self.slave) else {
                self.slave_poll_retry = origin + 64;
                return Ok(0);
            };
            Some(cycle)
        } else {
            None
        };
        let result = self.bus.cached_batch(
            &self.cpu,
            limit,
            peer.as_ref().map(bus::PeerSchedule::Retired).as_ref(),
            origin.max(self.cpu_ready[0]),
        );
        let Some(batch) = result.inspect_err(|error| {
            self.fault = Some(error.clone());
        })?
        else {
            if peer.is_some() {
                self.slave_poll_retry = origin + 64;
            }
            return Ok(0);
        };
        #[cfg(test)]
        {
            self.cached_retired += u64::from(batch.retired);
        }
        self.cpu = batch.cpu;
        if let Some(peer) = peer {
            let clocks = self.bus.main_clocks - origin;
            assert!(peer.can_retire_at(clocks));
            #[cfg(test)]
            {
                let origin = self.slave.state().pc;
                if !self.cached_poll_origins.contains(&origin) {
                    self.cached_poll_origins.push(origin);
                }
                self.cached_poll_periods += clocks / peer.period;
                self.cached_poll_prefixes += u64::from(!clocks.is_multiple_of(peer.period));
            }
            self.cpu_ready[1] = self.bus.commit_poll_cycle(peer, &mut self.slave, clocks);
        }
        self.cpu_ready[0] = batch.ready;
        self.clocks += batch.reference;
        // Match the last ordinary step's post-retirement advancement. In
        // particular, a field edge can precede this CPU's next issue time.
        if let Err(error) = self.finish_hardware_boundary(true) {
            self.fault = Some(error.clone());
            return Err(error);
        }
        Ok(batch.retired)
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

    /// Every hardware reset samples PC and SP through the same replayed bus
    /// cycles. Neither architectural value is published until both complete.
    /// Cancel any older instruction/vector journal before starting a new reset.
    fn schedule_cpu_reset(&mut self, processor: Processor, reset: onchip::ResetRequest) {
        let i = processor as usize;
        self.bus.cancel_cpu_transaction(processor);
        match processor {
            Processor::Master => {
                self.cpu = Cpu::default();
                self.nmi_pending = false;
            }
            Processor::Slave => self.slave = Cpu::default(),
        }
        self.dma_errors[i] = false;
        self.cpu_ready[i] = self.bus.main_clocks;
        self.cpu_boundary[i] = None;
        self.cached_issue[i] = None;
        self.cpu_reset_vectors[i] = Some(reset);
    }

    fn issue_cpu(&mut self, processor: Processor) -> Result<bool, MachineError> {
        self.issue_cpu_with_inline(processor, false)
    }
    fn issue_cpu_with_inline(
        &mut self,
        processor: Processor,
        optimized: bool,
    ) -> Result<bool, MachineError> {
        let i = processor as usize;
        let slave = processor == Processor::Slave;
        if optimized && !slave && self.bus.slave_enabled {
            // Master issue uses its own IRQ. An effect on the slave from a
            // shared bus access only publishes at the hardware completion.
            self.bus.sample_interrupt(Processor::Master);
        } else {
            self.bus.sample_interrupts();
        }
        if self.nmi_pending {
            self.nmi_detected.get_or_insert(self.bus.main_clocks);
        } else {
            self.nmi_detected = None;
        }
        let nmi_recognized = !slave
            && self.nmi_pending
            && self
                .nmi_detected
                .is_some_and(|r| self.bus.main_clocks >= r + 2);
        let state = if slave {
            self.slave.state()
        } else {
            self.cpu.state()
        };
        if state.status == cpu::CpuStatus::Sleeping
            && self.cpu_boundary[i].is_none()
            && (nmi_recognized
                || self
                    .bus
                    .recognized_interrupt(processor)
                    .is_some_and(|r| u32::from(r.level) > (state.sr >> 4) & 15))
        {
            self.cpu_ready[i] = self.cpu_ready[i].min(self.bus.main_clocks);
        }
        if self.bus.cpu_reset_active(processor) {
            self.bus.hold_cpu_reset(processor);
            self.cpu_boundary[i] = None;
            self.cached_issue[i] = None;
            self.cpu_reset_vectors[i] = None;
            if slave {
                self.slave = Cpu::default();
            } else {
                self.cpu = Cpu::default();
            }
            self.cpu_ready[i] = self.bus.main_clocks + u64::from(CLOCKS_PER_STEP);
            return Ok(true);
        }
        if self.cpu_ready[i] > self.bus.main_clocks {
            return Ok(false);
        }
        if self.cached_issue[i].is_some() {
            return self.resume_cached_issue(processor);
        }
        if !self.bus.nmi_ready(processor) {
            // Software standby gates the entire processor, including external
            // IRQ and DMA-error entry. NMI becomes eligible after WDT settling.
            self.cpu_ready[i] = self.bus.main_clocks + u64::from(CLOCKS_PER_STEP);
            return Ok(true);
        }
        if let Some(reset) = self.cpu_reset_vectors[i] {
            let result = {
                let memory = self.bus.replay_cpu(processor);
                match reset {
                    onchip::ResetRequest::PowerOn => Cpu::power_on(&memory),
                    onchip::ResetRequest::Manual => Cpu::manual_reset(&memory),
                }
            };
            return match result {
                Err(BusError::Pending) => {
                    self.cpu_ready[i] = u64::MAX;
                    Ok(false)
                }
                Err(source) => {
                    self.bus.cancel_cpu_transaction(processor);
                    let error = CpuError::Bus { pc: 0, source };
                    if slave {
                        self.slave.deferred_write_fault(error.clone());
                        Err(MachineError::SlaveCpu(error))
                    } else {
                        self.cpu.deferred_write_fault(error.clone());
                        Err(MachineError::Cpu(error))
                    }
                }
                Ok(cpu) => {
                    if slave {
                        self.slave = cpu;
                    } else {
                        self.cpu = cpu;
                    }
                    self.cpu_reset_vectors[i] = None;
                    self.bus.finish_cpu_transaction(processor);
                    // A reset does not retire an instruction. Let the bounded
                    // machine turn continue to the first executable boundary.
                    Ok(false)
                }
            };
        }
        let irq = self
            .bus
            .recognized_interrupt(processor)
            .filter(|r| u32::from(r.level) > (state.sr >> 4) & 15);
        if optimized
            && self.bus.slave_enabled
            && self.cpu_boundary[i].is_none()
            && !self.dma_errors[i]
            && (slave || !nmi_recognized)
            && irq.is_none()
        {
            let cpu = if slave {
                &mut self.slave
            } else {
                &mut self.cpu
            };
            if self.bus.issue_cached_scalar(processor, cpu) {
                let states = self.bus.cpu_main_delay(processor, cpu.last_clocks());
                self.cpu_ready[i] = self.bus.main_clocks + u64::from(states);
                self.bus.finish_cpu_transaction(processor);
                #[cfg(test)]
                {
                    self.cached_dual_scalars += 1;
                }
                return Ok(true);
            }
            if let Some((branch, waiting)) = self.bus.prepare_cached_branch(processor, cpu) {
                self.cached_issue[i] = Some(CachedIssue::Branch(branch));
                if waiting {
                    self.cpu_ready[i] = u64::MAX;
                    return Ok(false);
                }
                return self.resume_cached_issue(processor);
            }
            if let Some((load, waiting)) = self.bus.prepare_cached_load(processor, cpu) {
                self.cached_issue[i] = Some(CachedIssue::Load(load));
                if waiting {
                    self.cpu_ready[i] = u64::MAX;
                    return Ok(false);
                }
                return self.resume_cached_issue(processor);
            }
            if let Some((store, waiting)) = self.bus.prepare_cached_store(processor, cpu) {
                self.cached_issue[i] = Some(CachedIssue::Store(store));
                if waiting {
                    self.cpu_ready[i] = u64::MAX;
                    return Ok(false);
                }
                return self.resume_cached_issue(processor);
            }
        }
        let boundary = *self.cpu_boundary[i].get_or_insert_with(|| CpuBoundary {
            dma: self.dma_errors[i],
            nmi: !slave
                && self.nmi_pending
                && self.bus.nmi_ready(processor)
                && self
                    .nmi_detected
                    .is_some_and(|r| self.bus.main_clocks >= r + 2),
            irq,
            started: self.bus.main_clocks,
        });
        // Keep the architectural state at its previous boundary until all bus
        // cycles have completed. The journal reuses their sampled data and
        // effects while the original interpreter resumes through this clone.
        let mut cpu = if slave {
            self.slave.clone()
        } else {
            self.cpu.clone()
        };
        let inline = optimized && !slave && !self.bus.slave_enabled;
        if inline {
            self.bus.begin_inline(true);
        }
        let result = if !boundary.dma && !boundary.nmi && boundary.irq.is_none() {
            cpu.step(&mut self.bus.replay_cpu(processor))
                .map(|()| false)
        } else {
            self.resume_interrupt_boundary(processor, &mut cpu, boundary)
        };
        if boundary.nmi && self.bus.take_nmi_clear(processor) {
            self.nmi_pending = false;
            self.nmi_detected = None;
        }
        if let Some(progress) = self.bus.take_inline() {
            self.clocks += progress.reference;
            if let Some(ready) = progress.ready {
                self.cpu_ready[i] = ready;
            }
            if let Some(error) = progress.error {
                return Err(MachineError::Scu(error));
            }
            if let Some(error) = progress.write_error {
                self.cpu.deferred_write_fault(error.clone());
                return Err(MachineError::Cpu(error));
            }
        }
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
            Ok(dma) => {
                if dma {
                    self.dma_errors[i] = false;
                }
                let states = cpu.last_clocks();
                // External data phases have already consumed their clocks.
                // Independent internal states can overlap them, never precede
                // their completion. On-chip wait states remain CPU-local.
                // A MOV or multiplier memory MA can retire after its successor's
                // EX starts. Recover that slot, retaining the completion clamp.
                self.cpu_ready[i] = (cpu.execution_clock().unwrap_or(boundary.started)
                    + u64::from(self.bus.cpu_main_delay(processor, states)))
                .max(
                    self.bus.main_clocks + u64::from(if states == 0 { CLOCKS_PER_STEP } else { 0 }),
                );
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

    fn resume_cached_issue(&mut self, processor: Processor) -> Result<bool, MachineError> {
        let i = processor as usize;
        let cpu = match processor {
            Processor::Master => &mut self.cpu,
            Processor::Slave => &mut self.slave,
        };
        let result = match self.cached_issue[i].as_mut().unwrap() {
            CachedIssue::Branch(_) => Ok(0),
            CachedIssue::Load(load) => self.bus.resume_cached_load(processor, load),
            CachedIssue::Store(store) => self
                .bus
                .resume_cached_store(processor, cpu.state().pc, store)
                .map(|()| 0),
        };
        let value = match result {
            Ok(value) => value,
            Err(BusError::Pending) => {
                self.cpu_ready[i] = u64::MAX;
                return Ok(false);
            }
            Err(source) => {
                self.cached_issue[i] = None;
                self.bus.cancel_cpu_transaction(processor);
                let error = CpuError::Bus {
                    pc: cpu.state().pc,
                    source,
                };
                cpu.deferred_write_fault(error.clone());
                return Err(match processor {
                    Processor::Master => MachineError::Cpu(error),
                    Processor::Slave => MachineError::SlaveCpu(error),
                });
            }
        };
        match self.cached_issue[i].take().unwrap() {
            CachedIssue::Branch(branch) => {
                cpu.retire_cached_control(self.bus.commit_cached_branch(processor, branch));
            }
            CachedIssue::Load(load) => {
                let pc = cpu.state().pc;
                cpu.retire_prepared_load(load.plan, load.future, value, self.bus.main_clocks);
                self.bus.prepared_memory_retired(processor, pc);
            }
            CachedIssue::Store(store) => {
                let pc = cpu.state().pc;
                cpu.retire_prepared_store(store.plan, store.future, self.bus.main_clocks);
                self.bus.prepared_memory_retired(processor, pc);
            }
        }
        let states = self.bus.cpu_main_delay(processor, cpu.last_clocks());
        self.cpu_ready[i] =
            (cpu.execution_clock().unwrap() + u64::from(states)).max(self.bus.main_clocks);
        self.bus.finish_cpu_transaction(processor);
        Ok(true)
    }

    fn resume_interrupt_boundary(
        &mut self,
        processor: Processor,
        cpu: &mut Cpu,
        boundary: CpuBoundary,
    ) -> Result<bool, CpuError> {
        self.bus.prepare_interrupt(processor, None);
        let mut memory = self.bus.replay_cpu(processor);
        if boundary.dma && cpu.accept_dma_address_error(&mut memory)? {
            return Ok(true);
        }
        if boundary.nmi && cpu.accept_nmi(&mut memory)? {
            return Ok(false);
        }
        if let Some(irq) = boundary.irq {
            self.bus.prepare_interrupt(processor, Some(irq));
            if cpu.accept_interrupt(&mut self.bus.replay_cpu(processor), irq.level, irq.vector)? {
                return Ok(false);
            }
        }
        // A deferred request must not supply a vector or acknowledge a
        // source for a synchronous exception raised by the instruction.
        self.bus.prepare_interrupt(processor, None);
        cpu.step(&mut self.bus.replay_cpu(processor))?;
        Ok(false)
    }

    fn step_inner(&mut self) -> Result<(), MachineError> {
        self.step_inner_with_idle(false)
    }
    fn step_inner_with_idle(&mut self, coalesced: bool) -> Result<(), MachineError> {
        self.step_inner_with_retry(coalesced, 1).map(|_| ())
    }

    fn step_inner_with_retry(
        &mut self,
        coalesced: bool,
        retry_limit: u32,
    ) -> Result<u32, MachineError> {
        let field = self.frames();
        // Complete an architectural boundary when possible. Long bus-master
        // bursts can keep both CPUs waiting; yield after bounded hardware work
        // without manufacturing a CPU fault or freezing the browser caller.
        for turn in 0..1024 {
            // The frame runner already tried at entry. After a hardware wait
            // reaches the existing ready time, retry before issuing through
            // replay. Within the same field the frame runner can resume a
            // whole bounded burst. A wait crossing the field edge retains the
            // ordinary step's single-instruction completion contract.
            if coalesced && turn != 0 && !self.bus.slave_enabled {
                let limit = if self.frames() == field {
                    retry_limit
                } else {
                    1
                };
                let retired = self.try_cached_batch(limit)?;
                if retired != 0 {
                    return Ok(retired);
                }
            }
            if self.step_hardware_boundary_with_idle(coalesced)? {
                return Ok(1);
            }
        }
        Ok(0)
    }

    #[cfg(test)]
    fn step_hardware_boundary(&mut self) -> Result<bool, MachineError> {
        self.step_hardware_boundary_with_idle(false)
    }
    fn step_hardware_boundary_with_idle(&mut self, coalesced: bool) -> Result<bool, MachineError> {
        let mut committed = if coalesced {
            self.issue_cpu_with_inline(Processor::Master, true)?
        } else {
            self.issue_cpu(Processor::Master)?
        };
        if self.bus.slave_enabled {
            committed |= self.issue_cpu_with_inline(Processor::Slave, coalesced)?;
        }
        self.finish_hardware_boundary_with_idle(committed, coalesced)
    }

    fn finish_hardware_boundary(&mut self, committed: bool) -> Result<bool, MachineError> {
        self.finish_hardware_boundary_with_idle(committed, false)
    }
    fn finish_hardware_boundary_with_idle(
        &mut self,
        committed: bool,
        coalesced: bool,
    ) -> Result<bool, MachineError> {
        self.bus.grant_bus_transaction();
        let next = if self.bus.slave_enabled {
            self.cpu_ready[0].min(self.cpu_ready[1])
        } else {
            self.cpu_ready[0]
        };
        if committed && next <= self.bus.main_clocks {
            return Ok(true);
        }
        // Even an earlier event would be handled by the bus scheduler inside
        // the minimum one-clock advance. Avoid querying all horizons twice.
        let can_coalesce = self.can_coalesce_wait(next, committed, coalesced);
        let elapsed = if next.saturating_sub(self.bus.main_clocks) <= 1 {
            1
        } else {
            self.hardware_idle_span(next, can_coalesce)
        };
        let short_wait = coalesced
            && committed
            && elapsed == 1
            && next > self.bus.main_clocks
            && self.bus.cached_wait_allowed();
        let result = if short_wait {
            self.bus
                .try_retirement_wait()
                .unwrap_or_else(|| self.bus.advance_cpu_clocks(elapsed))
        } else if can_coalesce {
            self.bus.advance_cpu_clocks_with_sound(elapsed, true)
        } else {
            self.bus.advance_cpu_clocks(elapsed)
        };
        let (reference, command) = result.map_err(MachineError::Scu)?;
        self.clocks += u64::from(reference);
        self.complete_hardware_boundary(command)?;
        Ok(committed)
    }

    fn can_coalesce_wait(&self, next: u64, committed: bool, coalesced: bool) -> bool {
        let waiting = if self.bus.slave_enabled {
            next.saturating_sub(self.bus.main_clocks) > 1
        } else {
            self.cpu_boundary[0].is_some() || next > self.bus.main_clocks
        };
        // Both CPUs are waiting for their existing readiness or bus edges.
        // Debugger steps retain their ordinary hardware path.
        coalesced && !committed && waiting && self.bus.cached_wait_allowed()
    }

    fn hardware_idle_span(&self, next: u64, can_coalesce: bool) -> u32 {
        let nmi_wait = self.nmi_detected.map_or(u64::MAX, |r| {
            let remaining = (r + 2).saturating_sub(self.bus.main_clocks);
            if remaining == 0 { u64::MAX } else { remaining }
        });
        let main_deadline = if can_coalesce {
            self.bus.coalesced_main_deadline()
        } else {
            self.bus.main_deadline()
        };
        next.saturating_sub(self.bus.main_clocks)
            .min(nmi_wait)
            // A device event can assert a new request inside an idle batch.
            // Return to CPU scheduling at that edge so the newly established
            // recognition deadline can bound the following advance.
            .min(u64::from(main_deadline))
            .min(u64::from(self.bus.bus_transaction_deadline()))
            .max(1)
            .min(u64::from(CLOCKS_PER_STEP)) as u32
    }

    fn complete_hardware_boundary(&mut self, command: Option<u8>) -> Result<(), MachineError> {
        match command {
            Some(0x02) if !self.bus.slave_enabled => {
                self.bus.set_slave_enabled(true);
                self.schedule_cpu_reset(Processor::Slave, onchip::ResetRequest::PowerOn);
            }
            Some(0x03) => {
                self.bus.set_slave_enabled(false);
                self.slave = Cpu::default();
                self.cpu_boundary[1] = None;
                self.cached_issue[1] = None;
                self.cpu_reset_vectors[1] = None;
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
                self.nmi_pending = false;
                self.dma_errors = [false; 2];
                self.cpu_ready = [self.bus.main_clocks; 2];
                self.cpu_boundary = [None; 2];
                self.cached_issue = [None, None];
                self.cpu_reset_vectors = [None; 2];
                match self.mode {
                    Some(BootMode::HleDisc) => {
                        if let Some(media) = self.bus.cd.media().cloned() {
                            self.boot_disc(&media)?;
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
                    _ => self.schedule_cpu_reset(Processor::Master, onchip::ResetRequest::PowerOn),
                }
            }
            Some(0x0E | 0x0F) => {
                self.slave = Cpu::default();
                self.dma_errors[1] = false;
                self.cpu_ready[1] = self.bus.main_clocks;
                self.cpu_boundary[1] = None;
                self.cached_issue[1] = None;
                self.cpu_reset_vectors[1] = None;
                // Clock completion wakes software standby through the same
                // input, DMAC inhibition and scheduled exception path as NMI.
                // Preserve a master's in-flight instruction journal: the
                // command is not a reset of the master SH-2.
                self.request_nmi();
            }
            Some(0x18) => {
                self.request_nmi();
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
                self.schedule_cpu_reset(processor, reset);
            }
        }
        for processor in [Processor::Master, Processor::Slave] {
            if let Some(error) = self.bus.take_cpu_write_error(processor) {
                return Err(match processor {
                    Processor::Master => {
                        self.cpu.deferred_write_fault(error.clone());
                        MachineError::Cpu(error)
                    }
                    Processor::Slave => {
                        self.slave.deferred_write_fault(error.clone());
                        MachineError::SlaveCpu(error)
                    }
                });
            }
            if self.bus.take_cpu_transaction_completion(processor) {
                self.cpu_ready[processor as usize] = self.bus.main_clocks;
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

#[cfg(any(test, feature = "kernel-experiment"))]
pub mod kernel;
