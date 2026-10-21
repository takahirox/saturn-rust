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
    fad: u32,
    raw: [u8; RAW_SECTOR],
}
struct Transfer {
    bytes: Vec<u8>,
    position: usize,
    put: Option<(usize, usize)>,
    delete: Option<(usize, usize, usize)>,
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
    end: u32,
    play_start: u32,
    repeat: u8,
    repeats: u8,
    speed: u32,
    phase: u64,
    report_phase: u64,
    pause_clocks: u64,
    standby_seconds: u16,
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
    directory: u32,
    volume_end: u32,
    files: Vec<File>,
    file_read: Option<(u32, u32, u8)>,
}
#[derive(Clone)]
struct File {
    fad: u32,
    size: u32,
    flags: u8,
    id: u32,
}
impl File {
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
            || record[1] != 0
            || record[26] != 0
            || record[27] != 0
            || record[25] & !2 != 0
            || record[28..32] != [1, 0, 0, 1]
        {
            return None;
        }
        let dual = |offset| {
            let little = u32::from_le_bytes(record[offset..offset + 4].try_into().ok()?);
            let big = u32::from_be_bytes(record[offset + 4..offset + 8].try_into().ok()?);
            (little == big).then_some(little)
        };
        let fad = dual(2)?.checked_add(150)?;
        let size = dual(10)?;
        let end = fad.checked_add(size.div_ceil(2048))?;
        if fad > limit || end > limit || end > media.leadout {
            return None;
        }
        if size != 0 {
            let track = media.track(fad)?;
            if track.mode == TrackMode::Audio || fad < track.start || end > track.end {
                return None;
            }
        }
        Some(Self {
            fad,
            size,
            flags: record[25],
            id,
        })
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
            end: 150,
            play_start: 150,
            repeat: 0,
            repeats: 0,
            speed: 2,
            phase: 0,
            report_phase: 0,
            pause_clocks: 0,
            standby_seconds: 180,
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
            directory: 0,
            volume_end: 0,
            files: Vec::new(),
            file_read: None,
        }
    }
}
impl CdBlock {
    pub fn mount(&mut self, media: Media) {
        self.tray_open = false;
        self.files.clear();
        self.directory = 0;
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
        self.end = media.leadout;
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
            && (self.playing || self.scan.is_some() || self.pending.is_some() || self.seeking != 0)
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
        let (ctrl, track, index) = self
            .media
            .as_ref()
            .filter(|_| !self.tray_open)
            .and_then(|m| m.track(self.fad))
            .map_or((255u8, 255u8, 255u8), |t| {
                (t.control_adr(), t.number, t.index_at(self.fad))
            });
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
    pub fn read16(&self, address: u32) -> Result<u16, BusError> {
        if address & 1 != 0 {
            return Err(BusError::Unaligned { address, width: 2 });
        }
        if address == DATA || address == DATA + 2 {
            let mut transfer = self.transfer.borrow_mut();
            let Some(t) = transfer.as_mut().filter(|t| t.put.is_none()) else {
                return Ok(0xFFFF);
            };
            if t.position + 2 > t.bytes.len() {
                return Ok(0xFFFF);
            }
            let value = u16::from_be_bytes([t.bytes[t.position], t.bytes[t.position + 1]]);
            t.position += 2;
            return Ok(value);
        }
        match address.wrapping_sub(REGISTERS) {
            8 => Ok(self.hirq.get()),
            12 => Ok(self.mask),
            n @ (0x18 | 0x1C | 0x20 | 0x24) => {
                if n == 0x24 {
                    self.response_latched.set(false);
                }
                Ok(self.response.get()[((n - 0x18) / 4) as usize])
            }
            _ => Err(Self::error(address, 2)),
        }
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
                    let lower = self.play_start.max(150);
                    let next = if reverse {
                        self.fad.saturating_sub(10)
                    } else {
                        self.fad.saturating_add(10)
                    };
                    self.fad = next.clamp(lower, self.end.saturating_sub(1).max(lower));
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                    if next < lower || next >= self.end {
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
        if self.fad >= self.end || self.fad >= media.leadout {
            self.playing = false;
            self.status = 1;
            self.raise(PEND);
            return;
        }
        let audio = media
            .track(self.fad)
            .is_some_and(|t| t.mode == TrackMode::Audio);
        if audio {
            return;
        } // CD-DA advances exactly every 588 sample frames.
        if self.used() >= POOL {
            self.buffer_paused = true;
            self.status = 1;
            self.raise(BFUL);
            return;
        }
        if self.buffer_paused {
            self.buffer_paused = false;
            self.status = 3;
        }
        match media.raw_sector(self.fad) {
            Ok(raw) => {
                self.route(Sector { fad: self.fad, raw });
                self.raise(CSCT);
                self.fad += 1;
                if let Some((_, end, _)) = self.file_read
                    && self.fad >= end
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
            }
        }
    }
    fn finish_play(&mut self) {
        if self.fad >= self.end {
            if self.repeats < self.repeat || self.repeat == 15 {
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
    fn used(&self) -> usize {
        self.partitions.iter().map(Vec::len).sum::<usize>()
            + self
                .transfer
                .borrow()
                .as_ref()
                .and_then(|t| t.put)
                .map_or(0, |(_, n)| n)
    }
    fn route(&mut self, sector: Sector) {
        let mut selector = self.connection;
        if let Some((_, _, sel)) = self.file_read {
            selector = sel;
        }
        self.last = self.route_from(sector, selector);
    }
    fn route_from(&mut self, sector: Sector, mut selector: u8) -> u8 {
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
                let target = f.yes;
                if let Some(p) = self.partitions.get_mut(target as usize) {
                    p.push(sector);
                    return target;
                }
                return 255;
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
    fn begin_get(&mut self, bytes: Vec<u8>, delete: Option<(usize, usize, usize)>) -> bool {
        if self.transfer.borrow().is_some() {
            return false;
        }
        *self.transfer.get_mut() = Some(Transfer {
            bytes,
            position: 0,
            put: None,
            delete,
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
        if self.transfer.borrow().is_some() && !matches!(command, 0x00 | 0x01 | 0x06) {
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
        if self.tray_open && matches!(command, 0x02 | 0x03 | 0x20) {
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
                return self.begin_get(toc, None);
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
                if c[1] != 0xFFFF {
                    self.standby_seconds = if c[1] == 0 { 180 } else { c[1] };
                }
                self.pause_clocks = 0;
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.playing = false;
                self.seeking = 0;
                self.audio_index = 0;
                self.deemphasis = crate::cd_audio::Deemphasis::default();
                self.file_read = None;
                self.speed = if c[0] & 0x10 != 0 { 1 } else { 2 };
                if c[0] & 1 != 0 {
                    self.filters = std::array::from_fn(Filter::reset);
                    for p in &mut self.partitions {
                        p.clear();
                    }
                    self.connection = 255;
                    *self.transfer.get_mut() = None;
                    self.get_length = 2048;
                    self.put_length = 2048;
                }
                self.status = if self.media.is_some() { 1 } else { 7 };
                self.report(false);
                if c[0] & 1 != 0 {
                    self.raise(ESEL | EHST | ECPY | EFLS | 0x800);
                }
            }
            0x05 => {
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
                if let Some((partition, start, count)) = t.delete {
                    self.partitions[partition].drain(start..start + count);
                }
                if let Some((partition, count)) = t.put {
                    let complete = (t.position / self.put_length).min(count);
                    for n in 0..complete {
                        let mut raw = [0; RAW_SECTOR];
                        raw[15] = 0;
                        let offset = if self.put_length == 2048 {
                            24
                        } else {
                            RAW_SECTOR - self.put_length
                        };
                        raw[offset..offset + self.put_length].copy_from_slice(
                            &t.bytes[n * self.put_length..(n + 1) * self.put_length],
                        );
                        self.partitions[partition].push(Sector { fad: 0, raw });
                    }
                }
                self.hirq.set(self.hirq.get() & !DRDY);
                if t.delete.is_some() || t.put.is_some() || t.bytes.len() != 408 {
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
                let Some(start) = self.position(field1, self.fad) else {
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
                if start < 150 || start >= end || end > m.leadout {
                    return false;
                }
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.pause_clocks = 0;
                self.fad = start;
                self.end = end;
                // ST-162 §8.2.2: an unchanged start cancels pause without
                // replaying the current sector or restarting the loop range.
                if field1 != 0xFF_FFFF {
                    self.play_start = start;
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                    self.repeats = 0;
                }
                self.playing = true;
                self.seeking = if field1 == 0xFF_FFFF { 0 } else { 26_875 };
                self.status = if self.seeking == 0 { 3 } else { 4 };
                self.repeat = ((c[2] >> 8) & 15) as u8;
                self.report(false);
            }
            0x11 => {
                if field1 == 0 {
                    if self.media.is_none() {
                        return false;
                    }
                    self.tray_open = false;
                    self.playing = false;
                    self.scan = None;
                    self.seeking = 0;
                    self.status = 2;
                    self.report(false);
                    return true;
                }
                let Some(fad) = self.position(field1, self.fad) else {
                    return false;
                };
                if self.media.as_ref().is_none_or(|m| fad >= m.leadout) {
                    return false;
                }
                self.tray_open = false;
                self.scan = None;
                self.buffer_paused = false;
                self.pause_clocks = 0;
                self.fad = fad;
                self.playing = false;
                if field1 != 0xFF_FFFF {
                    self.audio_index = 0;
                    self.deemphasis = crate::cd_audio::Deemphasis::default();
                }
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
                let Some(t) = self.media.as_ref().and_then(|m| m.track(self.fad)) else {
                    return false;
                };
                if c[0] & 255 != 0 {
                    return false;
                }
                let relative = self.fad.abs_diff(t.start);
                let ctrl = t.control_adr();
                let mut bytes = vec![ctrl, t.number, t.index_at(self.fad), 0, 0, 0, 0, 0, 0, 0];
                bytes[3..6].copy_from_slice(&relative.to_be_bytes()[1..]);
                bytes[7..10].copy_from_slice(&self.fad.to_be_bytes()[1..]);
                response(self, 5, 0, 0);
                return self.begin_get(bytes, None);
            }
            0x30 => {
                if selector >= SELECTORS && selector != 255 {
                    return false;
                }
                self.connection = selector as u8;
                self.raise(ESEL);
            }
            0x31 => response(self, 0, u16::from(self.connection) << 8, 0),
            0x32 => response(self, 0, u16::from(self.last) << 8, 0),
            0x40..=0x47 => {
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
                                Some((selector, start, count))
                            } else {
                                None
                            },
                        );
                    }
                    0x62 => {
                        self.partitions[selector].drain(start..start + count);
                        self.raise(EHST);
                    }
                    0x65 | 0x66 => {
                        let destination = (c[0] & 255) as usize;
                        if destination >= SELECTORS
                            || (command == 0x65 && self.used() + count > POOL)
                        {
                            self.copy_error = 1;
                            return false;
                        }
                        let sectors = self.partitions[selector][start..start + count].to_vec();
                        if command == 0x66 {
                            self.partitions[selector].drain(start..start + count);
                        }
                        for sector in sectors {
                            self.route_from(sector, destination as u8);
                        }
                        self.copy_error = 0;
                        self.raise(ECPY);
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
                if selector >= SELECTORS
                    || n == 0
                    || n > POOL - self.used()
                    || self.transfer.borrow().is_some()
                {
                    self.raise(EHST);
                    return false;
                }
                *self.transfer.get_mut() = Some(Transfer {
                    bytes: vec![0; n * self.put_length],
                    position: 0,
                    put: Some((selector, n)),
                    delete: None,
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
            0x70 => {
                if selector >= SELECTORS {
                    return false;
                }
                if !self.load_directory(field2) {
                    return false;
                }
                self.raise(EFLS);
            }
            0x71 => {
                if selector >= SELECTORS {
                    return false;
                }
                if !self.load_directory(field2) {
                    return false;
                }
                self.raise(EFLS);
            }
            0x72 => response(self, self.files.len().saturating_sub(2) as u16, 0x0100, 2),
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
                    bytes.extend([0, 0, 0, f.flags]);
                }
                response(self, (bytes.len() / 2) as u16, 0, 0);
                return self.begin_get(bytes, None);
            }
            0x74 => {
                let Some(f) = self
                    .files
                    .iter()
                    .find(|f| f.id == field2 && f.flags & 2 == 0)
                else {
                    return false;
                };
                if selector >= SELECTORS {
                    return false;
                }
                let Some(start) = f.fad.checked_add(field1) else {
                    return false;
                };
                let Some(end) = f.fad.checked_add(f.size.div_ceil(2048)) else {
                    return false;
                };
                if start >= end {
                    return false;
                }
                self.file_read = Some((start, end, selector as u8));
                self.fad = start;
                self.end = end;
                self.play_start = start;
                self.playing = true;
                self.status = 3;
            }
            0x75 => {
                self.file_read = None;
                self.playing = false;
                self.status = 1;
                self.raise(EFLS);
            }
            _ => return false,
        }
        true
    }
    fn load_directory(&mut self, id: u32) -> bool {
        let Some(media) = &self.media else {
            return false;
        };
        let (extent, volume_end) = if id == 0xFF_FFFF {
            let Ok(pvd) = media.cooked_sector(166) else {
                return false;
            };
            if pvd.len() != 2048
                || &pvd[..7] != b"\x01CD001\x01"
                || pvd[128..132] != [0, 8, 8, 0]
                || pvd[156] != 34
                || pvd[181] != 2
                || pvd[188..190] != [1, 0]
            {
                return false;
            }
            let blocks = u32::from_le_bytes(pvd[80..84].try_into().unwrap());
            if blocks != u32::from_be_bytes(pvd[84..88].try_into().unwrap()) {
                return false;
            }
            let Some(limit) = blocks.checked_add(150).filter(|&end| end <= media.leadout) else {
                return false;
            };
            let Some(root) = File::from_record(&pvd[156..190], media, limit, 0) else {
                return false;
            };
            ((root.fad, root.size), limit)
        } else {
            let Some(f) = self.files.iter().find(|f| f.id == id && f.flags & 2 != 0) else {
                return false;
            };
            ((f.fad, f.size), self.volume_end)
        };
        if extent.1 == 0 || extent.1 > 1024 * 1024 {
            return false;
        }
        let mut bytes = Vec::new();
        for i in 0..extent.1.div_ceil(2048) {
            let Ok(sector) = media.cooked_sector(extent.0 + i) else {
                return false;
            };
            bytes.extend(sector);
        }
        let mut files = Vec::new();
        let mut offset = 0;
        while offset < extent.1 as usize {
            let len = bytes[offset] as usize;
            if len == 0 {
                offset = (offset / 2048 + 1) * 2048;
                continue;
            }
            if len < 34 || offset + len > extent.1 as usize || offset % 2048 + len > 2048 {
                return false;
            }
            let r = &bytes[offset..offset + len];
            let Some(file) = File::from_record(r, media, volume_end, files.len() as u32) else {
                return false;
            };
            files.push(file);
            offset += len;
        }
        self.files = files;
        self.directory = id;
        self.volume_end = volume_end;
        true
    }
}
