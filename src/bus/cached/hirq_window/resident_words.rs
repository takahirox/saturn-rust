//! Retain decoded instruction words during one immutable HIRQ window.
//!
//! Proving a future IF does not update retained IF or replacement state.
//! The accepted instruction publishes both effects in their original order.
use super::super::*;

#[derive(Clone, Copy)]
struct ResidentWord {
    address: u32,
    bytes: [u8; 4],
    touch: CacheTouch,
}

#[derive(Default)]
pub(super) struct Words {
    entries: [Option<ResidentWord>; 8],
    #[cfg(test)]
    pub(super) reused: u32,
}

pub(super) struct Fetch {
    word: u16,
    retained: Option<(u32, u16)>,
    touch: Option<CacheTouch>,
}

impl Fetch {
    pub(super) fn publish(self, memory: &Cached<'_>) -> u16 {
        if let Some(touch) = self.touch {
            touch.apply(&mut memory.lru.borrow_mut());
        }
        memory.retained.set(self.retained);
        self.word
    }
}

impl Words {
    pub(super) fn prepare(&mut self, memory: &Cached<'_>, address: u32) -> Option<Fetch> {
        if !ordinary_memory(address) || address & 1 != 0 {
            return None;
        }
        if let Some((retained_at, word)) = memory.retained.get()
            && retained_at == address
        {
            return Some(Fetch {
                word,
                retained: None,
                touch: None,
            });
        }
        // This table is private to a resident, read-only HIRQ execution
        // window. No virtual fill or write can change its tags or bytes.
        debug_assert_eq!(memory.version_count.get(), 0);
        debug_assert!(memory.store.get().is_none());
        let base = address & !3;
        let slot = &mut self.entries[(base as usize >> 2) & 7];
        let word = match *slot {
            Some(word) if word.address == base => {
                #[cfg(test)]
                {
                    self.reused += 1;
                }
                word
            }
            _ => {
                let (bytes, touch) = memory.cache.peek_cached_touch::<4>(base)?;
                let word = ResidentWord {
                    address: base,
                    bytes,
                    touch,
                };
                *slot = Some(word);
                word
            }
        };
        let [a, b, c, d] = word.bytes;
        let high = u16::from_be_bytes([a, b]);
        let low = u16::from_be_bytes([c, d]);
        let (word_at, retained) = if address & 2 == 0 {
            (high, Some((address + 2, low)))
        } else {
            (low, None)
        };
        Some(Fetch {
            word: word_at,
            retained,
            touch: Some(word.touch),
        })
    }
}
