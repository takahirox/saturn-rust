//! Bounded, architecture-independent state transport for owned sound handoff.
//! No pointers, native object layouts, external dependencies or guest code.
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

pub(crate) type Error = &'static str;
pub(crate) const LIMIT: usize = 1024 * 1024;
pub(crate) struct Input<'a> {
    bytes: &'a [u8],
    allocation: usize,
}
impl<'a> Input<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            allocation: 4 * LIMIT,
        }
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let (head, tail) = self
            .bytes
            .split_at_checked(n)
            .ok_or("truncated sound state")?;
        self.bytes = tail;
        Ok(head)
    }
    fn allocate(&mut self, bytes: usize) -> Result<(), Error> {
        self.allocation = self
            .allocation
            .checked_sub(bytes)
            .ok_or("sound allocation budget")?;
        Ok(())
    }
    pub fn finished(&self) -> bool {
        self.bytes.is_empty()
    }
}
pub(crate) trait Wire: Sized {
    fn put(&self, out: &mut Vec<u8>);
    fn get(input: &mut Input<'_>) -> Result<Self, Error>;
}
macro_rules! structure {
    ($ty:ty { $($field:ident),* $(,)? } $(bytes { $($raw:ident),* $(,)? })?
        $(defaults { $($(#[$attr:meta])* $skip:ident : $value:expr),* $(,)? })?) => {
        impl $crate::sound::wire::Wire for $ty {
            fn put(&self, out: &mut Vec<u8>) {
                $( $crate::sound::wire::Wire::put(&self.$field, out); )*
                $($( $crate::sound::wire::put_bytes(&self.$raw, out); )*)?
            }
            fn get(input: &mut $crate::sound::wire::Input<'_>)
                -> ::core::result::Result<Self, $crate::sound::wire::Error> {
                $( let $field = $crate::sound::wire::Wire::get(input)?; )*
                $($( let $raw = $crate::sound::wire::get_bytes(input)?; )*)?
                Ok(Self { $($field,)* $($($raw,)*)?
                    $($( $(#[$attr])* $skip: $value, )*)? })
            }
        }
    };
}
pub(crate) use structure;
macro_rules! enumeration {
    ($ty:ty { $($variant:ident = $tag:literal),* $(,)? }) => {
        impl $crate::sound::wire::Wire for $ty {
            fn put(&self, out: &mut Vec<u8>) {
                let tag: u8=match self { $(Self::$variant=>$tag,)* };
                $crate::sound::wire::Wire::put(&tag, out);
            }
            fn get(input: &mut $crate::sound::wire::Input<'_>) -> ::core::result::Result<Self,
                $crate::sound::wire::Error> {
                match <u8 as $crate::sound::wire::Wire>::get(input)? {
                    $($tag=>Ok(Self::$variant),)* _=>Err("invalid sound enum")
                }
            }
        }
    };
}
pub(crate) use enumeration;
macro_rules! integer {
    ($($ty:ty),*) => { $(impl Wire for $ty {
        fn put(&self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
        fn get(input: &mut Input<'_>) -> Result<Self, Error> {
            Ok(Self::from_le_bytes(input.take(size_of::<Self>())?.try_into().unwrap()))
        }
    })* };
}
integer!(u8, u16, u32, u64, i16, i32, i64);
impl Wire for usize {
    fn put(&self, out: &mut Vec<u8>) {
        u32::try_from(*self).expect("bounded sound index").put(out);
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(u32::get(input)? as usize)
    }
}
impl Wire for bool {
    fn put(&self, out: &mut Vec<u8>) {
        u8::from(*self).put(out);
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        match u8::get(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("invalid sound boolean"),
        }
    }
}
impl<T: Wire> Wire for Option<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.is_some().put(out);
        if let Some(value) = self {
            value.put(out);
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(if bool::get(input)? {
            Some(T::get(input)?)
        } else {
            None
        })
    }
}
impl<T: Wire, E: Wire> Wire for Result<T, E> {
    fn put(&self, out: &mut Vec<u8>) {
        self.is_ok().put(out);
        match self {
            Ok(v) => v.put(out),
            Err(v) => v.put(out),
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(if bool::get(input)? {
            Ok(T::get(input)?)
        } else {
            Err(E::get(input)?)
        })
    }
}
impl<T: Wire + Copy> Wire for Cell<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.get().put(out);
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(Self::new(T::get(input)?))
    }
}
impl<T: Wire> Wire for RefCell<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.borrow().put(out);
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(Self::new(T::get(input)?))
    }
}
impl<A: Wire, B: Wire> Wire for (A, B) {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
        self.1.put(out);
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok((A::get(input)?, B::get(input)?))
    }
}
impl<T: Wire, const N: usize> Wire for [T; N] {
    fn put(&self, out: &mut Vec<u8>) {
        for value in self {
            value.put(out);
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        input.allocate(
            N.checked_mul(size_of::<T>())
                .ok_or("sound array overflow")?,
        )?;
        let mut values = Vec::with_capacity(N);
        for _ in 0..N {
            values.push(T::get(input)?);
        }
        values.try_into().map_err(|_| "sound array length")
    }
}
impl<T: Wire> Wire for Vec<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.len().put(out);
        for value in self {
            value.put(out);
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        let n = usize::get(input)?;
        if n > 16384 {
            return Err("sound collection length");
        }
        input.allocate(
            n.checked_mul(size_of::<T>().max(1))
                .ok_or("sound vector overflow")?,
        )?;
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(T::get(input)?);
        }
        Ok(values)
    }
}
impl<T: Wire> Wire for VecDeque<T> {
    fn put(&self, out: &mut Vec<u8>) {
        self.len().put(out);
        for value in self {
            value.put(out);
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(Vec::<T>::get(input)?.into())
    }
}
pub(crate) fn put_bytes(bytes: &[u8], out: &mut Vec<u8>) {
    bytes.len().put(out);
    out.extend_from_slice(bytes);
}
pub(crate) fn get_bytes(input: &mut Input<'_>) -> Result<Vec<u8>, Error> {
    let n = usize::get(input)?;
    if n > LIMIT {
        return Err("sound byte length");
    }
    input.allocate(n)?;
    Ok(input.take(n)?.to_vec())
}
pub(crate) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xCBF29CE484222325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001B3)
    })
}

use crate::smpc::diagnostics::{AccessFault, CommandState, ReadUnit, Rejection};
structure!(AccessFault {
    address,
    width,
    write,
    reason,
    state,
    main_clock,
    instruction,
    refill
});
structure!(CommandState {
    sf,
    busy,
    command,
    pending,
    deferred_control,
    remaining,
    await_continue,
    await_vblank,
    peripheral_phase,
    input,
    status,
    command_report
});
structure!(ReadUnit { address, width });
enumeration!(Rejection { WriteOnly=0, ReadOnly=1, ByteOnly=2, InvalidSf=3, Busy=4,
    UnsupportedCommand=5, InvalidParameters=6, ProhibitedIssuer=7, UnsupportedRegister=8 });
impl Wire for crate::bus::BusError {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Smpc(fault) => {
                0u8.put(out);
                fault.put(out)
            }
            Self::Pending => 1u8.put(out),
            Self::Unaligned { address, width } => {
                2u8.put(out);
                address.put(out);
                width.put(out)
            }
            Self::Unmapped { address, width } => {
                3u8.put(out);
                address.put(out);
                width.put(out)
            }
            Self::ReadOnly { address, width } => {
                4u8.put(out);
                address.put(out);
                width.put(out)
            }
            Self::BiosLength { actual } => {
                5u8.put(out);
                actual.put(out)
            }
            Self::BackupLength { actual } => {
                6u8.put(out);
                actual.put(out)
            }
            Self::UnsupportedDevice { address, width } => {
                7u8.put(out);
                address.put(out);
                width.put(out)
            }
        }
    }
    fn get(input: &mut Input<'_>) -> Result<Self, Error> {
        Ok(match u8::get(input)? {
            0 => Self::Smpc(AccessFault::get(input)?),
            1 => Self::Pending,
            2 => Self::Unaligned {
                address: u32::get(input)?,
                width: usize::get(input)?,
            },
            3 => Self::Unmapped {
                address: u32::get(input)?,
                width: usize::get(input)?,
            },
            4 => Self::ReadOnly {
                address: u32::get(input)?,
                width: usize::get(input)?,
            },
            5 => Self::BiosLength {
                actual: usize::get(input)?,
            },
            6 => Self::BackupLength {
                actual: usize::get(input)?,
            },
            7 => Self::UnsupportedDevice {
                address: u32::get(input)?,
                width: usize::get(input)?,
            },
            _ => return Err("invalid sound bus error"),
        })
    }
}

impl super::Sound {
    /// Lossless owned state for a same-build remote sound domain. The trace
    /// recorder, test-only oracles and derived host memos stay with their owner.
    pub fn domain_snapshot(&self) -> Result<Vec<u8>, Error> {
        if self.projection.is_some() {
            return Err("sound projection is not canonical state");
        }
        let mut out = Vec::with_capacity(super::SOUND_RAM_SIZE + 32768);
        out.extend_from_slice(b"SRSD\x02\0\0\0");
        self.put(&mut out);
        if out.len() + 8 > LIMIT {
            return Err("sound snapshot size");
        }
        let check = checksum(&out);
        check.put(&mut out);
        Ok(out)
    }
    /// Decode and validate first; failure leaves the live owner unchanged.
    pub fn restore_domain_snapshot(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let restored = Self::from_domain_snapshot(bytes)?;
        #[cfg(any(test, feature = "kernel-experiment"))]
        let restored = Self {
            domain_trace: self.domain_trace.clone(),
            ..restored
        };
        *self = restored;
        Ok(())
    }
    pub(crate) fn from_domain_snapshot(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 16 || bytes.len() > LIMIT || bytes[..8] != *b"SRSD\x02\0\0\0" {
            return Err("sound snapshot header");
        }
        let (payload, trailer) = bytes.split_at(bytes.len() - 8);
        if checksum(payload) != u64::from_le_bytes(trailer.try_into().unwrap()) {
            return Err("sound snapshot checksum");
        }
        let mut input = Input::new(&payload[8..]);
        let restored = Self::get(&mut input)?;
        if !input.finished() || restored.ram.len() != super::SOUND_RAM_SIZE {
            return Err("sound snapshot trailing bytes or RAM size");
        }
        Ok(restored)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::sound::{SOUND_RAM_SIZE, Sound, memory};

    pub(crate) fn fixture(reset: bool, features: u32) -> Sound {
        let mut sound = Sound::default();
        sound.scsp.write16(0x400, 0x20F).unwrap();
        sound.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
        sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
        let program: &[u16] = if reset {
            &[0x4E70, 0x60FC]
        } else {
            &[0x33FC, 0x1234, 0, 0x800, 0x60F6]
        };
        for (i, word) in program.iter().enumerate() {
            sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        for (i, byte) in sound.ram[0x1000..0x1400].iter_mut().enumerate() {
            *byte = i as u8;
        }
        for (at, value) in [
            (2, 0x1000),
            (6, 256),
            (12, 0x100),
            (22, 0xE000),
            (0, 0x1820),
        ] {
            sound.scsp.write16(at, value).unwrap();
        }
        if features & 1 != 0 {
            sound.scsp.write16(0x780, 0x1000).unwrap();
            sound.scsp.write16(0x80C, 0xC000).unwrap();
        }
        if features & 2 != 0 {
            for (at, value) in [(0x412, 0x3000), (0x414, 0), (0x416, 0x1008)] {
                sound.scsp.write16(at, value).unwrap();
            }
        }
        sound.set_enabled(true).unwrap();
        sound.main_ram_request(Some(memory::Request {
            address: 0x4000,
            width: 4,
            write: Some(0xCDEF1234),
        }));
        sound
    }
    fn advance(sound: &mut Sound, target: u64, chunk: u32) {
        while sound.time < target {
            sound
                .advance((target - sound.time).min(u64::from(chunk)) as u32)
                .unwrap();
        }
    }
    #[test]
    fn handoff_keeps_inflight_cpu_pcm_dsp_dma_and_split_dram_words() {
        let mut pcm = false;
        let mut dma = false;
        for reset in [false, true] {
            for features in 0..4 {
                for at in [
                    0, 1, 4, 5, 9, 10, 14, 15, 19, 20, 47, 48, 49, 95, 96, 97, 128, 129, 255, 256,
                    610, 611, 997, 2048,
                ] {
                    let mut original = fixture(reset, features);
                    advance(&mut original, at, 17);
                    pcm |= original.scsp.pcm_word.is_some();
                    dma |= original.scsp.dma_word.is_some();
                    let snapshot = original.domain_snapshot().unwrap();
                    assert!(snapshot.len() > SOUND_RAM_SIZE && snapshot.len() < LIMIT);
                    let mut moved = Sound::default();
                    moved.restore_domain_snapshot(&snapshot).unwrap();
                    assert_eq!(
                        moved.domain_digest(),
                        original.domain_digest(),
                        "at={at}, reset={reset}, features={features}"
                    );
                    assert_eq!(moved.domain_snapshot().unwrap(), snapshot);
                    if original.main_ram_ready() {
                        assert_eq!(original.complete_main_ram().unwrap(), 0xCDEF1234);
                        assert_eq!(moved.complete_main_ram().unwrap(), 0xCDEF1234);
                    }
                    for more in [1, 17, 999, 5000] {
                        let target = original.time + more;
                        advance(&mut original, target, 1);
                        advance(&mut moved, target, 97);
                        assert_eq!(moved.domain_digest(), original.domain_digest());
                    }
                    assert_eq!(moved.scsp.drain(8192), original.scsp.drain(8192));
                }
            }
        }
        assert!(
            pcm && dma,
            "test must exercise retained PCM and DMA operands"
        );
    }
    #[test]
    fn malformed_snapshots_fail_without_changing_live_state() {
        let mut live = fixture(false, 3);
        advance(&mut live, 97, 1);
        let snapshot = live.domain_snapshot().unwrap();
        let before = live.domain_digest();
        let mut cases = vec![
            vec![],
            snapshot[..7].to_vec(),
            snapshot[..snapshot.len() - 1].to_vec(),
            vec![0; LIMIT + 1],
        ];
        let mut bad = snapshot.clone();
        bad[8] ^= 1;
        cases.push(bad);
        let mut trailing = snapshot[..snapshot.len() - 8].to_vec();
        trailing.push(0);
        let hash = checksum(&trailing);
        trailing.extend_from_slice(&hash.to_le_bytes());
        cases.push(trailing);
        for bad in cases {
            assert!(live.restore_domain_snapshot(&bad).is_err());
            assert_eq!(live.domain_digest(), before);
        }
        // Nested collection lengths, tags and allocation accounting reject a
        // correctly checksummed but structurally invalid producer packet too.
        let bad_length = u32::MAX.to_le_bytes();
        let mut input = Input::new(&bad_length);
        assert!(Vec::<u64>::get(&mut input).is_err());
        let mut input = Input::new(&[2]);
        assert!(bool::get(&mut input).is_err());
        let mut input = Input::new(&[255]);
        assert!(crate::bus::BusError::get(&mut input).is_err());
    }
    #[test]
    fn handoff_preserves_a_recorded_bus_error_during_exception_entry() {
        let mut original = fixture(false, 0);
        for (i, word) in [0x3039u16, 0x0020, 0, 0x60F8].into_iter().enumerate() {
            original.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
        }
        original.ram[8..12].copy_from_slice(&0x180u32.to_be_bytes());
        original.ram[0x180..0x182].copy_from_slice(&0x60FEu16.to_be_bytes());
        let mut captured = false;
        for _ in 0..2000 {
            original.advance(1).unwrap();
            if original.transactions.domain_error_count() != 0 {
                captured = true;
                let mut moved = Sound::default();
                moved
                    .restore_domain_snapshot(&original.domain_snapshot().unwrap())
                    .unwrap();
                assert_eq!(
                    moved.transactions.domain_error_count(),
                    original.transactions.domain_error_count()
                );
                assert_eq!(moved.domain_digest(), original.domain_digest());
                let target = original.time + 2000;
                advance(&mut original, target, 1);
                advance(&mut moved, target, 17);
                assert_eq!(moved.domain_digest(), original.domain_digest());
                assert!(original.cpu.state.exceptions > 0);
                break;
            }
        }
        assert!(captured, "must snapshot an actual in-flight recorded error");
    }
}
