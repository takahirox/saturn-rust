//! ST-097 §3.2 pp.62–72, figs.3.24–30/tables3.10–24.
//! A-bus pin timing; CS2 has fixed-cycle timing and external WAIT, rather
//! than the normal/burst wait fields belonging to the other three selects.
use std::cell::Cell;

#[derive(Clone, Copy)]
struct Previous {
    address: u32,
    width: usize,
    write: bool,
    busy: u32,
    precharge: u32,
}
#[derive(Default)]
pub(super) struct Abus {
    config: [u16; 4],
    refresh: u8,
    previous: Cell<Option<Previous>>,
    refresh_remaining: Cell<u32>,
}
impl Abus {
    pub fn width(&self, address: u32) -> Option<usize> {
        Self::select(address).map(|select| if self.config[select] & 1 != 0 { 1 } else { 2 })
    }
    pub fn write(&mut self, offset: u32, value: u32) {
        match offset {
            0xB0 | 0xB4 => {
                let first = if offset == 0xB0 { 0 } else { 2 };
                self.config[first] =
                    (value >> 16) as u16 & if first == 2 { 0xF00D } else { 0xFFFD };
                self.config[first + 1] = value as u16 & 0xFFFD;
                self.previous.set(None);
            }
            0xB8 => self.refresh = value as u8 & 31,
            _ => unreachable!(),
        }
    }
    fn select(address: u32) -> Option<usize> {
        match address {
            0x0200_0000..=0x03FF_FFFF => Some(0),
            0x0400_0000..=0x04FF_FFFF => Some(1),
            0x0500_0000..=0x057F_FFFF => Some(3),
            0x0580_0000..=0x058F_FFFF => Some(2),
            _ => None,
        }
    }
    /// Additional main clocks beyond the board's ordinary bus cycle. The
    /// external/D0 interface uses the main clock (ST-097 §4.1 p.79);
    /// only the DSP execution pipeline runs at phi/2. Burst/preread is valid only at the next
    /// physical address and cannot cross the programmed address boundary.
    pub fn access(&self, address: u32, width: usize, write: bool) -> u32 {
        let Some(select) = Self::select(address) else {
            return 0;
        };
        let config = self.config[select];
        let bus_width = if config & 1 != 0 { 1 } else { 2 };
        let prior = self.previous.get();
        let mut waits = self
            .refresh_remaining
            .get()
            .saturating_sub(prior.map_or(0, |p| p.busy))
            .max(prior.map_or(0, |p| p.precharge));
        let mut previous_address = prior
            .filter(|p| !p.write && !write)
            .map(|p| (p.address, p.width));
        for offset in (0..width).step_by(bus_width) {
            let at = address + offset as u32;
            let burst_length = match config >> 2 & 3 {
                1 => 4,
                2 => 256,
                _ => 0,
            };
            let sequential = previous_address.is_some_and(|(before, size)| {
                Self::select(before) == Some(select)
                    && before.wrapping_add(size as u32) == at
                    && (burst_length == 0 || before / burst_length == at / burst_length)
            });
            let burst = config & 12 != 0 && sequential;
            let preread = config & 0x8000 != 0 && sequential;
            // CS2's bits27–20 are reserved. The CD block supplies its ready
            // signal; it has no programmable normal/burst waits here.
            let programmed = if select == 2 || preread {
                0
            } else if burst {
                config >> 8 & 15
            } else {
                config >> 4 & 15
            };
            waits += u32::from(programmed);
            previous_address = (!write).then_some((at, bus_width.min(width - offset)));
        }
        let precharge = config & if write { 0x4000 } else { 0x2000 } != 0;
        self.previous.set(Some(Previous {
            address: address + (width - bus_width.min(width)) as u32,
            width: bus_width.min(width),
            write,
            busy: prior.map_or(0, |p| p.busy) + 2 + waits,
            precharge: u32::from(precharge),
        }));
        waits
    }
    /// Refresh input from the master BSC. No cartridge RAM is populated in
    /// the base console, but the bus pulse still occupies its programmed time.
    pub fn request_refresh(&self) {
        if self.refresh & 16 != 0 {
            let preceding = self.previous.get().map_or(0, |p| p.busy);
            self.refresh_remaining.set(
                self.refresh_remaining
                    .get()
                    .max(preceding + 2 + u32::from(self.refresh & 15)),
            );
            self.previous.set(None);
        }
    }
    pub fn advance(&self, clocks: u32) {
        if let Some(mut previous) = self.previous.get() {
            previous.precharge = previous
                .precharge
                .saturating_sub(clocks.saturating_sub(previous.busy));
            previous.busy = previous.busy.saturating_sub(clocks);
            self.previous.set(Some(previous));
        }
        self.refresh_remaining
            .set(self.refresh_remaining.get().saturating_sub(clocks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_select_fields_waits_widths_burst_boundaries_and_preread() {
        for (select, base) in [(0, 0x02000000), (1, 0x04000000), (3, 0x05000000)] {
            for normal in 0..16 {
                for narrow in [0, 1] {
                    let mut a = Abus::default();
                    let word = normal << 4 | narrow;
                    a.write(
                        if select < 2 { 0xB0 } else { 0xB4 },
                        if select == 0 { word << 16 } else { word },
                    );
                    assert_eq!(
                        a.access(base, 4, false),
                        normal * if narrow == 0 { 2 } else { 4 }
                    );
                }
            }
            for (length, boundary) in [(1, 4), (2, 256), (3, u32::MAX)] {
                let mut a = Abus::default();
                let word = 0xA0 | 0x300 | length << 2;
                a.write(
                    if select < 2 { 0xB0 } else { 0xB4 },
                    if select == 0 { word << 16 } else { word },
                );
                assert_eq!(a.access(base, 2, false), 10);
                a.advance(100);
                assert_eq!(a.access(base + 2, 2, false), 3);
                a.advance(100);
                assert_eq!(
                    a.access(base + 4, 2, false),
                    if length == 1 { 10 } else { 3 }
                );
                if boundary != u32::MAX {
                    a.advance(100);
                    a.access(base + boundary - 2, 2, false);
                    a.advance(100);
                    assert_eq!(a.access(base + boundary, 2, false), 10);
                }
                let mut a = Abus::default();
                a.write(
                    if select < 2 { 0xB0 } else { 0xB4 },
                    if select == 0 { 0x80A0 << 16 } else { 0x80A0 },
                );
                assert_eq!(a.access(base, 2, false), 10);
                a.advance(100);
                assert_eq!(a.access(base + 2, 2, false), 0);
                a.advance(100);
                assert_eq!(a.access(base + 4, 2, true), 10); // writes never preread
            }
        }
    }
    #[test]
    fn precharge_and_refresh_use_elapsed_clocks_and_reserved_cd_wait_bits_do_not_exist() {
        for write in [false, true] {
            let mut a = Abus::default();
            a.write(0xB4, 0x60000000);
            assert_eq!(a.access(0x05800000, 2, write), 0);
            assert_eq!(a.access(0x05800004, 2, false), 1);
            a.advance(1);
            assert_eq!(a.access(0x05800008, 2, write), 1);
            a.advance(20);
            assert_eq!(a.access(0x0580000C, 2, write), 0);
        }
        for wait in 0..16 {
            let mut a = Abus::default();
            a.write(0xB8, wait);
            a.request_refresh();
            assert_eq!(a.access(0x02000000, 2, false), 0);
            a.advance(2);
            a.write(0xB8, 16 | wait);
            a.request_refresh();
            a.advance(1);
            assert_eq!(a.access(0x02000000, 2, false), 1 + wait);
            a.advance(100);
            assert_eq!(a.access(0x02000002, 2, false), 0);
        }
    }
}
