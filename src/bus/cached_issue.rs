//! Prepare a CPU's private IF stages and retain the exact retirement edge.
//! Data phases still use the ordinary bus arbiter.
use super::{Bus, BusError, Cache, Cell, Memory, Processor, cached::ordinary_memory};
use crate::cache::CacheTouch;
use crate::cpu::{CachedInstruction, Cpu, PreparedBranch};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CachedLoad {
    pub(crate) plan: CachedInstruction,
    pub(crate) future: u16,
    pub(super) scheduled: bool,
    pub(super) cd_word: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CachedStore {
    pub(crate) plan: CachedInstruction,
    pub(crate) future: u16,
    pub(crate) value: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CachedBranch {
    plan: PreparedBranch,
    touches: [Option<CacheTouch>; 5],
    retained: Option<(u32, u16)>,
}

struct BranchMemory<'a> {
    cache: &'a Cache,
    clock: u64,
    end: u64,
    touches: Cell<[Option<CacheTouch>; 5]>,
    retained: Cell<Option<(u32, u16)>>,
}

impl BranchMemory<'_> {
    fn resident_plan(&mut self, cpu: &Cpu) -> Option<PreparedBranch> {
        let plan = cpu.prepare_resident_control(self.clock)?;
        if plan.at > self.end {
            return None;
        }
        let mut words = [0; 3];
        for (i, &(address, _, discard)) in plan.fetches.iter().flatten().enumerate() {
            if discard {
                self.sh2_discard_prefetch();
            }
            words[i] = self.fetch16(address).ok()?;
        }
        // Every IF above used a retained or resident cache word. The plan
        // already accounts for the pipeline waits between those same reads.
        self.clock = plan.at;
        Some(plan.finish(words))
    }

    fn into_branch(self, plan: PreparedBranch) -> CachedBranch {
        CachedBranch {
            plan,
            touches: self.touches.into_inner(),
            retained: self.retained.get(),
        }
    }
}

