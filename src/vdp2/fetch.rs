//! ST-058-R2 §3.3 tables3.2–3.5: name, character and cell-scroll
//! accesses use independent display paths. A fetched physical word is held
//! while its packed dots are consumed, independently of later CPU VRAM writes.
use super::{Vdp2, rotation::RotationSample};
use crate::vdp1::Vdp1;
use std::{cell::Cell, num::NonZeroU8};

pub(super) type NormalSample = (usize, usize, usize, u16, u16);
type PlannedSample = (NormalSample, (usize, usize));

type Words = [Option<(usize, u16)>; 2];

// Standalone decoder tests keep their two-word serializers.
// Clocked backgrounds retain physical transfers and per-dot address inputs.
// Normal ports have separate acquisition cursors; rotation uses per-dot edges.
#[derive(Default)]
pub(crate) struct DisplayFetch {
    words: [[Words; 3]; 5],
    #[cfg(test)]
    pub trace: Vec<DisplayRead>,
    pub pixel: usize,
    pub rotation: Cell<bool>,
    scheduled: bool,
    pub prepared_line: Option<usize>,
    prepared_field: Option<u64>,
    planning: bool,
    group: (usize, usize),
    request: Option<(usize, usize)>,
    // Slot 5 is RBG1: it shares composition/register id0 with NBG0,
    // but must not replace NBG0 operands already in the normal pipeline.
    pixels: Vec<[[Option<CapturedWords>; 3]; 6]>,
    samples: Vec<[Option<NormalSample>; 4]>,
    planned_samples: Vec<[Option<PlannedSample>; 4]>,
    #[cfg(test)]
    literal_addresses: bool,
    #[cfg(test)]
    direct_bitmaps: usize,
    cursor: [[usize; 3]; 4],
    retained: [[Option<Acquired>; 3]; 4],
    edge: Option<(u64, u16, u32)>,
    rotation_edge: Option<(u64, u16, u32)>,
    acquiring_rotation: bool,
    rotation_samples: Vec<[Option<RotationSample>; 2]>,
}

impl DisplayFetch {
    pub(super) fn reset(&mut self) {
        let mut pixels = std::mem::take(&mut self.pixels);
        let mut samples = std::mem::take(&mut self.samples);
        let mut planned_samples = std::mem::take(&mut self.planned_samples);
        let mut rotation_samples = std::mem::take(&mut self.rotation_samples);
        pixels.clear();
        samples.clear();
        planned_samples.clear();
        rotation_samples.clear();
        *self = Self {
            pixels,
            samples,
            planned_samples,
            rotation_samples,
            ..Self::default()
        };
    }
}

#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayRead {
    pub phase: u32,
    pub slot: usize,
    pub layer: usize,
    pub path: usize,
    pub address: usize,
    pub words: [u16; 2],
}

#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct CapturedWords {
    address: usize,
    bytes: NonZeroU8,
    words: [u16; 2],
}
impl CapturedWords {
    fn new(address: usize, bytes: usize, words: [u16; 2]) -> Self {
        debug_assert!(matches!(bytes, 2 | 4));
        Self {
            address,
            bytes: NonZeroU8::new(bytes as u8).unwrap(),
            words,
        }
    }
    fn word(self, address: usize) -> Option<u16> {
        let offset = address.wrapping_sub(self.address) & 0x7FFFF;
        (offset < usize::from(self.bytes.get())).then(|| self.words[offset / 2])
    }
}
// Only the retained normal-port group needs this key. Per-dot consumers use
// captured address/width/data directly, preserving the original read edges.
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct Acquired {
    group: (usize, usize),
    data: CapturedWords,
}
impl Acquired {
    fn word(self, address: usize) -> Option<u16> {
        self.data.word(address)
    }
}

impl Vdp2 {
    /// Only hardware deadlines may fill clocked image serializers.
    /// Snapshot decoding keeps an independent, non-mutating live-memory path.
    pub(super) fn fetch_word(&self, layer: usize, path: usize, address: usize) -> Option<u16> {
        let address = address & 0x7FFFE;
        let mut fetch = self.display_fetch.borrow_mut();
        if !self.beam_rendering || !(self.clocked || fetch.scheduled) {
            drop(fetch);
            return Some(self.display_word(layer, path, address));
        }
        let x = fetch.pixel;
        let storage = if fetch.rotation.get() && layer == 0 {
            5
        } else {
            layer
        };
        if let Some(value) = fetch
            .pixels
            .get(x)
            .and_then(|p| p[storage][path])
            .and_then(|v| v.word(address))
        {
            return Some(value);
        }
        if fetch.rotation.get() {
            if fetch.acquiring_rotation {
                let requested = address;
                let bytes = if path == 0 && self.word(0x30 + layer * 2) & 0x8000 != 0 {
                    2
                } else {
                    4
                };
                let address = if path == 1 { address & !3 } else { address };
                let role = if path == 0 { 2 } else { 3 };
                if !self.rotation_access(layer, address, role)
                    || !self.rotation_access(layer, address + bytes - 2, role)
                {
                    return None;
                }
                let acquired = CapturedWords::new(
                    address,
                    bytes,
                    [
                        self.vword(address),
                        if bytes == 4 {
                            self.vword(address + 2)
                        } else {
                            0
                        },
                    ],
                );
                #[cfg(test)]
                let phase = fetch.rotation_edge.unwrap().2;
                #[cfg(test)]
                fetch.trace.push(DisplayRead {
                    phase,
                    slot: x,
                    layer,
                    path,
                    address,
                    words: acquired.words,
                });
                fetch.pixels[x][storage][path] = Some(acquired);
                return acquired.word(requested);
            }
            return None;
        }
        if fetch.planning {
            if let Some(held) = fetch.retained[layer][path]
                && held.group == fetch.group
                && let Some(value) = held.word(address)
            {
                fetch.pixels[x][layer][path] = Some(held.data);
                return Some(value);
            }
            fetch.request = Some((path, address));
        }
        None
    }

    // Identify the source group before address wrapping or character-number
    // lookup. Repeated characters in later groups must issue fresh transfers;
    // enlargement may consume one group's packed data across multiple cycles.
    pub(super) fn fetch_group(&self, group: (usize, usize)) {
        let mut fetch = self.display_fetch.borrow_mut();
        if fetch.planning {
            fetch.group = group;
        }
    }

    pub(super) fn fetch_byte(&self, layer: usize, address: usize) -> Option<u8> {
        Some((self.fetch_word(layer, 1, address)? >> ((1 - (address & 1)) * 8)) as u8)
    }

