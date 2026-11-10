//! MC68000UM §5.1: a byte/word data cycle takes four sound-CPU clocks.
//! Journal completed accesses so instruction replay cannot repeat MMIO reads
//! or already-published writes. Other clock domains see data at completion.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    InternalWait(u32),
    FinishClocks(u32),
    Read(u32, usize),
    Write(u32, usize, u32),
    ResetOutput,
    InterruptAcknowledge(u8),
}
impl Operation {
    fn complete(self, memory: &mut impl Memory) -> Result<u32, BusError> {
        match self {
            Self::InternalWait(_) | Self::FinishClocks(_) => Ok(0),
            Self::ResetOutput => Ok(0),
            Self::InterruptAcknowledge(level) => {
                memory.m68k_interrupt_acknowledge(level).map(u32::from)
            }
            Self::Read(a, 1) => memory.read8(a).map(u32::from),
            Self::Read(a, 2) => memory.read16(a).map(u32::from),
            Self::Write(a, 1, v) => memory.write8(a, v as u8).map(|_| v),
            Self::Write(a, 2, v) => memory.write16(a, v as u16).map(|_| v),
            _ => unreachable!("68000 bus cycles are byte or word accesses"),
        }
    }
}

#[derive(Default, Clone)]
pub(super) struct Transactions {
    // Successful replay is the hot path. Keep large diagnostic errors out of
    // each completed access; their original values remain available by index.
    completed: Vec<(Operation, Result<u32, usize>)>,
    errors: Vec<BusError>,
    cursor: usize,
    waiting: Option<Operation>,
    start: u64,
    #[cfg(test)]
    literal_finishes: bool,
    #[cfg(test)]
    elided_finishes: u64,
    stalls: u64,
    reset: Option<ResetSequence>,
    cpu: Option<M68k>,
}
#[derive(Default, Clone)]
struct ResetSequence {
    word: usize,
    high: u16,
}
impl Transactions {
    /// A RAM transfer cannot change SCSP configuration or signal the main CPU.
    /// A newly issued control write/reset needs at least four sound CPU clocks.
    pub fn control_write_pending(&self) -> bool {
        matches!(self.waiting, Some(Operation::Write(a, _, _)) if a & 0xFF_FFFF >= SOUND_RAM_SIZE as u32)
            || self.waiting == Some(Operation::ResetOutput)
    }
    pub fn ram_request(&self, pc: u32) -> Option<memory::Request> {
        let (address, width, write) = if let Some(reset) = &self.reset {
            let address = if reset.word < 4 {
                reset.word as u32 * 2
            } else {
                pc.wrapping_add((reset.word as u32 - 4) * 2) & 0xFF_FFFF
            };
            (address, 2, None)
        } else {
            match self.waiting? {
                Operation::Read(a, width) => (a, width, None),
                Operation::Write(a, width, value) => (a, width, Some(value)),
                _ => return None,
            }
        };
        (address < SOUND_RAM_SIZE as u32 && (width == 1 || address & 1 == 0)).then_some(
            memory::Request {
                address: address as usize,
                width,
                write,
            },
        )
    }
    pub fn stall(&mut self, clocks: u64) {
        if self.cpu.is_some() {
            self.stalls += clocks;
        }
    }
    pub fn reset() -> Self {
        Self {
            reset: Some(ResetSequence::default()),
            ..Self::default()
        }
    }
}
struct Replay<'a>(RefCell<&'a mut Transactions>, u64);
impl Replay<'_> {
    fn access(&self, operation: Operation) -> Result<u32, BusError> {
        let mut journal = self.0.borrow_mut();
        if journal.waiting.is_some() {
            // The core may attempt exception entry after Pending. That entire
            // speculative CPU state is discarded; no extra bus cycle may issue.
            return Err(BusError::Pending);
        }
        if let Some(&(old, result)) = journal.completed.get(journal.cursor) {
            assert_eq!(old, operation, "68k replay changed its bus sequence");
            journal.cursor += 1;
            return result.map_err(|index| journal.errors[index]);
        }
        journal.waiting = Some(operation);
        Err(BusError::Pending)
    }
}
impl Memory for Replay<'_> {
    fn m68k_finish_clocks(&self, clocks: u32) -> Result<(), BusError> {
        let ready = {
            let journal = self.0.borrow();
            #[cfg(test)]
            let enabled = !journal.literal_finishes;
            #[cfg(not(test))]
            let enabled = true;
            enabled
                && journal.waiting.is_none()
                && self.1 >= journal.start + journal.stalls + u64::from(clocks)
                && !journal
                    .completed
                    .get(journal.cursor)
                    .is_some_and(|(op, _)| *op == Operation::FinishClocks(clocks))
        };
        if ready {
            // This internal barrier has already elapsed. It owns no RAM grant
            // or physical bus phase. A recorded completion still consumes its
            // journal entry above, including when trace replays an older tail.
            #[cfg(test)]
            {
                self.0.borrow_mut().elided_finishes += 1;
            }
            return Ok(());
        }
        self.access(Operation::FinishClocks(clocks)).map(|_| ())
    }
    fn m68k_internal_wait(&self, clocks: u32) -> Result<(), BusError> {
        self.access(Operation::InternalWait(clocks)).map(|_| ())
    }
    fn m68k_interrupt_acknowledge(&mut self, level: u8) -> Result<u8, BusError> {
        self.access(Operation::InterruptAcknowledge(level))
            .map(|v| v as u8)
    }
    fn m68k_reset_output(&mut self) -> Result<(), BusError> {
        self.access(Operation::ResetOutput).map(|_| ())
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.access(Operation::Read(a, 1)).map(|v| v as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.access(Operation::Read(a, 2)).map(|v| v as u16)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        Ok(u32::from(self.read16(a)?) << 16 | u32::from(self.read16(a.wrapping_add(2))?))
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.access(Operation::Write(a, 1, u32::from(v)))
            .map(|_| ())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.access(Operation::Write(a, 2, u32::from(v)))
            .map(|_| ())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.write16(a, (v >> 16) as u16)?;
        self.write16(a.wrapping_add(2), v as u16)
    }
}
impl Sound {
    pub(super) fn advance_cpu_cycle(&mut self) {
        if let Some(reset) = &mut self.transactions.reset {
            let address = if reset.word < 4 {
                reset.word as u32 * 2
            } else {
                self.cpu.state.pc.wrapping_add((reset.word as u32 - 4) * 2)
            };
            let value = if (address & 0xFF_FFFF) < SOUND_RAM_SIZE as u32 && address & 1 == 0 {
                Ok(self
                    .memory
                    .take(memory::Owner::Cpu)
                    .expect("reset read completed") as u16)
            } else {
                SoundBus {
                    ram: &mut self.ram,
                    scsp: &mut self.scsp,
                }
                .read16(address & 0xFF_FFFF)
            };
            let Ok(value) = value else {
                // §5.4.2/§5.5: BERR during reset halts, without a stack frame.
                self.cpu.state.halted = true;
                self.transactions.reset = None;
                self.cpu_ticks += 4;
                return;
            };
            match reset.word {
                0 | 2 => reset.high = value,
                1 => {
                    self.cpu.state.a[7] = u32::from(reset.high) << 16 | u32::from(value);
                    self.cpu.state.ssp = self.cpu.state.a[7];
                }
                3 => self.cpu.state.pc = u32::from(reset.high) << 16 | u32::from(value),
                _ => self.cpu.reset_prefetch(address, value),
            }
            reset.word += 1;
            if reset.word < 6 {
                self.cpu_ticks += 4;
                return;
            }
            self.transactions.reset = None;
        }
        let ram_result = self.memory.take(memory::Owner::Cpu);
        assert_eq!(
            ram_result.is_some(),
            self.transactions.ram_request(self.cpu.state.pc).is_some(),
            "every scheduled RAM access must consume exactly one admitted completion"
        );
        let journal = &mut self.transactions;
        if let Some(operation) = journal.waiting.take() {
            let result = ram_result.map_or_else(
                || {
                    operation.complete(&mut SoundBus {
                        ram: &mut self.ram,
                        scsp: &mut self.scsp,
                    })
                },
                Ok,
            );
            let result = result.map_err(|error| {
                let index = journal.errors.len();
                journal.errors.push(error);
                index
            });
            journal.completed.push((operation, result));
        } else {
            journal.start = self.cpu_ticks;
            journal.stalls = 0;
            journal.cpu = Some(self.cpu.begin_replay());
        }
        journal.cursor = 0;
        let mut execution = journal.cpu.as_ref().unwrap().clone();
        let clocks = execution.step(&mut Replay(RefCell::new(journal), self.cpu_ticks));
        let journal = &mut self.transactions;
        if journal.waiting.is_some() {
            if let Some(state) = execution.retired_state() {
                self.cpu.state = state.clone();
            }
            if journal.waiting == Some(Operation::ResetOutput) {
                self.reset_output_start = Some(
                    self.cpu_origin
                        + ((journal.start + journal.stalls + 4) * REFERENCE_HZ)
                            .div_ceil(SOUND_CPU_HZ),
                );
                self.reset_output_end = Some(
                    self.cpu_origin
                        + ((journal.start + journal.stalls + 128) * REFERENCE_HZ)
                            .div_ceil(SOUND_CPU_HZ),
                );
                self.cpu_ticks = journal.start + journal.stalls + 128;
            } else if let Some(Operation::InternalWait(clocks)) = journal.waiting {
                self.cpu_ticks += u64::from(clocks);
            } else if let Some(Operation::FinishClocks(clocks)) = journal.waiting {
                // Data cycles already published remain in the journal. Only
                // the remaining internal clocks delay architectural results;
                // trace stacking cannot begin until this barrier completes.
                self.cpu_ticks = self
                    .cpu_ticks
                    .max(journal.start + journal.stalls + u64::from(clocks));
            } else {
                self.cpu_ticks += 4;
            }
        } else {
            execution.finish_replay(&self.cpu);
            self.cpu = execution;
            self.cpu_ticks = self
                .cpu_ticks
                .max(journal.start + journal.stalls + u64::from(clocks));
            journal.completed.clear();
            journal.errors.clear();
            journal.cpu = None;
        }
    }
}

#[cfg(test)]
#[path = "irq_tests.rs"]
mod irq_tests;

#[cfg(test)]
#[path = "finish_tests.rs"]
mod finish_tests;
