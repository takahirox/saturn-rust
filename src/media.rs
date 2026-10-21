//! Owned CD media, independent of bootstrap extraction (ST-040 §§3–5).
//! CUE references are resolved only against caller-supplied bytes, never the
//! filesystem or network. Track times are binary 75 Hz frames; FAD starts at 150.
mod integrity;
use crate::disc::{DiscError, SECTOR};
use std::sync::Arc;

pub const RAW_SECTOR: usize = 2352;
const MAX_FRAMES: u32 = 100 * 60 * 75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackMode {
    Audio,
    Mode1,
    Mode2,
}
/// A CUE MODE2/2352 label does not distinguish ECMA-130 Mode 2 user
/// data from CD-ROM XA. Auto recognizes the repeated XA subheader (including
/// one damaged field). A caller with explicit format metadata can override
/// this inherently ambiguous byte-level detection without rewriting sectors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode2Format {
    #[default]
    Auto,
    Data,
    Xa,
}
#[derive(Clone)]
pub struct Track {
    pub number: u8,
    pub mode: TrackMode,
    /// Red Book CONTROL nibble: data, copy permission, pre-emphasis, channels.
    pub control: u8,
    pub start: u32,
    pub index0: u32,
    pub end: u32,
    indices: Vec<(u8, u32)>,
    bytes: Arc<[u8]>,
    offset: usize,
    byte_end: usize,
    stride: usize,
    stored_start: u32,
    mode2_format: Mode2Format,
}
impl Track {
    pub fn control_adr(&self) -> u8 {
        (self.control << 4) | 1
    }
    /// Disc indices are binary index numbers, distinct from track numbers and
    /// absolute FAD. INDEX 00 covers the entire stored/synthesized pregap.
    pub fn index(&self, number: u8) -> Option<u32> {
        if number == 0 {
            Some(self.index0)
        } else {
            self.indices
                .iter()
                .find(|(n, _)| *n == number)
                .map(|(_, fad)| *fad)
        }
    }
    pub fn index_at(&self, fad: u32) -> u8 {
        self.indices
            .iter()
            .rev()
            .find(|(_, start)| fad >= *start)
            .map_or(0, |(number, _)| *number)
    }
}
#[derive(Clone)]
pub struct Media {
    pub(crate) tracks: Vec<Track>,
    pub(crate) leadout: u32,
}
fn invalid(message: &str) -> DiscError {
    DiscError(message.into())
}
fn time(text: &str) -> Result<u32, DiscError> {
    let fields: Vec<_> = text.split(':').collect();
    if fields.len() != 3 {
        return Err(invalid("CUE time must be MM:SS:FF"));
    }
    let mut n = [0; 3];
    for (out, field) in n.iter_mut().zip(fields) {
        *out = field
            .parse::<u32>()
            .map_err(|_| invalid("Invalid CUE time"))?;
    }
    if n[1] >= 60 || n[2] >= 75 || n[0] >= 100 {
        return Err(invalid("CUE time outside disc bounds"));
    }
    Ok((n[0] * 60 + n[1]) * 75 + n[2])
}
struct CueTrack {
    flags: Option<u8>,
    file: usize,
    mode: TrackMode,
    stride: usize,
    index0: Option<u32>,
    index1: Option<u32>,
    indices: Vec<(u8, u32)>,
    pregap: u32,
    postgap: u32,
}
impl Media {
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }
    pub fn leadout(&self) -> u32 {
        self.leadout
    }

    pub fn iso(bytes: &[u8]) -> Result<Self, DiscError> {
        if bytes.is_empty()
            || !bytes.len().is_multiple_of(SECTOR)
            || bytes.len() / SECTOR > MAX_FRAMES as usize
        {
            return Err(invalid(
                "ISO must contain whole 2048-byte sectors within disc bounds",
            ));
        }
        let end = 150 + (bytes.len() / SECTOR) as u32;
        Ok(Self {
            tracks: vec![Track {
                number: 1,
                mode: TrackMode::Mode1,
                control: 4,
                start: 150,
                index0: 150,
                end,
                indices: vec![(1, 150)],
                bytes: Arc::from(bytes),
                offset: 0,
                byte_end: bytes.len(),
                stride: SECTOR,
                stored_start: 150,
                mode2_format: Mode2Format::Data,
            }],
            leadout: end,
        })
    }
    pub fn cue(cue: &str, files: &[(&str, &[u8])]) -> Result<Self, DiscError> {
        Self::cue_with_mode2_format(cue, files, Mode2Format::Auto)
    }
    pub fn cue_with_mode2_format(
        cue: &str,
        files: &[(&str, &[u8])],
        mode2_format: Mode2Format,
    ) -> Result<Self, DiscError> {
        if cue.len() > 1024 * 1024
            || files.len() > 99
            || files
                .iter()
                .try_fold(0usize, |n, (_, b)| n.checked_add(b.len()))
                .is_none_or(|n| n > 512 * 1024 * 1024)
        {
            return Err(invalid("CUE exceeds media limits"));
        }
        let mut tracks: Vec<CueTrack> = Vec::new();
        let mut file = None;
        for line in cue.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            let Some((op, rest)) = line.split_once(char::is_whitespace) else {
                if !line.is_empty() {
                    return Err(invalid("Malformed CUE directive"));
                }
                continue;
            };
            let rest = rest.trim();
            match op.to_ascii_uppercase().as_str() {
                "REM" | "TITLE" | "PERFORMER" | "SONGWRITER" | "CATALOG" | "ISRC" => (),
                "FILE" => {
                    let (name, kind) = if let Some(rest) = rest.strip_prefix('"') {
                        rest.split_once('"')
                            .ok_or_else(|| invalid("Unterminated CUE filename"))?
                    } else {
                        rest.split_once(char::is_whitespace)
                            .ok_or_else(|| invalid("Missing CUE file type"))?
                    };
                    if !kind.trim().eq_ignore_ascii_case("BINARY") {
                        return Err(invalid("CUE FILE requires BINARY bytes"));
                    }
                    let matches: Vec<_> = files
                        .iter()
                        .enumerate()
                        .filter(|(_, (n, _))| *n == name)
                        .collect();
                    if matches.len() != 1 {
                        return Err(invalid("Missing or ambiguous CUE file reference"));
                    }
                    file = Some(matches[0].0);
                }
                "TRACK" => {
                    let fields: Vec<_> = rest.split_whitespace().collect();
                    if fields.len() != 2
                        || tracks.len() >= 99
                        || fields[0].parse::<usize>().ok() != Some(tracks.len() + 1)
                    {
                        return Err(invalid("CUE tracks must be consecutive 01–99"));
                    }
                    let (mode, stride) = match fields[1].to_ascii_uppercase().as_str() {
                        "AUDIO" => (TrackMode::Audio, RAW_SECTOR),
                        "MODE1/2048" => (TrackMode::Mode1, SECTOR),
                        "MODE1/2352" => (TrackMode::Mode1, RAW_SECTOR),
                        "MODE2/2352" => (TrackMode::Mode2, RAW_SECTOR),
                        _ => return Err(invalid("Unsupported CUE track format")),
                    };
                    tracks.push(CueTrack {
                        flags: None,
                        file: file.ok_or_else(|| invalid("CUE TRACK before FILE"))?,
                        mode,
                        stride,
                        index0: None,
                        index1: None,
                        indices: Vec::new(),
                        pregap: 0,
                        postgap: 0,
                    });
                }
                "INDEX" => {
                    let t = tracks
                        .last_mut()
                        .ok_or_else(|| invalid("CUE INDEX before TRACK"))?;
                    let fields: Vec<_> = rest.split_whitespace().collect();
                    if fields.len() != 2 {
                        return Err(invalid("Malformed CUE INDEX"));
                    }
                    let n = time(fields[1])?;
                    let number = fields[0]
                        .parse::<u8>()
                        .ok()
                        .filter(|i| {
                            *i <= 99
                                && fields[0].len() == 2
                                && fields[0].bytes().all(|b| b.is_ascii_digit())
                        })
                        .ok_or_else(|| invalid("Invalid CUE index number"))?;
                    match number {
                        0 if t.index0.is_none() && t.index1.is_none() => t.index0 = Some(n),
                        1 if t.index1.is_none() && t.index0.is_none_or(|i| i <= n) => {
                            t.index1 = Some(n)
                        }
                        2..=99
                            if t.index1.is_some_and(|first| n > first)
                                && t.indices.last().is_none_or(|(previous, time)| {
                                    number > *previous && n > *time
                                }) =>
                        {
                            t.indices.push((number, n));
                        }
                        _ => {
                            return Err(invalid(
                                "CUE requires increasing index numbers and times after INDEX 01",
                            ));
                        }
                    }
                }
                "PREGAP" | "POSTGAP" => {
                    let t = tracks
                        .last_mut()
                        .ok_or_else(|| invalid("CUE gap before TRACK"))?;
                    let field = if op.eq_ignore_ascii_case("PREGAP") {
                        &mut t.pregap
                    } else {
                        &mut t.postgap
                    };
                    if *field != 0 {
                        return Err(invalid("Duplicate CUE gap"));
                    }
                    *field = time(rest)?;
                }
                "FLAGS" => {
                    let track = tracks
                        .last_mut()
                        .ok_or_else(|| invalid("CUE FLAGS before TRACK"))?;
                    if track.flags.is_some() || rest.is_empty() {
                        return Err(invalid("Duplicate or empty CUE FLAGS"));
                    }
                    let mut flags = 0;
                    for flag in rest.split_whitespace() {
                        let bit = match flag.to_ascii_uppercase().as_str() {
                            "PRE" if track.mode == TrackMode::Audio => 1,
                            "DCP" => 2,
                            "4CH" if track.mode == TrackMode::Audio => 8,
                            _ => return Err(invalid("Unsupported CUE track flag")),
                        };
                        if flags & bit != 0 {
                            return Err(invalid("Duplicate CUE track flag"));
                        }
                        flags |= bit;
                    }
                    track.flags = Some(flags);
                }
                _ => return Err(invalid("Unsupported CUE directive")),
            }
        }
        if tracks.is_empty() {
            return Err(invalid("CUE has no tracks"));
        }
        let owned: Vec<Arc<[u8]>> = files.iter().map(|(_, b)| Arc::from(*b)).collect();
        let mut result = Vec::new();
        let mut cursor: u32 = 150;
        for (i, t) in tracks.iter().enumerate() {
            let index = t
                .index1
                .ok_or_else(|| invalid("CUE track missing INDEX 01"))?;
            let begin = t.index0.unwrap_or(index);
            let data = &owned[t.file];
            if !data.len().is_multiple_of(t.stride) || data.len() / t.stride > MAX_FRAMES as usize {
                return Err(invalid(
                    "BIN contains truncated sectors or exceeds disc bounds",
                ));
            }
            if tracks
                .iter()
                .any(|other| other.file == t.file && other.stride != t.stride)
            {
                return Err(invalid("Inconsistent sector sizes in shared BIN"));
            }
            let end = if let Some(next) = tracks.get(i + 1).filter(|n| n.file == t.file) {
                next.index0
                    .or(next.index1)
                    .ok_or_else(|| invalid("Missing next CUE index"))?
            } else {
                (data.len() / t.stride) as u32
            };
            if begin > index
                || index >= end
                || end as usize > data.len() / t.stride
                || t.indices.iter().any(|(_, position)| *position >= end)
            {
                return Err(invalid("CUE indices overlap or exceed file"));
            }
            // First-track pregap precedes FAD150. Subsequent gaps occupy disc time.
            let index0 = if i == 0 {
                150u32.saturating_sub(index - begin + t.pregap)
            } else {
                cursor
            };
            let stored_start = if i == 0 {
                150 - (index - begin).min(150)
            } else {
                cursor + t.pregap
            };
            let start = if i == 0 {
                150
            } else {
                stored_start + index - begin
            };
            let track_end = start + end - index + t.postgap;
            if track_end > MAX_FRAMES {
                return Err(invalid("CUE exceeds disc duration"));
            }
            result.push(Track {
                number: (i + 1) as u8,
                mode: t.mode,
                control: t.flags.unwrap_or(0) | if t.mode == TrackMode::Audio { 0 } else { 4 },
                start,
                index0,
                end: track_end,
                indices: std::iter::once((1, start))
                    .chain(
                        t.indices
                            .iter()
                            .map(|(number, position)| (*number, start + position - index)),
                    )
                    .collect(),
                bytes: owned[t.file].clone(),
                offset: (if i == 0 {
                    index - (index - begin).min(150)
                } else {
                    begin
                }) as usize
                    * t.stride,
                byte_end: end as usize * t.stride,
                stride: t.stride,
                stored_start,
                mode2_format: if t.mode == TrackMode::Mode2 && mode2_format == Mode2Format::Auto {
                    // Inspect at most one second of stored sectors. Use the
                    // track's source layout, never the result of ECC recovery:
                    // bad XA CRC/parity must not turn into unchecked data.
                    let xa = data[index as usize * t.stride..end as usize * t.stride]
                        .chunks_exact(t.stride)
                        .take(75)
                        .any(|s| {
                            s[15] == 2 && (0..4).filter(|i| s[16 + i] == s[20 + i]).count() >= 3
                        });
                    if xa {
                        Mode2Format::Xa
                    } else {
                        Mode2Format::Data
                    }
                } else {
                    mode2_format
                },
            });
            cursor = track_end;
        }
        Ok(Self {
            tracks: result,
            leadout: cursor,
        })
    }
    pub fn track(&self, fad: u32) -> Option<&Track> {
        self.tracks
            .iter()
            .rev()
            .find(|t| (t.index0..t.end).contains(&fad))
    }
    pub fn raw_sector(&self, fad: u32) -> Result<[u8; RAW_SECTOR], DiscError> {
        let mut sector = self.received_sector(fad)?;
        let t = self.track(fad).unwrap();
        if t.stride == RAW_SECTOR && self.has_stored_sector(t, fad) {
            if t.mode == TrackMode::Mode2 && t.mode2_format == Mode2Format::Data {
                integrity::mode2_data(&sector)?;
            } else {
                integrity::validate(&mut sector, t.mode)?;
            }
        }
        Ok(sector)
    }
    /// Runtime CD decode differs from strict boot/parser reads: the guest
    /// controls correction effort and may request even erroneous sector bytes.
    pub(crate) fn decode_sector(
        &self,
        fad: u32,
        passes: u8,
        confirm_subheader: bool,
    ) -> Result<([u8; RAW_SECTOR], bool), DiscError> {
        let mut sector = self.received_sector(fad)?;
        let t = self.track(fad).unwrap();
        let valid = t.stride != RAW_SECTOR
            || !self.has_stored_sector(t, fad)
            || if t.mode == TrackMode::Mode2 && t.mode2_format == Mode2Format::Data {
                integrity::mode2_data(&sector).is_ok()
            } else {
                integrity::decode(&mut sector, t.mode, passes, confirm_subheader).is_ok()
            };
        Ok((sector, valid))
    }
    fn has_stored_sector(&self, track: &Track, fad: u32) -> bool {
        fad >= track.stored_start
            && track.offset + (fad - track.stored_start) as usize * track.stride + track.stride
                <= track.byte_end
    }
    fn received_sector(&self, fad: u32) -> Result<[u8; RAW_SECTOR], DiscError> {
        let t = self
            .track(fad)
            .ok_or_else(|| invalid("Sector outside disc"))?;
        let mut sector = [0; RAW_SECTOR];
        if fad < t.stored_start {
            return Ok(sector);
        }
        let offset = t.offset + (fad - t.stored_start) as usize * t.stride;
        let Some(bytes) = t
            .bytes
            .get(..t.byte_end)
            .and_then(|b| b.get(offset..offset + t.stride))
        else {
            return Ok(sector);
        };
        if t.stride == RAW_SECTOR {
            sector.copy_from_slice(bytes);
        } else {
            sector[1..11].fill(255);
            let bcd = |v: u32| (v / 10 * 16 + v % 10) as u8;
            sector[12..16].copy_from_slice(&[
                bcd(fad / 4500),
                bcd(fad / 75 % 60),
                bcd(fad % 75),
                1,
            ]);
            sector[16..2064].copy_from_slice(bytes);
            mode1_checksums(&mut sector);
        }
        Ok(sector)
    }
    pub fn cooked_sector(&self, fad: u32) -> Result<Vec<u8>, DiscError> {
        let raw = self.raw_sector(fad)?;
        match self.track(fad).unwrap().mode {
            TrackMode::Mode1 => Ok(raw[16..2064].to_vec()),
            TrackMode::Mode2 => {
                if self.track(fad).unwrap().mode2_format == Mode2Format::Data {
                    // ECMA-130 §14/fig.12: all 2336 bytes following the
                    // header are user data, with no XA subheader or EDC/ECC.
                    return Ok(raw[16..].to_vec());
                }
                if raw[16..20] != raw[20..24] {
                    return Err(invalid("Mismatched Mode2 subheaders"));
                }
                Ok(raw[24..if raw[18] & 0x20 != 0 { 2348 } else { 2072 }].to_vec())
            }
            TrackMode::Audio => Err(invalid("Audio track has no cooked sector")),
        }
    }
    pub fn boot_iso(&self) -> Result<Vec<u8>, DiscError> {
        let t = self
            .tracks
            .first()
            .ok_or_else(|| invalid("Disc has no data track"))?;
        if t.mode == TrackMode::Audio {
            return Err(invalid("First track is audio"));
        }
        if t.end - t.start > (crate::disc::MAX_DISC_SIZE / SECTOR) as u32 {
            return Err(invalid("Boot data exceeds loader limit"));
        }
        let mut image = Vec::new();
        for fad in t.start
            ..t.start + ((t.byte_end - t.offset) / t.stride) as u32 - (t.start - t.stored_start)
        {
            let data = self.cooked_sector(fad)?;
            if data.len() != SECTOR {
                return Err(invalid("Bootstrap requires 2048-byte logical sectors"));
            }
            image.extend(data);
        }
        Ok(image)
    }
    pub fn toc(&self) -> Vec<u8> {
        let mut table = vec![255; 408];
        for t in &self.tracks {
            let control = t.control_adr();
            let value = u32::from(control) << 24 | t.start;
            table[(t.number as usize - 1) * 4..t.number as usize * 4]
                .copy_from_slice(&value.to_be_bytes());
        }
        table[396..400].copy_from_slice(&[self.tracks[0].control_adr(), 1, 0, 0]);
        table[400..404].copy_from_slice(&[
            self.tracks.last().unwrap().control_adr(),
            self.tracks.len() as u8,
            0,
            0,
        ]);
        table[404..408].copy_from_slice(&(0x0100_0000 | self.leadout).to_be_bytes());
        table
    }
}
// ECMA-130 §14.3/annex A; cooked images acquire real raw-sector parity.
fn mode1_checksums(sector: &mut [u8; RAW_SECTOR]) {
    let crc = integrity::edc(&sector[..2064]);
    sector[2064..2068].copy_from_slice(&crc.to_le_bytes());
    integrity::encode(sector);
}