    pub(super) fn scheduled_sample(&self, layer: usize) -> Option<Option<NormalSample>> {
        let fetch = self.display_fetch.borrow();
        if self.beam_rendering && (self.clocked || fetch.scheduled) && !fetch.planning {
            Some(fetch.samples.get(fetch.pixel).and_then(|p| p[layer]))
        } else {
            None
        }
    }

    pub(super) fn planned_sample(&self, layer: usize) -> Option<NormalSample> {
        let mut fetch = self.display_fetch.borrow_mut();
        #[cfg(test)]
        if fetch.literal_addresses {
            return None;
        }
        if self.beam_rendering && fetch.planning {
            let (sample, group) = fetch.planned_samples.get(fetch.pixel)?[layer]?;
            fetch.group = group;
            Some(sample)
        } else {
            None
        }
    }
    pub(super) fn plan_sample(&self, layer: usize, sample: NormalSample) {
        let mut fetch = self.display_fetch.borrow_mut();
        if self.beam_rendering && fetch.planning {
            let (pixel, group) = (fetch.pixel, fetch.group);
            fetch.planned_samples[pixel][layer] = Some((sample, group));
        }
    }
    pub(super) fn invalidate_planned_samples(&mut self) {
        self.display_fetch.get_mut().planned_samples.fill([None; 4]);
    }

    pub(super) fn reconfigure_display(&mut self) {
        self.invalidate_planned_samples();
        let fetch = self.display_fetch.get_mut();
        for layer in 0..4 {
            let unfinished = fetch
                .samples
                .iter()
                .position(|p| p[layer].is_none())
                .unwrap_or(fetch.samples.len());
            fetch.cursor[layer] = [unfinished; 3];
        }
    }

    pub(crate) fn display_active(&self) -> bool {
        self.word(0) & 0x8000 != 0 && self.word(0x20) & 0x3F != 0
    }

    pub(crate) fn rotation_display_active(&self) -> bool {
        self.word(0) & 0x8000 != 0 && self.word(0x20) & 0x30 != 0
    }

    pub(crate) fn clocked() -> Self {
        Self {
            clocked: true,
            ..Self::default()
        }
    }

    fn prepare_display_line(&mut self, field: u64, line: usize) {
        let fetch = self.display_fetch.get_mut();
        fetch.reset();
        fetch.scheduled = true;
        fetch.prepared_line = Some(line);
        fetch.prepared_field = Some(field);
        let y = if self.word(0) & 0xC0 == 0xC0 {
            line * 2 + usize::from(!field.is_multiple_of(2))
        } else {
            line
        };
        self.read_line_tables(y);
    }

    // Unit-step bitmaps have no name/cell dependencies. Resolve their
    // coordinates once per edge and consume each retained physical word
    // directly, preserving per-dot storage and every real command/grant.
    fn acquire_unit_bitmap(
        &mut self,
        layer: usize,
        y: usize,
        horizon: usize,
        phase: u32,
        slot: usize,
        used: &mut [bool; 4],
    ) -> bool {
        if layer >= 2
            || self.word(0) & 6 != 0
            || self.word(0x22) & (1 << layer) != 0
            || self.word(0x9A) >> (layer * 8) & 15 != 0
            || self.long(0x78 + layer * 0x10) & 0x7FF00 != 0x10000
        {
            return false;
        }
        let (ctl, format) = self.character_control(layer);
        if ctl & 2 == 0 || format >= 4 {
            return false;
        }
        #[cfg(test)]
        {
            self.display_fetch.get_mut().direct_bitmaps += 1;
        }
        let width = if ctl & 8 == 0 { 512 } else { 1024 };
        let height = if ctl & 4 == 0 { 256 } else { 512 };
        let scroll = 0x70 + layer * 0x10;
        let sx = (self.long(scroll) & 0x7FFFF00) as usize >> 16;
        let sy = ((u64::from(self.long(scroll + 4) & 0x7FFFF00)
            + y as u64 * u64::from(self.long(scroll + 12) & 0x7FF00))
            >> 16) as usize;
        let base = usize::from(self.word(0x3C) >> (layer * 4) & 7) * 0x20000;
        let palette = usize::from(self.word(0x2C) >> (layer * 8) & 7) * 256;
        let special = self.word(0x2C) >> (layer * 8 + 4) & 3;
        let mut policies = [None; 4];
        loop {
            let x = self.display_fetch.get_mut().cursor[layer][1];
            if x >= horizon {
                break;
            }
            let group = ((sx + x) / 8, sy);
            let sample = (
                base,
                (sy % height) * width + (sx + x) % width,
                palette,
                special,
                format,
            );
            let address = Self::dot_address(sample.0, sample.1, format) & 0x7FFFE;
            let bank = self.display_bank(address);
            let allowed = if let Some(allowed) = policies[bank] {
                allowed
            } else {
                let allowed = self.character_access(layer, address, format, None, false);
                policies[bank] = Some(allowed);
                allowed
            };
            {
                let fetch = self.display_fetch.get_mut();
                fetch.pixel = x;
                fetch.group = group;
                fetch.request = None;
                fetch.planning = false;
                if !allowed {
                    fetch.cursor[layer][1] += 1;
                    continue;
                }
                fetch.planned_samples[x][layer] = Some((sample, group));
            }
            let held = {
                let fetch = self.display_fetch.get_mut();
                fetch.pixels[x][layer][1]
                    .filter(|v| v.word(address).is_some())
                    .or_else(|| {
                        fetch.retained[layer][1]
                            .filter(|v| v.group == group && v.word(address).is_some())
                            .map(|v| v.data)
                    })
            };
            let acquired = if let Some(held) = held {
                held
            } else {
                self.display_fetch.get_mut().request = Some((1, address));
                if used[bank]
                    || self.rotation_bank(bank)
                    || self.access_command(bank, slot) != (4 + layer) as u16
                {
                    break;
                }
                used[bank] = true;
                let address = address & !3;
                let acquired =
                    CapturedWords::new(address, 4, [self.vword(address), self.vword(address + 2)]);
                #[cfg(test)]
                self.display_fetch.get_mut().trace.push(DisplayRead {
                    phase,
                    slot,
                    layer,
                    path: 1,
                    address,
                    words: acquired.words,
                });
                #[cfg(not(test))]
                let _ = phase;
                self.display_fetch.get_mut().retained[layer][1] = Some(Acquired {
                    group,
                    data: acquired,
                });
                acquired
            };
            let fetch = self.display_fetch.get_mut();
            fetch.request = None;
            fetch.pixels[x][layer][1] = Some(acquired);
            fetch.samples[x][layer] = Some(sample);
            fetch.cursor[layer][1] += 1;
        }
        true
    }

