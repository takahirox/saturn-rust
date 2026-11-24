//! Same-build Sound handoff with generation-checked RAM page differences.
use super::*;
use wire::{Error, Input, LIMIT, Wire, checksum};
const PAGE: usize = 4096;
const HEADER: &[u8; 8] = b"SRDP\x02\0\0\0";

// The generic Wasm memcmp scans bytes. Compare complete, equal-sized pages
// in words, retaining exact equality and early exit without SIMD or alignment.
fn same_page(current: &[u8; PAGE], prior: &[u8; PAGE]) -> bool {
    current
        .as_chunks::<8>()
        .0
        .iter()
        .zip(prior.as_chunks::<8>().0)
        .all(|(a, b)| u64::from_le_bytes(*a) == u64::from_le_bytes(*b))
}

pub(crate) struct Delta {
    generation: u64,
    ram: Vec<u8>,
}
impl Default for Delta {
    fn default() -> Self {
        Self {
            generation: 0,
            ram: vec![0; SOUND_RAM_SIZE],
        }
    }
}
impl Delta {
    pub fn export(&mut self, sound: &mut Sound) -> Result<Vec<u8>, Error> {
        if sound.projected() || sound.ram.len() != SOUND_RAM_SIZE {
            return Err("delta export requires canonical sound");
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or("sound delta generation exhausted")?;
        let mut metadata = Vec::new();
        let ram = std::mem::take(&mut sound.ram);
        sound.put(&mut metadata);
        sound.ram = ram;
        let pages: Vec<_> = sound
            .ram
            .as_chunks::<PAGE>()
            .0
            .iter()
            .zip(self.ram.as_chunks::<PAGE>().0.iter())
            .enumerate()
            .filter_map(|(index, (current, prior))| (!same_page(current, prior)).then_some(index))
            .collect();
        let mut out = Vec::with_capacity(32 + metadata.len() + pages.len() * (PAGE + 4));
        out.extend_from_slice(HEADER);
        next.put(&mut out);
        metadata.len().put(&mut out);
        out.extend_from_slice(&metadata);
        pages.len().put(&mut out);
        for &index in &pages {
            index.put(&mut out);
            out.extend_from_slice(&sound.ram[index * PAGE..(index + 1) * PAGE]);
        }
        if out.len() + 8 > LIMIT {
            return Err("sound delta size");
        }
        checksum(&out).put(&mut out);
        for index in pages {
            self.ram[index * PAGE..(index + 1) * PAGE]
                .copy_from_slice(&sound.ram[index * PAGE..(index + 1) * PAGE]);
        }
        self.generation = next;
        Ok(out)
    }
    /// Parse and validate the entire packet and expected time before mutation.
    pub fn import(
        &mut self,
        sound: &mut Sound,
        bytes: &[u8],
        expected_time: Option<u64>,
    ) -> Result<(), Error> {
        if bytes.len() < 32 || bytes.len() > LIMIT || bytes[..8] != *HEADER {
            return Err("sound delta header");
        }
        let (payload, trailer) = bytes.split_at(bytes.len() - 8);
        if checksum(payload) != u64::from_le_bytes(trailer.try_into().unwrap()) {
            return Err("sound delta checksum");
        }
        let mut input = Input::new(&payload[8..]);
        let next = u64::get(&mut input)?;
        if self.generation.checked_add(1) != Some(next) {
            return Err("stale sound delta generation");
        }
        let length = usize::get(&mut input)?;
        let mut metadata = Input::new(input.take(length)?);
        let mut restored = Sound::get(&mut metadata)?;
        if !metadata.finished()
            || !restored.ram.is_empty()
            || sound.ram.len() != SOUND_RAM_SIZE
            || expected_time.is_some_and(|at| at != restored.time)
        {
            return Err("sound delta metadata or committed time");
        }
        let count = usize::get(&mut input)?;
        if count > SOUND_RAM_SIZE / PAGE {
            return Err("sound delta page count");
        }
        let mut pages = Vec::with_capacity(count);
        let mut previous = None;
        for _ in 0..count {
            let index = usize::get(&mut input)?;
            if index >= SOUND_RAM_SIZE / PAGE || previous.is_some_and(|prior| index <= prior) {
                return Err("sound delta page order or index");
            }
            pages.push((index, input.take(PAGE)?));
            previous = Some(index);
        }
        if !input.finished() {
            return Err("sound delta trailing bytes");
        }
        restored.ram = std::mem::take(&mut sound.ram);
        #[cfg(any(test, feature = "kernel-experiment"))]
        {
            restored.domain_trace = sound.domain_trace.clone();
        }
        for (index, page) in pages {
            restored.ram[index * PAGE..(index + 1) * PAGE].copy_from_slice(page);
            self.ram[index * PAGE..(index + 1) * PAGE].copy_from_slice(page);
        }
        *sound = restored;
        self.generation = next;
        Ok(())
    }
}

/// Persistent Worker owner; idle hardware does not execute while Main owns it.
pub struct DeltaOwner {
    idle: Option<Sound>,
    forecast: Option<forecast::Forecast>,
    delta: Delta,
}
impl Default for DeltaOwner {
    fn default() -> Self {
        Self {
            idle: Some(Sound::default()),
            forecast: None,
            delta: Delta::default(),
        }
    }
}
impl DeltaOwner {
    pub fn begin(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let sound = self
            .idle
            .as_mut()
            .ok_or("sound owner already forecasting")?;
        self.delta.import(sound, bytes, None)?;
        self.forecast = Some(forecast::Forecast::new(self.idle.take().unwrap()));
        Ok(())
    }
    pub fn extend(&mut self, to: u64) -> Result<(), String> {
        self.forecast
            .as_mut()
            .ok_or("sound owner is idle")?
            .extend(to)
            .map_err(|error| error.to_string())
    }
    pub fn prefix(&self) -> Result<Vec<u32>, Error> {
        let f = self.forecast.as_ref().ok_or("sound owner is idle")?;
        let (origin, through, stable) = (f.origin(), f.through(), f.stable_frontier());
        Ok(vec![
            origin as u32,
            (origin >> 32) as u32,
            through as u32,
            (through >> 32) as u32,
            stable as u32,
            (stable >> 32) as u32,
            u32::from(f.initial_irq()),
        ])
    }
    pub fn finish(&mut self, at: u64) -> Result<Vec<u8>, String> {
        let f = self.forecast.as_mut().ok_or("sound owner is idle")?;
        f.rewind(at).map_err(|error| error.to_string())?;
        let mut sound = self.forecast.take().unwrap().into_sound();
        let packet = self.delta.export(&mut sound).map_err(str::to_owned);
        self.idle = Some(sound);
        packet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternating_owners_preserve_all_ram_and_hardware_without_full_ram_packets() {
        let (mut left, mut right) = (Sound::default(), Sound::default());
        let (mut a, mut b) = (Delta::default(), Delta::default());
        for reset in [false, true] {
            for features in [0, 1, 2, 3] {
                left = wire::tests::fixture(reset, features);
                let packet = a.export(&mut left).unwrap();
                b.import(&mut right, &packet, Some(0)).unwrap();
                assert_eq!(left.domain_snapshot(), right.domain_snapshot());
                right.advance(20000).unwrap();
                let packet = b.export(&mut right).unwrap();
                assert!(
                    packet.len() < 65536,
                    "unchanged RAM pages stay out of the packet"
                );
                a.import(&mut left, &packet, Some(20000)).unwrap();
                assert_eq!(left.domain_snapshot(), right.domain_snapshot());
            }
        }
        for (index, page) in left.ram.as_chunks_mut::<PAGE>().0.iter_mut().enumerate() {
            let offset = [0, 7, 8, 31, PAGE / 2, PAGE - 1][index % 6];
            page[offset] ^= (index as u8).wrapping_add(1);
        }
        let packet = a.export(&mut left).unwrap();
        assert!(packet.len() > SOUND_RAM_SIZE);
        b.import(&mut right, &packet, Some(20000)).unwrap();
        assert_eq!(left.domain_snapshot(), right.domain_snapshot());
        let _skipped = a.export(&mut left).unwrap();
        left.advance(12345).unwrap();
        let packet = a.export(&mut left).unwrap();
        // This side exported twice: the peer must reject a skipped generation.
        let state = right.domain_snapshot().unwrap();
        assert!(b.import(&mut right, &packet, None).is_err());
        assert_eq!(right.domain_snapshot().unwrap(), state);
    }
    #[test]
    fn malformed_stale_and_wrong_time_packets_leave_owner_and_mirror_unchanged() {
        let mut source = wire::tests::fixture(false, 3);
        source.ram[PAGE] = 1;
        let (mut a, mut b) = (Delta::default(), Delta::default());
        let mut live = Sound::default();
        let packet = a.export(&mut source).unwrap();
        let before = live.domain_snapshot().unwrap();
        let mirror = b.ram.clone();
        assert!(b.import(&mut live, &packet, Some(1)).is_err());
        let recheck = |bytes: &mut Vec<u8>| {
            let at = bytes.len() - 8;
            let hash = checksum(&bytes[..at]);
            bytes[at..].copy_from_slice(&hash.to_le_bytes());
        };
        let mut bad = Vec::new();
        bad.push(packet[..packet.len() - 1].to_vec());
        let mut checksum_bad = packet.clone();
        checksum_bad[20] ^= 1;
        bad.push(checksum_bad);
        let mut generation = packet.clone();
        generation[8..16].copy_from_slice(&2u64.to_le_bytes());
        recheck(&mut generation);
        bad.push(generation);
        let metadata = u32::from_le_bytes(packet[16..20].try_into().unwrap()) as usize;
        let count_at = 20 + metadata;
        assert!(u32::from_le_bytes(packet[count_at..count_at + 4].try_into().unwrap()) >= 2);
        let mut invalid_page = packet.clone();
        invalid_page[count_at + 4..count_at + 8].copy_from_slice(&128u32.to_le_bytes());
        recheck(&mut invalid_page);
        bad.push(invalid_page);
        let mut duplicate = packet.clone();
        let index = duplicate[count_at + 4..count_at + 8].to_vec();
        duplicate[count_at + 8 + PAGE..count_at + 12 + PAGE].copy_from_slice(&index);
        recheck(&mut duplicate);
        bad.push(duplicate);
        let mut trailing = packet.clone();
        trailing.insert(trailing.len() - 8, 0);
        recheck(&mut trailing);
        bad.push(trailing);
        for bytes in bad {
            assert!(b.import(&mut live, &bytes, None).is_err());
            assert_eq!(live.domain_snapshot().unwrap(), before);
            assert_eq!(b.generation, 0);
            assert_eq!(b.ram, mirror);
        }
        b.import(&mut live, &packet, None).unwrap();
        let valid = live.domain_snapshot().unwrap();
        assert!(b.import(&mut live, &packet, None).is_err());
        assert_eq!(live.domain_snapshot().unwrap(), valid);
    }
}
