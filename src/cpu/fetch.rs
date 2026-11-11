//! Bounded IF/ID state. P figs7.1/4/6/7: an IF belongs to a slot,
//! independently of the instruction whose EX occupies that slot.
use crate::bus::{BusError, Memory};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    pc: u32,
    value: Value,
}

// Keep exceptional device diagnostics out of the frequently cloned IF/ID
// state. A speculative non-fetch needs no allocation or fabricated fault.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Ready(u16),
    Pending,
    Fault(Box<BusError>),
}
impl Value {
    fn result(&self) -> Result<u16, BusError> {
        match self {
            Self::Ready(v) => Ok(*v),
            Self::Pending => Err(BusError::Pending),
            Self::Fault(e) => Err(**e),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Fetch {
    decoded: Option<Word>,
    fetched: Option<Word>,
    decode_ready: u64,
}

impl Fetch {
    pub(super) fn cached_pair(&self, pc: u32, clock: u64) -> Option<(u16, u16)> {
        self.cached_pair_for(pc, pc.wrapping_add(2), clock)
    }
    pub(super) fn cached_pair_for(
        &self,
        pc: u32,
        successor: u32,
        clock: u64,
    ) -> Option<(u16, u16)> {
        if self.decode_ready > clock {
            return None;
        }
        let decoded = self.decoded.as_ref().filter(|w| w.pc == pc)?;
        let fetched = self.fetched.as_ref().filter(|w| w.pc == successor)?;
        Some((decoded.value.result().ok()?, fetched.value.result().ok()?))
    }

    pub(super) fn finish_cached_pair(&mut self, pc: u32, words: (u16, u16), ready: u64) {
        self.decoded = Some(Word {
            pc,
            value: Value::Ready(words.0),
        });
        self.fetched = Some(Word {
            pc: pc.wrapping_add(2),
            value: Value::Ready(words.1),
        });
        self.decode_ready = ready;
    }

    pub fn shift_cached_clock(&mut self, clocks: u64) {
        if self.decode_ready != 0 {
            self.decode_ready += clocks;
        }
    }

    pub(super) fn decoded_opcode(&self, pc: u32) -> Option<u16> {
        self.decoded
            .as_ref()
            .filter(|word| word.pc == pc)?
            .value
            .result()
            .ok()
    }
    pub fn handler(&mut self, bus: &mut impl Memory, pc: u32) -> Result<(), BusError> {
        if bus.sh2_clock().is_some() {
            let start = bus.sh2_clock().unwrap();
            self.decoded = Some(Self::word(bus, pc)?);
            Self::finish_slot(bus, start)?;
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    pub fn prime(&mut self, bus: &impl Memory, pc: u32) {
        self.decoded = Some(Self::word(bus, pc).unwrap());
        self.fetched = Some(Self::word(bus, pc.wrapping_add(2)).unwrap());
    }

    fn word(bus: &impl Memory, pc: u32) -> Result<Word, BusError> {
        let value = bus.fetch16(pc);
        if value == Err(BusError::Pending) {
            return Err(BusError::Pending);
        }
        // An error on an overrun fetch is retained as data, never dispatched
        // until this address actually reaches the execution boundary.
        let value = match value {
            Ok(v) => Value::Ready(v),
            Err(error) => Value::Fault(Box::new(error)),
        };
        Ok(Word { pc, value })
    }

    fn finish_slot(bus: &mut impl Memory, start: u64) -> Result<(), BusError> {
        let now = bus.sh2_clock().unwrap();
        bus.sh2_pipeline_wait(start.saturating_add(1).saturating_sub(now) as u32)
    }

    pub fn select(&mut self, bus: &mut impl Memory, pc: u32) -> Result<u16, BusError> {
        if bus.sh2_clock().is_none() {
            return bus.fetch16(pc);
        }
        let now = bus.sh2_clock().unwrap();
        bus.sh2_pipeline_wait(self.decode_ready.saturating_sub(now) as u32)?;
        if self
            .decoded
            .as_ref()
            .is_none_or(|word| word.pc != pc || matches!(word.value, Value::Pending))
        {
            self.clear();
            let start = bus.sh2_clock().unwrap();
            self.decoded = Some(Self::word(bus, pc)?);
            Self::finish_slot(bus, start)?;
        }
        if self.fetched.is_none() {
            // ID of the selected instruction and IF/if of its successor.
            let start = bus.sh2_clock().unwrap();
            self.fetched = Some(Self::lookahead(bus, pc.wrapping_add(2))?);
            Self::finish_slot(bus, start)?;
        }
        self.decoded.as_ref().unwrap().value.result()
    }

    fn lookahead(bus: &impl Memory, pc: u32) -> Result<Word, BusError> {
        if bus.sh2_speculative_fetch(pc) {
            Self::word(bus, pc)
        } else {
            // No speculative MMIO and no speculative architectural error.
            // The ordinary checked fetch will run if this entry is selected.
            Ok(Word {
                pc,
                value: Value::Pending,
            })
        }
    }

    pub fn advance(&mut self, bus: &impl Memory, successor: u32, ex: u64) -> Result<(), BusError> {
        if bus.sh2_clock().is_none() {
            return Ok(());
        }
        self.decoded = self.fetched.take();
        self.fetched = Some(Self::lookahead(bus, successor.wrapping_add(2))?);
        self.decode_ready = (ex + 1).max(bus.sh2_clock().unwrap());
        Ok(())
    }

    pub fn hold_delay_slot(&mut self, bus: &impl Memory, ex: u64) {
        if let Some(clock) = bus.sh2_clock() {
            // P figs7.82/92: retain the slot without a younger sequential IF.
            // Redirect will supply its target successor after EX / both MAs.
            self.decoded = self.fetched.take();
            self.decode_ready = (ex + 1).max(clock);
        }
    }

    pub fn redirect(
        &mut self,
        bus: &mut impl Memory,
        target: u32,
        delayed: bool,
        target_if: u64,
    ) -> Result<(), BusError> {
        let Some(now) = bus.sh2_clock() else {
            self.clear();
            return Ok(());
        };
        // The caller supplies the target IF boundary: after branch EX
        // (P figs7.78/80/82), or after both RTE MAs (fig7.92).
        bus.sh2_pipeline_wait(target_if.saturating_sub(now) as u32)?;
        if delayed {
            // The slot is already decoded. Replace only its younger IF.
            self.fetched = Some(Self::lookahead(bus, target)?);
        } else {
            self.clear();
            let start = bus.sh2_clock().unwrap();
            self.decoded = Some(Self::lookahead(bus, target)?);
            Self::finish_slot(bus, start)?;
            // Complete target ID alongside its successor's IF before making
            // the redirected execution boundary available.
            let start = bus.sh2_clock().unwrap();
            self.fetched = Some(Self::lookahead(bus, target.wrapping_add(2))?);
            Self::finish_slot(bus, start)?;
        }
        Ok(())
    }
}
