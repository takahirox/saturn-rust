//! IEC 60908 §§19.3/19.7, figures20–24; Sega CD I/F §4.3.
//! Input is after disc deinterleaving, before P then Q correction.
//! Timing and the representation boundary are specified in docs/cd-rw.md.
use crate::media::{Media, TrackMode};

const HZ: u64 = 26_874_100;
pub(crate) const PREROLL: u32 = (HZ * 2).div_ceil(75) as u32;
const DECODE: u64 = (HZ * 32).div_ceil(10000);

const fn multiply(mut a: u8, mut b: u8) -> u8 {
    let mut result = 0;
    while b != 0 {
        if b & 1 != 0 {
            result ^= a;
        }
        b >>= 1;
        a <<= 1;
        if a & 64 != 0 {
            a ^= 0x43;
        }
    }
    result
}
const fn powers() -> [u8; 63] {
    let mut result = [1; 63];
    let mut i = 1;
    while i < 63 {
        result[i] = multiply(result[i - 1], 2);
        i += 1;
    }
    result
}
const POW: [u8; 63] = powers();
fn divide(a: u8, b: u8) -> u8 {
    debug_assert_ne!(b, 0);
    let inverse = POW.iter().position(|v| *v == b).unwrap();
    multiply(a, POW[(63 - inverse) % 63])
}
fn syndrome(code: &[u8], checks: usize) -> [u8; 4] {
    let mut s = [0; 4];
    for (row, out) in s.iter_mut().enumerate().take(checks) {
        for (i, symbol) in code.iter().enumerate() {
            *out ^= multiply(*symbol, POW[(row * (code.len() - 1 - i)) % 63]);
        }
    }
    s
}
// Bounded-distance correction. Solve the first two syndrome equations for
// each possible location pair, then verify ALL equations before mutation.
// The tiny fixed codes avoid a general-purpose polynomial/RS dependency.
fn correct(code: &mut [u8], checks: usize) -> bool {
    let s = syndrome(code, checks);
    if s == [0; 4] {
        return true;
    }
    for i in 0..code.len() {
        let x = POW[code.len() - 1 - i];
        if (1..checks).all(|r| s[r] == multiply(s[0], POW[(r * (code.len() - 1 - i)) % 63])) {
            code[i] ^= s[0];
            return true;
        }
        if checks < 4 {
            continue;
        }
        for j in i + 1..code.len() {
            let y = POW[code.len() - 1 - j];
            let a = divide(s[1] ^ multiply(s[0], y), x ^ y);
            let b = s[0] ^ a;
            if (2..checks).all(|r| {
                s[r] == multiply(a, POW[(r * (code.len() - 1 - i)) % 63])
                    ^ multiply(b, POW[(r * (code.len() - 1 - j)) % 63])
            }) {
                code[i] ^= a;
                code[j] ^= b;
                return true;
            }
        }
    }
    false
}
pub(crate) fn decode(mut bytes: [u8; 24]) -> ([u8; 24], bool) {
    let p = correct(&mut bytes, 4);
    let q = correct(&mut bytes[..4], 2);
    // Figure24 places Q after P. Recheck P after Q so a Q miscorrection
    // cannot turn a failed full pack into a clean one.
    let valid = p && q && syndrome(&bytes, 4) == [0; 4] && syndrome(&bytes[..4], 2) == [0; 4];
    (bytes, !valid)
}

#[derive(Clone, Copy, Default)]
struct Pack {
    bytes: [u8; 24],
    error: bool,
}

