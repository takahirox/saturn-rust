//! SH7604 §8: 4 KiB, four-way, write-through cache. No hardware snooping.
//! The LRU masks below are derived from tables 8.3/8.4, not another core.
use crate::bus::BusError;
use std::sync::atomic::{AtomicU32, Ordering};

mod shadow;
pub(crate) use shadow::CachedLines;

pub const CCR: u32 = 0xFFFF_FE92;
#[derive(Clone, Default)]
struct Line {
    tag: u32,
    valid: bool,
    data: [u8; 16],
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct CacheTouch {
    set: u8,
    way: u8,
}
pub(crate) struct Cache {
    control: u8,
    lines: Vec<[Line; 4]>,
    lru: [u8; 64],
    lookup: [AtomicU32; 64],
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            control: 0,
            lines: (0..64)
                .map(|_| std::array::from_fn(|_| Line::default()))
                .collect(),
            lru: [0; 64],
            lookup: std::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}
impl Cache {
    #[cfg(test)]
    pub(crate) fn assert_same_contents(&self, other: &Self) {
        assert_eq!(self.control, other.control);
        assert_eq!(self.lru, other.lru);
        for (a, b) in self
            .lines
            .iter()
            .flatten()
            .zip(other.lines.iter().flatten())
        {
            assert_eq!((a.tag, a.valid, a.data), (b.tag, b.valid, b.data));
        }
    }
    pub fn control(&self) -> u8 {
        self.control
    }
    pub fn set_control(&mut self, value: u8) {
        for lookup in &self.lookup {
            lookup.store(0, Ordering::Relaxed);
        }
        self.control = value & 0xCF;
        if value & 0x10 != 0 {
            for set in &mut self.lines {
                for line in set {
                    line.valid = false;
                }
            }
            self.lru.fill(0);
        }
    }
    pub fn reset_control(&mut self) {
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
        if lookup & !3 == tag | 4 {
            return Some((lookup & 3) as usize);
        }
        let way = self.lines[set]
            .iter()
            .enumerate()
            .skip(self.first_cache_way())
            .find_map(|(way, line)| (line.valid && line.tag == tag).then_some(way));
        if let Some(way) = way {
            self.lookup[set].store(tag | 4 | way as u32, Ordering::Relaxed);
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
        bytes.copy_from_slice(&self.lines[set][way].data[offset..offset + N]);
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
        bytes.copy_from_slice(&self.lines[set][way].data[offset..offset + N]);
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
            self.lines[set][way] = Line {
                tag,
                valid: true,
                data,
            };
            self.lookup[set].store(tag | 4 | way as u32, Ordering::Relaxed);
            way
        };
        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.lines[set][way].data[offset..offset + N]);
        self.touch(set, way);
        Ok(Some(bytes))
    }
    pub fn written<const N: usize>(&mut self, address: u32, bytes: [u8; N]) {
        let ways = self.written_ways(address);
        let (set, _, offset) = Self::parts(address);
        for way in 0..4 {
            if ways & (1 << way) != 0 {
                self.lines[set][way].data[offset..offset + N].copy_from_slice(&bytes);
                self.touch(set, way);
            }
        }
    }
    fn written_ways(&self, address: u32) -> u8 {
        if address >= 0x2000_0000 || self.control & 1 == 0 {
            return 0;
        }
        let (set, tag, _) = Self::parts(address);
        let mut ways = 0;
        for way in self.first_cache_way()..4 {
            if self.lines[set][way].valid && self.lines[set][way].tag == tag {
                ways |= 1 << way;
            }
        }
        ways
    }
    pub(crate) fn plan_written_replacement(&self, address: u32, lru: &mut [u8; 64]) {
        let ways = self.written_ways(address);
        let (set, _, _) = Self::parts(address);
        for way in 0..4 {
            if ways & (1 << way) != 0 {
                lru[set] = Self::touch_replacement(lru[set], way);
            }
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
                let line = &self.lines[set][(self.control >> 6) as usize];
                let value =
                    line.tag | (u32::from(self.lru[set]) << 4) | (u32::from(line.valid) << 2);
                bytes.copy_from_slice(&value.to_be_bytes());
            }
            6 => {
                let offset = (address & 0xFFF) as usize;
                let line = &self.lines[(offset >> 4) & 63][offset >> 10];
                bytes.copy_from_slice(&line.data[offset & 15..(offset & 15) + N]);
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
                for line in &mut self.lines[set] {
                    if line.tag == tag {
                        line.valid = false;
                    }
                }
                self.lookup[set].store(0, Ordering::Relaxed);
            }
            3 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                let value = u32::from_be_bytes(bytes.as_slice().try_into().unwrap());
                let line = &mut self.lines[set][(self.control >> 6) as usize];
                line.tag = tag;
                line.valid = address & 4 != 0;
                self.lru[set] = ((value >> 4) & 63) as u8;
                self.lookup[set].store(0, Ordering::Relaxed);
            }
            6 => {
                let way = ((address >> 10) & 3) as usize;
                self.lines[set][way].data[offset..offset + N].copy_from_slice(&bytes);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

impl Cache {
    pub(crate) fn copy_private(&mut self, source: &Self, replacement: u8) {
        self.control = source.control;
        self.lines.clone_from(&source.lines);
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
        cache.set_control(9);
        assert_eq!(cache.peek_cached::<4>(address), Some([3; 4]));
        cache
            .array_write(0x4000_0000 | address, 0u32.to_be_bytes())
            .unwrap();
        assert_eq!(cache.peek_cached::<4>(address), None);
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
        assert!(c.lines[0][3].valid);
        assert_eq!(c.lines[0][3].tag, 0x06001000);
        assert_eq!(
            c.read::<4>(0x06001000, false, |_| panic!("lost old line"))
                .unwrap(),
            Some([1; 4])
        );
    }
}
