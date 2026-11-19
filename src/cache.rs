//! SH7604 §8: 4 KiB, four-way, write-through cache. No hardware snooping.
//! The LRU masks below are derived from tables 8.3/8.4, not another core.
use crate::bus::BusError;
use std::sync::atomic::{AtomicU32, Ordering};

mod shadow;
pub(crate) use shadow::CachedLines;

pub const CCR: u32 = 0xFFFF_FE92;
static NEXT_CODE_REVISION: AtomicU32 = AtomicU32::new(1);
// Keep the four address-array words contiguous. Bit 2 is the hardware
// valid bit and is disjoint from every address tag.
const VALID: u32 = 4;
#[derive(Clone, Default)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct CacheSet {
    tags: [u32; 4],
    data: [[u8; 16]; 4],
}
impl CacheSet {
    fn matching_ways(&self, tag: u32, first_way: usize) -> u8 {
        let tag = tag | VALID;
        #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
        let ways = {
            use core::arch::wasm32::{i32x4_bitmask, i32x4_eq, i32x4_splat, v128_load};
            // The array owns all 16 readable bytes. The intrinsic accepts
            // an address with the array's ordinary u32 alignment.
            let tags = unsafe { v128_load(self.tags.as_ptr().cast()) };
            i32x4_bitmask(i32x4_eq(tags, i32x4_splat(tag as i32)))
        };
        #[cfg(not(all(target_arch = "wasm32", target_feature = "simd128")))]
        let ways = {
            let [a, b, c, d] = self.tags;
            u8::from(a == tag)
                | (u8::from(b == tag) << 1)
                | (u8::from(c == tag) << 2)
                | (u8::from(d == tag) << 3)
        };
        ways & (0xF << first_way)
    }

    fn first_match(&self, tag: u32, first_way: usize) -> Option<usize> {
        let ways = self.matching_ways(tag, first_way);
        (ways != 0).then(|| ways.trailing_zeros() as usize)
    }

