//! Transactional cache-line versions for an observer-bounded read/write burst.
use super::{Cache, CacheSet, CacheTouch, Ordering};

struct SetVersion {
    index: usize,
    ways: CacheSet,
    previous: usize,
    read: bool,
}

#[derive(Default)]
pub(crate) struct CachedLines {
    versions: Vec<SetVersion>,
    reads: usize,
    heads: Option<Box<[usize; 64]>>,
}

impl CachedLines {
    pub fn len(&self) -> usize {
        self.versions.len()
    }
    pub fn read_count(&self) -> usize {
        self.reads
    }
    pub fn truncate(&mut self, len: usize) {
        while self.versions.len() > len {
            let version = self.versions.pop().unwrap();
            self.reads -= usize::from(version.read);
            self.heads.as_mut().unwrap()[version.index] = version.previous;
        }
    }
    fn unchanged(&self, address: u32) -> bool {
        self.heads
            .as_ref()
            .is_none_or(|heads| heads[((address >> 4) & 63) as usize] == 0)
    }
    fn set<'a>(&'a self, cache: &'a Cache, index: usize) -> &'a CacheSet {
        let head = self.heads.as_ref().map_or(0, |heads| heads[index]);
        if head == 0 {
            &cache.sets[index]
        } else {
            &self.versions[head - 1].ways
        }
    }
    fn hit<'a>(&'a self, cache: &'a Cache, address: u32) -> Option<(usize, &'a [u8; 16])> {
        if address >= 0x2000_0000 || cache.control & 1 == 0 {
            return None;
        }
        let (set, tag, _) = Cache::parts(address);
        let set = self.set(cache, set);
        let way = set.first_match(tag, cache.first_cache_way())?;
        Some((way, &set.data[way]))
    }
    pub fn read_hit(&self, cache: &Cache, address: u32) -> bool {
        if self.unchanged(address) {
            cache.read_hit(address)
        } else {
            self.hit(cache, address).is_some()
        }
    }
    pub fn peek<const N: usize>(&self, cache: &Cache, address: u32) -> Option<[u8; N]> {
        if self.unchanged(address) {
            return cache.peek_cached(address);
        }
        if !address.is_multiple_of(N as u32) {
            return None;
        }
        let (_, line) = self.hit(cache, address)?;
        let offset = (address & 15) as usize;
        Some(line[offset..offset + N].try_into().unwrap())
    }
    pub fn peek_touch<const N: usize>(
        &self,
        cache: &Cache,
        address: u32,
    ) -> Option<([u8; N], CacheTouch)> {
        if self.unchanged(address) {
            return cache.peek_cached_touch(address);
        }
        if !address.is_multiple_of(N as u32) {
            return None;
        }
        let (way, line) = self.hit(cache, address)?;
        let (set, _, offset) = Cache::parts(address);
        Some((
            line[offset..offset + N].try_into().unwrap(),
            CacheTouch {
                set: set as u8,
                way: way as u8,
            },
        ))
    }
    pub fn read<const N: usize>(
        &self,
        cache: &Cache,
        address: u32,
        lru: &mut [u8; 64],
    ) -> Option<[u8; N]> {
        if self.unchanged(address) {
            return cache.cached_only(address, lru);
        }
        if !address.is_multiple_of(N as u32) {
            return None;
        }
        let (way, line) = self.hit(cache, address)?;
        let (set, _, offset) = Cache::parts(address);
        let bytes = line[offset..offset + N].try_into().unwrap();
        lru[set] = Cache::touch_replacement(lru[set], way);
        Some(bytes)
    }
    pub fn written<const N: usize>(
        &mut self,
        cache: &Cache,
        address: u32,
        bytes: [u8; N],
        lru: &mut [u8; 64],
    ) {
        if address >= 0x2000_0000 || cache.control & 1 == 0 {
            return;
        }
        let (index, tag, offset) = Cache::parts(address);
        let current = self.set(cache, index);
        let mut mask = current.matching_ways(tag, cache.first_cache_way());
        if mask == 0 {
            return;
        }
        let mut ways = current.clone();
        while mask != 0 {
            let way = mask.trailing_zeros() as usize;
            ways.data[way][offset..offset + N].copy_from_slice(&bytes);
            lru[index] = Cache::touch_replacement(lru[index], way);
            mask &= mask - 1;
        }
        let previous = self.heads.as_ref().map_or(0, |heads| heads[index]);
        self.versions.push(SetVersion {
            index,
            ways,
            previous,
            read: false,
        });
        self.heads.get_or_insert_with(|| Box::new([0; 64]))[index] = self.versions.len();
    }
    pub fn fill(&mut self, cache: &Cache, address: u32, bytes: [u8; 16], lru: &[u8; 64]) {
        assert!(address < 0x2000_0000 && cache.control & 1 == 1);
        let (index, tag, _) = Cache::parts(address);
        let mut ways = self.set(cache, index).clone();
        let way = Cache::victim_from(cache.control, lru[index]);
        ways.fill(way, tag, bytes);
        // Append even when this set already has a version. A rejected CPU
        // attempt restores its prefix by truncation without copying the cache.
        let previous = self.heads.as_ref().map_or(0, |heads| heads[index]);
        self.versions.push(SetVersion {
            index,
            ways,
            previous,
            read: true,
        });
        self.reads += 1;
        self.heads.get_or_insert_with(|| Box::new([0; 64]))[index] = self.versions.len();
    }
    fn reset_heads(&mut self) {
        self.reads = 0;
        if let Some(heads) = &mut self.heads {
            heads.fill(0);
        }
    }
    pub fn discard(&mut self) {
        if !self.versions.is_empty() {
            self.versions.clear();
            self.reset_heads();
        }
    }
    pub fn commit(&mut self, cache: &mut Cache) {
        if self.versions.is_empty() {
            return;
        }
        Self::publish(cache, self.versions.drain(..));
        self.reset_heads();
    }
    pub fn commit_and_release(self, cache: &mut Cache) {
        Self::publish(cache, self.versions.into_iter());
    }
    fn publish(cache: &mut Cache, versions: impl ExactSizeIterator<Item = SetVersion>) {
        if versions.len() != 0 {
            cache.record_contents_change();
            for version in versions {
                cache.sets[version.index] = version.ways;
                cache.lookup[version.index].store(0, Ordering::Relaxed);
            }
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
            loop_touches: None,
            code_revision: std::sync::atomic::AtomicU32::new(0),
            control: cache.control,
            sets: cache.sets.clone(),
            lru: cache.lru,
            lookup: std::array::from_fn(|_| std::sync::atomic::AtomicU32::new(0)),
        }
    }

    #[test]
    fn reused_version_storage_exposes_only_new_burst_edits() {
        let mut cache = Cache::default();
        cache.set_control(1);
        let mut shadow = CachedLines::default();
        let lru = cache.replacement_state();
        for discard in [true, false] {
            shadow.fill(&cache, 0x0600_0000, [0x12; 16], &lru);
            shadow.fill(&cache, 0x0600_0010, [0x34; 16], &lru);
            assert_eq!(shadow.read_count(), 2);
            if discard {
                shadow.discard();
            } else {
                shadow.commit(&mut cache);
            }
            assert_eq!(shadow.read_count(), 0);
            assert_eq!(shadow.len(), 0);
            assert_eq!(
                shadow.peek::<4>(&cache, 0x0600_0000),
                (!discard).then_some([0x12; 4])
            );
            shadow.fill(&cache, 0x0600_0020, [0x56; 16], &lru);
            assert_eq!(shadow.read_count(), 1);
            assert_eq!(shadow.peek::<4>(&cache, 0x0600_0020), Some([0x56; 4]));
            shadow.discard();
            assert_eq!(shadow.peek::<4>(&cache, 0x0600_0020), None);
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
            base.assert_same_contents(&literal);
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
    #[test]
    fn ordered_writes_refills_and_rollback_match_every_literal_way() {
        for control in [1, 9] {
            let mut base = Cache::default();
            let address = 0x0600_1000;
            // Both duplicate matching ways must receive a cache write.
            for (mode, offset, value) in [(0x81, 0x800, 2), (0xC1, 0xC00, 3)] {
                base.set_control(mode);
                base.array_write(0x6000_0004 | address, [0; 4]).unwrap();
                base.array_write(0xC000_0000 | offset, [value; 4]).unwrap();
            }
            base.set_control(control);
            let mut literal = duplicate(&base);
            let mut shadow = CachedLines::default();
            let mut lru = base.lru;
            let mut reads = 0;
            let mut saved = None;
            for step in 0..40u32 {
                if step == 16 {
                    saved = Some((shadow.len(), lru, duplicate(&literal), reads));
                }
                if step == 32 {
                    let (len, prior_lru, prior_literal, prior_reads) = saved.take().unwrap();
                    shadow.truncate(len);
                    lru = prior_lru;
                    literal = prior_literal;
                    reads = prior_reads;
                }
                let at = address + (step % 8) * 0x400 + (step % 4) * 4;
                match step % 3 {
                    0 => {
                        literal.written(at, [step as u8]);
                        shadow.written(&base, at, [step as u8], &mut lru);
                    }
                    1 => {
                        let v = (step as u16 * 17).to_be_bytes();
                        literal.written(at, v);
                        shadow.written(&base, at, v, &mut lru);
                    }
                    _ => {
                        let v = (0xA5000000 | step).to_be_bytes();
                        literal.written(at, v);
                        shadow.written(&base, at, v, &mut lru);
                    }
                }
                assert_eq!(lru, literal.lru);
                let expected = literal
                    .read::<4>(at, true, |a| {
                        reads += 1;
                        Ok(bytes(a))
                    })
                    .unwrap();
                let mut actual = shadow.read::<4>(&base, at, &mut lru);
                if actual.is_none() {
                    shadow.fill(&base, at, bytes(at & !15), &lru);
                    actual = shadow.read(&base, at, &mut lru);
                }
                assert_eq!(actual, expected);
                assert_eq!(lru, literal.lru);
                assert_eq!(shadow.read_count(), reads / 4);
            }
            shadow.commit(&mut base);
            base.commit_replacement_state(lru);
            base.assert_same_contents(&literal);
        }
    }
}
