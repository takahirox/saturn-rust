//! CHD container adapter only: MAME v5 CD units -> the existing Media reader.
//! Format references and decoder tradeoffs: docs/media.md.
use super::{MAX_FRAMES, Media, Mode2Format, RAW_SECTOR, Track, TrackMode, invalid};
use crate::disc::{DiscError, SECTOR};
use ::chd::{
    Chd,
    map::{CompressionTypeV5, MapEntry},
};
use sha1::{Digest, Sha1};
use std::{
    collections::HashSet,
    io::Cursor,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
struct Bytes(Arc<Vec<u8>>);
impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
const UNIT: usize = 2448;
const MAX_HUNK: usize = 1_048_576;
const MAX_UNITS: usize = MAX_FRAMES as usize + 99 * 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Mode1,
    Raw,
    Audio,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Subcode {
    None,
    Raw,
    Cooked,
}
#[derive(Clone, Copy)]
pub(super) struct Layout {
    pub kind: Kind,
    pub subcode: Subcode,
}

pub(super) struct Source {
    image: Chd<Cursor<Bytes>>,
    targets: Vec<u32>,
    compressed: Vec<u8>,
    cache: Vec<u8>,
    cached: Option<u32>,
}
fn error(e: impl std::fmt::Display) -> DiscError {
    invalid(&format!("Invalid CHD: {e}"))
}
fn range(bytes: &[u8], offset: u64, len: usize) -> Result<&[u8], DiscError> {
    let at = usize::try_from(offset).map_err(error)?;
    bytes
        .get(
            at..at
                .checked_add(len)
                .ok_or_else(|| error("offset overflow"))?,
        )
        .ok_or_else(|| error("truncated header, map, metadata or hunk"))
}
fn number(bytes: &[u8], offset: usize, len: usize) -> u64 {
    bytes[offset..offset + len]
        .iter()
        .fold(0, |n, b| n * 256 + u64::from(*b))
}
fn layout(kind: &str, sub: &str) -> Result<Layout, DiscError> {
    let kind = match kind {
        "MODE1" => Kind::Mode1,
        "MODE1_RAW" | "MODE2_RAW" => Kind::Raw,
        "AUDIO" => Kind::Audio,
        _ => {
            return Err(error(format!(
                "unsupported CD track type {kind}; supported: MODE1, MODE1_RAW, MODE2_RAW, AUDIO"
            )));
        }
    };
    let subcode = match sub {
        "NONE" => Subcode::None,
        "RW_RAW" => Subcode::Raw,
        "RW" => Subcode::Cooked,
        _ => return Err(error(format!("unsupported subchannel type {sub}"))),
    };
    Ok(Layout { kind, subcode })
}
// Upstream's RLE tree reader indexes before checking run length. Check its
// small 16-symbol prefix and Kraft bound before allowing it to parse the map.
fn check_tree(bytes: &[u8]) -> Result<(), DiscError> {
    let mut nibble = 0;
    let mut next = || {
        let b = *bytes
            .get(nibble / 2)
            .ok_or_else(|| error("truncated map Huffman tree"))?;
        let n = (b >> if nibble % 2 == 0 { 4 } else { 0 }) & 15;
        nibble += 1;
        Ok::<_, DiscError>(n as usize)
    };
    let (mut codes, mut weight) = (0, 0);
    while codes < 16 {
        let mut bits = next()?;
        let mut count = 1;
        if bits == 1 {
            bits = next()?;
            if bits != 1 {
                count = next()? + 3;
            }
        }
        if bits > 8 || codes + count > 16 {
            return Err(error("invalid map Huffman run"));
        }
        codes += count;
        if bits != 0 {
            weight += count * (256 >> bits);
        }
    }
    if weight == 0 || weight > 256 {
        return Err(error("invalid map Huffman tree"));
    }
    Ok(())
}

// CD FLAC is stereo signed 16-bit PCM. The upstream codec's optional channel
// iterator avoids an assertion but does not reject non-stereo frames or ensure
// that they filled the output. Validate every frame, not only the first one.
fn check_flac(input: &[u8], hunk: usize) -> Result<(), DiscError> {
    let mut remaining = hunk / UNIT * (RAW_SECTOR / 4);
    let mut offset = 0;
    let mut buffer = Vec::new();
    while remaining != 0 {
        let flags = *input
            .get(offset + 3)
            .ok_or_else(|| error("truncated CD FLAC frame"))?;
        if !matches!(flags >> 4, 1 | 8 | 9 | 10) || (flags >> 1) & 7 != 4 {
            return Err(error("CD FLAC requires stereo 16-bit frames"));
        }
        let mut reader = claxon::frame::FrameReader::new(Cursor::new(&input[offset..]));
        let block = reader
            .read_next_or_eof(buffer)
            .map_err(error)?
            .ok_or_else(|| error("missing CD FLAC frame"))?;
        let samples = block.len() as usize / 2;
        if block.channels() != 2 || samples == 0 || samples > remaining {
            return Err(error("CD FLAC frame exceeds stereo hunk bounds"));
        }
        remaining -= samples;
        offset += reader.into_inner().position() as usize;
        buffer = block.into_buffer();
    }
    Ok(())
}

impl Source {
    pub(super) fn frame(&mut self, frame: u32) -> Result<[u8; UNIT], DiscError> {
        let byte = frame as usize * UNIT;
        let hunk = byte / self.cache.len();
        let target = *self
            .targets
            .get(hunk)
            .ok_or_else(|| error("sector outside CHD"))?;
        self.load(target)?;
        self.cache[byte % self.cache.len()..][..UNIT]
            .try_into()
            .map_err(error)
    }
    fn load(&mut self, target: u32) -> Result<(), DiscError> {
        if self.cached != Some(target) {
            self.cached = None;
            self.cache.fill(0);
            let read = self
                .image
                .hunk(target)
                .map_err(error)?
                .read_hunk_in(&mut self.compressed, &mut self.cache)
                .map_err(error)?;
            if read != self.cache.len() {
                return Err(error("short decoded hunk"));
            }
            self.cached = Some(target);
        }
        Ok(())
    }
}

impl Media {
    pub fn chd(bytes: &[u8]) -> Result<Self, DiscError> {
        if bytes.len() > crate::disc::MAX_MEDIA_BYTES {
            return Err(error("input exceeds 1058400000 bytes"));
        }
        Self::chd_owned(crate::disc::copy(bytes)?)
    }
    /// Takes ownership of compressed bytes, validates with one reusable hunk,
    /// then retains compressed storage plus a single decoded hunk. No files,
    /// host program, boot policy, per-title data or expanded disc is involved.
    pub fn chd_owned(bytes: Vec<u8>) -> Result<Self, DiscError> {
        if bytes.len() > crate::disc::MAX_MEDIA_BYTES {
            return Err(error("input exceeds 1058400000 bytes"));
        }
        let header = range(&bytes, 0, 124)?;
        if &header[..8] != b"MComprHD" || number(header, 8, 4) != 124 || number(header, 12, 4) != 5
        {
            return Err(error(
                "expected a self-contained CHD v5 CD-ROM (124-byte header)",
            ));
        }
        if header[104..124].iter().any(|b| *b != 0) {
            return Err(error(
                "parent/differencing CHDs are unsupported; select a self-contained image",
            ));
        }
        let codecs = header[16..32].as_chunks::<4>().0;
        let compressed = codecs[0] != [0; 4];
        let mut ended = false;
        for codec in codecs {
            if *codec == [0; 4] {
                ended = true;
            } else if ended || !matches!(codec, b"cdzl" | b"cdlz" | b"cdfl") {
                return Err(error(format!(
                    "unsupported codec {:?}; supported: cdzl, cdlz, cdfl, none",
                    String::from_utf8_lossy(codec)
                )));
            }
        }
        let logical = number(header, 32, 8);
        let hunk = number(header, 56, 4) as usize;
        if number(header, 60, 4) != UNIT as u64
            || !(UNIT..=MAX_HUNK).contains(&hunk)
            || !hunk.is_multiple_of(UNIT)
            || logical == 0
            || !logical.is_multiple_of(UNIT as u64)
            || logical > (MAX_UNITS * UNIT) as u64
        {
            return Err(error(
                "CD requires 2448-byte units, whole units, <=450297 stored frames and hunks <=1 MiB",
            ));
        }
        let count = logical.div_ceil(hunk as u64) as usize;
        let map_at = number(header, 40, 8);
        if map_at < 124 {
            return Err(error("missing hunk map"));
        }
        if compressed {
            let m = range(&bytes, map_at, 16)?;
            let length = number(m, 0, 4) as usize;
            if length == 0
                || length > count * 16 + 256
                || m[12] > 24
                || m[13] > 32
                || m[14] > 32
                || m[15] != 0
            {
                return Err(error(
                    "compressed map exceeds bounds or has invalid bit widths",
                ));
            }
            check_tree(range(
                &bytes,
                map_at
                    .checked_add(16)
                    .ok_or_else(|| error("map overflow"))?,
                length,
            )?)?;
        } else {
            range(&bytes, map_at, count * 4)?;
        }

        let mut tracks = Vec::new();
        let mut seen = HashSet::new();
        let mut meta = number(header, 48, 8);
        let (mut cursor, mut chd_frame) = (150u32, 0usize);
        while meta != 0 {
            if seen.len() >= 99 || !seen.insert(meta) {
                return Err(error("too many or cyclic metadata entries"));
            }
            let m = range(&bytes, meta, 16)?;
            let tag = &m[..4];
            if tag != b"CHT2" && tag != b"CHTR" {
                return Err(error(format!(
                    "unsupported metadata {:?}; single-session CD track metadata required",
                    String::from_utf8_lossy(tag)
                )));
            }
            let len = number(m, 5, 3) as usize;
            if !(1..=1024).contains(&len) || m[4] & !1 != 0 {
                return Err(error("metadata size or flags invalid"));
            }
            let text = range(
                &bytes,
                meta.checked_add(16)
                    .ok_or_else(|| error("metadata overflow"))?,
                len,
            )?;
            let text = std::str::from_utf8(
                text.strip_suffix(&[0])
                    .ok_or_else(|| error("unterminated track metadata"))?,
            )
            .map_err(error)?;
            let fields: Vec<_> = text.split_ascii_whitespace().collect();
            let keys = [
                "TRACK", "TYPE", "SUBTYPE", "FRAMES", "PREGAP", "PGTYPE", "PGSUB", "POSTGAP",
            ];
            let n = if tag == b"CHT2" { 8 } else { 4 };
            if fields.len() != n {
                return Err(error("malformed track metadata"));
            }
            let mut values = Vec::new();
            for (field, key) in fields.iter().zip(keys) {
                values.push(
                    field
                        .strip_prefix(key)
                        .and_then(|s| s.strip_prefix(':'))
                        .ok_or_else(|| error("unexpected track metadata field"))?,
                );
            }
            let integer = |i: usize| values[i].parse::<u32>().map_err(error);
            if integer(0)? as usize != tracks.len() + 1 {
                return Err(error("CD track numbers must be consecutive from 1"));
            }
            let spec = layout(values[1], values[2])?;
            let frames = integer(3)?;
            let pregap = if n == 8 { integer(4)? } else { 0 };
            let postgap = if n == 8 { integer(7)? } else { 0 };
            let stored_gap = n == 8 && values[5].starts_with('V');
            if n == 8 {
                let pg = layout(values[5].strip_prefix('V').unwrap_or(values[5]), values[6])?;
                if pregap != 0 && stored_gap && (pg.kind != spec.kind || pg.subcode != spec.subcode)
                {
                    return Err(error(
                        "pregap with a different sector/subcode type is unsupported",
                    ));
                }
            }
            if frames == 0
                || frames > MAX_FRAMES
                || pregap > MAX_FRAMES
                || postgap > MAX_FRAMES
                || (stored_gap && pregap >= frames)
            {
                return Err(error("invalid track duration or pregap"));
            }
            let gap_in_file = if stored_gap { pregap } else { 0 };
            let first = tracks.is_empty();
            let index0 = if first {
                150u32.saturating_sub(pregap)
            } else {
                cursor
            };
            let stored_start = if first {
                150 - gap_in_file.min(150)
            } else {
                cursor + if stored_gap { 0 } else { pregap }
            };
            let start = if first { 150 } else { cursor + pregap };
            let end = start + frames - gap_in_file + postgap;
            if end > MAX_FRAMES + 150 {
                return Err(error("disc duration exceeds 450000 frames"));
            }
            let skip = if first {
                gap_in_file.saturating_sub(150)
            } else {
                0
            };
            let stride = if spec.kind == Kind::Mode1 {
                SECTOR
            } else {
                RAW_SECTOR
            };
            let mode = match values[1] {
                "AUDIO" => TrackMode::Audio,
                "MODE2_RAW" => TrackMode::Mode2,
                _ => TrackMode::Mode1,
            };
            tracks.push(Track {
                number: tracks.len() as u8 + 1,
                mode,
                control: if mode == TrackMode::Audio { 0 } else { 4 },
                start,
                index0,
                end,
                indices: vec![(1, start)],
                bytes: Arc::new(Vec::new()),
                offset: (chd_frame + skip as usize) * stride,
                byte_end: (chd_frame + frames as usize) * stride,
                stride,
                stored_start,
                mode2_format: Mode2Format::Data,
                chd_layout: Some(spec),
            });
            chd_frame += (frames as usize).div_ceil(4) * 4;
            cursor = end;
            meta = number(m, 8, 8);
        }
        if tracks.is_empty() || chd_frame as u64 * UNIT as u64 != logical {
            return Err(error(
                "missing CD metadata or track lengths disagree with logical size",
            ));
        }
        let bytes = Arc::new(bytes);
        let image = Chd::open(Cursor::new(Bytes(Arc::clone(&bytes))), None).map_err(error)?;
        // Flatten self references iteratively: no attacker-controlled recursion.
        let mut references = Vec::with_capacity(count);
        let mut concrete = vec![true; count];
        for (i, entry) in image.map().iter().enumerate() {
            let (offset, size, codec) = match entry {
                MapEntry::V5Compressed(e) => match e.hunk_type().map_err(error)? {
                    CompressionTypeV5::CompressionSelf => {
                        let target = usize::try_from(e.block_offset()).map_err(error)?;
                        if target >= count {
                            return Err(error("self hunk outside map"));
                        }
                        concrete[i] = false;
                        references.push(target as u32);
                        continue;
                    }
                    CompressionTypeV5::CompressionNone => (e.block_offset(), e.block_size(), None),
                    CompressionTypeV5::CompressionType0 => {
                        (e.block_offset(), e.block_size(), Some(0))
                    }
                    CompressionTypeV5::CompressionType1 => {
                        (e.block_offset(), e.block_size(), Some(1))
                    }
                    CompressionTypeV5::CompressionType2 => {
                        (e.block_offset(), e.block_size(), Some(2))
                    }
                    CompressionTypeV5::CompressionType3 => {
                        (e.block_offset(), e.block_size(), Some(3))
                    }
                    _ => return Err(error("parent-backed or unsupported hunk type")),
                },
                MapEntry::V5Uncompressed(e) => (e.block_offset(), e.block_size(), None),
                _ => return Err(error("unsupported map version")),
            };
            if size == 0 || size as usize > hunk {
                return Err(error("compressed hunk size exceeds resource bounds"));
            }
            if offset != 0 {
                let input = range(&bytes, offset, size as usize)?;
                if let Some(codec) = codec {
                    let header = &bytes[16 + codec * 4..20 + codec * 4];
                    if header == b"cdfl" {
                        check_flac(input, hunk)?;
                    } else {
                        if header != b"cdzl" && header != b"cdlz" {
                            return Err(error("hunk selects an unused codec"));
                        }
                        let ecc = (hunk / UNIT).div_ceil(8);
                        let length_bytes = if hunk < 65536 { 2 } else { 3 };
                        if input.len() < ecc + length_bytes
                            || number(input, ecc, length_bytes) as usize
                                > input.len() - ecc - length_bytes
                        {
                            return Err(error("invalid CD codec block lengths"));
                        }
                    }
                }
            } else if compressed {
                return Err(error("hunk points into header"));
            }
            references.push(i as u32);
        }
        let mut targets = vec![u32::MAX; count];
        for i in 0..count {
            let mut path = Vec::new();
            let mut at = i as u32;
            while targets[at as usize] == u32::MAX {
                targets[at as usize] = u32::MAX - 1;
                path.push(at);
                let next = references[at as usize];
                if concrete[at as usize] {
                    break;
                }
                at = next;
            }
            if targets[at as usize] == u32::MAX - 1 && !concrete[at as usize] {
                return Err(error("cyclic self hunk references"));
            }
            let target = if concrete[at as usize] {
                at
            } else {
                targets[at as usize]
            };
            for p in path {
                targets[p as usize] = target;
            }
        }
        let mut source = Source {
            image,
            targets,
            compressed: Vec::new(),
            cache: crate::disc::buffer(hunk)?,
            cached: None,
        };
        // Validate each logical hunk/CRC and raw SHA-1 before changing a
        // machine. Reuse one buffer; never retain expanded media.
        let mut digest = Sha1::new();
        for i in 0..count {
            source.load(source.targets[i])?;
            let length = (logical as usize - i * hunk).min(hunk);
            digest.update(&source.cache[..length]);
        }
        let expected = &bytes[64..84];
        if expected.iter().any(|b| *b != 0) && &digest.finalize()[..] != expected {
            return Err(error("raw SHA-1 mismatch (corrupt disc bytes)"));
        }
        for t in &mut tracks {
            if t.mode == TrackMode::Mode2 {
                let first = t.offset / t.stride + (t.start - t.stored_start) as usize;
                let end = (t.byte_end / t.stride).min(first + 75);
                for frame in first..end {
                    let s = source.frame(frame as u32)?;
                    if s[15] == 2 && (0..4).filter(|i| s[16 + i] == s[20 + i]).count() >= 3 {
                        t.mode2_format = Mode2Format::Xa;
                        break;
                    }
                }
            }
        }
        Ok(Media {
            tracks,
            leadout: cursor,
            rw: None,
            chd: Some(Arc::new(Mutex::new(source))),
        })
    }
}