    /// One command per physical bank at an actual T0–T7/T0–T3 boundary.
    /// ST-058-R2 §3.3 fig.3.2, tables3.2–3.4 and fig.3.5. The
    /// 32-bit character/table port supplies two retained 16-bit words;
    /// a name access supplies one complete one/two-word entry.
    pub(crate) fn acquire_display(&mut self, field: u64, line: u16, phase: u32, slot: usize) {
        self.acquire_display_paths(field, line, phase, slot, true);
    }

    fn acquire_display_paths(
        &mut self,
        field: u64,
        line: u16,
        phase: u32,
        slot: usize,
        omit_unused: bool,
    ) {
        if !self.display_active() || self.validate().is_err() {
            return;
        }
        let edge = (field, line, phase);
        if self.display_fetch.borrow().edge == Some(edge) {
            return;
        }
        if self.display_fetch.borrow().prepared_line != Some(usize::from(line))
            || self.display_fetch.borrow().prepared_field != Some(field)
        {
            self.prepare_display_line(field, usize::from(line));
        }
        #[cfg(test)]
        {
            self.display_fetch.get_mut().literal_addresses = !omit_unused;
        }
        let width = self.dimensions().0;
        {
            let fetch = self.display_fetch.get_mut();
            fetch.scheduled = true;
            fetch.edge = Some(edge);
            fetch.pixels.resize(width, [[None; 3]; 6]);
            fetch.samples.resize(width, [None; 4]);
            fetch.planned_samples.resize(width, [None; 4]);
        }
        let y = if self.word(0) & 0xC0 == 0xC0 {
            usize::from(line) * 2 + usize::from(!field.is_multiple_of(2))
        } else {
            usize::from(line)
        };
        let count = if self.word(0) & 6 == 0 { 8 } else { 4 };
        let clocks = if self.word(0) & 4 == 0 { 4 } else { 2 };
        // Table3.3 defines an eight-dot source group. Keep the existing
        // exclusive-monitor output multiplexing in the coordinate decoder.
        let horizon = (((phase / (count * clocks) + 1) * 8) as usize).min(width);
        self.beam_rendering = true;
        let mut used = [false; 4];
        // Cell scrolling feeds names, which feed character addressing.
        for path in [2, 0, 1] {
            for layer in 0..4 {
                if self.word(0x20) & (1 << layer) == 0
                    || !self.normal_layer_available(layer)
                    || self.word(0x20) & 0x20 != 0
                {
                    continue;
                }
                // These disabled dependency paths cannot request memory. The
                // later enabled paths still resolve and latch every operand
                // at this same slot, before another bus master can run.
                if omit_unused
                    && ((path == 2
                        && (layer >= 2
                            || self.word(0x9A) & (1 << (layer * 8)) == 0
                            || self.word(0x22) & (1 << layer) != 0))
                        || (path == 0 && layer < 2 && self.character_control(layer).0 & 2 != 0))
                {
                    self.display_fetch.get_mut().cursor[layer][path] = horizon;
                    continue;
                }
                if path == 1
                    && omit_unused
                    && self.display_fetch.get_mut().cursor[layer][1] >= horizon
                {
                    continue;
                }
                if path == 1
                    && omit_unused
                    && self.acquire_unit_bitmap(layer, y, horizon, phase, slot, &mut used)
                {
                    continue;
                }
                loop {
                    let x = self.display_fetch.borrow().cursor[layer][path];
                    if x >= horizon {
                        break;
                    }
                    {
                        let fetch = self.display_fetch.get_mut();
                        fetch.pixel = x;
                        fetch.planning = true;
                        fetch.request = None;
                    }
                    if let Some(sample @ (base, pixel, _, _, format)) =
                        self.normal_sample(layer, x, y)
                    {
                        let address = Self::dot_address(base, pixel, format);
                        if self.fetch_word(layer, 1, address).is_some()
                            && (format != 4 || self.fetch_word(layer, 1, address + 2).is_some())
                        {
                            self.display_fetch.get_mut().samples[x][layer] = Some(sample);
                        }
                    }
                    let request = self.display_fetch.borrow().request;
                    self.display_fetch.get_mut().planning = false;
                    let Some((needed, address)) = request else {
                        self.display_fetch.get_mut().cursor[layer][path] += 1;
                        continue;
                    };
                    let order = |p| match p {
                        2 => 0,
                        0 => 1,
                        _ => 2,
                    };
                    if order(needed) > order(path) {
                        self.display_fetch.get_mut().cursor[layer][path] += 1;
                        continue;
                    }
                    if needed != path {
                        break;
                    }
                    let bank = self.display_bank(address);
                    let command = match path {
                        0 => layer,
                        1 => 4 + layer,
                        _ => 12 + layer,
                    };
                    if used[bank]
                        || self.rotation_bank(bank)
                        || self.access_command(bank, slot) != command as u16
                    {
                        break;
                    }
                    used[bank] = true;
                    let bytes = if path == 0 && self.word(0x30 + layer * 2) & 0x8000 != 0 {
                        2
                    } else {
                        4
                    };
                    let address = if path == 1 { address & !3 } else { address };
                    let acquired = Acquired {
                        group: self.display_fetch.borrow().group,
                        data: CapturedWords::new(
                            address,
                            bytes,
                            [
                                self.vword(address),
                                if bytes == 4 {
                                    self.vword(address + 2)
                                } else {
                                    0
                                },
                            ],
                        ),
                    };
                    #[cfg(test)]
                    self.display_fetch.get_mut().trace.push(DisplayRead {
                        phase,
                        slot,
                        layer,
                        path,
                        address,
                        words: acquired.data.words,
                    });
                    self.display_fetch.get_mut().retained[layer][path] = Some(acquired);
                }
            }
        }
        self.beam_rendering = false;
    }

    pub(super) fn scheduled_rotation_sample(&self, rbg1: bool) -> Option<Option<RotationSample>> {
        let fetch = self.display_fetch.borrow();
        if self.beam_rendering && (self.clocked || fetch.scheduled) && !fetch.acquiring_rotation {
            Some(
                fetch
                    .rotation_samples
                    .get(fetch.pixel)
                    .and_then(|p| p[usize::from(rbg1)]),
            )
        } else {
            None
        }
    }

