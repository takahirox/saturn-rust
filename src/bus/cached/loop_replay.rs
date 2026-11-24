//! Experimental replay of resident memory effects from an observed CPU cycle.
use super::*;
use crate::bus::loop_probe::trace::CapturedLoop;

#[derive(Clone, Copy)]
pub(super) struct ReplayKey {
    pub first: usize,
    pub shift: u64,
    pub(super) ways: [u8; 256],
}
pub(super) struct Replayed {
    pub cpu: Cpu,
    pub ready: u64,
    pub at: u64,
    pub retired: u32,
    pub last: usize,
    pub key: ReplayKey,
}

impl Cached<'_> {
    pub(super) fn replay_loop(
        &mut self,
        data: &CapturedLoop,
        initial: &Cpu,
        count: u32,
        diagnostics: Option<&crate::bus::loop_probe::Mismatches>,
        mut permitted: Option<impl FnMut(u64) -> bool>,
    ) -> Option<Replayed> {
        if !data.complete
            || !data.multiplier_constant
            || !initial.loop_multiplier_idle()
            || self.passive.is_some()
            || self.refill_count.get() != 0
        {
            return None;
        }
        let current_clock = initial.execution_clock()?;
        let (first_index, shift) = data
            .by_pc
            .get(&initial.state().pc)?
            .candidates(diagnostics.is_some())
            .iter()
            .find_map(|&i| {
                let phase = &data.phases[i];
                if !phase.cpu.loop_multiplier_idle() {
                    return None;
                }
                let shift = current_clock.checked_sub(phase.cpu.execution_clock()?)?;
                if let Some(diagnostics) = diagnostics {
                    let [gpr, architecture, private] =
                        initial.cached_cycle_differences(&phase.cpu, shift);
                    let key = [
                        gpr,
                        architecture,
                        private,
                        u32::from(phase.ready.checked_add(shift) == Some(self.clock.get())),
                        u32::from(phase.retained == self.retained.get()),
                    ];
                    let mut rows = diagnostics.borrow_mut();
                    if rows.len() < 128 || rows.contains_key(&key) {
                        *rows.entry(key).or_default() += 1;
                    }
                }
                (phase.ready.checked_add(shift) == Some(self.clock.get())
                    && phase.retained == self.retained.get()
                    && initial.same_loop_origin(&phase.cpu, shift))
                .then_some((i, shift))
            })?;
        let first = &data.phases[first_index];
        if (first.cpu.state().sr ^ initial.state().sr) & 0xF0 != 0 {
            return None;
        }
        let timeline = data.timeline.as_ref()?;
        let mut last_index = timeline.prefix_end(first_index, count, self.end.get(), shift)?;
        if let Some(permitted) = &mut permitted {
            for (index, &at) in timeline
                .clocks
                .iter()
                .enumerate()
                .take(last_index + 1)
                .skip(first_index + 1)
            {
                if !permitted(at + shift) {
                    last_index = index - 1;
                    break;
                }
            }
        }
        if last_index == first_index {
            return None;
        }
        let origin = self.snapshot(Some(Checkpoint::Attempt));
        let result = (|| {
            let first_owner = first_index as u64 + 1;
            let last_owner = last_index as u64;
            if self.cache.control() != first.cache.control || self.version_count.get() != 0 {
                return None;
            }
            let mut way_map = [u8::MAX; 256];
            for guard in data.compiled.get(first_index)? {
                if guard.needed_by > last_owner as u32 {
                    continue;
                }
                let actual = self.cache.loop_guard_matches(
                    guard.address,
                    guard.ways,
                    guard.input_mask(last_owner as u32),
                    &guard.bytes,
                )?;
                if guard.ways != 0 {
                    let set = (guard.address >> 4 & 63) as usize;
                    way_map[set * 4 + guard.ways.trailing_zeros() as usize] =
                        actual.trailing_zeros() as u8;
                }
            }
            let stores = &data.stores[data.stores.partition_point(|s| s.owner < first_owner)
                ..data.stores.partition_point(|s| s.owner <= last_owner)];
            if self.ram_stores.len() + stores.len() > 16 {
                return None;
            }
            for store in stores {
                let at = store.requested?.checked_add(shift)?;
                if at < origin.clock || self.write_bus.is_none() {
                    return None;
                }
                self.clock.set(at);
                self.write_pc.set(store.pc);
                match store.width {
                    // All reads and final CPU phases are already proven.
                    // Actual WRAM publication and buffer acceptance update
                    // the live cache; this path consumes no private versions.
                    1 => self
                        .write::<1, false>(store.address, [store.value as u8])
                        .ok()?,
                    2 => self
                        .write::<2, false>(store.address, (store.value as u16).to_be_bytes())
                        .ok()?,
                    4 => self
                        .write::<4, false>(store.address, store.value.to_be_bytes())
                        .ok()?,
                    _ => return None,
                }
                if self.ram_stores.last()?.store.at != store.accepted.checked_add(shift)? {
                    return None;
                }
            }
            let last = &data.phases[last_index];
            let mut lru = origin.lru;
            for (_, touch) in data.replay_touches(first_index, last_index) {
                if !touch.apply_mapped(&mut lru, &way_map) {
                    return None;
                }
            }
            *self.lru.borrow_mut() = lru;
            self.retained.set(last.retained);
            let mut cpu = last.cpu.clone();
            cpu.rebase_loop_prefix(&first.cpu, initial, shift);
            let ready = last.ready.checked_add(shift)?;
            self.clock.set(ready.min(self.end.get()));
            self.epochs
                .set(origin.epochs + last.epoch.checked_sub(first.epoch)?);
            Some(Replayed {
                cpu,
                ready,
                at: last.at + shift,
                retired: (last_index - first_index) as u32,
                last: last_index,
                key: ReplayKey {
                    first: first_index,
                    shift,
                    ways: way_map,
                },
            })
        })();
        if result.is_none() {
            self.restore(origin);
        }
        result
    }
    pub(super) fn select_loop_prefix(
        &mut self,
        data: &CapturedLoop,
        key: ReplayKey,
        initial: &Cpu,
        selected: u32,
    ) -> (Cpu, u64, u64) {
        let first = &data.phases[key.first];
        let last = &data.phases[key.first + selected as usize];
        let keep = data
            .stores
            .partition_point(|s| s.owner <= (key.first + selected as usize) as u64)
            - data.stores.partition_point(|s| s.owner <= key.first as u64);
        assert!(keep <= self.ram_stores.len());
        self.ram_stores.truncate(keep);
        self.bus_events.get_mut().truncate(keep);
        debug_assert_eq!(self.fills.get_mut().len(), 0);
        debug_assert_eq!(self.version_count.get(), 0);
        self.write_pc
            .set(self.ram_stores.last().map_or(0, |s| s.pc));
        let mut lru = self.cache.replacement_state();
        for (_, touch) in data.replay_touches(key.first, key.first + selected as usize) {
            assert!(touch.apply_mapped(&mut lru, &key.ways));
        }
        *self.lru.get_mut() = lru;
        self.retained.set(last.retained);
        self.epochs.set(last.epoch - first.epoch);
        let mut cpu = last.cpu.clone();
        cpu.rebase_loop_prefix(&first.cpu, initial, key.shift);
        let ready = last.ready + key.shift;
        self.clock.set(ready.min(self.end.get()));
        (cpu, ready, last.at + key.shift)
    }
}
