//! Inputs and line identities for a suffix of the observed period.
use super::*;

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
fn metadata(image: &[u8], address: u32) -> Option<Guard> {
    let control = *image.first()?;
    if control & 1 == 0 || address >= 0x20000000 {
        return None;
    }
    let first = if control & 8 == 0 { 0 } else { 2 };
    let active = 4 - first;
    if image.len() != 65 + 64 * active * 21 {
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
        let at = 65 + (((address >> 4 & 63) as usize) * active + way - first) * 21;
        let tag = u32::from_be_bytes(image[at..at + 4].try_into().ok()?);
        if image[at + 4] != 0 && tag == address & 0x1FFFFC00 {
            guard.ways |= 1 << way;
            guard.bytes.copy_from_slice(&image[at + 5..at + 21]);
        }
    }
    // Snapshot construction already rejects aliases; retain this boundary too.
    (guard.ways.count_ones() <= 1).then_some(guard)
}
fn touch_address(image: &[u8], touch: crate::cache::CacheTouch) -> Option<u32> {
    let (set, way) = touch.loop_key();
    let first = if image[0] & 8 == 0 { 0 } else { 2 };
    if way < first {
        return None;
    }
    let at = 65 + (set * (4 - first) + way - first) * 21;
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
        if self.stores.iter().any(|s| {
            self.reads.iter().any(|r| {
                r.instruction
                    && s.address < (r.address & !3) + 4
                    && (r.address & !3) < s.address + s.width as u32
            })
        }) {
            return None;
        }
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
