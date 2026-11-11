//! A resident FTCSR polling period uses the peer's private cache and
//! peripheral bus. Master work retains every shared observer deadline.
use super::{Bus, BusError, Cache, Cell, Memory, cached::ordinary_memory};
use crate::{cache::CacheTouch, cpu::Cpu, onchip::FRT};

pub(crate) struct PollCycle {
    pub(crate) period: u64,
    pub(crate) mask: u32,
    retirements: u32,
    template: usize,
    instructions: u64,
    local: u64,
    touches: [Option<CacheTouch>; 16],
    status: u8,
}
impl PollCycle {
    pub(crate) fn can_retire_at(&self, elapsed: u64) -> bool {
        let phase = elapsed % self.period;
        elapsed != 0 && (phase == 0 || (phase < 32 && self.retirements & (1 << phase) != 0))
    }
}

pub(super) struct PollTemplate {
    initial: Cpu,
    origin: u64,
    retained: Option<(u32, u16)>,
    fetched: [Option<FetchCheck>; 16],
    cycle: PollCycle,
    prefixes: Vec<PollPrefix>,
}
struct PollPrefix {
    at: u64,
    ready: u64,
    cpu: Cpu,
    touches: [Option<CacheTouch>; 16],
    retained: Option<(u32, u16)>,
    read: bool,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct FetchCheck {
    address: u32,
    words: [u8; 4],
    touch: CacheTouch,
}

struct PollMemory<'a> {
    cache: &'a Cache,
    clock: Cell<u64>,
    end: u64,
    completion: Cell<u64>,
    fetched: Cell<[Option<FetchCheck>; 16]>,
    retained: Cell<Option<(u32, u16)>>,
    reads: Cell<u8>,
    status: u8,
}
impl PollMemory<'_> {
    fn move_to(&self, clock: u64) -> Result<(), BusError> {
        if clock > self.end {
            return Err(BusError::Pending);
        }
        self.clock.set(clock);
        Ok(())
    }
}
impl Memory for PollMemory<'_> {
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock.get())
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_speculative_fetch(&self, address: u32) -> bool {
        super::CpuBus::instruction_prefetchable(address)
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        self.move_to(self.clock.get() + u64::from(clocks))
    }
    fn sh2_schedule_memory_access(&self, address: u32, write: bool, completion: u64) -> bool {
        if address == FRT + 1 && !write {
            self.completion.set(completion);
        }
        true
    }
    fn sh2_discard_prefetch(&self) {
        self.retained.set(None);
    }
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        if !ordinary_memory(address) || address & 1 != 0 {
            return Err(BusError::Pending);
        }
        if let Some((at, word)) = self.retained.get()
            && at == address
        {
            self.retained.set(None);
            return Ok(word);
        }
        let (words, touch) = self
            .cache
            .peek_cached_touch(address & !3)
            .ok_or(BusError::Pending)?;
        let mut fetched = self.fetched.get();
        *fetched
            .iter_mut()
            .find(|p| p.is_none())
            .ok_or(BusError::Pending)? = Some(FetchCheck {
            address: address & !3,
            words,
            touch,
        });
        self.fetched.set(fetched);
        let high = u16::from_be_bytes([words[0], words[1]]);
        let low = u16::from_be_bytes([words[2], words[3]]);
        if address & 2 == 0 {
            self.retained.set(Some((address + 2, low)));
            Ok(high)
        } else {
            self.retained.set(None);
            Ok(low)
        }
    }
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        if address != FRT + 1 || self.reads.get() != 0 {
            return Err(BusError::Pending);
        }
        // An uncontended CPU peripheral read consumes two states. The
        // instruction's MA horizon can hold the grant for longer.
        self.move_to((self.clock.get() + 2).max(self.completion.get()))?;
        self.reads.set(1);
        Ok(self.status)
    }
    fn read16(&self, _address: u32) -> Result<u16, BusError> {
        Err(BusError::Pending)
    }
    fn read32(&self, _address: u32) -> Result<u32, BusError> {
        Err(BusError::Pending)
    }
    fn write8(&mut self, _address: u32, _value: u8) -> Result<(), BusError> {
        Err(BusError::Pending)
    }
    fn write16(&mut self, _address: u32, _value: u16) -> Result<(), BusError> {
        Err(BusError::Pending)
    }
    fn write32(&mut self, _address: u32, _value: u32) -> Result<(), BusError> {
        Err(BusError::Pending)
    }
}
impl Bus {
    pub(crate) fn prepare_poll_cycle(&mut self, initial: &Cpu) -> Option<PollCycle> {
        let chip = &self.onchip[1];
        let status = chip.ftcsr();
        if !self.cached_bus_compatible()
            || !chip.cached_batch_allowed()
            || chip.monitors_external_bus()
        {
            return None;
        }
        let origin = self.main_clocks;
        let mask = (initial.state().sr >> 4) & 15;
        let cache = self.cache[1].borrow();
        let retained = self.instruction_word[1].get();
        if let Some((index, template)) =
            self.poll_templates
                .iter()
                .enumerate()
                .find(|(_, template)| {
                    template.cycle.status == status
                        && template.retained == retained
                        && template.initial.state().pc == initial.state().pc
                        && initial.same_cached_cycle_origin(
                            &template.initial,
                            origin.saturating_sub(template.origin),
                        )
                        && template.fetched.iter().flatten().all(|check| {
                            cache.peek_cached_touch(check.address)
                                == Some((check.words, check.touch))
                        })
                })
        {
            return Some(PollCycle {
                period: template.cycle.period,
                mask,
                retirements: template.cycle.retirements,
                template: index,
                instructions: template.cycle.instructions,
                local: template.cycle.local,
                touches: template.cycle.touches,
                status,
            });
        }
        let mut memory = PollMemory {
            cache: &cache,
            clock: Cell::new(origin),
            end: origin + 32,
            completion: Cell::new(0),
            fetched: Cell::new([None; 16]),
            retained: Cell::new(retained),
            reads: Cell::new(0),
            status,
        };
        let mut cpu = initial.clone();
        let mut retirements = 0;
        let mut prefixes = Vec::new();
        for instructions in 1..=8 {
            let op = cpu.decoded_opcode()?;
            if !Cpu::is_scalar(op)
                && !cpu.has_cached_control(op)
                && cpu.cached_load_address(op) != Some(FRT + 1)
            {
                return None;
            }
            let started = memory.clock.get();
            if !cpu.step_cached(&mut memory)
                || cpu.state().instructions != initial.state().instructions + instructions
                || (cpu.state().sr >> 4) & 15 != mask
            {
                return None;
            }
            let at = memory.clock.get();
            let ready =
                (cpu.execution_clock().unwrap_or(started) + u64::from(cpu.last_clocks())).max(at);
            prefixes.push(PollPrefix {
                at: at - origin,
                ready: ready - origin,
                cpu: cpu.clone(),
                touches: memory
                    .fetched
                    .get()
                    .map(|check| check.map(|read| read.touch)),
                retained: memory.retained.get(),
                read: memory.reads.get() != 0,
            });
            if at > origin && at - origin < 32 {
                retirements |= 1 << (at - origin);
            }
            if cpu.state().pc == initial.state().pc && memory.reads.get() == 1 {
                let period = ready - origin;
                if period == 0
                    || memory.retained.get() != retained
                    || !cpu.repeat_cached_cycle(initial, period, 0, None)
                {
                    return None;
                }
                let fetched = memory.fetched.into_inner();
                let touches = fetched.map(|check| check.map(|read| read.touch));
                let local = cpu.cached_local_clock() - initial.cached_local_clock();
                if self.poll_templates.len() == 8 {
                    self.poll_templates.remove(0);
                }
                let index = self.poll_templates.len();
                self.poll_templates.push(PollTemplate {
                    initial: initial.clone(),
                    origin,
                    retained,
                    fetched,
                    cycle: PollCycle {
                        period,
                        mask,
                        retirements,
                        template: index,
                        instructions,
                        local,
                        touches,
                        status,
                    },
                    prefixes,
                });
                return Some(PollCycle {
                    period,
                    mask,
                    retirements,
                    template: index,
                    instructions,
                    local,
                    touches,
                    status,
                });
            }
            memory.move_to(ready).ok()?;
        }
        None
    }
    pub(crate) fn commit_poll_cycle(
        &mut self,
        cycle: PollCycle,
        cpu: &mut Cpu,
        elapsed: u64,
    ) -> u64 {
        assert!(cycle.can_retire_at(elapsed));
        let periods = elapsed / cycle.period;
        let phase = elapsed % cycle.period;
        let chip = &self.onchip[1];
        if periods != 0 {
            cpu.publish_poll_periods(cycle.period, cycle.instructions, cycle.local, periods);
            assert_eq!(chip.read8(FRT + 1), Ok(cycle.status));
            chip.count_cached_instructions(cycle.instructions * periods);
            for touch in cycle.touches.into_iter().flatten() {
                self.cache[1].get_mut().publish_touch(touch);
            }
        }
        if phase == 0 {
            return self.main_clocks;
        }
        // Prefix states were computed by the shared interpreter during proof.
        // The admission guard rechecked every fetched byte and physical way.
        let template = &self.poll_templates[cycle.template];
        let prefix = template
            .prefixes
            .iter()
            .find(|prefix| prefix.at == phase)
            .expect("proven polling retirement");
        let mut next = prefix.cpu.clone();
        let retired = next.state().instructions - template.initial.state().instructions;
        next.rebase_poll_prefix(
            &template.initial,
            cpu,
            self.main_clocks - phase - template.origin,
        );
        *cpu = next;
        if prefix.read {
            assert_eq!(self.onchip[1].read8(FRT + 1), Ok(cycle.status));
        }
        self.instruction_word[1].set(prefix.retained);
        for touch in prefix.touches.into_iter().flatten() {
            self.cache[1].get_mut().publish_touch(touch);
        }
        self.onchip[1].count_cached_instructions(retired);
        self.main_clocks - phase + prefix.ready
    }
}

#[cfg(test)]
mod tests {
    use super::PollCycle;

    #[test]
    fn retirement_mask_does_not_wrap_past_its_recorded_clock_range() {
        let cycle = PollCycle {
            period: 35,
            mask: 15,
            retirements: 1 << 1,
            template: 0,
            instructions: 3,
            local: 0,
            touches: [None; 16],
            status: 0,
        };
        for elapsed in [1, 35, 36, 70] {
            assert!(cycle.can_retire_at(elapsed));
        }
        for elapsed in [0, 32, 33, 34, 68] {
            assert!(!cycle.can_retire_at(elapsed));
        }
    }
}