struct Acquisition {
    first: u32,
    elapsed: u64,
    origin: u64,
    next: u64,
    frames: u64,
    packs: std::collections::VecDeque<Option<[u8; 24]>>,
}
impl Acquisition {
    fn capture(&mut self, media: Option<&Media>) {
        let fad = self.first + self.frames as u32;
        for pack in 0..4 {
            // Keep real lookahead beyond PLAY's end: an unchanged-start PLAY
            // may extend the range before these packs reach publication.
            // The decoder checks the current end before consuming a pack.
            let bytes = media
                .filter(|m| fad < m.leadout())
                .filter(|m| m.track(fad).is_some_and(|t| t.mode == TrackMode::Audio))
                .and_then(|m| m.rw_pack(fad, pack));
            self.packs.push_back(bytes);
        }
        self.frames += 1;
        debug_assert!(self.packs.len() <= 8);
    }
}
#[derive(Default)]
pub(crate) struct Decoder {
    ring: [Pack; 24],
    written: u64,
    read: u64,
    // Acquisition has its own 75 Hz deadlines and an eight-pack input window.
    // Neither this position nor the 300 Hz decoder depends on audio draining.
    input: Option<Acquisition>,
    stream: Option<(u32, u32)>,
    elapsed: u64,
    next: u64,
    packs: u64,
}
impl Decoder {
    pub fn stop(&mut self) {
        self.stream = None;
        self.input = None;
    }
    pub fn set_end(&mut self, end: u32) {
        if let Some((_, current)) = &mut self.stream {
            *current = end;
        }
    }
    pub fn prepare(&mut self, first: u32, seek: u32) {
        self.stream = None;
        self.input = Some(Acquisition {
            first,
            elapsed: 0,
            origin: u64::from(seek),
            next: u64::from(seek) + HZ.div_ceil(75),
            frames: 0,
            packs: std::collections::VecDeque::with_capacity(8),
        });
    }
    pub fn start(&mut self, first: u32, end: u32, media: Option<&Media>) {
        // A positioned pause/resume has the already deinterleaved lookahead
        // available. Explicit seeks instead fill it on two acquisition deadlines.
        if self
            .input
            .as_ref()
            .is_none_or(|i| i.first != first || i.packs.len() != 8)
        {
            self.prepare(first, 0);
            let input = self.input.as_mut().unwrap();
            input.capture(media);
            input.capture(media);
            input.elapsed = u64::from(PREROLL);
            input.next = (3 * HZ).div_ceil(75);
        }
        self.written = 0;
        self.read = 0;
        self.stream = Some((first, end));
        self.elapsed = 0;
        self.next = DECODE;
        self.packs = 0;
    }
    pub fn clocks(&self) -> u32 {
        let input = self
            .input
            .as_ref()
            .map_or(u32::MAX, |i| (i.next - i.elapsed) as u32);
        input.min(
            self.stream
                .map_or(u32::MAX, |_| (self.next - self.elapsed) as u32),
        )
    }
    pub fn advance(&mut self, clocks: u32, media: Option<&Media>) {
        if let Some(input) = &mut self.input {
            input.elapsed += u64::from(clocks);
            if input.elapsed >= input.next {
                input.capture(media);
                input.next = input.origin + ((input.frames + 1) * HZ).div_ceil(75);
            }
        }
        let Some((first, end)) = self.stream else {
            return;
        };
        self.elapsed += u64::from(clocks);
        if self.elapsed < self.next {
            return;
        }
        let fad = first + (self.packs / 4) as u32;
        if media.is_none_or(|m| {
            fad >= end.min(m.leadout()) || m.track(fad).is_none_or(|t| t.mode != TrackMode::Audio)
        }) {
            self.stop();
            return;
        }
        if let Some(bytes) = self
            .input
            .as_mut()
            .and_then(|i| i.packs.pop_front())
            .flatten()
        {
            let (bytes, error) = decode(bytes);
            self.ring[(self.written % 24) as usize] = Pack { bytes, error };
            self.written += 1;
        }
        self.packs += 1;
        self.next = DECODE + (self.packs * HZ).div_ceil(300);
    }
    pub fn front(&self) -> Option<(u64, u16)> {
        (self.read < self.written).then(|| {
            let pack = &self.ring[(self.read % 24) as usize];
            (
                self.read,
                u16::from(pack.error) | if self.written - self.read > 24 { 2 } else { 0 },
            )
        })
    }
    pub fn bytes(&self, sequence: u64) -> &[u8; 24] {
        &self.ring[(sequence % 24) as usize].bytes
    }
    pub fn end(&mut self, sequence: u64) {
        // 24 physical slots; the later manual separately specifies recovery
        // after 23 unacquired packs. Never reduce the physical capacity to 23.
        self.read = (sequence + 1).max(self.written.saturating_sub(23));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Independently solved parity-matrix vectors. Expectations are literals,
    // not an encoder/decoder round trip. See the matrix worksheet in the builder.
    const CLEAN: [u8; 24] = [
        56, 0, 46, 22, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 39, 59, 11, 23,
    ];
    #[test]
    fn literal_codewords_and_header_payload_parity_errors() {
        assert_eq!(decode(CLEAN), (CLEAN, false));
        assert_eq!(decode([0; 24]), ([0; 24], false));
        for i in 0..24 {
            for value in 1..64 {
                let mut received = CLEAN;
                received[i] ^= value;
                assert_eq!(decode(received), (CLEAN, false), "symbol {i} xor {value}");
            }
            for j in i + 1..24 {
                let mut received = CLEAN;
                received[i] ^= 17;
                received[j] ^= 39;
                assert_eq!(decode(received), (CLEAN, false), "symbols {i}/{j}");
            }
        }
        for locations in [[4, 5, 6], [0, 1, 4]] {
            let mut received = CLEAN;
            for (i, e) in locations.into_iter().zip([1, 2, 4]) {
                received[i] ^= e;
            }
            assert!(decode(received).1, "three-symbol pattern {locations:?}");
        }
        // Q remains independently checked even for a P-valid codeword.
        let wrong_header = [
            56, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 53, 5, 44, 36,
        ];
        assert!(decode(wrong_header).1);
    }

    #[test]
    fn every_publication_has_before_at_after_deadlines_and_a_fixed_horizon() {
        let raw = vec![0; 2352 * 30];
        let media = Media::cue(
            "FILE \"audio\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
            &[("audio", &raw)],
        )
        .unwrap();
        let mut bytes = b"SRW1".to_vec();
        bytes.extend(150u32.to_be_bytes());
        bytes.extend(30u32.to_be_bytes());
        for _ in 0..120 {
            bytes.extend(CLEAN);
        }
        let media = media.with_subchannel(&bytes).unwrap();
        let mut traces = Vec::new();
        for chunk in [1, 3, 17, 10000] {
            let mut d = Decoder::default();
            d.start(150, 180, Some(&media));
            let mut at = 0;
            let mut trace = Vec::new();
            for n in 0..100u64 {
                let deadline = 85_998 + (n * 26_874_100).div_ceil(300);
                let mut left = deadline - 1 - at;
                while left != 0 {
                    let delta = left.min(chunk).min(u64::from(d.clocks()));
                    d.advance(delta as u32, Some(&media));
                    left -= delta;
                    // Passive reads must not move either pointer.
                    let _ = (d.front(), d.bytes(0));
                }
                assert_eq!(d.written, n);
                d.advance(1, Some(&media));
                assert_eq!(d.written, n + 1);
                d.advance(1, Some(&media));
                assert_eq!(d.written, n + 1);
                trace.push((d.written, d.front(), *d.bytes(0)));
                at = deadline + 1;
            }
            traces.push(trace);
        }
        assert!(traces.iter().all(|t| t == &traces[0]));
    }
    #[test]
    fn acquisition_has_two_preroll_deadlines_and_cancellation_discards_the_window() {
        let mut d = Decoder::default();
        d.prepare(150, 26875);
        assert_eq!(d.clocks(), 385197);
        d.advance(385196, None);
        assert!(d.input.as_ref().unwrap().packs.is_empty());
        d.advance(1, None);
        assert_eq!(d.input.as_ref().unwrap().packs.len(), 4);
        assert_eq!(d.clocks(), 358321);
        d.advance(358320, None);
        assert_eq!(d.input.as_ref().unwrap().packs.len(), 4);
        d.advance(1, None);
        assert_eq!(d.input.as_ref().unwrap().packs.len(), 8);
        assert_eq!(d.front(), None); // Acquisition cannot publish decoder output.
        d.start(150, 180, None);
        assert_eq!(d.clocks(), 85998);
        d.advance(85997, None);
        assert_eq!(d.input.as_ref().unwrap().packs.len(), 8);
        d.stop();
        d.advance(1_000_000, None);
        assert!(d.input.is_none());
        assert_eq!(d.clocks(), u32::MAX);
        assert_eq!(d.front(), None);
    }
}
