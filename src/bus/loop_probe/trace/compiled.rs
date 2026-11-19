//! Inputs and line identities for a suffix of the observed period.
use super::*;

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(in crate::bus) struct Guard {
    pub(in crate::bus) address: u32,
    pub(in crate::bus) ways: u8,
    pub(in crate::bus) mask: u16,
    pub(in crate::bus) bytes: [u8; 16],
    pub(in crate::bus) needed_by: u32,
    input_by: [u32; 16],
    last_input: u32,
}
impl Guard {
    pub(in crate::bus) fn input_mask(&self, owner: u32) -> u16 {
        if owner >= self.last_input {
            return self.mask;
        }
        self.input_by
            .iter()
            .enumerate()
            .fold(0, |mask, (bit, &at)| mask | (u16::from(at <= owner) << bit))
    }
}
fn metadata(cache: &PhaseCache, address: u32) -> Option<Guard> {
    let control = cache.control;
    let image = cache.lines();
    if control & 1 == 0 || address >= 0x20000000 {
        return None;
    }
    let first = if control & 8 == 0 { 0 } else { 2 };
    let active = 4 - first;
    if image.len() != 64 * active * 21 {
        return None;
    }
    let mut guard = Guard {
        address: address & !15,
        ways: 0,
        mask: 0,
        bytes: [0; 16],
        needed_by: u32::MAX,
        input_by: [u32::MAX; 16],
        last_input: 0,
    };
    for way in first..4 {
        let at = (((address >> 4 & 63) as usize) * active + way - first) * 21;
        let tag = u32::from_be_bytes(image[at..at + 4].try_into().ok()?);
        if image[at + 4] != 0 && tag == address & 0x1FFFFC00 {
            guard.ways |= 1 << way;
            guard.bytes.copy_from_slice(&image[at + 5..at + 21]);
        }
    }
    // Snapshot construction already rejects aliases; retain this boundary too.
    (guard.ways.count_ones() <= 1).then_some(guard)
}
fn touch_address(cache: &PhaseCache, touch: crate::cache::CacheTouch) -> Option<u32> {
    let image = cache.lines();
    let (set, way) = touch.loop_key();
    let first = if cache.control & 8 == 0 { 0 } else { 2 };
    if way < first {
        return None;
    }
    let at = (set * (4 - first) + way - first) * 21;
    if *image.get(at + 4)? == 0 {
        return None;
    }
    Some(u32::from_be_bytes(image.get(at..at + 4)?.try_into().ok()?) | (set as u32) << 4)
}

impl CapturedLoop {
    pub(super) fn compile(&self) -> Option<Vec<Vec<Guard>>> {
        // Existing short-cycle folding already amortizes small loops. A
        // physically distant branch can still have a short executed cycle.
        if !self.complete || !self.multiplier_constant || self.phases.len() <= 32 {
            return None;
        }
        // Retained IF is handled exactly, but this first compiled backend
        // conservatively leaves self-modifying instructions on the interpreter.
        if !self.stores.is_empty() {
            let instruction_words: BTreeSet<_> = self
                .reads
                .iter()
                .filter(|read| read.instruction)
                .map(|read| read.address & !3)
                .collect();
            let changes_code = self.stores.iter().any(|store| {
                instruction_words
                    .range((store.address & !3)..store.address + store.width as u32)
                    .next()
                    .is_some()
            });
            #[cfg(test)]
            assert_eq!(
                changes_code,
                self.stores
                    .iter()
                    .any(|store| self.reads.iter().any(|read| {
                        read.instruction
                            && store.address < (read.address & !3) + 4
                            && (read.address & !3) < store.address + store.width as u32
                    }))
            );
            if changes_code {
                return None;
            }
        }
        // Real CPU stores are naturally aligned. Retain the literal plan for
        // any observation that spans two lines, since the first line's cache
        // residency governs whether that store overwrites its input bytes.
        if self
            .stores
            .iter()
            .any(|store| (store.address & 15) as usize + store.width > 16)
        {
            return self.compile_literal();
        }
        let compiled = self.compile_backward();
        #[cfg(test)]
        assert_eq!(compiled, self.compile_literal());
        compiled
    }