impl Memory for BranchMemory<'_> {
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock)
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        let clock = self.clock + u64::from(clocks);
        if clock > self.end {
            return Err(BusError::Pending);
        }
        self.clock = clock;
        Ok(())
    }
    fn sh2_speculative_fetch(&self, address: u32) -> bool {
        // Match the ordinary adapter's decision even on an unsupported
        // target. fetch16 then rejects anything outside resident code.
        super::CpuBus::instruction_prefetchable(address)
    }
    fn sh2_discard_prefetch(&self) {
        self.retained.set(None);
    }
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        if !ordinary_memory(address) || address & 1 != 0 {
            return Err(BusError::Pending);
        }
        if let Some((at, value)) = self.retained.get()
            && at == address
        {
            self.retained.set(None);
            return Ok(value);
        }
        let (words, touch): ([u8; 4], _) = self
            .cache
            .peek_cached_touch(address & !3)
            .ok_or(BusError::Pending)?;
        let mut touches = self.touches.get();
        *touches
            .iter_mut()
            .find(|touch| touch.is_none())
            .ok_or(BusError::Pending)? = Some(touch);
        self.touches.set(touches);
        if address & 2 == 0 {
            self.retained.set(Some((
                address + 2,
                u16::from_be_bytes([words[2], words[3]]),
            )));
            Ok(u16::from_be_bytes([words[0], words[1]]))
        } else {
            self.retained.set(None);
            Ok(u16::from_be_bytes([words[2], words[3]]))
        }
    }
    fn read8(&self, _address: u32) -> Result<u8, BusError> {
        Err(BusError::Pending)
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
    pub(crate) fn prepare_cached_load(
        &mut self,
        processor: Processor,
        cpu: &Cpu,
    ) -> Option<(CachedLoad, bool)> {
        let op = cpu.decoded_opcode()?;
        let index = processor as usize;
        // Each admitted operand keeps its real MA and external bus phases.
        let address = cpu.cached_load_address(op)?;
        let physical = Bus::physical(address);
        let ftcsr = address == crate::onchip::FRT + 1;
        #[cfg(any(test, feature = "kernel-experiment"))]
        let prepared_cd_load = self.prepared_cd_load;
        #[cfg(not(any(test, feature = "kernel-experiment")))]
        let prepared_cd_load = true;
        let cd = prepared_cd_load
            && (0x2000_0000..0x4000_0000).contains(&address)
            && (matches!(physical, crate::cd::DATA | crate::cd::WORD_DATA)
                || physical == crate::cd::DATA + 2
                || matches!(
                    physical.wrapping_sub(crate::cd::REGISTERS),
                    8 | 12 | 0x18 | 0x1C | 0x20 | 0x24
                ));
        if !ordinary_memory(cpu.state().pc)
            || !self.onchip[index].cached_batch_allowed()
            || (!ftcsr
                && !cd
                && (address >= 0x4000_0000
                    || !ordinary_memory(physical)
                    || (physical < super::BIOS_SIZE as u32 && self.bios.is_none())))
        {
            return None;
        }
        let now = self.main_clocks;
        let plan = cpu.prepare_cached_instruction(op, now)?;
        let (_, width) = plan.operand?;
        if (ftcsr && width != 1)
            || (cd && width != 2 && !(width == 4 && physical == crate::cd::DATA))
            || plan.issue_at() != now
        {
            return None;
        }
        let scheduled = !self.cache[index].borrow().read_hit(address);
        let future = self.fetch_resident_instruction(processor, plan.future)?;
        self.onchip[index].count_cached_instructions(1);
        #[cfg(any(test, feature = "kernel-experiment"))]
        if cd {
            self.prepared_cd_loads += 1;
        }
        let wait = plan.load_wait(now, scheduled);
        if wait != 0 {
            self.queue_cached_pipeline_wait(processor, wait);
        }
        Some((
            CachedLoad {
                plan,
                future,
                scheduled,
                cd_word: cd && scheduled && width == 2 && address & 1 == 0,
            },
            wait != 0,
        ))
    }

    pub(crate) fn prepare_cached_store(
        &mut self,
        processor: Processor,
        cpu: &Cpu,
    ) -> Option<(CachedStore, bool)> {
        let op = cpu.decoded_opcode()?;
        cpu.cached_store_address(op)?;
        let index = processor as usize;
        if !ordinary_memory(cpu.state().pc) || !self.onchip[index].cached_batch_allowed() {
            return None;
        }
        let now = self.main_clocks;
        let plan = cpu.prepare_cached_store(op, now)?;
        let value = plan.store_value()?;
        let (address, _) = plan.operand?;
        if !super::cached::buffered_memory(address) || plan.issue_at() != now {
            return None;
        }
        let future = self.fetch_resident_instruction(processor, plan.future)?;
        self.onchip[index].count_cached_instructions(1);
        let wait = plan.load_wait(now, true);
        if wait != 0 {
            self.queue_cached_pipeline_wait(processor, wait);
        }
        Some((
            CachedStore {
                plan,
                future,
                value,
            },
            wait != 0,
        ))
    }

    pub(crate) fn prepared_memory_retired(&self, processor: Processor, pc: u32) {
        self.onchip[processor as usize].instruction_break(pc, true);
    }

    pub(crate) fn prepare_cached_branch(
        &mut self,
        processor: Processor,
        initial: &Cpu,
    ) -> Option<(CachedBranch, bool)> {
        let op = initial.decoded_opcode()?;
        let index = processor as usize;
        if !initial.has_cached_control(op)
            || !ordinary_memory(initial.state().pc)
            || !self.onchip[index].resident_issue_allowed()
        {
            return None;
        }
        // No UBC or reset transition may see an intermediate IF. A newly
        // issued SMPC reset command takes more clocks than this plan can span.
        let fixed = self.smpc.clocks_to_cpu_reset().map_or(u32::MAX, |clocks| {
            self.system_clock
                .preview(clocks.saturating_sub(1))
                .saturating_add(self.pending_main_clocks)
        });
        let span = self.onchip[index]
            .clocks_to_event()
            .saturating_sub(1)
            .min(fixed)
            .min(8);
        let cache = self.cache[index].borrow();
        let clock = self.main_clocks;
        let retained = self.instruction_word[index].get();
        let make_memory = || BranchMemory {
            cache: &cache,
            clock,
            end: clock + u64::from(span),
            touches: Cell::new([None; 5]),
            retained: Cell::new(retained),
        };
        let mut memory = make_memory();
        #[cfg(any(test, feature = "kernel-experiment"))]
        let try_resident = self.resident_branch_issues;
        #[cfg(not(any(test, feature = "kernel-experiment")))]
        let try_resident = true;
        let plan = if try_resident {
            if let Some(plan) = memory.resident_plan(initial) {
                #[cfg(debug_assertions)]
                {
                    let mut literal = make_memory();
                    let expected = initial.prepare_cached_control(&mut literal).unwrap();
                    debug_assert!(plan == expected && memory.clock == literal.clock);
                    debug_assert!(memory.touches.get() == literal.touches.get());
                    debug_assert!(memory.retained.get() == literal.retained.get());
                }
                #[cfg(any(test, feature = "kernel-experiment"))]
                {
                    self.resident_branch_issue_count += 1;
                }
                plan
            } else {
                // A denied operand can follow successful IF peeks. Discard
                // those private touches before retrying the original plan.
                memory = make_memory();
                initial.prepare_cached_control(&mut memory)?
            }
        } else {
            initial.prepare_cached_control(&mut memory)?
        };
        let wait = (memory.clock - clock) as u32;
        let branch = memory.into_branch(plan);
        drop(cache);
        if wait != 0 {
            self.queue_cached_pipeline_wait(processor, wait);
        }
        Some((branch, wait != 0))
    }

    pub(crate) fn commit_cached_branch(
        &mut self,
        processor: Processor,
        branch: CachedBranch,
    ) -> PreparedBranch {
        let index = processor as usize;
        for touch in branch.touches.into_iter().flatten() {
            self.cache[index].get_mut().publish_touch(touch);
        }
        self.instruction_word[index].set(branch.retained);
        self.onchip[index].count_cached_instructions(1);
        branch.plan
    }
}

impl CachedBranch {
    pub(crate) fn shift_private_clock(&mut self, elapsed: u64) {
        self.plan.shift_private_clock(elapsed);
    }
}

impl CachedBranch {
    pub(crate) fn private_code_addresses(&self) -> [Option<u32>; 2] {
        self.plan.private_code_addresses()
    }
}
