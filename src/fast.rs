//! Relaxed-timing playback scheduler.
//!
//! The ordinary scheduler journals every SH-2 bus phase so the board observes
//! each access at its exact clock. Playback can instead execute each CPU with
//! immediate functional memory for a short slice, then advance every device
//! over the same interval. Instruction and memory-wait clocks still come from
//! the interpreter and the BSC/SCU/VDP2 wait models; only the interleaving of
//! CPU accesses with device time is coarser. This mode is not a conformance
//! oracle: hardware tests keep using the exact scheduler.
use crate::bus::Processor;
use crate::cpu::{Cpu, CpuError, CpuStatus};
use crate::{Machine, MachineError, onchip};

/// Fixed-domain clocks each CPU may run ahead of the devices.
const SLICE: u32 = 128;

/// Longest single-instruction execution (DIVU/MAC interlocks are shorter).
const MAX_STATES: u32 = 48;
/// Longest single-instruction delay including external bus waits.
const MAX_DELAY: u32 = 256;
/// Longest backward branch treated as a candidate polling loop.
const LOOP_SPAN: u32 = 64;

#[derive(Default)]
pub(crate) struct Fast {
    pub(crate) enabled: bool,
    /// Main clock at which each CPU issues its next instruction.
    time: [u64; 2],
    idle: [Option<LoopHead>; 2],
}

/// Architectural state at a short loop head. Devices do not change within
/// a slice, so an iteration that writes nothing and returns to an identical
/// state repeats until the next device update.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LoopHead {
    pc: u32,
    r: [u32; 16],
    sr: u32,
    gbr: u32,
    pr: u32,
    mach: u32,
    macl: u32,
    writes: u64,
}
impl LoopHead {
    fn capture(cpu: &Cpu, writes: u64) -> Self {
        let s = cpu.state();
        Self {
            pc: s.pc,
            r: s.r,
            sr: s.sr,
            gbr: s.gbr,
            pr: s.pr,
            mach: s.mach,
            macl: s.macl,
            writes,
        }
    }
}

impl Fast {
    pub(crate) fn reset(&mut self) {
        self.idle = [None; 2];
    }
}

impl Machine {
    /// Select relaxed-timing playback. Changing the mode takes effect at the
    /// next field; the exact scheduler remains the default.
    pub fn set_fast_mode(&mut self, enabled: bool) {
        self.fast.enabled = enabled;
        self.bus.set_fast_video(enabled);
    }
    pub fn fast_mode(&self) -> bool {
        self.fast.enabled
    }

    /// The exact scheduler can leave an instruction between bus phases.
    /// Switch only at a completed architectural boundary.
    pub(crate) fn fast_ready(&self) -> bool {
        self.fast.enabled
            && self.fault.is_none()
            && self.cpu_boundary.iter().all(Option::is_none)
            && self.cached_issue.iter().all(Option::is_none)
            && self.bus.fast_bus_idle()
    }

    pub(crate) fn advance_frame_fast(&mut self, target: u64) -> Result<(), MachineError> {
        // The exact scheduler (debugger steps) may have run since the last
        // relaxed field; resume from its CPU readiness.
        let now = self.bus.main_clocks;
        self.fast.time = [self.cpu_ready[0].max(now), self.cpu_ready[1].max(now)];
        self.bus.set_fast_running(true);
        let result = self.run_fast_fields(target);
        self.bus.set_fast_running(false);
        result?;
        self.cpu_ready = [
            self.fast.time[0].max(self.bus.main_clocks),
            self.fast.time[1].max(self.bus.main_clocks),
        ];
        #[cfg(any(test, feature = "video-worker"))]
        self.bus.drain_video_journal();
        Ok(())
    }

    fn run_fast_fields(&mut self, target: u64) -> Result<(), MachineError> {
        while self.frames() < target {
            let end = self.bus.main_clocks + u64::from(self.bus.fast_main_span(SLICE));
            self.fast_run_cpu(Processor::Master, end)?;
            if self.bus.slave_enabled {
                self.fast_run_cpu(Processor::Slave, end)?;
            }
            // Devices follow the CPUs over the same interval.
            let slave_was = self.bus.slave_enabled;
            let command = self.bus.fast_advance(SLICE).map_err(MachineError::Scu)?;
            self.clocks += u64::from(SLICE);
            self.complete_hardware_boundary(command)?;
            let now = self.bus.main_clocks;
            if self.bus.slave_enabled && !slave_was {
                self.fast.time[1] = now;
            }
            // A clock change or reset can rewind no CPU behind the board.
            for time in &mut self.fast.time {
                *time = (*time).max(now.saturating_sub(u64::from(self.bus.fast_main_span(SLICE))));
            }
        }
        Ok(())
    }

