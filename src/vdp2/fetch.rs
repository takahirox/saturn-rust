//! ST-058-R2 §3.3 tables3.2–3.5: name, character and cell-scroll
//! accesses use independent display paths. A fetched physical word is held
//! while its packed dots are consumed, independently of later CPU VRAM writes.
use super::Vdp2;
use std::cell::Cell;

pub(super) type NormalSample = (usize, usize, usize, u16, u16);

type Words = [Option<(usize, u16)>; 2];

// Rotation and standalone decoder tests keep their two-word serializers.
// Clocked normal backgrounds have separate acquisition cursors, retained
// physical transfers and per-dot operands, so scanout cannot fill a miss.
#[derive(Default)]
pub(crate) struct DisplayFetch {
    words: [[Words; 3]; 5],
    #[cfg(test)]
    pub trace: Vec<DisplayRead>,
    pub pixel: usize,
    pub rotation: Cell<bool>,
    scheduled: bool,
    pub prepared_line: Option<usize>,
    planning: bool,
    request: Option<(usize, usize)>,
    pixels: Vec<[[Option<Acquired>; 3]; 4]>,
    samples: Vec<[Option<NormalSample>; 4]>,
    cursor: [[usize; 3]; 4],
    retained: [[Option<Acquired>; 3]; 4],
    edge: Option<(u64, u16, u32)>,
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
struct Acquired {
    address: usize,
    bytes: usize,
    words: [u16; 2],
}
impl Acquired {
    fn word(self, address: usize) -> Option<u16> {
        let offset = address.wrapping_sub(self.address) & 0x7FFFF;
        (offset < self.bytes).then(|| self.words[offset / 2])
    }
}

impl Vdp2 {
    /// Only the hardware scheduler may fill the normal-screen serializers.
    /// Snapshot decoding and rotation keep their existing independent paths.
    pub(super) fn fetch_word(&self, layer: usize, path: usize, address: usize) -> Option<u16> {
        let address = address & 0x7FFFE;
        let mut fetch = self.display_fetch.borrow_mut();
        if !self.beam_rendering
            || !(self.clocked || fetch.scheduled)
            || layer == 4
            || fetch.rotation.get()
        {
            drop(fetch);
            return Some(self.display_word(layer, path, address));
        }
        let x = fetch.pixel;
        if let Some(value) = fetch
            .pixels
            .get(x)
            .and_then(|p| p[layer][path])
            .and_then(|v| v.word(address))
        {
            return Some(value);
        }
        if fetch.planning {
            if let Some(held) = fetch.retained[layer][path]
                && let Some(value) = held.word(address)
            {
                fetch.pixels[x][layer][path] = Some(held);
                return Some(value);
            }
            fetch.request = Some((path, address));
        }
        None
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

    pub(super) fn reconfigure_display(&mut self) {
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
        self.word(0) & 0x8000 != 0 && self.word(0x20) & 15 != 0
    }

    pub(crate) fn clocked() -> Self {
        Self {
            clocked: true,
            ..Self::default()
        }
    }

    fn prepare_display_line(&mut self, field: u64, line: usize) {
        *self.display_fetch.get_mut() = DisplayFetch {
            scheduled: true,
            prepared_line: Some(line),
            ..DisplayFetch::default()
        };
        let y = if self.word(0) & 0xC0 == 0xC0 {
            line * 2 + usize::from(!field.is_multiple_of(2))
        } else {
            line
        };
        self.read_line_tables(y);
    }

    /// One command per physical bank at an actual T0–T7/T0–T3 boundary.
    /// ST-058-R2 §3.3 fig.3.2, tables3.2–3.4 and fig.3.5. The
    /// 32-bit character/table port supplies two retained 16-bit words;
    /// a name access supplies one complete one/two-word entry.
    pub(crate) fn acquire_display(&mut self, field: u64, line: u16, phase: u32, slot: usize) {
        if !self.display_active() || self.validate().is_err() {
            return;
        }
        let edge = (field, line, phase);
        if self.display_fetch.borrow().edge == Some(edge) {
            return;
        }
        if self.display_fetch.borrow().prepared_line != Some(usize::from(line)) {
            self.prepare_display_line(field, usize::from(line));
        }
        let width = self.dimensions().0;
        {
            let fetch = self.display_fetch.get_mut();
            fetch.scheduled = true;
            fetch.edge = Some(edge);
            fetch.pixels.resize(width, [[None; 3]; 4]);
            fetch.samples.resize(width, [None; 4]);
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
                        address,
                        bytes,
                        words: [
                            self.vword(address),
                            if bytes == 4 {
                                self.vword(address + 2)
                            } else {
                                0
                            },
                        ],
                    };
                    #[cfg(test)]
                    self.display_fetch.get_mut().trace.push(DisplayRead {
                        phase,
                        slot,
                        layer,
                        path,
                        address,
                        words: acquired.words,
                    });
                    self.display_fetch.get_mut().retained[layer][path] = Some(acquired);
                }
            }
        }
        self.beam_rendering = false;
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