    fn fill(&mut self, way: usize, tag: u32, data: [u8; 16]) {
        self.tags[way] = tag | VALID;
        self.data[way] = data;
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheTouch {
    set: u8,
    way: u8,
}
impl CacheTouch {
    pub(crate) fn apply_mapped(self, lru: &mut [u8; 64], ways: &[u8; 256]) -> bool {
        let (set, old) = self.loop_key();
        let way = ways[set * 4 + old];
        if way >= 4 {
            return false;
        }
        lru[set] = Cache::touch_replacement(lru[set], usize::from(way));
        true
    }
    pub(crate) fn loop_key(self) -> (usize, usize) {
        (usize::from(self.set), usize::from(self.way))
    }
    pub(crate) fn apply(self, lru: &mut [u8; 64]) {
        let set = usize::from(self.set);
        lru[set] = Cache::touch_replacement(lru[set], usize::from(self.way));
    }
}
/// Keep the last touch of each set/way, in its original order. A later
/// identical touch overwrites every replacement bit changed by an earlier one.
pub(crate) fn compact_touch_prefixes(
    touches: &[CacheTouch],
    ends: impl IntoIterator<Item = usize>,
) -> Vec<Vec<(usize, CacheTouch)>> {
    let mut prefixes = Vec::new();
    let mut summary = Vec::new();
    let mut cursor = 0;
    for end in ends {
        for (index, &touch) in touches.iter().enumerate().take(end).skip(cursor) {
            if let Some(at) = summary.iter().position(|&(_, old)| old == touch) {
                summary.remove(at);
            }
            summary.push((index, touch));
        }
        prefixes.push(summary.clone());
        cursor = end;
    }
    prefixes
}
#[cfg(test)]
mod compact_touch_tests {
    use super::*;

    #[test]
    fn every_prefix_suffix_and_way_permutation_preserves_replacement_bits() {
        let keys = [
            (0, 0),
            (0, 1),
            (0, 0),
            (1, 3),
            (0, 2),
            (1, 1),
            (0, 0),
            (0, 3),
            (1, 3),
            (1, 0),
            (0, 2),
            (0, 1),
            (1, 2),
            (1, 0),
            (0, 3),
            (1, 2),
            (1, 1),
            (0, 0),
        ];
        let touches: Vec<_> = keys
            .into_iter()
            .map(|(set, way)| CacheTouch { set, way })
            .collect();
        let prefixes = compact_touch_prefixes(&touches, 0..=touches.len());
        assert!(prefixes.last().unwrap().len() < touches.len());
        for a in 0..4 {
            for b in 0..4 {
                for c in 0..4 {
                    for d in 0..4 {
                        let permutation = [a, b, c, d];
                        if (0..4).any(|i| (i + 1..4).any(|j| permutation[i] == permutation[j])) {
                            continue;
                        }
                        let mut ways = [u8::MAX; 256];
                        ways[..4].copy_from_slice(&permutation);
                        ways[4..8].copy_from_slice(&permutation);
                        for first in 0..=touches.len() {
                            for last in first..=touches.len() {
                                let compact = &prefixes[last];
                                let compact =
                                    &compact[compact.partition_point(|&(at, _)| at < first)..];
                                for state in 0..64 {
                                    let mut expected = [state; 64];
                                    let mut actual = expected;
                                    for touch in &touches[first..last] {
                                        assert!(touch.apply_mapped(&mut expected, &ways));
                                    }
                                    for (_, touch) in compact {
                                        assert!(touch.apply_mapped(&mut actual, &ways));
                                    }
                                    assert_eq!(actual, expected);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
#[derive(Default)]
struct LoopTouches {
    touches: Vec<CacheTouch>,
    contents_version: u64,
}

pub(crate) struct Cache {
    loop_touches: Option<LoopTouches>,
    code_revision: AtomicU32,
    control: u8,
    sets: Vec<CacheSet>,
    lru: [u8; 64],
    lookup: [AtomicU32; 64],
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            loop_touches: None,
            code_revision: AtomicU32::new(0),
            control: 0,
            sets: vec![CacheSet::default(); 64],
            lru: [0; 64],
            lookup: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}
impl Cache {
    pub(crate) fn loop_guard_matches(
        &self,
        address: u32,
        ways: u8,
        mask: u16,
        expected: &[u8; 16],
    ) -> Option<u8> {
        let actual = self.written_ways(address);
        if (actual == 0) != (ways == 0) || actual.count_ones() > 1 {
            return None;
        }
        if mask == 0 {
            return Some(actual);
        }
        let bytes = self.peek_cached::<16>(address)?;
        if mask == u16::MAX {
            return (bytes == *expected).then_some(actual);
        }
        (0..16)
            .all(|i| mask & (1 << i) == 0 || bytes[i] == expected[i])
            .then_some(actual)
    }
    pub(crate) fn begin_loop_touches(&mut self) {
        self.loop_touches = Some(LoopTouches::default());
    }
    pub(crate) fn loop_touch_count(&self) -> usize {
        self.loop_touches
            .as_ref()
            .map_or(0, |log| log.touches.len())
    }
    pub(crate) fn loop_contents_version(&self) -> u64 {
        self.loop_touches
            .as_ref()
            .map_or(0, |log| log.contents_version)
    }
    fn record_contents_change(&mut self) {
        *self.code_revision.get_mut() = 0;
        if let Some(log) = &mut self.loop_touches {
            log.contents_version += 1;
        }
    }
    // Reuse a proof only for the same cache image, even if a template is
    // checked against another cache. Exhaustion leaves the literal check active.
    pub(crate) fn code_revision(&self) -> u32 {
        let revision = self.code_revision.load(Ordering::Relaxed);
        if revision != 0 {
            return revision;
        }
        let Ok(next) =
            NEXT_CODE_REVISION.try_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                revision.checked_add(1)
            })
        else {
            return 0;
        };
        match self
            .code_revision
            .compare_exchange(0, next, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => next,
            Err(existing) => existing,
        }
    }
    pub(crate) fn end_loop_touches(&mut self) -> Vec<CacheTouch> {
        self.loop_touches
            .take()
            .map_or_else(Vec::new, |log| log.touches)
    }
    pub(crate) fn loop_probe_snapshot(
        &self,
        lru: [u8; 64],
        mut read: impl FnMut(u32) -> Option<([u8; 16], CacheTouch)>,
    ) -> Option<Vec<u8>> {
        let mut bytes = Vec::with_capacity(65 + 64 * 4 * 21);
        bytes.push(self.control);
        bytes.extend(lru);
        for (set, ways) in self.sets.iter().enumerate() {
            for (way, &encoded_tag) in ways.tags.iter().enumerate().skip(self.first_cache_way()) {
                let tag = encoded_tag & !VALID;
                let valid = encoded_tag & VALID != 0;
                bytes.extend(tag.to_be_bytes());
                bytes.push(u8::from(valid));
                if valid {
                    let address = tag | (set as u32) << 4;
                    let (data, touch) = read(address)?;
                    // An ordinary lookup returns the first duplicate tag.
                    // Reject an ambiguous snapshot rather than copy that
                    // first way's bytes into a different way.
                    if usize::from(touch.set) != set || usize::from(touch.way) != way {
                        return None;
                    }
                    bytes.extend(data);
                } else {
                    bytes.extend(ways.data[way]);
                }
            }
        }
        Some(bytes)
    }
    #[cfg(test)]
    pub(crate) fn assert_same_contents(&self, other: &Self) {
        assert_eq!(self.control, other.control);
        assert_eq!(self.lru, other.lru);
        for (set, (a, b)) in self.sets.iter().zip(&other.sets).enumerate() {
            assert_eq!(a, b, "cache set {set}");
        }
    }
    pub fn control(&self) -> u8 {
        self.control
    }
    pub fn set_control(&mut self, value: u8) {
        self.record_contents_change();
        for lookup in &self.lookup {
            lookup.store(0, Ordering::Relaxed);
        }
        self.control = value & 0xCF;
        if value & 0x10 != 0 {
            for set in &mut self.sets {
                for tag in &mut set.tags {
                    *tag &= !VALID;
                }
            }
            self.lru.fill(0);
        }
    }
    pub fn reset_control(&mut self) {
        self.record_contents_change();
        self.control = 0;
        for lookup in &self.lookup {
            lookup.store(0, Ordering::Relaxed);
        }
    }
    fn parts(address: u32) -> (usize, u32, usize) {
        (
            ((address >> 4) & 63) as usize,
            address & 0x1FFF_FC00,
            (address & 15) as usize,
        )
    }
    fn touch(&mut self, set: usize, way: usize) {
        if let Some(log) = &mut self.loop_touches {
            log.touches.push(CacheTouch {
                set: set as u8,
                way: way as u8,
            });
        }
        self.lru[set] = Self::touch_replacement(self.lru[set], way);
    }
    fn touch_replacement(lru: u8, way: usize) -> u8 {
        let (mask, value) = [(0x38, 0), (0x26, 0x20), (0x15, 0x14), (0x0B, 0x0B)][way];
        (lru & !mask) | value
    }
    fn victim(&self, set: usize) -> usize {
        Self::victim_from(self.control, self.lru[set])
    }
    fn victim_from(control: u8, lru: u8) -> usize {
        if control & 8 != 0 {
            return if lru & 1 == 0 { 3 } else { 2 };
        }
        [(0x38, 0x38), (0x26, 0x06), (0x15, 0x01), (0x0B, 0)]
            .into_iter()
            .position(|(mask, value)| lru & mask == value)
            .unwrap_or(3)
    }
    fn first_cache_way(&self) -> usize {
        // H §8.5.4: ways 0/1 are RAM in TW mode, never tag matches.
        if self.control & 8 != 0 { 2 } else { 0 }
    }
    fn hit_way(&self, address: u32) -> Option<usize> {
        let (set, tag, _) = Self::parts(address);
        let lookup = self.lookup[set].load(Ordering::Relaxed);
        if lookup & !3 == tag | VALID {
            return Some((lookup & 3) as usize);
        }
        let way = self.sets[set].first_match(tag, self.first_cache_way());
        if let Some(way) = way {
            self.lookup[set].store(tag | VALID | way as u32, Ordering::Relaxed);
        }
        way
    }
    pub(crate) fn cached_only<const N: usize>(
        &self,
        address: u32,
        lru: &mut [u8; 64],
    ) -> Option<[u8; N]> {
        if address >= 0x2000_0000 || self.control & 1 == 0 || address as usize & (N - 1) != 0 {
            return None;
        }
        let (set, _, offset) = Self::parts(address);
        let way = self.hit_way(address)?;
        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.sets[set].data[way][offset..offset + N]);
        lru[set] = Self::touch_replacement(lru[set], way);
        Some(bytes)
    }
    /// Decode-only inspection must not touch replacement state. This avoids
    /// copying the whole LRU array just to discard one hypothetical touch.
    pub(crate) fn peek_cached<const N: usize>(&self, address: u32) -> Option<[u8; N]> {
        self.peek_cached_touch(address).map(|(bytes, _)| bytes)
    }
    pub(crate) fn peek_cached_touch<const N: usize>(
        &self,
        address: u32,
    ) -> Option<([u8; N], CacheTouch)> {
        if address >= 0x2000_0000 || self.control & 1 == 0 || address as usize & (N - 1) != 0 {
            return None;
        }
        let (set, _, offset) = Self::parts(address);
        let way = self.hit_way(address)?;
        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.sets[set].data[way][offset..offset + N]);
        Some((
            bytes,
            CacheTouch {
                set: set as u8,
                way: way as u8,
            },
        ))
    }
    pub(crate) fn publish_touch(&mut self, touch: CacheTouch) {
        self.touch(usize::from(touch.set), usize::from(touch.way));
    }
    pub(crate) fn replacement_state(&self) -> [u8; 64] {
        self.lru
    }
    pub(crate) fn commit_replacement_state(&mut self, lru: [u8; 64]) {
        self.lru = lru;
    }
    pub(crate) fn commit_private_replacement(
        &mut self,
        keep: &[u8; 64],
        value: &[u8; 64],
        mut changed: u64,
    ) {
        while changed != 0 {
            let set = changed.trailing_zeros() as usize;
            self.lru[set] = (self.lru[set] & keep[set]) | value[set];
            changed &= changed - 1;
        }
    }
    pub fn read_hit(&self, address: u32) -> bool {
        address < 0x2000_0000 && self.control & 1 != 0 && self.hit_way(address).is_some()
    }
    pub fn read<const N: usize>(
        &mut self,
        address: u32,
        instruction: bool,
        mut fetch: impl FnMut(u32) -> Result<[u8; 4], BusError>,
    ) -> Result<Option<[u8; N]>, BusError> {
        if address >= 0x2000_0000 || self.control & 1 == 0 {
            return Ok(None);
        }
        let (set, tag, offset) = Self::parts(address);
        let way = if let Some(way) = self.hit_way(address) {
            way
        } else {
            let disable = if instruction { 2 } else { 4 };
            if self.control & disable != 0 {
                return Ok(None);
            }
            // H §8.4.1/fig.8.4: four individual longword bus cycles, starting
            // after the requested longword and wrapping within the line. The
            // requested longword arrives last. Commit the line only after
            // all reads succeed; external MMIO effects keep their bus order.
            let mut data = [0; 16];
            for beat in 1..=4 {
                let at = ((address & 12) + beat * 4) & 15;
                data[at as usize..at as usize + 4].copy_from_slice(&fetch((address & !15) | at)?);
            }
            let way = self.victim(set);
            self.record_contents_change();
            self.sets[set].fill(way, tag, data);
            self.lookup[set].store(tag | VALID | way as u32, Ordering::Relaxed);
            way
        };
        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.sets[set].data[way][offset..offset + N]);
        self.touch(set, way);
        Ok(Some(bytes))
    }
    pub fn written<const N: usize>(&mut self, address: u32, bytes: [u8; N]) {
        let mut ways = self.written_ways(address);
        let (set, _, offset) = Self::parts(address);
        if ways != 0 {
            self.record_contents_change();
        }
        while ways != 0 {
            let way = ways.trailing_zeros() as usize;
            self.sets[set].data[way][offset..offset + N].copy_from_slice(&bytes);
            self.touch(set, way);
            ways &= ways - 1;
        }
    }
    fn written_ways(&self, address: u32) -> u8 {
        if address >= 0x2000_0000 || self.control & 1 == 0 {
            return 0;
        }
        let (set, tag, _) = Self::parts(address);
        self.sets[set].matching_ways(tag, self.first_cache_way())
    }
    pub(crate) fn plan_written_replacement(&self, address: u32, lru: &mut [u8; 64]) {
        let mut ways = self.written_ways(address);
        let (set, _, _) = Self::parts(address);
        while ways != 0 {
            lru[set] = Self::touch_replacement(lru[set], ways.trailing_zeros() as usize);
            ways &= ways - 1;
        }
    }
    pub fn array_read<const N: usize>(&self, address: u32) -> Result<Option<[u8; N]>, BusError> {
        let mut bytes = [0; N];
        match address >> 29 {
            3 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                let set = ((address >> 4) & 63) as usize;
                let tag = self.sets[set].tags[(self.control >> 6) as usize];
                let value = tag | (u32::from(self.lru[set]) << 4);
                bytes.copy_from_slice(&value.to_be_bytes());
            }
            6 => {
                let offset = (address & 0xFFF) as usize;
                let data = &self.sets[(offset >> 4) & 63].data[offset >> 10];
                bytes.copy_from_slice(&data[offset & 15..(offset & 15) + N]);
            }
            _ => return Ok(None),
        }
        Ok(Some(bytes))
    }
    pub fn array_write<const N: usize>(
        &mut self,
        address: u32,
        bytes: [u8; N],
    ) -> Result<bool, BusError> {
        let (set, tag, offset) = Self::parts(address);
        match address >> 29 {
            2 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                for entry in &mut self.sets[set].tags {
                    if *entry & !VALID == tag {
                        *entry &= !VALID;
                    }
                }
                self.lookup[set].store(0, Ordering::Relaxed);
            }
            3 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                let value = u32::from_be_bytes(bytes.as_slice().try_into().unwrap());
                self.sets[set].tags[(self.control >> 6) as usize] = tag | (address & VALID);
                self.lru[set] = ((value >> 4) & 63) as u8;
                self.lookup[set].store(0, Ordering::Relaxed);
            }
            6 => {
                let way = ((address >> 10) & 3) as usize;
                self.sets[set].data[way][offset..offset + N].copy_from_slice(&bytes);
            }
            _ => return Ok(false),
        }
        self.record_contents_change();
        Ok(true)
    }
}

impl Cache {
    pub(crate) fn copy_private(&mut self, source: &Self, replacement: u8) {
        self.record_contents_change();
        self.control = source.control;
        self.sets.clone_from(&source.sets);
        self.lru.fill(replacement);
        for lookup in &self.lookup {
            lookup.store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn memoized_lookup_preserves_first_way_after_duplicate_tags_purges_and_mode_changes() {
        let mut cache = Cache::default();
        let _: &(dyn Send + Sync) = &cache;
        let address = 0x0600_1000;
        // Install a later-way tag and warm the host lookup.
        cache.set_control(0xC1);
        cache
            .array_write(0x6000_0004 | address, 0u32.to_be_bytes())
            .unwrap();
        cache.array_write::<4>(0xC000_0C00, [3; 4]).unwrap();
        assert_eq!(cache.peek_cached::<4>(address), Some([3; 4]));
        // An earlier matching way must win even with a warmed later-way entry.
        cache.set_control(1);
        cache
            .array_write(0x6000_0004 | address, 0u32.to_be_bytes())
            .unwrap();
        cache.array_write::<4>(0xC000_0000, [1; 4]).unwrap();
        assert_eq!(cache.peek_cached::<4>(address), Some([1; 4]));
        cache.written(address, [0x55; 4]);
        assert_eq!(cache.array_read::<4>(0xC000_0000), Ok(Some([0x55; 4])));
        assert_eq!(cache.array_read::<4>(0xC000_0C00), Ok(Some([0x55; 4])));
        cache.set_control(0xC9);
        assert_eq!(cache.peek_cached::<4>(address), Some([0x55; 4]));
        cache.written(address, [0x99; 4]);
        assert_eq!(cache.array_read::<4>(0xC000_0000), Ok(Some([0x55; 4])));
        assert_eq!(cache.array_read::<4>(0xC000_0C00), Ok(Some([0x99; 4])));
        cache
            .array_write(0x4000_0000 | address, 0u32.to_be_bytes())
            .unwrap();
        assert_eq!(cache.peek_cached::<4>(address), None);
        let invalid_tag = address | (u32::from(cache.lru[0]) << 4);
        assert_eq!(
            cache.array_read::<4>(0x6000_0000),
            Ok(Some(invalid_tag.to_be_bytes()))
        );
    }
    #[test]
    fn decode_peeks_match_discarded_cache_touches_without_changing_lru() {
        for control in [0, 1, 9] {
            let mut cache = Cache::default();
            cache.set_control(control);
            for line in 0..16 {
                cache
                    .read::<4>(0x0600_0000 + line * 0x400, false, |a| Ok(a.to_be_bytes()))
                    .unwrap();
                for base in [0x0600_0000, 0x2600_0000, 0xFFFF_0000] {
                    for offset in 0..16 {
                        let address = base + line * 0x400 + offset;
                        let before = cache.replacement_state();
                        macro_rules! check {
                            ($width:literal) => {
                                let mut hypothetical = before;
                                assert_eq!(
                                    cache.peek_cached::<$width>(address),
                                    cache.cached_only::<$width>(address, &mut hypothetical)
                                );
                                let mut prepared = before;
                                let bytes = cache.peek_cached_touch::<$width>(address).map(
                                    |(bytes, touch)| {
                                        touch.apply(&mut prepared);
                                        bytes
                                    },
                                );
                                assert_eq!(bytes, cache.peek_cached::<$width>(address));
                                assert_eq!(prepared, hypothetical);
                            };
                        }
                        check!(1);
                        check!(2);
                        check!(4);
                        assert_eq!(cache.replacement_state(), before);
                    }
                }
            }
        }
    }

    #[test]
    fn refill_bus_beats_wrap_and_requested_longword_is_last_for_every_offset() {
        // H §8.4.1 fig.8.4; expected beat orders are independently authored.
        for (offset, order) in [
            (0, [4, 8, 12, 0]),
            (4, [8, 12, 0, 4]),
            (8, [12, 0, 4, 8]),
            (12, [0, 4, 8, 12]),
        ] {
            let mut c = Cache::default();
            c.set_control(0x11);
            let mut trace = Vec::new();
            assert_eq!(
                c.read::<4>(0x06001000 + offset, false, |a| {
                    trace.push(a - 0x06001000);
                    Ok(a.to_be_bytes())
                })
                .unwrap(),
                Some((0x06001000 + offset).to_be_bytes())
            );
            assert_eq!(trace, order);
            for i in 0..4 {
                assert_eq!(
                    c.read::<4>(0x06001000 + i * 4, true, |_| panic!(
                        "cache hit must not fetch"
                    ))
                    .unwrap(),
                    Some((0x06001000 + i * 4).to_be_bytes())
                );
            }
        }
    }
    #[test]
    fn failed_refill_preserves_the_victim_and_stops_external_reads_at_the_error() {
        let mut c = Cache::default();
        c.set_control(0x11);
        for i in 0..4 {
            c.read::<4>(0x06001000 + i * 1024, false, |_| Ok([i as u8 + 1; 4]))
                .unwrap();
        }
        // All ways hold valid lines; replacement would evict way3. A later
        // failure must preserve that victim and must not publish any of
        // the partial data or its tag/valid/LRU state.
        let before = c.lru;
        let mut trace = Vec::new();
        let fail = BusError::Unmapped {
            address: 0x06002008,
            width: 4,
        };
        assert_eq!(
            c.read::<4>(0x06002000, false, |a| {
                trace.push(a);
                if a == 0x06002008 {
                    Err(fail)
                } else {
                    Ok([5; 4])
                }
            }),
            Err(fail)
        );
        assert_eq!(trace, [0x06002004, 0x06002008]);
        assert_eq!(c.lru, before);
        assert_eq!(
            c.read::<4>(0x06001000, false, |_| panic!("lost old line"))
                .unwrap(),
            Some([1; 4])
        );
        c.set_control(0xC1);
        let victim = 0x0600_1004u32 | (u32::from(c.lru[0]) << 4);
        assert_eq!(
            c.array_read::<4>(0x6000_0000),
            Ok(Some(victim.to_be_bytes()))
        );
    }
}
