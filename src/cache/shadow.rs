//! Transactional cache-line versions for an observer-bounded read-only burst.
use super::{Cache, Line, Ordering};

struct SetVersion {
    index: usize,
    ways: [Line; 4],
    previous: usize,
}

#[derive(Default)]
pub(crate) struct CachedLines {
    versions: Vec<SetVersion>,
    heads: Option<Box<[usize; 64]>>,
}

impl CachedLines {
    pub fn len(&self) -> usize {
        self.versions.len()
    }
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }
    pub fn truncate(&mut self, len: usize) {
        while self.versions.len() > len {
            let version = self.versions.pop().unwrap();
            self.heads.as_mut().unwrap()[version.index] = version.previous;
        }
    }
    fn set<'a>(&'a self, cache: &'a Cache, index: usize) -> &'a [Line; 4] {
        let head = self.heads.as_ref().map_or(0, |heads| heads[index]);
        if head == 0 {
            &cache.lines[index]
        } else {
            &self.versions[head - 1].ways
        }
    }
    fn hit<'a>(&'a self, cache: &'a Cache, address: u32) -> Option<(usize, &'a Line)> {
        if address >= 0x2000_0000 || cache.control & 1 == 0 {
            return None;
        }
        let (set, tag, _) = Cache::parts(address);
        self.set(cache, set)
            .iter()
            .enumerate()
            .skip(cache.first_cache_way())
            .find_map(|(way, line)| (line.valid && line.tag == tag).then_some((way, line)))
    }
    pub fn read_hit(&self, cache: &Cache, address: u32) -> bool {
        if self.is_empty() {
            cache.read_hit(address)
        } else {
            self.hit(cache, address).is_some()
        }
    }
    pub fn peek<const N: usize>(&self, cache: &Cache, address: u32) -> Option<[u8; N]> {
        if self.is_empty() {
            return cache.peek_cached(address);
        }
        if !address.is_multiple_of(N as u32) {
            return None;
        }
        let (_, line) = self.hit(cache, address)?;
        let offset = (address & 15) as usize;
        Some(line.data[offset..offset + N].try_into().unwrap())
    }
    pub fn read<const N: usize>(
        &self,
        cache: &Cache,
        address: u32,
        lru: &mut [u8; 64],
    ) -> Option<[u8; N]> {
        if self.is_empty() {
            return cache.cached_only(address, lru);
        }
        if !address.is_multiple_of(N as u32) {
            return None;
        }
        let (way, line) = self.hit(cache, address)?;
        let (set, _, offset) = Cache::parts(address);
        let bytes = line.data[offset..offset + N].try_into().unwrap();
        lru[set] = Cache::touch_replacement(lru[set], way);
        Some(bytes)
    }
    pub fn fill(&mut self, cache: &Cache, address: u32, bytes: [u8; 16], lru: &[u8; 64]) {
        assert!(address < 0x2000_0000 && cache.control & 1 == 1);
        let (index, tag, _) = Cache::parts(address);
        let mut ways = self.set(cache, index).clone();
        let way = Cache::victim_from(cache.control, lru[index]);
        ways[way] = Line {
            tag,
            valid: true,
            data: bytes,
        };
        // Append even when this set already has a version. A rejected CPU
        // attempt restores its prefix by truncation without copying the cache.
        let previous = self.heads.as_ref().map_or(0, |heads| heads[index]);
        self.versions.push(SetVersion {
            index,
            ways,
            previous,
        });
        self.heads.get_or_insert_with(|| Box::new([0; 64]))[index] = self.versions.len();
    }
    pub fn commit(self, cache: &mut Cache) {
        for version in self.versions {
            cache.lines[version.index] = version.ways;
            cache.lookup[version.index].store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes<const N: usize>(address: u32) -> [u8; N] {
        std::array::from_fn(|i| {
            let at = address + i as u32;
            ((at >> 13) ^ at.wrapping_mul(37)) as u8
        })
    }
    fn duplicate(cache: &Cache) -> Cache {
        Cache {
            control: cache.control,
            lines: cache.lines.clone(),
            lru: cache.lru,
            lookup: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
        }
    }

    #[test]
    fn versions_match_ordinary_cache_refills_hits_evictions_and_disabled_modes() {
        for control in [0, 1, 3, 5, 9, 11, 13, 0xC1, 0xC9] {
            let mut base = Cache::default();
            base.set_control(1 | (control & 8));
            for tag in 0..4 {
                for set in 0..8 {
                    base.read::<4>(0x0600_0000 + tag * 0x400 + set * 16, true, |a| Ok(bytes(a)))
                        .unwrap();
                }
            }
            base.set_control(control);
            let mut literal = duplicate(&base);
            let mut shadow = CachedLines::default();
            let mut lru = base.lru;
            for i in 0..128 {
                let address = 0x0600_0000 + ((i * 7) % 11) * 0x400 + (i % 8) * 16 + (i % 4) * 4;
                let expected = literal.read::<4>(address, true, |a| Ok(bytes(a))).unwrap();
                let mut actual = shadow.read::<4>(&base, address, &mut lru);
                if actual.is_none() && control & 3 == 1 {
                    shadow.fill(&base, address, bytes(address & !15), &lru);
                    actual = shadow.read(&base, address, &mut lru);
                }
                assert_eq!(actual, expected, "control={control:X}, address={address:X}");
                assert_eq!(lru, literal.lru);
                for tag in 0..11 {
                    let probe = 0x0600_0000 + tag * 0x400 + (i % 8) * 16;
                    assert_eq!(shadow.peek::<2>(&base, probe), literal.peek_cached(probe));
                    assert_eq!(shadow.read_hit(&base, probe), literal.read_hit(probe));
                }
            }
            shadow.commit(&mut base);
            base.commit_replacement_state(lru);
            for set in 0..64 {
                for way in 0..4 {
                    let (a, b) = (&base.lines[set][way], &literal.lines[set][way]);
                    assert_eq!((a.tag, a.valid, a.data), (b.tag, b.valid, b.data));
                }
            }
            assert_eq!(base.lru, literal.lru);
        }
    }

    #[test]
    fn committing_shadow_evictions_invalidates_a_warmed_host_lookup() {
        let mut base = Cache::default();
        base.set_control(1);
        let old = 0x0600_0000;
        base.read::<4>(old, true, |a| Ok(bytes(a))).unwrap();
        let mut shadow = CachedLines::default();
        let mut lru = base.lru;
        for tag in 1..=4 {
            let address = old + tag * 0x400;
            shadow.fill(&base, address, bytes(address), &lru);
            shadow.read::<4>(&base, address, &mut lru).unwrap();
        }
        assert!(base.peek_cached::<4>(old).is_some());
        assert_eq!(shadow.peek::<4>(&base, old), None);
        shadow.commit(&mut base);
        base.commit_replacement_state(lru);
        assert_eq!(base.peek_cached::<4>(old), None);
        assert_eq!(
            base.peek_cached::<4>(old + 0x1000),
            Some(bytes(old + 0x1000))
        );
    }

    #[test]
    fn rejecting_a_tail_restores_prior_versions_of_the_same_set() {
        let mut base = Cache::default();
        base.set_control(1);
        let mut shadow = CachedLines::default();
        let mut lru = base.lru;
        let mut prefix = Cache::default();
        prefix.set_control(1);
        for tag in 0..3 {
            let address = 0x0600_0000 + tag * 0x400;
            shadow.fill(&base, address, bytes(address), &lru);
            shadow.read::<4>(&base, address, &mut lru).unwrap();
            prefix.read::<4>(address, true, |a| Ok(bytes(a))).unwrap();
        }
        let saved = (shadow.len(), lru);
        for tag in 3..12 {
            let address = 0x0600_0000 + tag * 0x400;
            shadow.fill(&base, address, bytes(address), &lru);
            shadow.read::<4>(&base, address, &mut lru).unwrap();
        }
        shadow.truncate(saved.0);
        lru = saved.1;
        for tag in 0..12 {
            let address = 0x0600_0000 + tag * 0x400;
            assert_eq!(
                shadow.peek::<4>(&base, address),
                prefix.peek_cached(address)
            );
        }
        assert_eq!(lru, prefix.lru);
        assert_eq!(shadow.peek::<2>(&base, 0x2600_0000), None);
        assert_eq!(shadow.peek::<4>(&base, 0x0600_0001), None);
    }
}
