//! Runtime CD block: register protocol, drive, stream selectors and sector pool.
//! Primary reference: ST-162-062094, CD Communication Interface §§3–8
//! (not ST-162-R1, which describes the peripheral library).
use crate::bus::BusError;
use crate::media::{Media, RAW_SECTOR, TrackMode};
use std::cell::{Cell, RefCell};

pub const REGISTERS: u32 = 0x0589_0000;
pub const DATA: u32 = 0x0581_8000;
const HZ: u64 = 26_874_100;
const CMOK: u16 = 1;
const DRDY: u16 = 2;
const CSCT: u16 = 4;
const BFUL: u16 = 8;
const PEND: u16 = 16;
const ESEL: u16 = 64;
const EHST: u16 = 128;
const ECPY: u16 = 256;
const EFLS: u16 = 512;
const SCDQ: u16 = 1024;
const POOL: usize = 200;
const SELECTORS: usize = 24;
// Functional selector engine bandwidth: one complete raw sector per 2352
// reference clocks. ST-162 specifies asynchronous completion, not a physical
// copy-clock count; retain this explicit deterministic timing policy.
const COPY_CLOCKS: u32 = RAW_SECTOR as u32;

#[derive(Clone)]
struct Filter {
    start: u32,
    count: u32,
    subheader: [u8; 4],
    mask: [u8; 2],
    mode: u8,
    yes: u8,
    no: u8,
}
impl Filter {
    fn reset(index: usize) -> Self {
        Self {
            start: 0,
            count: 0,
            subheader: [0; 4],
            mask: [0; 2],
            mode: 0,
            yes: index as u8,
            no: 255,
        }
    }
    fn accepts(&self, fad: u32, sub: [u8; 4]) -> bool {
        let checks = [
            sub[0] == self.subheader[0],
            sub[1] == self.subheader[1],
            (sub[2] & self.mask[0]) == self.subheader[2],
            (sub[3] & self.mask[1]) == self.subheader[3],
        ];
        let subheader = checks
            .into_iter()
            .enumerate()
            .all(|(i, v)| self.mode & (1 << i) == 0 || v);
        let range = self.mode & 0x40 == 0
            || (fad >= self.start && fad < self.start.saturating_add(self.count));
        range && (subheader ^ (self.mode & 0x10 != 0))
    }
}
#[derive(Clone)]
struct Sector {
    id: u64,
    fad: u32,
    raw: [u8; RAW_SECTOR],
}
struct Transfer {
    bytes: Vec<u8>,
    position: usize,
    put: Option<(usize, usize, usize)>, // filter, count, retained sector length
    delete: Option<(usize, Vec<u64>)>,
    // ST-162 §3.2 table3.2: EHST belongs to sector-buffer I/O, not
    // TOC, subcode or file-information transfers of a coincident byte count.
    host_io: bool,
}

struct SelectorTransfer {
    source: usize,
    sectors: std::collections::VecDeque<u64>,
    destination: u8,
    moving: bool,
    clocks: u32,
}

