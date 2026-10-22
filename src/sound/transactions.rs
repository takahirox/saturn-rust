//! MC68000UM §5.1: a byte/word data cycle takes four sound-CPU clocks.
//! Journal completed accesses so instruction replay cannot repeat MMIO reads
//! or already-published writes. Other clock domains see data at completion.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Read(u32, usize),
    Write(u32, usize, u32),
    ResetOutput,
    InterruptAcknowledge(u8),
}
impl Operation {
    fn complete(self, memory: &mut impl Memory) -> Result<u32, BusError> {
        match self {
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

#[derive(Default)]
pub(super) struct Transactions {
    completed: Vec<(Operation, Result<u32, BusError>)>,
    cursor: usize,
    waiting: Option<Operation>,
    start: u64,
    reset: Option<ResetSequence>,
}
#[derive(Default)]
struct ResetSequence {
    word: usize,
    high: u16,
}
impl Transactions {
    pub fn reset() -> Self {
        Self {
            reset: Some(ResetSequence::default()),
            ..Self::default()
        }
    }
}
struct Replay<'a>(RefCell<&'a mut Transactions>);
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
            return result;
        }
        journal.waiting = Some(operation);
        Err(BusError::Pending)
    }
}
impl Memory for Replay<'_> {
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
            let value = SoundBus {
                ram: &mut self.ram,
                scsp: &mut self.scsp,
            }
            .read16(address & 0xFF_FFFF);
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
        let journal = &mut self.transactions;
        if let Some(operation) = journal.waiting.take() {
            let result = operation.complete(&mut SoundBus {
                ram: &mut self.ram,
                scsp: &mut self.scsp,
            });
            journal.completed.push((operation, result));
        } else {
            journal.start = self.cpu_ticks;
            self.cpu.set_interrupt(self.scsp.interrupt_level());
        }
        journal.cursor = 0;
        let before = self.cpu.clone();
        let clocks = self.cpu.step(&mut Replay(RefCell::new(journal)));
        let journal = &mut self.transactions;
        if journal.waiting.is_some() {
            self.cpu = before;
            if journal.waiting == Some(Operation::ResetOutput) {
                self.reset_output_start = Some(
                    self.cpu_origin + ((journal.start + 4) * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ),
                );
                self.reset_output_end = Some(
                    self.cpu_origin + ((journal.start + 128) * REFERENCE_HZ).div_ceil(SOUND_CPU_HZ),
                );
                self.cpu_ticks = journal.start + 128;
            } else {
                self.cpu_ticks += 4;
            }
        } else {
            self.cpu_ticks = self.cpu_ticks.max(journal.start + u64::from(clocks));
            journal.completed.clear();
        }
    }
}