    /// Dedicated rotation ports use per-dot deadlines, independently of normal
    /// cycle commands and the normal pipeline lead. See the transaction contract.
    /// X is an output dot: high-resolution picture pairs still acquire twice.
    pub(crate) fn acquire_rotation_display(
        &mut self,
        sprites: &Vdp1,
        field: u64,
        line: u16,
        phase: u32,
        x: usize,
    ) {
        if !self.rotation_display_active() || self.validate().is_err() {
            return;
        }
        let edge = (field, line, phase);
        if self.display_fetch.borrow().rotation_edge == Some(edge) {
            return;
        }
        if self.display_fetch.borrow().prepared_line != Some(usize::from(line))
            || self.display_fetch.borrow().prepared_field != Some(field)
        {
            self.prepare_display_line(field, usize::from(line));
        }
        let y = if self.word(0) & 0xC0 == 0xC0 {
            usize::from(line) * 2 + usize::from(!field.is_multiple_of(2))
        } else {
            usize::from(line)
        };
        // Initial reset line uses the same latch as scanout. Subsequent lines
        // have already latched at LineStart. Image reads never rearm requests.
        if !self.rotation_line_ready(y) {
            self.read_line_tables(y);
            self.beam_rendering = true;
            self.read_rotation_line(y, line == 0);
            self.beam_rendering = false;
        }
        let width = self.dimensions().0;
        if x >= width {
            return;
        }
        {
            let fetch = self.display_fetch.get_mut();
            fetch.scheduled = true;
            fetch.rotation_edge = Some(edge);
            fetch.pixels.resize(width, [[None; 3]; 6]);
            fetch.rotation_samples.resize(width, [None; 2]);
            fetch.pixel = x;
            fetch.acquiring_rotation = true;
            fetch.rotation.set(true);
        }
        self.rotation_read.line = Some(y);
        self.beam_rendering = true;
        let sprite_window = self.sprite_window(Some(sprites), x, y);
        for (rbg1, layer, enable) in [(false, 4, 0x10), (true, 0, 0x20)] {
            if self.word(0x20) & enable == 0 {
                continue;
            }
            self.display_fetch.get_mut().pixels[x][if rbg1 { 5 } else { 4 }] = [None; 3];
            self.display_fetch.get_mut().rotation_samples[x][usize::from(rbg1)] = None;
            if let Some(sample) = self.rotation_sample(rbg1, x, y, sprite_window) {
                let (base, pixel, _, _, format) = sample.image;
                let at = Self::dot_address(base, pixel, format);
                if self.fetch_word(layer, 1, at).is_some()
                    && (format != 4 || self.fetch_word(layer, 1, at + 2).is_some())
                {
                    self.display_fetch.get_mut().rotation_samples[x][usize::from(rbg1)] =
                        Some(sample);
                }
            }
        }
        self.beam_rendering = false;
        self.rotation_read.line = None;
        let fetch = self.display_fetch.get_mut();
        fetch.acquiring_rotation = false;
        fetch.rotation.set(false);
    }