pub struct CdBlock {
    media: Option<Media>,
    request: [u16; 4],
    response: Cell<[u16; 4]>,
    hirq: Cell<u16>,
    mask: u16,
    response_latched: Cell<bool>,
    pending: Option<([u16; 4], u32)>,
    status: u8,
    fad: u32,
    // Seek Home has no public position. Keep its origin separate from the
    // retained pickup/range/repeat state (ST-162 §4.2, §7.2.4(5)).
    at_home: bool,
    end: u32,
    play_start: u32,
    repeat: u8,
    repeats: u8,
    speed: u32,
    phase: u64,
    report_phase: u64,
    pause_clocks: u64,
    standby_seconds: u16,
    init_flags: u8,
    ecc: u8,
    retry: u8,
    retry_attempts: u8,
    // Scan speed is not specified by ST-162; advance ten frames per 75 Hz
    // drive tick. Preserve direction and whether scan began during playback.
    scan: Option<(bool, bool)>,
    buffer_paused: bool,
    tray_open: bool,
    seeking: u32,
    playing: bool,
    enabled: bool,
    audio_index: usize,
    deemphasis: crate::cd_audio::Deemphasis,
    connection: u8,
    last: u8,
    filters: [Filter; SELECTORS],
    partitions: [Vec<Sector>; SELECTORS],
    transfer: RefCell<Option<Transfer>>,
    get_length: usize,
    put_length: usize,
    actual_size: u32,
    search: [u16; 3],
    copy_error: u16,
    copying: Option<SelectorTransfer>,
    next_sector_id: u64,
    directory: Option<File>,
    volume_end: u32,
    files: Vec<File>,
    file_read: Option<FileRead>,
    directory_read: Option<DirectoryRead>,
}
// One reserved pool sector is reused at each drive deadline. Only the bounded
// 256-entry public table and a parsed primary record survive a sector read.
struct DirectoryRead {
    selector: usize,
    extent: Option<File>,
    volume_end: u32,
    primary: Option<(File, u32)>,
    next: u32,
    first: u32,
    fid: u32,
    files: Vec<File>,
}
struct FileRead {
    file: File,
    logical_sector: u32,
    selector: u8,
}
#[derive(Clone)]
struct File {
    fad: u32,
    size: u32,
    flags: u8,
    id: u32,
    unit: u8,
    gap: u8,
    number: u8,
    is_directory: bool,
}
impl File {
    // ISO9660 §§6.4.3/9.1.7–9.1.8: file size excludes interleave gaps.
    fn sector(&self, logical: u32) -> Option<u32> {
        let offset = if self.unit == 0 {
            logical
        } else {
            let unit = u32::from(self.unit);
            (logical / unit)
                .checked_mul(unit + u32::from(self.gap))?
                .checked_add(logical % unit)?
        };
        self.fad.checked_add(offset)
    }
    // ISO9660 §9.1 / ST-162 §6.2: both-endian numbers must agree. Validate
    // before converting LBA to FAD or publishing any new directory state.
    fn from_record(record: &[u8], media: &Media, limit: u32, id: u32) -> Option<Self> {
        if record.len() < 34
            || !record.len().is_multiple_of(2)
            || usize::from(record[0]) != record.len()
        {
            return None;
        }
        let name_length = usize::from(record[32]);
        let minimum = 33 + name_length + usize::from(name_length.is_multiple_of(2));
        if name_length == 0
            || minimum > record.len()
            || (record[26] == 0 && record[27] != 0)
            || record[25] & !3 != 0
            || record[28..32] != [1, 0, 0, 1]
        {
            return None;
        }
        let dual = |offset| {
            let little = u32::from_le_bytes(record[offset..offset + 4].try_into().ok()?);
            let big = u32::from_be_bytes(record[offset + 4..offset + 8].try_into().ok()?);
            (little == big).then_some(little)
        };
        let fad = dual(2)?
            .checked_add(150)?
            .checked_add(u32::from(record[1]))?;
        let size = dual(10)?;
        // CD-ROM XA system use is a 14-byte record after the padded name.
        // Its high attribute byte supplies public bits 3–7; the ISO directory
        // bit is retained for records without XA information (ST-162 §6.8).
        let xa = record
            .get(minimum..minimum + 14)
            .filter(|s| &s[6..8] == b"XA");
        let is_directory = xa.map_or(record[25] & 2 != 0, |s| s[4] & 0x80 != 0);
        if is_directory && record[26..28] != [0, 0] {
            return None;
        }
        let file = Self {
            fad,
            size,
            id,
            flags: record[25] & 2 | xa.map_or(0, |s| s[4] & 0xF8),
            unit: record[26],
            gap: record[27],
            number: xa.map_or(0, |s| s[8]),
            is_directory,
        };
        let end = if size == 0 {
            fad
        } else {
            file.sector(size.div_ceil(2048) - 1)?.checked_add(1)?
        };
        if fad > limit || end > limit || end > media.leadout {
            return None;
        }
        if size != 0 {
            let track = media.track(fad)?;
            if track.mode == TrackMode::Audio || fad < track.start || end > track.end {
                return None;
            }
        }
        Some(file)
    }
}
impl Default for CdBlock {
    fn default() -> Self {
        Self {
            media: None,
            request: [0; 4],
            response: Cell::new([0x0043, 0x4442, 0x4C4F, 0x434B]),
            hirq: Cell::new(0xBE1),
            mask: 0,
            response_latched: Cell::new(false),
            pending: None,
            status: 7,
            fad: 150,
            at_home: false,
            end: 150,
            play_start: 150,
            repeat: 0,
            repeats: 0,
            speed: 2,
            phase: 0,
            report_phase: 0,
            pause_clocks: 0,
            standby_seconds: 180,
            init_flags: 0,
            ecc: 0,
            retry: 0,
            retry_attempts: 0,
            scan: None,
            buffer_paused: false,
            tray_open: false,
            seeking: 0,
            playing: false,
            enabled: true,
            audio_index: 0,
            deemphasis: crate::cd_audio::Deemphasis::default(),
            connection: 255,
            last: 255,
            filters: std::array::from_fn(Filter::reset),
            partitions: std::array::from_fn(|_| Vec::new()),
            transfer: RefCell::new(None),
            get_length: 2048,
            put_length: 2048,
            actual_size: 0,
            search: [0; 3],
            copy_error: 0,
            copying: None,
            next_sector_id: 0,
            directory: None,
            volume_end: 0,
            files: Vec::new(),
            file_read: None,
            directory_read: None,
        }
    }
}
impl CdBlock {
    pub fn mount(&mut self, media: Media) {
        let cold = self.media.is_none();
        self.directory_read = None;
        self.pending = None;
        self.copying = None;
        self.copy_error = 0;
        self.retry_attempts = 0;
        self.tray_open = false;
        self.files.clear();
        self.directory = None;
        self.volume_end = 0;
        self.file_read = None;
        *self.transfer.get_mut() = None;
        self.playing = false;
        self.seeking = 0;
        self.audio_index = 0;
        self.deemphasis = crate::cd_audio::Deemphasis::default();
        self.scan = None;
        self.buffer_paused = false;
        self.pause_clocks = 0;
        self.fad = media.tracks[0].start;
        self.at_home = false;
        // ST-162 §5.5: tray changes retain host play information. A fresh
        // block still needs the initial disc range; power reset initializes
        // its retained medium's range separately.
        if cold {
            self.play_start = self.fad;
            self.end = media.leadout;
        }
        self.media = Some(media);
        self.status = 1;
        self.report(false);
        self.raise(0x20);
    }
    pub fn media(&self) -> Option<&Media> {
        self.media.as_ref()
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled && !enabled {
            let media = self.media.take();
            *self = Self {
                end: media.as_ref().map_or(150, |m| m.leadout),
                media,
                enabled: false,
                ..Self::default()
            };
        } else if !self.enabled && enabled {
            self.enabled = true;
            self.status = if self.media.is_some() { 1 } else { 7 };
            self.report(false);
        }
    }
    fn error(address: u32, width: usize) -> BusError {
        BusError::UnsupportedDevice { address, width }
    }
    fn raise(&self, bits: u16) {
        self.hirq.set(self.hirq.get() | bits);
    }
    pub fn interrupt(&self) -> bool {
        self.enabled && self.hirq.get() & self.mask != 0
    }
    pub fn active(&self) -> bool {
        self.enabled
            && (self.playing
                || self.scan.is_some()
                || self.pending.is_some()
                || self.seeking != 0
                || self.copying.is_some())
    }
    pub fn clocks_to_event(&self) -> u32 {
        if !self.enabled {
            return u32::MAX;
        }
        let drive = (HZ - self.phase).div_ceil(75 * u64::from(self.drive_speed())) as u32;
        let report = (HZ - self.report_phase).div_ceil(self.report_hz()) as u32;
        let standby = if self.status == 1 && !self.playing {
            (u64::from(self.standby_seconds) * HZ - self.pause_clocks).min(u64::from(u32::MAX))
                as u32
        } else {
            u32::MAX
        };
        drive
            .min(report)
            .min(standby)
            .min(self.pending.map_or(u32::MAX, |(_, n)| n))
            .min(self.copying.as_ref().map_or(u32::MAX, |job| job.clocks))
            .min(if self.seeking == 0 {
                u32::MAX
            } else {
                self.seeking
            })
    }
    fn drive_speed(&self) -> u32 {
        if self.scan.is_some()
            || self
                .media
                .as_ref()
                .and_then(|m| m.track(self.fad))
                .is_some_and(|t| t.mode == TrackMode::Audio)
        {
            1
        } else {
            self.speed
        }
    }
    fn report_hz(&self) -> u64 {
        // ST-162 §3.4: 75/150 Hz during play, 60 Hz while stopped/paused.
        if self.playing || self.scan.is_some() {
            75 * u64::from(self.drive_speed())
        } else {
            60
        }
    }
    fn report(&self, periodic: bool) {
        if periodic && self.response_latched.get() {
            return;
        }
        // Invalid public bytes must never replace retained recovery state.
        // ST-162 §7.2.4(4), later manual §7.2.5(4).
        if self.invalid_report() {
            self.response.set([
                u16::from(self.status | if periodic { 0x20 } else { 0 }) << 8 | 0xFF,
                0xFFFF,
                0xFFFF,
                0xFFFF,
            ]);
            return;
        }
        let (ctrl, track, index, _) = self.track_report().unwrap_or((255, 255, 255, 0));
        let flags = if ctrl != 255 && ctrl & 0x40 != 0 && self.status != 4 && self.scan.is_none() {
            0x80
        } else {
            0
        };
        self.response.set([
            u16::from(self.status | if periodic { 0x20 } else { 0 }) << 8
                | flags
                | u16::from(self.repeats),
            u16::from(ctrl) << 8 | u16::from(track),
            u16::from(index) << 8 | ((self.fad >> 16) as u16 & 255),
            self.fad as u16,
        ]);
    }
    fn invalid_report(&self) -> bool {
        matches!(self.status, 6 | 7 | 9) || (self.status == 2 && self.at_home)
    }
    fn track_report(&self) -> Option<(u8, u8, u8, u32)> {
        let media = self.media.as_ref().filter(|_| !self.tray_open)?;
        if self.fad == media.leadout {
            // §4.2: completion at disc end reports the lead-out area.
            // §7.2.5: TNO=AA, relative time starts at the lead-out boundary.
            return Some((media.tracks.last()?.control_adr(), 0xAA, 1, 0));
        }
        let track = media.track(self.fad)?;
        Some((
            track.control_adr(),
            track.number,
            track.index_at(self.fad),
            self.fad.abs_diff(track.start),
        ))
    }
    fn pause_position(&self) -> u32 {
        if self.at_home { 150 } else { self.fad }
    }
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        let value = self.peek16(address)?;
        if address == DATA || address == DATA + 2 {
            if let Some(t) = self
                .transfer
                .borrow_mut()
                .as_mut()
                .filter(|t| t.put.is_none())
                && t.position + 2 <= t.bytes.len()
            {
                t.position += 2;
            }
        } else if address == REGISTERS + 0x24 {
            self.response_latched.set(false);
        }
        Ok(value)
    }
    /// Passive debugger word access: neither release CR4 nor consume DATATRNS.
    pub fn peek16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if address == DATA || address == DATA + 2 {
            return Ok(self.peek_data(0));
        }
        match address.wrapping_sub(REGISTERS) {
            8 => Ok(self.hirq.get()),
            12 => Ok(self.mask),
            n @ (0x18 | 0x1C | 0x20 | 0x24) => Ok(self.response.get()[((n - 0x18) / 4) as usize]),
            _ => Err(Self::error(address, 2)),
        }
    }
    fn peek_data(&self, offset: usize) -> u16 {
        let transfer = self.transfer.borrow();
        let Some(t) = transfer.as_ref().filter(|t| t.put.is_none()) else {
            return 0xFFFF;
        };
        let position = t.position + offset;
        t.bytes
            .get(position..position + 2)
            .map_or(0xFFFF, |word| u16::from_be_bytes([word[0], word[1]]))
    }
    /// Preview the next two FIFO words without advancing the transfer.
    pub(crate) fn peek_data32(&self) -> u32 {
        u32::from(self.peek_data(0)) << 16 | u32::from(self.peek_data(2))
    }
    pub fn write16(&mut self, address: u32, value: u16) -> Result<(), BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if address == DATA || address == DATA + 2 {
            if let Some(t) = self.transfer.get_mut().as_mut().filter(|t| t.put.is_some())
                && t.position + 2 <= t.bytes.len()
            {
                t.bytes[t.position..t.position + 2].copy_from_slice(&value.to_be_bytes());
                t.position += 2;
            }
            return Ok(());
        }
        match address.wrapping_sub(REGISTERS) {
            8 => self.hirq.set(self.hirq.get() & value),
            12 => self.mask = value & 0x3FFF,
            n @ (0x18 | 0x1C | 0x20 | 0x24) => {
                if self.pending.is_some() || !self.enabled {
                    return Err(Self::error(address, 2));
                }
                self.request[((n - 0x18) / 4) as usize] = value;
                if n == 0x24 {
                    self.hirq.set(self.hirq.get() & !CMOK);
                    // Bounded functional command latency within the documented
                    // command response envelope; no instantaneous host callback.
                    self.pending = Some((self.request, 1075));
                }
            }
            _ => return Err(Self::error(address, 2)),
        }
        Ok(())
    }
    pub fn advance(&mut self, mut clocks: u32) {
        if !self.enabled {
            return;
        }
        while clocks != 0 {
            let delta = clocks.min(self.clocks_to_event());
            let speed = self.drive_speed();
            let report_hz = self.report_hz();
            if self.status == 1 && !self.playing {
                self.pause_clocks += u64::from(delta);
                if self.pause_clocks >= u64::from(self.standby_seconds) * HZ {
                    self.status = 2;
                }
            } else {
                self.pause_clocks = 0;
            }
            let was_seeking = self.seeking != 0;
            self.seeking = self.seeking.saturating_sub(delta);
            if was_seeking && self.seeking == 0 {
                self.status = if self.playing { 3 } else { 1 };
            }
            self.phase += u64::from(delta) * 75 * u64::from(speed);
            while self.phase >= HZ {
                self.phase -= HZ;
                if let Some((reverse, _)) = self.scan {
                    // As with playback, replacement media bounds movement
                    // without overwriting retained host settings. The old
                    // lower bound can also be beyond the new disc's end.
                    let end = self.play_end();
                    let upper = end.saturating_sub(1);
                    let lower = self.play_start.max(150).min(upper);
                    let next = if reverse {
                        self.fad.saturating_sub(10)
                    } else {
                        self.fad.saturating_add(10)
                    };
                    self.fad = next.clamp(lower, upper);
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                    if next < lower || next >= end {
                        self.scan = None;
                        self.status = 1;
                        self.raise(PEND);
                    }
                } else if self.playing && self.seeking == 0 {
                    self.drive_frame();
                }
            }
            self.report_phase += u64::from(delta) * report_hz;
            if self.report_phase >= HZ {
                self.report_phase -= HZ;
                self.raise(SCDQ);
                if self.pending.is_none() {
                    self.report(true);
                }
            }
            // Drive input wins a simultaneous free-sector race. A copy sees
            // that allocation before its own deadline; newly accepted jobs
            // never consume time preceding command completion.
            if let Some(job) = &mut self.copying {
                job.clocks -= delta;
                if job.clocks == 0 {
                    self.copy_sector();
                }
            }
            // A completed command takes effect at the end of this interval,
            // never retroactively on the drive clocks preceding its deadline.
            if let Some((cmd, left)) = self.pending {
                if delta >= left {
                    self.pending = None;
                    if !self.execute(cmd) {
                        let mut r = self.response.get();
                        r[0] = 0xFF00;
                        self.response.set(r);
                    }
                    self.response_latched.set(true);
                    self.raise(CMOK);
                } else {
                    self.pending = Some((cmd, left - delta));
                }
            }
            clocks -= delta;
        }
    }
    fn drive_frame(&mut self) {
        let Some(media) = &self.media else {
            self.playing = false;
            self.status = 7;
            return;
        };
        let acquiring_directory = self
            .directory_read
            .as_ref()
            .is_some_and(|j| j.next == self.fad);
        if self.fad >= media.leadout {
            if acquiring_directory {
                self.status = 9;
                self.finish_directory(false);
                return;
            }
            self.finish_play();
            return;
        }
        if self.fad >= self.end {
            // A host-selected play range can obstruct filesystem progress
            // without making its next required sector unreadable.
            self.finish_play();
            return;
        }
        let audio = media
            .track(self.fad)
            .is_some_and(|t| t.mode == TrackMode::Audio);
        if audio {
            if acquiring_directory {
                // Audio cannot supply a filesystem sector. Fail at this read
                // deadline only when acquiring the job's required sector;
                // concurrent CD Play may have moved to an unrelated audio track.
                self.status = 9;
                self.finish_directory(false);
            }
            return;
        } // CD-DA advances exactly every 588 sample frames.
        if !acquiring_directory && self.used() >= POOL {
            self.buffer_paused = true;
            self.status = 1;
            self.raise(BFUL);
            return;
        }
        if self.buffer_paused {
            self.buffer_paused = false;
            self.status = 3;
        }
        let passes = if self.ecc == 0x80 { 0 } else { self.ecc + 1 };
        match media.decode_sector(self.fad, passes, self.init_flags & 4 == 0) {
            Ok((raw, valid)) => {
                // ST-162 §8.2.1, CDC_CdInit: Form 2 errors are delivered
                // immediately unless Form 2 retry is enabled. Each failed read
                // consumes one drive deadline, never an unbounded host loop.
                let form2 = raw[15] == 2 && raw[18] & 0x20 != 0;
                if !valid && (!form2 || self.init_flags & 8 != 0) {
                    if self.retry & 0x80 != 0 || self.retry_attempts < self.retry & 15 {
                        self.retry_attempts = self.retry_attempts.saturating_add(1);
                        self.status = 8;
                        return;
                    }
                    if self.retry & 0x40 == 0 {
                        self.playing = false;
                        self.status = 9;
                        if acquiring_directory {
                            self.finish_directory(false);
                        }
                        return;
                    }
                }
                self.retry_attempts = 0;
                self.status = 3;
                // Drive commands remain executable and may obstruct the
                // filesystem by moving away from its next required sector.
                if acquiring_directory {
                    match self.directory_sector(&raw) {
                        Some(done) => {
                            // Directory workspace replaces stream routing, but
                            // retains normal CD Play range/repeat processing.
                            self.finish_play();
                            if done {
                                self.finish_directory(true);
                            }
                        }
                        None => {
                            self.status = 9;
                            self.finish_directory(false);
                        }
                    }
                    return;
                }
                let selected = self
                    .file_read
                    .as_ref()
                    .is_none_or(|r| r.file.sector(r.logical_sector) == Some(self.fad));
                if selected {
                    let sector = self.new_sector(self.fad, raw);
                    self.route(sector);
                    self.raise(CSCT);
                    if let Some(r) = &mut self.file_read {
                        r.logical_sector += 1;
                    }
                }
                self.fad += 1;
                if self
                    .file_read
                    .as_ref()
                    .is_some_and(|r| r.logical_sector >= r.file.size.div_ceil(2048))
                {
                    self.file_read = None;
                    self.playing = false;
                    self.status = 1;
                    self.raise(EFLS | PEND);
                }
                self.finish_play();
            }
            Err(_) => {
                self.playing = false;
                self.status = 9;
                if acquiring_directory {
                    self.finish_directory(false);
                }
            }
        }
    }
    fn finish_play(&mut self) {
        let end = self.play_end();
        if self.fad >= end {
            if self.play_start < end && (self.repeats < self.repeat || self.repeat == 15) {
                self.repeats = (self.repeats + 1).min(14);
                // Start is separately preserved in the play range, not the filter.
                self.fad = self.play_start;
            } else {
                self.playing = false;
                self.status = 1;
                self.raise(PEND);
            }
        }
    }
    fn play_end(&self) -> u32 {
        // ST-162 §5.5 / §7.2.6: retain host settings across tray changes,
        // but bound their effective end to the currently inserted disc.
        self.media
            .as_ref()
            .map_or(self.end, |m| self.end.min(m.leadout))
    }
    fn used(&self) -> usize {
        self.partitions.iter().map(Vec::len).sum::<usize>()
            + usize::from(self.directory_read.is_some())
            + self
                .transfer
                .borrow()
                .as_ref()
                .and_then(|t| t.put)
                .map_or(0, |(_, n, _)| n)
    }
    fn new_sector(&mut self, fad: u32, raw: [u8; RAW_SECTOR]) -> Sector {
        let id = self.next_sector_id;
        self.next_sector_id = self.next_sector_id.wrapping_add(1);
        Sector { id, fad, raw }
    }
    fn copy_sector(&mut self) {
        let mut job = self.copying.take().unwrap();
        let id = job.sectors.pop_front().unwrap();
        let source = self.partitions[job.source].iter().position(|s| s.id == id);
        let target = source.map_or(255, |at| {
            self.route_target(&self.partitions[job.source][at], job.destination)
        });
        if target == 255 || (!job.moving && self.used() >= POOL) {
            self.copy_error = 1;
            self.raise(ECPY);
            return;
        }
        let at = source.unwrap();
        let sector = if job.moving {
            self.partitions[job.source].remove(at)
        } else {
            let sector = &self.partitions[job.source][at];
            self.new_sector(sector.fad, sector.raw)
        };
        self.partitions[target as usize].push(sector);
        if job.sectors.is_empty() {
            self.copy_error = 0;
            self.raise(ECPY);
        } else {
            job.clocks = COPY_CLOCKS;
            self.copying = Some(job);
        }
    }
    fn route(&mut self, sector: Sector) {
        let mut selector = self.connection;
        if let Some(r) = &self.file_read {
            selector = r.selector;
        }
        self.last = self.route_from(sector, selector);
    }
    fn route_from(&mut self, sector: Sector, selector: u8) -> u8 {
        let target = self.route_target(&sector, selector);
        if let Some(partition) = self.partitions.get_mut(target as usize) {
            partition.push(sector);
        }
        target
    }
    fn route_target(&self, sector: &Sector, mut selector: u8) -> u8 {
        for _ in 0..SELECTORS {
            let Some(f) = self.filters.get(selector as usize) else {
                return 255;
            };
            let sub = if sector.raw[15] == 2 {
                sector.raw[16..20].try_into().unwrap()
            } else {
                [0; 4]
            };
            let matched = f.accepts(sector.fad, sub);
            if matched {
                return f.yes;
            }
            selector = f.no;
        }
        255 // finite cyclic selector networks drop rather than hang host
    }
    /// External SCSP input: signed stereo, 588 frames per Red Book sector.
    pub fn audio_sample(&mut self) -> [i16; 2] {
        let scanning = self.scan.is_some();
        if !self.enabled
            || (!self.playing && !self.scan.is_some_and(|(_, audible)| audible))
            || self.seeking != 0
        {
            return [0; 2];
        }
        let Some(media) = &self.media else {
            return [0; 2];
        };
        let Some(track) = media.track(self.fad).filter(|t| t.mode == TrackMode::Audio) else {
            return [0; 2];
        };
        let preemphasis = track.control & 1 != 0;
        let Ok(raw) = media.raw_sector(self.fad) else {
            self.status = 9;
            self.playing = false;
            return [0; 2];
        };
        let i = self.audio_index * 4;
        // CDRWIN BINARY audio is little endian signed PCM, not SH-2 bus order.
        let sample = [
            i16::from_le_bytes([raw[i], raw[i + 1]]),
            i16::from_le_bytes([raw[i + 2], raw[i + 3]]),
        ];
        let sample = if preemphasis {
            self.deemphasis.sample(sample)
        } else {
            self.deemphasis = crate::cd_audio::Deemphasis::default();
            sample
        };
        let sample = if scanning {
            // -12 dB is two binary amplitude halvings (ST-162 §8.2.2).
            sample.map(|s| s / 4)
        } else {
            sample
        };
        self.audio_index += 1;
        if self.audio_index == 588 {
            self.audio_index = 0;
            if !scanning {
                self.fad += 1;
                self.finish_play();
            }
        }
        sample
    }
    fn range(&self, partition: usize, start: u16, count: u16) -> Option<(usize, usize)> {
        let n = self.partitions.get(partition)?.len();
        let start = if start == 0xFFFF {
            n.checked_sub(1)?
        } else {
            usize::from(start)
        };
        let count = if count == 0xFFFF {
            n.checked_sub(start)?
        } else {
            usize::from(count)
        };
        if start >= n || count == 0 || count > n - start {
            None
        } else {
            Some((start, count))
        }
    }
    fn data<'a>(&self, s: &'a Sector) -> &'a [u8] {
        match self.get_length {
            2352 => &s.raw,
            2340 => &s.raw[12..],
            2336 => &s.raw[16..],
            _ => {
                if s.raw[15] != 1 {
                    &s.raw[24..if s.raw[18] & 0x20 != 0 { 2348 } else { 2072 }]
                } else {
                    &s.raw[16..2064]
                }
            }
        }
    }
    fn begin_get(
        &mut self,
        bytes: Vec<u8>,
        delete: Option<(usize, Vec<u64>)>,
        host_io: bool,
    ) -> bool {
        if self.transfer.borrow().is_some() {
            return false;
        }
        *self.transfer.get_mut() = Some(Transfer {
            bytes,
            position: 0,
            put: None,
            delete,
            host_io,
        });
        let mut r = self.response.get();
        r[0] |= 0x4000;
        self.response.set(r);
        self.raise(DRDY);
        true
    }
    fn position(&self, field: u32, current: u32) -> Option<u32> {
        if field == 0xFF_FFFF {
            Some(current)
        } else if field & 0x80_0000 != 0 {
            Some(field & 0x7F_FFFF)
        } else if field == 0 {
            Some(150)
        } else {
            let track = (field >> 8) as u8;
            self.media
                .as_ref()?
                .tracks
                .iter()
                .find(|t| t.number == track)
                .and_then(|t| t.index(field as u8))
        }
    }
    fn wait(&self) -> bool {
        let mut r = self.response.get();
        r[0] |= 0x8000;
        self.response.set(r);
        true
    }
    fn execute(&mut self, c: [u16; 4]) -> bool {
        self.report(false);
        let command = (c[0] >> 8) as u8;
        let software_reset = command == 4 && c[0] & 0x81 == 1;
        // ST-162 §3.2 table3.6: ESEL/EHST/ECPY/EFLS are independent
        // engines. DATATRNS is shared by the commands that actually use it;
        // an active host transfer must not block drive/selector/query commands.
        let transfer = self.transfer.borrow();
        let transfer_busy = transfer.as_ref().is_some_and(|t| {
            matches!(command, 0x02 | 0x20 | 0x61 | 0x63 | 0x64 | 0x73)
                || (t.host_io && command == 0x62)
        });
        drop(transfer);
        if !software_reset
            && (transfer_busy || (self.copying.is_some() && matches!(command, 0x65 | 0x66)))
        {
            return self.wait();
        }
        let field1 = (u32::from(c[0] & 255) << 16) | u32::from(c[1]);
        let field2 = (u32::from(c[2] & 255) << 16) | u32::from(c[3]);
        let selector = (c[2] >> 8) as usize;
        let response = |this: &Self, a: u16, b: u16, d: u16| {
            let mut r = this.response.get();
            r[1] = a;
            r[2] = b;
            r[3] = d;
            this.response.set(r);
        };
        if self.tray_open && matches!(command, 0x02 | 0x03) {
            return false;
        }
        match command {
            0x00 => (),
            0x01 => response(self, 0x0002, 0, 0),
            0x02 => {
                let Some(m) = &self.media else {
                    return false;
                };
                let toc = m.toc();
                response(self, 204, 0, 0);
                return self.begin_get(toc, None, false);
            }
            0x03 => {
                let Some(m) = &self.media else {
                    return false;
                };
                let value = match c[0] & 255 {
                    0 => 0x0100_0000 | m.leadout,
                    1 => 0x0100_0096,
                    _ => 0xFFFF_FFFF,
                };
                response(self, 0, (value >> 16) as u16, value as u16);
            }
            0x04 => {
                let flags = c[0] as u8;
                let ecc = (c[3] >> 8) as u8;
                let retry = c[3] as u8;
                // Validate before any drive/selector/configuration mutation.
                // A software reset ignores every other initialization argument.
                if !software_reset
                    && (!matches!(ecc, 0..=5 | 0x80 | 0xFF)
                        || !matches!(retry, 0..=15 | 0x40..=0x4F | 0x80 | 0xFF))
                {
                    return false;
                }
                if software_reset {
                    self.init_flags = 0;
                    self.ecc = 0;
                    self.retry = 0;
                    self.standby_seconds = 180;
                } else {
                    if flags & 0x80 == 0 {
                        self.init_flags = flags & 0x1E;
                    }
                    if ecc != 0xFF {
                        self.ecc = ecc;
                    }
                    if retry != 0xFF {
                        self.retry = retry;
                    }
                }
                if !software_reset && c[1] != 0xFFFF {
                    self.standby_seconds = if c[1] == 0 { 180 } else { c[1] };
                }
                self.pause_clocks = 0;
                self.fad = self.pause_position();
                self.at_home = false;
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.playing = false;
                self.seeking = 0;
                self.audio_index = 0;
                self.deemphasis = crate::cd_audio::Deemphasis::default();
                self.finish_directory(false);
                self.file_read = None;
                self.retry_attempts = 0;
                self.speed = if self.init_flags & 0x10 != 0 { 1 } else { 2 };
                if software_reset {
                    self.filters = std::array::from_fn(Filter::reset);
                    for p in &mut self.partitions {
                        p.clear();
                    }
                    self.connection = 255;
                    *self.transfer.get_mut() = None;
                    self.get_length = 2048;
                    self.put_length = 2048;
                    self.actual_size = 0;
                    self.search = [0; 3];
                    self.copy_error = 0;
                    self.copying = None;
                    self.last = 255;
                    self.play_start = 150;
                    self.end = self.media.as_ref().map_or(self.fad, |m| m.leadout);
                    self.repeat = 0;
                    self.repeats = 0;
                    self.files.clear();
                    self.directory = None;
                    self.volume_end = 0;
                    self.hirq
                        .set(self.hirq.get() & !(DRDY | CSCT | BFUL | PEND));
                }
                self.status = if self.media.is_some() { 1 } else { 7 };
                self.report(false);
                if software_reset {
                    self.raise(ESEL | EHST | ECPY | EFLS | 0x800);
                }
            }
            0x05 => {
                self.finish_directory(false);
                self.retry_attempts = 0;
                self.files.clear();
                self.directory = None;
                self.volume_end = 0;
                self.playing = false;
                self.scan = None;
                self.seeking = 0;
                self.audio_index = 0;
                self.deemphasis = crate::cd_audio::Deemphasis::default();
                self.tray_open = true;
                self.status = 6;
                self.file_read = None;
                self.report(false);
                self.raise(0x20 | EFLS);
            }
            0x06 => {
                let Some(t) = self.transfer.get_mut().take() else {
                    let mut r = self.response.get();
                    r[0] = (r[0] & 0xFF00) | 255;
                    r[1] = 0xFFFF;
                    r[2] = 0;
                    r[3] = 0;
                    self.response.set(r);
                    return true;
                };
                let words = t.position / 2;
                if let Some((partition, ids)) = t.delete {
                    // Another engine may move/remove earlier sectors while
                    // DATATRNS is active. Never delete a shifted neighbour.
                    self.partitions[partition].retain(|s| !ids.contains(&s.id));
                }
                if let Some((selector, count, length)) = t.put {
                    // ST-162 §8.2.7 function7.5: all reserved sectors are
                    // written even when the host ends early. Untransferred
                    // bytes are unspecified; this model deterministically
                    // retains the zero initialization of the reservation.
                    for n in 0..count {
                        let mut raw = [0; RAW_SECTOR];
                        raw[15] = 0;
                        let offset = if length == 2048 {
                            24
                        } else {
                            RAW_SECTOR - length
                        };
                        raw[offset..offset + length]
                            .copy_from_slice(&t.bytes[n * length..(n + 1) * length]);
                        // Host writes enter the selected filter, just like
                        // CD/copy input; they do not bypass its connectors.
                        let sector = self.new_sector(0, raw);
                        self.route_from(sector, selector as u8);
                    }
                }
                self.hirq.set(self.hirq.get() & !DRDY);
                if t.host_io {
                    self.raise(EHST);
                }
                let mut r = self.response.get();
                r[0] = (r[0] & 0xFF00) | ((words >> 16) as u16 & 255);
                r[1] = words as u16;
                self.response.set(r);
            }
            0x10 => {
                let Some(m) = &self.media else {
                    return false;
                };
                let Some(start) = self.position(field1, self.pause_position()) else {
                    return false;
                };
                let end = if field2 == 0xFF_FFFF {
                    self.end
                } else if field2 & 0x80_0000 != 0 {
                    start.saturating_add(field2 & 0x7F_FFFF)
                } else if field2 == 0 {
                    m.leadout
                } else {
                    let Some(end) = self.position(field2, m.leadout) else {
                        return false;
                    };
                    end
                };
                if start < 150
                    || (field1 != 0xFF_FFFF && start >= end.min(m.leadout))
                    || (field2 != 0xFF_FFFF && end > m.leadout)
                {
                    return false;
                }
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.pause_clocks = 0;
                let from_home = self.at_home;
                self.at_home = false;
                let repeat = if c[2] >> 8 & 0x7F == 0x7F {
                    self.repeat
                } else {
                    (c[2] >> 8 & 15) as u8
                };
                if (field1 != 0xFF_FFFF && start != self.play_start)
                    || end != self.end
                    || repeat != self.repeat
                {
                    self.repeats = 0;
                }
                self.fad = start;
                self.end = end;
                // ST-162 §8.2.2: an unchanged start cancels pause without
                // replaying the current sector or restarting the loop range.
                if field1 != 0xFF_FFFF {
                    self.play_start = start;
                }
                if field1 != 0xFF_FFFF || from_home {
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                }
                // ST-162 §4.2 p.37: canceling pause outside either retained
                // range bound stays paused; it must not read or repeat a sector.
                self.playing = (self.play_start..self.play_end()).contains(&start);
                if !self.playing {
                    self.raise(PEND);
                }
                self.retry_attempts = 0;
                self.seeking = if !self.playing || (field1 == 0xFF_FFFF && !from_home) {
                    0
                } else {
                    26_875
                };
                self.status = if self.seeking != 0 {
                    4
                } else if self.playing {
                    3
                } else {
                    1
                };
                self.repeat = repeat;
                self.report(false);
            }
            0x11 => {
                if field1 == 0 {
                    if self.media.is_none() {
                        return false;
                    }
                    self.tray_open = false;
                    self.playing = false;
                    self.retry_attempts = 0;
                    self.scan = None;
                    self.buffer_paused = false;
                    self.seeking = 0;
                    self.at_home = true;
                    self.status = 2;
                    self.report(false);
                    return true;
                }
                let Some(fad) = self.position(field1, self.pause_position()) else {
                    return false;
                };
                if self
                    .media
                    .as_ref()
                    .is_none_or(|m| fad > m.leadout || (fad == m.leadout && field1 != 0xFF_FFFF))
                {
                    return false;
                }
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.pause_clocks = 0;
                self.fad = fad;
                self.playing = false;
                self.retry_attempts = 0;
                if field1 != 0xFF_FFFF || self.at_home {
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                }
                self.at_home = false;
                self.status = 4;
                self.seeking = 26_875;
                self.report(false);
            }
            0x12 => {
                if c[0] & 255 > 1 || self.media.is_none() {
                    return false;
                }
                let audible = self.scan.map_or(self.playing, |(_, audible)| audible);
                self.scan = Some((c[0] & 1 != 0, audible));
                self.fad = self.pause_position();
                self.at_home = false;
                self.tray_open = false;
                self.playing = false;
                self.buffer_paused = false;
                self.seeking = 0;
                self.status = 5;
                self.report(false);
            }
            0x20 => {
                // ISO/BIN/CUE carries no R–W pack stream. An empty pack FIFO
                // returns WAIT, not a fabricated all-zero pack (ST-162 §4.3).
                if c[0] & 255 == 1 {
                    return self.wait();
                }
                if c[0] & 255 != 0 {
                    return false;
                }
                // §4.3 allows Q in every drive state, including OPEN/NODISC.
                // §7.2.5 applies report validity to Q too. Invalid ADR=FF has
                // no meaningful position; preserve the fixed separator byte.
                let mut bytes = vec![0xFF; 10];
                bytes[6] = 0;
                if !self.invalid_report()
                    && let Some((ctrl, track, index, relative)) = self.track_report()
                {
                    bytes[..3].copy_from_slice(&[ctrl, track, index]);
                    bytes[3..6].copy_from_slice(&relative.to_be_bytes()[1..]);
                    bytes[7..10].copy_from_slice(&self.fad.to_be_bytes()[1..]);
                }
                response(self, 5, 0, 0);
                return self.begin_get(bytes, None, false);
            }
            0x30 => {
                // ST-162 §5.3 table5.1: a filter input has one source;
                // true outputs alone permit multiple sources per partition.
                if selector != 255
                    && (selector >= SELECTORS
                        || self.filters.iter().any(|f| usize::from(f.no) == selector))
                {
                    return false;
                }
                self.connection = selector as u8;
                self.raise(ESEL);
            }
            0x31 => response(self, 0, u16::from(self.connection) << 8, 0),
            0x32 => response(self, 0, u16::from(self.last) << 8, 0),
            0x40..=0x47 => {
                if command == 0x46 {
                    let yes = usize::from(c[1] >> 8);
                    let no = usize::from(c[1] as u8);
                    let valid = |n| n < SELECTORS || n == 255;
                    // Validate the entire command before either connection
                    // changes. Unselected bytes have no effect, even if invalid.
                    if (c[0] & 1 != 0 && !valid(yes))
                        || (c[0] & 2 != 0
                            && (!valid(no)
                                || (no != 255
                                    && (usize::from(self.connection) == no
                                        || self.filters.iter().enumerate().any(|(i, f)| {
                                            i != selector && usize::from(f.no) == no
                                        })))))
                    {
                        return false;
                    }
                }
                let Some(f) = self.filters.get_mut(selector) else {
                    return false;
                };
                match command {
                    0x40 => {
                        f.start = field1;
                        f.count = field2;
                        self.raise(ESEL);
                    }
                    0x41 => {
                        let (start, count) = (f.start, f.count);
                        let mut r = self.response.get();
                        r[0] |= (start >> 16) as u16 & 255;
                        r[1] = start as u16;
                        r[2] = (count >> 16) as u16 & 255;
                        r[3] = count as u16;
                        self.response.set(r);
                    }
                    0x42 => {
                        f.subheader = [c[2] as u8, c[0] as u8, (c[3] >> 8) as u8, c[3] as u8];
                        f.mask = [(c[1] >> 8) as u8, c[1] as u8];
                        self.raise(ESEL);
                    }
                    0x43 => {
                        let (s, m) = (f.subheader, f.mask);
                        let mut r = self.response.get();
                        r[0] = (r[0] & 0xFF00) | u16::from(s[1]);
                        r[1] = u16::from(m[0]) << 8 | u16::from(m[1]);
                        r[2] = u16::from(s[0]);
                        r[3] = u16::from(s[2]) << 8 | u16::from(s[3]);
                        self.response.set(r);
                    }
                    0x44 => {
                        if c[0] & 0x80 != 0 {
                            let (yes, no) = (f.yes, f.no);
                            *f = Filter::reset(selector);
                            f.yes = yes;
                            f.no = no;
                        } else {
                            f.mode = c[0] as u8 & 0x5F;
                        }
                        self.raise(ESEL);
                    }
                    0x45 => {
                        let mode = f.mode;
                        let mut r = self.response.get();
                        r[0] |= u16::from(mode);
                        self.response.set(r);
                    }
                    0x46 => {
                        if c[0] & 1 != 0 {
                            f.yes = (c[1] >> 8) as u8;
                        }
                        if c[0] & 2 != 0 {
                            f.no = c[1] as u8;
                        }
                        self.raise(ESEL);
                    }
                    _ => {
                        let (yes, no) = (f.yes, f.no);
                        response(self, u16::from(yes) << 8 | u16::from(no), 0, 0);
                    }
                }
            }
            0x48 => {
                if c[0] & 255 == 0 {
                    if selector >= SELECTORS {
                        return false;
                    }
                    self.partitions[selector].clear();
                } else {
                    for i in 0..SELECTORS {
                        if c[0] & 1 != 0 {
                            self.partitions[i].clear();
                        }
                        if c[0] & 4 != 0 {
                            let (yes, no) = (self.filters[i].yes, self.filters[i].no);
                            self.filters[i] = Filter::reset(i);
                            self.filters[i].yes = yes;
                            self.filters[i].no = no;
                        }
                        if c[0] & 16 != 0 {
                            self.filters[i].yes = i as u8;
                        }
                        if c[0] & 32 != 0 {
                            self.filters[i].no = 255;
                        }
                    }
                    if c[0] & 8 != 0 {
                        self.connection = 255;
                    }
                }
                self.raise(ESEL);
            }
            0x50 => response(self, (POOL - self.used()) as u16, 0x1800, POOL as u16),
            0x51 => {
                let Some(p) = self.partitions.get(selector) else {
                    return false;
                };
                response(self, 0, 0, p.len() as u16);
            }
            0x52 | 0x61..=0x63 | 0x65..=0x66 => {
                if selector >= SELECTORS {
                    return false;
                }
                let Some((start, count)) = self.range(selector, c[1], c[3]) else {
                    return self.wait();
                };
                match command {
                    0x52 => {
                        self.actual_size = self.partitions[selector][start..start + count]
                            .iter()
                            .map(|s| self.data(s).len() as u32 / 2)
                            .sum();
                        self.raise(ESEL);
                    }
                    0x61 | 0x63 => {
                        let bytes = self.partitions[selector][start..start + count]
                            .iter()
                            .flat_map(|s| self.data(s).iter().copied())
                            .collect();
                        return self.begin_get(
                            bytes,
                            if command == 0x63 {
                                Some((
                                    selector,
                                    self.partitions[selector][start..start + count]
                                        .iter()
                                        .map(|s| s.id)
                                        .collect(),
                                ))
                            } else {
                                None
                            },
                            true,
                        );
                    }
                    0x62 => {
                        self.partitions[selector].drain(start..start + count);
                        self.raise(EHST);
                    }
                    0x65 | 0x66 => {
                        let destination = (c[0] & 255) as usize;
                        if destination >= SELECTORS {
                            return false;
                        }
                        if command == 0x65 && self.used() + count > POOL {
                            return self.wait();
                        }
                        self.copying = Some(SelectorTransfer {
                            source: selector,
                            sectors: self.partitions[selector][start..start + count]
                                .iter()
                                .map(|s| s.id)
                                .collect(),
                            destination: destination as u8,
                            moving: command == 0x66,
                            clocks: COPY_CLOCKS,
                        });
                        self.copy_error = 255;
                    }
                    _ => unreachable!(),
                }
            }
            0x53 => {
                let mut r = [self.response.get()[0] & 0xFF00, 0, 0, 0];
                r[0] = (r[0] & 0xFF00) | ((self.actual_size >> 16) as u16 & 255);
                r[1] = self.actual_size as u16;
                self.response.set(r);
            }
            0x54 => {
                let Some(p) = self.partitions.get(selector) else {
                    return false;
                };
                let index = if c[1] == 0xFFFF {
                    p.len().saturating_sub(1)
                } else {
                    c[1] as usize
                };
                let Some(s) = p.get(index) else {
                    return self.wait();
                };
                let sub = if s.raw[15] == 2 {
                    s.raw[16..20].try_into().unwrap()
                } else {
                    [0; 4]
                };
                let mut r = self.response.get();
                r[0] = (r[0] & 0xFF00) | ((s.fad >> 16) as u16 & 255);
                r[1] = s.fad as u16;
                r[2] = u16::from(sub[0]) << 8 | u16::from(sub[1]);
                r[3] = u16::from(sub[2]) << 8 | u16::from(sub[3]);
                self.response.set(r);
            }
            0x55 => {
                let Some(p) = self.partitions.get(selector) else {
                    return false;
                };
                self.search = p
                    .iter()
                    .enumerate()
                    .skip(c[1] as usize)
                    .find(|(_, s)| s.fad >= field2)
                    .map_or([0xFFFF; 3], |(i, s)| {
                        [i as u16, (s.fad >> 16) as u16, s.fad as u16]
                    });
                self.raise(ESEL);
            }
            0x56 => response(self, self.search[0], self.search[1], self.search[2]),
            0x60 => {
                let sizes = [2048, 2336, 2340, 2352];
                let a = c[0] & 255;
                let b = c[1] >> 8;
                if a != 255 && a > 3 || b != 255 && b > 3 {
                    return false;
                }
                if a != 255 {
                    self.get_length = sizes[a as usize];
                }
                if b != 255 {
                    self.put_length = sizes[b as usize];
                }
                self.raise(ESEL);
            }
            0x64 => {
                let n = usize::from(c[3]);
                if selector >= SELECTORS || n == 0 || n > POOL {
                    return false;
                }
                if n > POOL - self.used() {
                    return self.wait();
                }
                *self.transfer.get_mut() = Some(Transfer {
                    bytes: vec![0; n * self.put_length],
                    position: 0,
                    put: Some((selector, n, self.put_length)),
                    delete: None,
                    host_io: true,
                });
                self.raise(DRDY);
                let mut r = self.response.get();
                r[0] |= 0x4000;
                self.response.set(r);
            }
            0x67 => {
                let mut r = self.response.get();
                r[0] = (r[0] & 0xFF00) | self.copy_error;
                self.response.set(r);
            }
            0x70 | 0x71 => {
                if selector >= SELECTORS || self.media.is_none() || self.tray_open {
                    return false;
                }
                if self.directory_read.is_some() {
                    return self.wait();
                }
                let id = if command == 0x70 { field2 } else { 0 };
                let first = if command == 0x70 || field2 == 0xFF_FFFF {
                    2
                } else {
                    field2.max(2)
                };
                let extent = if id == 0xFF_FFFF {
                    None
                } else {
                    let Some(file) = self.files.iter().find(|f| f.id == id && f.is_directory)
                    else {
                        return false;
                    };
                    Some(file.clone())
                };
                // Clearing the work partition can free a sector, but a full
                // pool owned by other selectors/host reservations returns WAIT
                // without changing either table or selector settings.
                if self.used() - self.partitions[selector].len() >= POOL {
                    return self.wait();
                }
                let next = extent.as_ref().map_or(166, |f| f.fad);
                let count = extent.as_ref().map_or(1, |f| f.size.div_ceil(2048));
                self.prepare_file_filter(selector, next, count, None);
                self.directory_read = Some(DirectoryRead {
                    selector,
                    extent,
                    volume_end: self.volume_end,
                    primary: None,
                    next,
                    first,
                    fid: 0,
                    files: Vec::new(),
                });
                self.files.clear();
                self.directory = None;
                self.file_read = None;
                self.scan = None;
                self.buffer_paused = false;
                self.fad = next;
                self.at_home = false;
                self.audio_index = 0;
                self.deemphasis = crate::cd_audio::Deemphasis::default();
                self.play_start = self.media.as_ref().unwrap().tracks[0].start;
                self.end = self.media.as_ref().unwrap().leadout;
                self.repeat = 0;
                self.repeats = 0;
                self.phase = 0;
                self.seeking = 26_875;
                self.playing = true;
                self.retry_attempts = 0;
                self.status = 4;
            }
            0x72 => {
                if self.directory.is_none() {
                    return false;
                }
                let first = self.files.get(2).map_or(2, |f| f.id);
                response(
                    self,
                    self.files.len().saturating_sub(2) as u16,
                    0x0100 | (first >> 16) as u16,
                    first as u16,
                );
            }
            0x73 => {
                let fid = field2;
                let selected: Vec<_> = if fid == 0xFF_FFFF {
                    self.files.clone()
                } else {
                    self.files.iter().filter(|f| f.id == fid).cloned().collect()
                };
                if selected.is_empty() {
                    return false;
                }
                let mut bytes = Vec::new();
                for f in selected {
                    bytes.extend(f.fad.to_be_bytes());
                    bytes.extend(f.size.to_be_bytes());
                    bytes.extend([f.unit, f.gap, f.number, f.flags]);
                }
                response(self, (bytes.len() / 2) as u16, 0, 0);
                return self.begin_get(bytes, None, false);
            }
            0x74 => {
                let Some(f) = self
                    .files
                    .iter()
                    .find(|f| f.id == field2 && !f.is_directory)
                else {
                    return false;
                };
                if selector >= SELECTORS {
                    return false;
                }
                let Some(start) = f.sector(field1) else {
                    return false;
                };
                let Some(end) = f
                    .size
                    .div_ceil(2048)
                    .checked_sub(1)
                    .and_then(|last| f.sector(last))
                    .and_then(|last| last.checked_add(1))
                else {
                    return false;
                };
                if start >= end {
                    return false;
                }
                let file = f.clone();
                self.prepare_file_filter(selector, start, end - start, Some(file.number));
                self.file_read = Some(FileRead {
                    file,
                    logical_sector: field1,
                    selector: selector as u8,
                });
                self.fad = start;
                self.at_home = false;
                self.end = end;
                self.play_start = start;
                self.playing = true;
                self.retry_attempts = 0;
                self.status = 3;
            }
            0x75 => {
                self.finish_directory(false);
                self.fad = self.pause_position();
                self.at_home = false;
                self.seeking = 0;
                self.scan = None;
                self.retry_attempts = 0;
                self.file_read = None;
                self.playing = false;
                self.status = 1;
                self.raise(EFLS);
            }
            _ => return false,
        }
        true
    }
    fn prepare_file_filter(&mut self, selector: usize, start: u32, count: u32, number: Option<u8>) {
        // ST-162 §6.2.3/table6.1: file access owns the selected filter and
        // partition; AbortFile deliberately leaves these settings intact.
        let mut filter = Filter::reset(selector);
        filter.start = start;
        filter.count = count;
        filter.mode = 0x40 | u8::from(number.is_some());
        filter.subheader[0] = number.unwrap_or(0);
        self.filters[selector] = filter;
        self.partitions[selector].clear();
        self.connection = selector as u8;
    }
    fn finish_directory(&mut self, success: bool) {
        let Some(job) = self.directory_read.take() else {
            return;
        };
        if success {
            self.files = job.files;
            self.directory = job.extent;
            self.volume_end = job.volume_end;
            self.status = 1;
        }
        self.playing = false;
        self.seeking = 0;
        self.retry_attempts = 0;
        // Never replace a retained command response with an asynchronous result.
        self.raise(EFLS);
    }
    fn directory_sector(&mut self, raw: &[u8; RAW_SECTOR]) -> Option<bool> {
        let media = self.media.as_ref()?;
        let job = self.directory_read.as_mut()?;
        let bytes = match raw[15] {
            1 => &raw[16..2064],
            2 if raw[18] & 0x20 == 0 => &raw[24..2072],
            _ => return None,
        };
        if job.extent.is_none() {
            let track = media.track(job.next)?;
            if job.next >= track.end || &bytes[1..7] != b"CD001\x01" {
                return None;
            }
            match bytes[0] {
                0 | 2 | 3 => (),
                1 if job.primary.is_none() => {
                    if bytes[128..132] != [0, 8, 8, 0]
                        || bytes[156] != 34
                        || bytes[181] != 2
                        || bytes[188..190] != [1, 0]
                    {
                        return None;
                    }
                    let blocks = u32::from_le_bytes(bytes[80..84].try_into().ok()?);
                    if blocks != u32::from_be_bytes(bytes[84..88].try_into().ok()?) {
                        return None;
                    }
                    let limit = blocks.checked_add(150).filter(|&n| n <= media.leadout)?;
                    let root = File::from_record(&bytes[156..190], media, limit, 0)?;
                    job.primary = Some((root, limit));
                }
                255 => {
                    let (root, limit) = job.primary.take()?;
                    if limit <= job.next
                        || root.fad <= job.next
                        || root.size == 0
                        || root.size > 1024 * 1024
                    {
                        return None;
                    }
                    job.next = root.fad;
                    self.filters[job.selector].start = root.fad;
                    self.filters[job.selector].count = root.size.div_ceil(2048);
                    job.extent = Some(root);
                    job.volume_end = limit;
                    self.fad = job.next;
                    // Same deterministic seek policy as CD play/seek. No claim
                    // of measured head movement or console seek timing.
                    self.seeking = 26_875;
                    self.status = 4;
                    return Some(false);
                }
                _ => return None,
            }
            job.next += 1;
            if job.next >= track.end {
                return None;
            }
        } else {
            let extent = job.extent.as_ref()?;
            if extent.size == 0 || extent.size > 1024 * 1024 {
                return None;
            }
            let remaining = extent.size.checked_sub((job.next - extent.fad) * 2048)?;
            let length = remaining.min(2048) as usize;
            let mut offset = 0;
            while offset < length {
                let len = bytes[offset] as usize;
                if len == 0 {
                    break;
                }
                if len < 34 || offset + len > length {
                    return None;
                }
                let r = &bytes[offset..offset + len];
                let file = File::from_record(r, media, job.volume_end, job.fid)?;
                if job.fid < 2 && (r[32..34] != [1, job.fid as u8] || !file.is_directory) {
                    return None;
                }
                if job.fid == 0 && (file.fad != extent.fad || file.size != extent.size) {
                    return None;
                }
                if job.fid < 2 || (job.fid >= job.first && job.files.len() < 256) {
                    job.files.push(file);
                }
                job.fid += 1;
                offset += len;
            }
            job.next += 1;
            if remaining <= 2048 {
                if job.fid < 2 || (job.first > 2 && job.first >= job.fid) {
                    return None;
                }
                self.fad = job.next;
                return Some(true);
            }
        }
        self.fad = job.next;
        Some(false)
    }
}