    fn fast_run_cpu(&mut self, processor: Processor, end: u64) -> Result<(), MachineError> {
        let i = processor as usize;
        let slave = processor == Processor::Slave;
        let wrap = |error: CpuError| {
            if slave {
                MachineError::SlaveCpu(error)
            } else {
                MachineError::Cpu(error)
            }
        };
        let mut check_irq = true;
        while self.fast.time[i] < end {
            if self.bus.cpu_reset_active(processor) {
                self.bus.hold_cpu_reset(processor);
                self.cpu_reset_vectors[i] = None;
                *self.fast_cpu(processor) = Cpu::default();
                self.fast.time[i] = end;
                return Ok(());
            }
            if let Some(reset) = self.cpu_reset_vectors[i].take() {
                let memory = self.bus.for_cpu(processor);
                let cpu = match reset {
                    onchip::ResetRequest::PowerOn => Cpu::power_on(&memory),
                    onchip::ResetRequest::Manual => Cpu::manual_reset(&memory),
                }
                .map_err(|source| wrap(CpuError::Bus { pc: 0, source }))?;
                *self.fast_cpu(processor) = cpu;
                continue;
            }
            if !self.bus.nmi_ready(processor) {
                // Software standby: only NMI (after WDT settling) resumes.
                self.fast.time[i] = end;
                return Ok(());
            }
            self.bus.set_fast_cpu_time(processor, self.fast.time[i]);
            if !slave && self.nmi_pending {
                let mut memory = self.bus.for_cpu(processor);
                if self.cpu.accept_nmi(&mut memory).map_err(wrap)? {
                    self.nmi_pending = false;
                    self.nmi_detected = None;
                    self.fast.time[i] += 1;
                    continue;
                }
            }
            if check_irq {
                check_irq = false;
                if let Some(irq) = self.bus.fast_interrupt(processor) {
                    let mask = (self.fast_cpu(processor).state().sr >> 4) & 15;
                    if u32::from(irq.level) > mask {
                        self.bus.begin_fast_vector(irq);
                        let (cpu, bus) = if slave {
                            (&mut self.slave, &mut self.bus)
                        } else {
                            (&mut self.cpu, &mut self.bus)
                        };
                        let result = cpu.accept_interrupt(
                            &mut bus.for_cpu(processor),
                            irq.level,
                            irq.vector,
                        );
                        self.bus.end_fast_vector();
                        if result.map_err(wrap)? {
                            self.fast.idle[i] = None;
                            let clocks = self.fast_cpu(processor).last_clocks();
                            self.fast.time[i] += u64::from(self.fast_delay(processor, clocks));
                            continue;
                        }
                        // Entry is deferred (delay slot); retry after the next instruction.
                        check_irq = true;
                    }
                }
            }
            if self.bus.scu.dma_pending() || self.bus.fast_dmac_holds_bus(processor) {
                // SCU DMA owns the board bus, and the CPU's own block DMAC
                // keeps it; CPU external cycles wait for them.
                self.fast.time[i] = end;
                return Ok(());
            }
            let status = self.fast_cpu(processor).state().status.clone();
            match status {
                CpuStatus::Running => (),
                CpuStatus::Sleeping | CpuStatus::Reset | CpuStatus::Halted => {
                    self.fast.time[i] = end;
                    return Ok(());
                }
                CpuStatus::Faulted(error) => return Err(wrap(error)),
            }
            let (cpu, bus) = if slave {
                (&mut self.slave, &mut self.bus)
            } else {
                (&mut self.cpu, &mut self.bus)
            };
            let (pc, sr) = (cpu.state().pc, cpu.state().sr);
            let result = cpu.step(&mut bus.for_cpu(processor));
            let clocks = cpu.last_clocks();
            let (next, next_sr) = (cpu.state().pc, cpu.state().sr);
            // Lowering the mask (RTE, LDC) can admit a pending request.
            check_irq |= next_sr & 0xF0 < sr & 0xF0;
            if next < pc && pc - next <= LOOP_SPAN {
                let head = LoopHead::capture(cpu, bus.fast_writes());
                if self.fast.idle[i] == Some(head) {
                    result.map_err(wrap)?;
                    self.fast.time[i] = end;
                    return Ok(());
                }
                self.fast.idle[i] = Some(head);
            }
            result.map_err(wrap)?;
            self.fast.time[i] += u64::from(self.fast_delay(processor, clocks));
        }
        Ok(())
    }

    /// Instruction states plus accumulated bus waits. Without a clock domain,
    /// the interpreter's multiplier/pipeline interlocks are not meaningful, and
    /// waits recorded by device DMA are not this instruction's; bound both.
    fn fast_delay(&mut self, processor: Processor, states: u32) -> u32 {
        self.bus
            .cpu_main_delay(processor, states.min(MAX_STATES))
            .min(MAX_DELAY)
    }

    fn fast_cpu(&mut self, processor: Processor) -> &mut Cpu {
        match processor {
            Processor::Master => &mut self.cpu,
            Processor::Slave => &mut self.slave,
        }
    }
}
