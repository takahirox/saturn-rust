//! Cache-hit inputs needed before the period overwrites them.
use super::*;
use std::collections::BTreeSet;

fn byte_index(cache: &PhaseCache, address: u32) -> Option<usize> {
    let control = cache.control;
    let image = cache.lines();
    if address >= 0x20000000 || control & 1 == 0 {
        return None;
    }
    let first = if control & 8 == 0 { 0 } else { 2 };
    if image.len() != 64 * (4 - first) * 21 {
        return None;
    }
    let set = (address >> 4 & 63) as usize;
    let tag = address & 0x1FFFFC00;
    for way in first..4 {
        let at = (set * (4 - first) + way - first) * 21;
        let found = u32::from_be_bytes(image[at..at + 4].try_into().unwrap());
        if image[at + 4] != 0 && found == tag {
            return Some(at + 5 + (address & 15) as usize);
        }
    }
    None
}
pub(super) fn repeats(data: &CapturedLoop) -> bool {
    let first = &data.phases[0];
    let last = data.phases.last().unwrap();
    let initial = first.cache.lines();
    let final_lines = last.cache.lines();
    if initial.len() != final_lines.len() || first.cache.control != last.cache.control {
        return false;
    }
    // Without cache fills/control writes, tag/way identity is invariant.
    if (0..initial.len())
        .step_by(21)
        .any(|at| initial[at..at + 5] != final_lines[at..at + 5])
    {
        return false;
    }
    enum Event<'a> {
        Read(&'a Read),
        Write(&'a Store),
    }
    let mut events: Vec<_> = data
        .reads
        .iter()
        .map(Event::Read)
        .chain(data.stores.iter().map(Event::Write))
        .collect();
    events.sort_unstable_by_key(|event| match event {
        Event::Read(read) => (read.owner, read.at, 0),
        Event::Write(write) => (write.owner, write.requested.unwrap_or(write.accepted), 1),
    });
    let mut written = BTreeSet::new();
    for event in events {
        match event {
            Event::Read(read) => {
                for offset in 0..read.width {
                    let Some(at) = byte_index(&first.cache, read.address + offset as u32) else {
                        return false;
                    };
                    if !written.contains(&at) && initial[at] != final_lines[at] {
                        return false;
                    }
                }
            }
            Event::Write(write) => {
                if write.requested.is_none() {
                    return false;
                }
                if write.address < 0x20000000 {
                    for offset in 0..write.width {
                        // Write-through misses do not allocate a line.
                        if let Some(at) = byte_index(&first.cache, write.address + offset as u32) {
                            written.insert(at);
                        }
                    }
                }
            }
        }
    }
    true
}