    fn compile_backward(&self) -> Option<Vec<Vec<Guard>>> {
        enum Event<'a> {
            Read(&'a Read),
            Write(&'a Store),
        }
        impl Event<'_> {
            fn position(&self) -> (u64, u64, u8) {
                match self {
                    Self::Read(read) => (read.owner, read.at, 0),
                    Self::Write(store) => {
                        (store.owner, store.requested.unwrap_or(store.accepted), 1)
                    }
                }
            }
        }
        struct Inputs {
            needed_by: u32,
            read: bool,
            lanes: [u32; 16],
        }
        let mut events: Vec<_> = self
            .reads
            .iter()
            .map(Event::Read)
            .chain(self.stores.iter().map(Event::Write))
            .collect();
        events.sort_unstable_by_key(Event::position);
        let mut inputs = BTreeMap::<u32, Inputs>::new();
        let mut touched = BTreeMap::new();
        let mut suffixes = Vec::with_capacity(self.phases.len() - 1);
        for index in (0..self.phases.len() - 1).rev() {
            let phase = &self.phases[index];
            let following = &self.phases[index + 1];
            for &touch in &self.touches[phase.touch_end..following.touch_end] {
                touched.insert(touch.loop_key(), (touch, (index + 1) as u32));
            }
            // Prepend only newly visible events to this suffix. A read needs
            // the entry byte until the preceding store overwrites it. Keep
            // read presence as well, since even a fully overwritten read
            // needs a real cached line at the suffix entry.
            while events
                .last()
                .is_some_and(|event| event.position().0 > index as u64)
            {
                let event = events.pop().unwrap();
                let owner = event.position().0 as u32;
                let (address, width, read) = match event {
                    Event::Read(read) if read.instruction => (read.address & !3, 4, true),
                    Event::Read(read) => (read.address, read.width, true),
                    Event::Write(store) if store.address < 0x20000000 => {
                        (store.address, store.width, false)
                    }
                    Event::Write(_) => continue,
                };
                let offset = (address & 15) as usize;
                if offset + width > 16 {
                    return None;
                }
                let line = inputs.entry(address & !15).or_insert(Inputs {
                    needed_by: owner,
                    read: false,
                    lanes: [u32::MAX; 16],
                });
                line.needed_by = owner;
                line.read |= read;
                line.lanes[offset..offset + width].fill(if read { owner } else { u32::MAX });
            }
            let mut guards = BTreeMap::new();
            for (&address, line) in &inputs {
                let mut guard = metadata(&phase.cache, address)?;
                if line.read && guard.ways == 0 {
                    return None;
                }
                guard.needed_by = line.needed_by;
                guard.input_by = line.lanes;
                for (lane, &owner) in line.lanes.iter().enumerate() {
                    if owner != u32::MAX {
                        guard.mask |= 1 << lane;
                        guard.last_input = guard.last_input.max(owner);
                    }
                }
                guards.insert(address, guard);
            }
            // A touch identifies a physical way. Resolve its tag at each
            // suffix's real entry image rather than assuming stable tags.
            for &(touch, owner) in touched.values() {
                let address = touch_address(&phase.cache, touch)?;
                let guard = match guards.entry(address) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(metadata(&phase.cache, address)?)
                    }
                };
                guard.needed_by = guard.needed_by.min(owner);
            }
            suffixes.push(guards.into_values().collect());
        }
        suffixes.reverse();
        Some(suffixes)
    }

    fn compile_literal(&self) -> Option<Vec<Vec<Guard>>> {
        let mut suffixes = Vec::new();
        for (index, phase) in self.phases.iter().enumerate().take(self.phases.len() - 1) {
            let mut guards = BTreeMap::<u32, Guard>::new();
            let mut overwritten = BTreeSet::new();
            enum Event<'a> {
                Read(&'a Read),
                Write(&'a Store),
            }
            let mut events: Vec<_> = self
                .reads
                .iter()
                .filter(|r| r.owner > index as u64)
                .map(Event::Read)
                .chain(
                    self.stores
                        .iter()
                        .filter(|s| s.owner > index as u64)
                        .map(Event::Write),
                )
                .collect();
            events.sort_unstable_by_key(|event| match event {
                Event::Read(r) => (r.owner, r.at, 0),
                Event::Write(s) => (s.owner, s.requested.unwrap_or(s.accepted), 1),
            });
            for event in events {
                match event {
                    Event::Read(read) => {
                        let (address, width) = if read.instruction {
                            (read.address & !3, 4)
                        } else {
                            (read.address, read.width)
                        };
                        let base = address & !15;
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            guards.entry(base)
                        {
                            entry.insert(metadata(&phase.cache, address)?);
                        }
                        let guard = guards.get_mut(&base).unwrap();
                        guard.needed_by = guard.needed_by.min(read.owner as u32);
                        if guard.ways == 0 || (address & 15) + width as u32 > 16 {
                            return None;
                        }
                        for offset in 0..width {
                            let at = address + offset as u32;
                            if !overwritten.contains(&at) {
                                guard.mask |= 1 << (at & 15);
                                let bit = (at & 15) as usize;
                                if guard.input_by[bit] == u32::MAX {
                                    guard.input_by[bit] = read.owner as u32;
                                    guard.last_input = guard.last_input.max(read.owner as u32);
                                }
                            }
                        }
                    }
                    Event::Write(store) => {
                        if store.address >= 0x20000000 {
                            continue;
                        }
                        let base = store.address & !15;
                        if let std::collections::btree_map::Entry::Vacant(entry) =
                            guards.entry(base)
                        {
                            entry.insert(metadata(&phase.cache, store.address)?);
                        }
                        let guard = guards.get_mut(&base).unwrap();
                        guard.needed_by = guard.needed_by.min(store.owner as u32);
                        if guard.ways != 0 {
                            for offset in 0..store.width {
                                overwritten.insert(store.address + offset as u32);
                            }
                        }
                    }
                }
            }
            let mut cursor = phase.touch_end;
            for (owner, later) in self.phases.iter().enumerate().skip(index + 1) {
                for touch in &self.touches[cursor..later.touch_end] {
                    let address = touch_address(&phase.cache, *touch)?;
                    if let std::collections::btree_map::Entry::Vacant(entry) = guards.entry(address)
                    {
                        entry.insert(metadata(&phase.cache, address)?);
                    }
                    let guard = guards.get_mut(&address).unwrap();
                    guard.needed_by = guard.needed_by.min(owner as u32);
                }
                cursor = later.touch_end;
            }
            suffixes.push(guards.into_values().collect());
        }
        Some(suffixes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffix_inputs_keep_read_before_write_and_store_acceptance_order() {
        let base = crate::bus::HIGH_WRAM;
        let mut image = vec![0; 65 + 64 * 4 * 21];
        image[0] = 1;
        image[65..69].copy_from_slice(&base.to_be_bytes());
        image[69] = 1;
        let phases = (0..34)
            .map(|index| {
                let mut cache = image.clone();
                cache[70] = index;
                Phase {
                    at: u64::from(index) * 8,
                    ready: u64::from(index) * 8,
                    cpu: Cpu::start(base, 0),
                    cache: PhaseCache {
                        control: 1,
                        replacement: [0; 64],
                        contents_version: u64::from(index),
                        image: Arc::new(cache),
                    },
                    retained: None,
                    epoch: u64::from(index),
                    touch_end: 0,
                }
            })
            .collect();
        let read = |owner, at, address, width| Read {
            owner,
            at,
            address,
            width,
            value: 0,
            instruction: false,
        };
        let store = |owner, requested, accepted, address, width| Store {
            owner,
            requested: Some(requested),
            accepted,
            pc: base + 0x100,
            address,
            width,
            value: 0,
        };
        let trace = CapturedLoop {
            phases,
            reads: vec![
                read(4, 30, base, 4),
                read(1, 7, base, 1),
                read(3, 25, base + 2, 2),
                read(3, 26, base + 4, 1),
            ],
            stores: vec![
                store(3, 25, 27, base, 4),
                store(2, 17, 24, base + 2, 2),
                store(3, 24, 29, base + 4, 1),
            ],
            external_reads: 0,
            unsupported: 0,
            complete: true,
            header_checks: Vec::new(),
            touches: Vec::new(),
            compact_touches: Vec::new(),
            by_pc: BTreeMap::new(),
            multiplier_constant: true,
            compiled: Vec::new(),
            timeline: None,
        };
        let compiled = trace.compile().unwrap();
        for (index, (owner, mask)) in [(1, 1), (2, 0), (3, 12), (4, 15)].into_iter().enumerate() {
            let [guard] = compiled[index].as_slice() else {
                panic!("expected one guard for suffix {index}");
            };
            assert_eq!(guard.needed_by, owner);
            assert_eq!(guard.mask, mask);
            assert_eq!(guard.input_mask(owner - 1), 0);
            assert_eq!(guard.input_mask(owner), mask);
            assert_eq!(guard.bytes[0], index as u8);
        }
        assert!(compiled[4..].iter().all(Vec::is_empty));
    }
}