    pub(super) fn display_word(&self, layer: usize, path: usize, address: usize) -> u16 {
        if !self.beam_rendering {
            return self.vword(address);
        }
        let address = address & 0x7FFFE;
        let mut fetch = self.display_fetch.borrow_mut();
        let word = &mut fetch.words[layer][path][(address >> 1) & 1];
        if let Some((old, value)) = *word
            && old == address
        {
            return value;
        }
        let value = self.vword(address);
        *word = Some((address, value));
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{devices::VDP2, vdp1::Vdp1};

    fn setup(format: u16) -> Vdp2 {
        let mut v = Vdp2::default();
        for (at, value) in [
            (0, 0x8000),
            (0x10, 0x4444),
            (0x12, 0xEEEE),
            (0x20, 0x101),
            (0x28, 2 | format << 4),
            (0x78, 1),
            (0x7C, 1),
            (0xF8, 1),
        ] {
            v.write16(VDP2 + at, value).unwrap();
        }
        for (index, value) in [(1, 0x801Fu16), (2, 0x83E0), (3, 0xFC00)] {
            v.cram[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
        v
    }
    fn pixel(v: &Vdp2, x: usize, y: usize) -> [u8; 4] {
        let at = (y * 320 + x) * 4;
        v.scanning[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn reloaded_line_coordinates_invalidate_unfinished_address_plans() {
        let configure = || {
            let mut v = setup(0);
            for (at, value) in [
                (0x10, 0xFFFF),
                (0x12, 0x4FFF),
                (0x9A, 2),
                (0xA0, 0),
                (0xA2, 0x8000),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            v.vram[..4].fill(0x11);
            v.vram[4..8].fill(0x22);
            v
        };
        let (mut optimized, mut literal) = (configure(), configure());
        optimized.acquire_display(0, 0, 0, 0);
        literal.acquire_display_paths(0, 0, 0, 0, false);
        // A name/address is resolved before the character's T4 grant. The
        // line-start latch can reload scroll operands after normal prefetch.
        assert!(optimized.display_fetch.borrow().planned_samples[0][0].is_some());
        for v in [&mut optimized, &mut literal] {
            v.vram[0x10000..0x10004].copy_from_slice(&0x0008_0000u32.to_be_bytes());
            v.read_line_tables(0);
        }
        for phase in (4..32).step_by(4) {
            optimized.acquire_display(0, 0, phase, phase as usize / 4);
            literal.acquire_display_paths(0, 0, phase, phase as usize / 4, false);
        }
        let (a, b) = (
            optimized.display_fetch.borrow(),
            literal.display_fetch.borrow(),
        );
        assert_eq!(a.trace, b.trace);
        assert_eq!(a.pixels, b.pixels);
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.trace[0].address, 4);
        assert_eq!(a.trace[0].words, [0x2222; 2]);
    }

    #[test]
    fn unit_bitmaps_preserve_literal_scroll_wrap_reconfiguration_and_live_word_reads() {
        for layer in 0..2 {
            for format in 0..4 {
                for size in [0, 4, 8, 12] {
                    for scroll in [0x00068000u32, 0x01FF8000] {
                        let configure = || {
                            let mut v = setup(0);
                            let registers = [
                                (0x20, (0x101 << layer) as u16),
                                (0x28, (2 | size | format << 4) << (layer * 8)),
                                (0x78 + layer * 16, 1),
                                (0x7A + layer * 16, 0),
                                (0x7C + layer * 16, 1),
                                (0x7E + layer * 16, 0x8000),
                                (0x70 + layer * 16, (scroll >> 16) as u16),
                                (0x72 + layer * 16, scroll as u16),
                                (0x74 + layer * 16, 0x200),
                                (0x76 + layer * 16, 0x8000),
                            ];
                            for (offset, value) in registers {
                                v.write16(VDP2 + offset as u32, value).unwrap();
                            }
                            for bank in 0..4 {
                                let command = (4 + layer) as u16 * 0x1111;
                                v.write16(VDP2 + 0x10 + bank * 4, command).unwrap();
                                v.write16(VDP2 + 0x12 + bank * 4, command).unwrap();
                            }
                            for (i, byte) in v.vram.iter_mut().enumerate() {
                                *byte = (i as u8).wrapping_mul(19);
                            }
                            v
                        };
                        let (mut fast, mut literal) = (configure(), configure());
                        for phase in (0..160).step_by(4) {
                            for v in [&mut fast, &mut literal] {
                                v.vram[..128].fill(phase as u8);
                                if phase == 92 {
                                    v.write16(VDP2 + 0x70 + layer as u32 * 16, 7).unwrap();
                                }
                            }
                            let slot = (phase as usize / 4) % 8;
                            fast.acquire_display(0, 7, phase, slot);
                            literal.acquire_display_paths(0, 7, phase, slot, false);
                            let (a, b) =
                                (fast.display_fetch.borrow(), literal.display_fetch.borrow());
                            assert_eq!(a.trace, b.trace);
                            assert_eq!(a.pixels, b.pixels);
                            assert_eq!(a.samples, b.samples);
                        }
                        assert!(fast.display_fetch.borrow().direct_bitmaps > 0);
                    }
                }
            }
        }
    }

    #[test]
    fn unused_dependency_paths_preserve_literal_acquisitions_and_latched_pixels() {
        let mut direct_bitmaps = 0;
        for layer in 0..4 {
            for bitmap in [false, true] {
                if bitmap && layer >= 2 {
                    continue;
                }
                for cell in [false, true] {
                    for mosaic in [false, true] {
                        for format in 0..=if layer < 2 { 3 } else { 1 } {
                            let configure = || {
                                let mut v = setup(0);
                                v.vram
                                    .iter_mut()
                                    .enumerate()
                                    .for_each(|(i, byte)| *byte = (i as u8).wrapping_mul(17));
                                for (at, value) in [
                                    (0x20, 1 << layer),
                                    (0x22, if mosaic { 0x2300 | 1 << layer } else { 0 }),
                                    (
                                        0x28,
                                        if layer < 2 {
                                            ((u16::from(bitmap) * 2) | (format << 4)) << (layer * 8)
                                        } else {
                                            0
                                        },
                                    ),
                                    (
                                        0x2A,
                                        if layer >= 2 {
                                            format << ((layer - 2) * 4 + 1)
                                        } else {
                                            0
                                        },
                                    ),
                                    (
                                        0x9A,
                                        if cell && layer < 2 {
                                            1 << (layer * 8)
                                        } else {
                                            0
                                        },
                                    ),
                                    (0x30 + layer * 2, 0x8000),
                                    (0xF8 + layer / 2 * 2, 1 << (layer % 2 * 8)),
                                ] {
                                    v.write16(VDP2 + at as u32, value).unwrap();
                                }
                                for bank in 0..4 {
                                    v.write16(
                                        VDP2 + 0x10 + bank * 4,
                                        ((12 + layer.min(1)) << 12
                                            | layer << 8
                                            | (4 + layer) << 4
                                            | 15) as u16,
                                    )
                                    .unwrap();
                                    v.write16(VDP2 + 0x12 + bank * 4, 0xFFFF).unwrap();
                                }
                                v
                            };
                            let (mut optimized, mut literal) = (configure(), configure());
                            for phase in (0..128).step_by(4) {
                                let slot = (phase as usize / 4) % 8;
                                optimized.acquire_display(0, 0, phase, slot);
                                literal.acquire_display_paths(0, 0, phase, slot, false);
                                let (a, b) = (
                                    optimized.display_fetch.borrow(),
                                    literal.display_fetch.borrow(),
                                );
                                assert_eq!(a.trace, b.trace);
                                assert_eq!(
                                    a.pixels, b.pixels,
                                    "layer={layer}, bitmap={bitmap}, cell={cell}, mosaic={mosaic}, format={format}, phase={phase}"
                                );
                                assert_eq!(a.samples, b.samples);
                                drop((a, b));
                                // A write between acquisition edges must affect only
                                // later transfers, including a live dependency change.
                                for v in [&mut optimized, &mut literal] {
                                    v.vram[(phase as usize * 64)..(phase as usize * 64 + 64)]
                                        .fill(phase as u8);
                                    if phase == 60 && layer < 2 {
                                        v.write16(
                                            VDP2 + 0x9A,
                                            if cell { 0 } else { 1 << (layer * 8) },
                                        )
                                        .unwrap();
                                    }
                                }
                            }
                            let sprites = Vdp1::default();
                            optimized.scan_to(&sprites, 0, 32);
                            literal.scan_to(&sprites, 0, 32);
                            assert_eq!(optimized.scanning, literal.scanning);
                            direct_bitmaps += optimized.display_fetch.borrow().direct_bitmaps;
                        }
                    }
                }
            }
        }
        assert!(
            direct_bitmaps > 0,
            "literal comparison must exercise the direct bitmap pipeline"
        );
    }

    #[test]
    fn repeated_characters_refetch_per_source_group_and_retain_each_groups_packed_dots() {
        // ST-058-R2 §3.3 tables3.2/3.3: one name and one 32-bit
        // character access per eight source dots at 1x/16 colors, even
        // when adjacent names select the same character. Table3.4 permits
        // NBG0/1 T0 characters with T0 names; all layers permit T4.
        for layer in 0..4 {
            for one in [true, false] {
                for enlarged in [false, true] {
                    if layer >= 2 && enlarged {
                        continue;
                    }
                    let mut v = setup(0);
                    let character_slot = if layer < 2 { 0 } else { 4 };
                    for (at, value) in [
                        (0xE, 0x300),
                        (0x20, 1 << layer),
                        (0x28, 0),
                        (0x10, (layer as u16) << 12 | 0xFFF),
                        (0x12, 0xFFFF),
                        (
                            0x18,
                            if character_slot == 0 {
                                (4 + layer as u16) << 12 | 0xFFF
                            } else {
                                0xFFFF
                            },
                        ),
                        (
                            0x1A,
                            if character_slot == 4 {
                                (4 + layer as u16) << 12 | 0xFFF
                            } else {
                                0xFFFF
                            },
                        ),
                        (0x30 + layer * 2, if one { 0x8008 } else { 0 }),
                        (0xF8 + layer / 2 * 2, 1 << (layer % 2 * 8)),
                    ] {
                        v.write16(VDP2 + at as u32, value).unwrap();
                    }
                    if layer < 2 {
                        v.write16(
                            VDP2 + 0x78 + layer as u32 * 0x10,
                            if enlarged { 0 } else { 1 },
                        )
                        .unwrap();
                        v.write16(
                            VDP2 + 0x7A + layer as u32 * 0x10,
                            if enlarged { 0x8000 } else { 0 },
                        )
                        .unwrap();
                    }
                    if !one {
                        for at in [2, 6] {
                            v.vram[at..at + 2].copy_from_slice(&0x2000u16.to_be_bytes());
                        }
                    }
                    v.vram[0x40000..0x40004].fill(0x11);
                    let dots = if enlarged { 16 } else { 8 };
                    let next_phase = if enlarged { 64 } else { 32 };
                    for phase in (0..next_phase).step_by(4) {
                        v.acquire_display(0, 0, phase, (phase / 4 % 8) as usize);
                        if phase == character_slot * 4 {
                            v.vram[0x40000..0x40004].fill(0x22);
                        }
                    }
                    for phase in (next_phase..next_phase * 2).step_by(4) {
                        v.acquire_display(0, 0, phase, (phase / 4 % 8) as usize);
                        if phase == next_phase + character_slot * 4 {
                            v.vram[0x40000..0x40004].fill(0x33);
                        }
                    }
                    let chars: Vec<_> = v
                        .display_fetch
                        .borrow()
                        .trace
                        .iter()
                        .filter(|r| r.path == 1)
                        .map(|r| (r.phase, r.address, r.words))
                        .collect();
                    assert_eq!(
                        chars,
                        [
                            (character_slot * 4, 0x40000, [0x1111; 2]),
                            (next_phase + character_slot * 4, 0x40000, [0x2222; 2]),
                        ],
                        "layer={layer}, one={one}, enlarged={enlarged}"
                    );
                    v.vram[0x40000..0x40004].fill(0x33);
                    v.scan_to(&Vdp1::default(), 0, dots * 2);
                    for x in 0..dots * 2 {
                        assert_eq!(
                            pixel(&v, x, 0),
                            if x < dots {
                                [255, 0, 0, 255]
                            } else {
                                [0, 255, 0, 255]
                            },
                            "layer={layer}, one={one}, enlarged={enlarged}, x={x}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn reduction_refetches_repeated_characters_within_the_same_output_packet() {
        // ST-058-R2 §3.3 tables3.2/3.3: 1/2 and 1/4 reduction
        // need respectively two/four name and 16-color character accesses
        // per cycle, even if every name selects the same physical address.
        for (reduction, dx, pattern) in [(1, 2, 0x00FF), (3, 4, 0x0000)] {
            let mut v = setup(0);
            for (at, value) in [
                (0xE, 0x300),
                (0x20, 1),
                (0x28, 0),
                (0x30, 0x8008),
                (0x10, pattern),
                (0x12, 0xFFFF),
                (0x18, pattern | 0x4444),
                (0x1A, 0xFFFF),
                (0x78, dx),
                (0x98, reduction),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            let mut expected = Vec::new();
            for slot in 0..dx {
                let word = if slot % 2 == 0 { 0x1111u16 } else { 0x2222 };
                v.vram[0x40000..0x40004]
                    .copy_from_slice(&[word.to_be_bytes(), word.to_be_bytes()].concat());
                v.acquire_display(0, 0, u32::from(slot) * 4, usize::from(slot));
                expected.push((u32::from(slot) * 4, 0x40000, [word; 2]));
            }
            let chars: Vec<_> = v
                .display_fetch
                .borrow()
                .trace
                .iter()
                .filter(|r| r.path == 1)
                .map(|r| (r.phase, r.address, r.words))
                .collect();
            assert_eq!(chars, expected, "reduction={reduction}");
            v.scan_to(&Vdp1::default(), 0, 8);
            for x in 0..8 {
                assert_eq!(
                    pixel(&v, x, 0),
                    if (x / (8 / usize::from(dx))) % 2 == 0 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 255]
                    },
                    "reduction={reduction}, x={x}"
                );
            }
        }
    }

    #[test]
    fn acquired_coordinates_survive_scroll_writes_but_future_packets_use_the_new_origin() {
        let mut v = setup(0);
        v.write16(VDP2 + 0x10, 0x4EEE).unwrap();
        v.vram[..4].fill(0x11);
        v.vram[8..12].fill(0x22);
        v.begin_scanline(0);
        v.acquire_display(0, 0, 0, 0);
        v.write16(VDP2 + 0x70, 8).unwrap();
        v.acquire_display(0, 0, 32, 0);
        v.scan_to(&Vdp1::default(), 0, 16);
        for x in 0..16 {
            assert_eq!(
                pixel(&v, x, 0),
                if x < 8 {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                }
            );
        }
        let trace: Vec<_> = v
            .display_fetch
            .borrow()
            .trace
            .iter()
            .map(|r| (r.address, r.phase))
            .collect();
        assert_eq!(trace, [(0, 0), (8, 32)]);
    }

    #[test]
    fn every_supported_tile_color_depth_has_independent_name_and_character_events() {
        for layer in 0..4 {
            let formats = if layer == 0 {
                5
            } else if layer == 1 {
                4
            } else {
                2
            };
            for format in 0..formats {
                let reads = [1usize, 2, 4, 4, 8][format];
                let mut v = Vdp2::default();
                let character_command = 4 + layer as u16;
                let mut commands = [15u16; 8];
                commands[..reads].fill(character_command);
                let packed = |cs: &[u16]| cs.iter().fold(0, |n, c| n << 4 | c);
                for (at, value) in [
                    (0, 0x8000),
                    (0x20, 1 << layer),
                    (0xE, 0x300),
                    (0x10, ((layer as u16) << 12) | 0xFFF),
                    (0x12, 0xFFFF),
                    (0x18, packed(&commands[..4])),
                    (0x1A, packed(&commands[4..])),
                    (0x30 + layer * 2, 0x8008),
                    (0x78, 1),
                    (0x88, 1),
                ] {
                    v.write16(VDP2 + at as u32, value).unwrap();
                }
                let at = if layer < 2 { 0x28 } else { 0x2A };
                let shift = if layer < 2 {
                    4 + layer * 8
                } else {
                    1 + (layer - 2) * 4
                };
                v.write16(VDP2 + at, (format as u16) << shift).unwrap();
                v.vram[0x40000..0x40020].fill(0x11);
                for slot in 0..8 {
                    v.acquire_display(0, 0, slot * 4, slot as usize);
                }
                let trace = &v.display_fetch.borrow().trace;
                assert_eq!(trace[0].path, 0);
                let chars: Vec<_> = trace.iter().filter(|r| r.path == 1).collect();
                assert_eq!(chars.len(), reads, "layer={layer}, format={format}");
                for (i, r) in chars.iter().enumerate() {
                    assert_eq!(
                        (r.layer, r.address, r.phase),
                        (layer, 0x40000 + 4 * i, i as u32 * 4)
                    );
                }
            }
        }
    }

    #[test]
    fn bitmap_banks_reductions_scroll_and_next_line_refetch_use_scheduled_data() {
        for split in [0, 0x300] {
            for bank in 0..4 {
                let mut v = setup(0);
                for at in (0x10..=0x1E).step_by(2) {
                    v.write16(VDP2 + at, 0xFFFF).unwrap();
                }
                let selected = if split == 0 { bank & !1 } else { bank };
                for (at, value) in [
                    (0xE, split),
                    (0x3C, bank as u16),
                    (0x7C, 0),
                    (0x7E, 0x100),
                    (0x10 + selected * 4, 0x4EEE),
                ] {
                    v.write16(VDP2 + at, value).unwrap();
                }
                let base = bank as usize * 0x20000;
                v.vram[base..base + 4].fill(0x11);
                v.acquire_display(0, 0, 0, 0);
                assert_eq!(v.display_fetch.borrow().trace[0].address, base);
                v.vram[base..base + 4].fill(0x22);
                v.acquire_display(0, 0, 32, 0);
                assert_eq!(v.display_fetch.borrow().trace[0].words, [0x1111; 2]);
                v.begin_scanline(1);
                v.acquire_display(0, 1, 0, 0);
                assert_eq!(v.display_fetch.borrow().trace[0].words, [0x2222; 2]);
            }
        }
        for (reduction, dx, pattern, expected) in [
            (1, 2, 0x44EE, vec![(0, 0), (4, 4), (8, 32)]),
            (
                3,
                4,
                0x4444,
                vec![(0, 0), (4, 4), (8, 8), (12, 12), (16, 32)],
            ),
        ] {
            let mut v = setup(0);
            for (at, value) in [
                (0x10, pattern),
                (0x12, 0xEEEE),
                (0x70, 3),
                (0x78, dx),
                (0x98, reduction),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            for slot in 0..9 {
                v.acquire_display(0, 0, slot * 4, (slot % 8) as usize);
            }
            let actual: Vec<_> = v
                .display_fetch
                .borrow()
                .trace
                .iter()
                .map(|r| (r.address, r.phase))
                .collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn rotation_owned_banks_and_disabled_layers_issue_no_normal_acquisition() {
        let mut v = setup(0);
        v.write16(VDP2 + 0x20, 0x111).unwrap();
        v.write16(VDP2 + 0xE, 3).unwrap();
        v.acquire_display(0, 0, 0, 0);
        assert!(v.display_fetch.borrow().trace.is_empty());
        v.write16(VDP2 + 0x3C, 2).unwrap();
        v.write16(VDP2 + 0x18, 0x4EEE).unwrap();
        // A separate line discards unfinished requests from the owned bank.
        v.acquire_display(0, 1, 0, 0);
        assert_eq!(v.display_fetch.borrow().trace[0].address, 0x40100);
        v.write16(VDP2 + 0x20, 0).unwrap();
        let trace = v.display_fetch.borrow().trace.clone();
        for slot in 1..8 {
            v.acquire_display(0, 1, slot * 4, slot as usize);
        }
        assert_eq!(v.display_fetch.borrow().trace, trace);
        assert_eq!(v.cpu_access_slots(0x05E00000), 255);
    }

    // Source-authored schedules: ST-058-R2 §3.3 fig.3.2 fixes T0..T7
    // (T0..T3 in four-timing modes); table3.3 supplies 1/2/4/4/8
    // 32-bit accesses per eight source dots. Four main clocks per slot
    // (two for exclusive monitors) are the shared raster phase convention.
    #[test]
    fn scheduled_bitmap_formats_sample_only_at_command_edges() {
        for mode in [0x8000, 0x8002, 0x8004] {
            for (format, reads) in [1usize, 2, 4, 4, 8].into_iter().enumerate() {
                if mode != 0x8000 && format == 4 {
                    continue;
                }
                let mut v = setup(format as u16);
                v.write16(VDP2, mode).unwrap();
                v.write16(VDP2 + 0x10, 0x4444).unwrap();
                v.write16(VDP2 + 0x12, 0x4444).unwrap();
                for (i, byte) in v.vram[..128].iter_mut().enumerate() {
                    *byte = i as u8;
                }
                let count = if mode == 0x8000 { 8 } else { 4 };
                let clocks = if mode & 4 == 0 { 4 } else { 2 };
                for slot in 0..reads {
                    let phase = slot as u32 * clocks;
                    assert_eq!(v.display_fetch.borrow().trace.len(), slot);
                    v.acquire_display(0, 0, phase, slot % count);
                    let trace = &v.display_fetch.borrow().trace;
                    assert_eq!(trace.len(), slot + 1);
                    let read = &trace[slot];
                    assert_eq!(
                        (read.phase, read.slot, read.layer, read.path, read.address),
                        (phase, slot % count, 0, 1, slot * 4)
                    );
                    assert_eq!(
                        read.words,
                        [
                            u16::from_be_bytes([(slot * 4) as u8, (slot * 4 + 1) as u8]),
                            u16::from_be_bytes([(slot * 4 + 2) as u8, (slot * 4 + 3) as u8])
                        ]
                    );
                }
                // The decoder cannot acquire a new address; repeated debugger
                // snapshots see live memory and cannot modify the trace/latches.
                let trace = v.display_fetch.borrow().trace.clone();
                v.vram[..4].fill(255);
                let _ = v.framebuffer();
                let _ = v.framebuffer();
                assert_eq!(v.display_fetch.borrow().trace, trace);
                v.beam_rendering = true;
                v.display_fetch.borrow_mut().pixel = 0;
                assert_eq!(v.fetch_word(0, 1, 0), Some(0x0001));
                assert_eq!(v.fetch_word(0, 1, 124), None);
                v.beam_rendering = false;
            }
        }
    }

    #[test]
    fn each_layer_uses_latched_one_or_two_word_names_before_character_slots() {
        for layer in 0..4 {
            for one in [true, false] {
                let mut v = Vdp2::default();
                for (at, value) in [
                    (0, 0x8000),
                    (0x20, 1 << layer),
                    (0xE, 0x300),
                    (0x10, 0xFFFF),
                    (0x12, 0xFFFF),
                    (0x18, 0xFFFF),
                    (0x1A, 0xFFFF),
                    (0x30 + layer * 2, if one { 0x8008 } else { 0 }),
                ] {
                    v.write16(VDP2 + at as u32, value).unwrap();
                }
                // Name at T2 in A0, character at T6 in B0 is permitted
                // by table3.4's T2 mask T0..T3,T6..T7 for all NBGs.
                v.write16(VDP2 + 0x10, 0xFF0F | (layer as u16) << 4)
                    .unwrap();
                v.write16(VDP2 + 0x1A, 0xFF0F | (4 + layer as u16) << 4)
                    .unwrap();
                if !one {
                    v.vram[2..4].copy_from_slice(&0x2000u16.to_be_bytes());
                }
                v.vram[0x40000..0x40004].fill(0x11);
                v.acquire_display(0, 0, 4, 1);
                assert!(v.display_fetch.borrow().trace.is_empty());
                v.acquire_display(0, 0, 8, 2);
                assert_eq!(v.display_fetch.borrow().trace.len(), 1);
                // Mutation after name acquisition must not redirect characters.
                v.vram[..4].fill(0xFF);
                v.acquire_display(0, 0, 20, 5);
                assert_eq!(v.display_fetch.borrow().trace.len(), 1);
                v.acquire_display(0, 0, 24, 6);
                let trace = v.display_fetch.borrow().trace.clone();
                assert_eq!(
                    (trace[1].path, trace[1].address, trace[1].words),
                    (1, 0x40000, [0x1111; 2])
                );
                v.vram[0x40000..0x40004].fill(0x22);
                v.beam_rendering = true;
                v.display_fetch.borrow_mut().pixel = 7;
                assert_eq!(v.fetch_word(layer, 1, 0x40002), Some(0x1111));
                v.beam_rendering = false;
            }
        }
    }

    #[test]
    fn cell_scroll_precedes_names_and_layers_have_independent_table_reads() {
        let mut v = setup(0);
        for (at, value) in [
            (0x20, 3),
            (0x28, 0x202),
            (0x9A, 0x101),
            (0x9C, 0),
            (0x9E, 0x8000),
            (0x10, 0xCD44),
            (0x12, 0x55FF),
            (0x88, 1),
            (0x8C, 1),
        ] {
            v.write16(VDP2 + at, value).unwrap();
        }
        // Fig.3.5: NBG0 T0 before NBG1 T1, in the same bank. Both
        // entries are complete longwords; source Y=1/2 selects row1/2.
        v.vram[0x10000..0x10008].copy_from_slice(&[0, 1, 0, 0, 0, 2, 0, 0]);
        v.acquire_display(0, 0, 0, 0);
        // NBG0 changes after its T0 read; NBG1 changes before its T1 read.
        v.vram[0x10000..0x10008].copy_from_slice(&[0, 3, 0, 0, 0, 4, 0, 0]);
        for slot in 1..6 {
            v.acquire_display(0, 0, slot * 4, slot as usize);
        }
        let trace = &v.display_fetch.borrow().trace;
        assert_eq!(
            (trace[0].path, trace[0].address, trace[0].phase),
            (2, 0x10000, 0)
        );
        assert_eq!(
            (
                trace[1].layer,
                trace[1].path,
                trace[1].address,
                trace[1].phase
            ),
            (1, 2, 0x10004, 4)
        );
        assert!(
            trace
                .iter()
                .any(|r| r.layer == 0 && r.path == 1 && r.address == 256)
        );
        assert!(
            trace
                .iter()
                .any(|r| r.layer == 1 && r.path == 1 && r.address == 1024)
        );
    }

    #[test]
    fn packed_dots_retain_the_fetched_word_but_an_unfetched_word_and_next_line_see_writes() {
        for (format, old, new, dots) in [(0, 0x1111u16, 0x2222u16, 4), (1, 0x0101, 0x0202, 2)] {
            for captured in 1..dots {
                let mut v = setup(format);
                v.vram[..4].copy_from_slice(&[old.to_be_bytes(), old.to_be_bytes()].concat());
                v.begin_scanline(0);
                v.scan_to(&Vdp1::default(), 0, captured);
                v.vram[..4].copy_from_slice(&[new.to_be_bytes(), new.to_be_bytes()].concat());
                v.scan_to(&Vdp1::default(), 0, dots * 2);
                for x in 0..dots * 2 {
                    assert_eq!(
                        pixel(&v, x, 0),
                        if x < dots {
                            [255, 0, 0, 255]
                        } else {
                            [0, 255, 0, 255]
                        }
                    );
                }
                // 256x vertical enlargement: line1 reads source row0 anew.
                v.write16(VDP2 + 0x7C, 0).unwrap();
                v.write16(VDP2 + 0x7E, 0x100).unwrap();
                v.begin_scanline(1);
                v.scan_to(&Vdp1::default(), 1, dots);
                assert_eq!(pixel(&v, 0, 1), [0, 255, 0, 255]);
                // Debugger snapshots inspect current memory without changing
                // or invalidating the beam's retained display words.
                assert_eq!(&v.framebuffer().unwrap()[..4], [0, 255, 0, 255]);
            }
        }
    }

    #[test]
    fn palette_11_bit_and_direct_16_32_bit_words_retain_complete_zoomed_pixels() {
        for (format, old, new, color) in [
            (2, [1u16, 0], [2u16, 0], [255, 0, 0, 255]),
            (3, [0x801F, 0], [0x83E0, 0], [255, 0, 0, 255]),
            (4, [0x80FF, 0], [0x8000, 0xFF00], [0, 0, 255, 255]),
        ] {
            let mut v = setup(format);
            for (at, value) in [
                (0x12, 0x4444),
                (0x78, 0),
                (0x7A, 0x100),
                (0x7C, 0),
                (0x7E, 0x100),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            v.vram[..4].copy_from_slice(&[old[0].to_be_bytes(), old[1].to_be_bytes()].concat());
            v.begin_scanline(0);
            v.scan_to(&Vdp1::default(), 0, 1);
            v.vram[..4].copy_from_slice(&[new[0].to_be_bytes(), new[1].to_be_bytes()].concat());
            v.scan_to(&Vdp1::default(), 0, 4);
            for x in 0..4 {
                assert_eq!(pixel(&v, x, 0), color, "format={format}, x={x}");
            }
            v.begin_scanline(1);
            v.scan_to(&Vdp1::default(), 1, 4);
            for x in 0..4 {
                assert_eq!(pixel(&v, x, 1), [0, 255, 0, 255], "format={format}, x={x}");
            }
        }
    }

    #[test]
    fn name_character_cell_and_layer_paths_cannot_replace_each_others_latches() {
        let mut v = Vdp2 {
            beam_rendering: true,
            ..Vdp2::default()
        };
        v.vram[..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(v.display_word(0, 0, 0), 0x1234);
        v.vram[..2].copy_from_slice(&[0xAB, 0xCD]);
        assert_eq!(v.display_word(0, 1, 0), 0xABCD);
        assert_eq!(v.display_word(1, 0, 0), 0xABCD);
        v.vram[..2].fill(0);
        assert_eq!(v.display_word(0, 2, 0), 0);
        assert_eq!(v.display_word(0, 0, 0), 0x1234);
        assert_eq!(v.display_word(0, 1, 0), 0xABCD);
        assert_eq!(v.display_word(1, 0, 0), 0xABCD);
        assert_eq!(v.display_word(0, 0, 2), 0x5678);
        assert_eq!(v.display_word(0, 0, 0x80000), 0x1234);
        v.begin_scanline(0);
        v.beam_rendering = true;
        assert_eq!(v.display_word(0, 0, 0), 0);
    }
}
