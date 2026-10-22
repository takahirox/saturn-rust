//! SH7604 §§7.1.4/7.2/7.5–7.9: keyed BSC, memory protocols and refresh.
//! Register addresses are the longword addresses; word reads use address + 2.
use crate::bus::BusError;
use std::cell::Cell;

pub const BSC: u32 = 0xFFFF_FFE0;
#[derive(Clone, Copy)]
struct Access {
    area: u32,
    read: bool,
    busy: u32,
    gap: u32,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Interface {
    Ordinary,
    Sdram,
    Dram,
    Pseudo,
}
pub(super) struct Bsc {
    registers: [u16; 7],
    slave: bool,
    sdram_mode: Option<u16>,
    observed: Cell<bool>,
    phase: u32,
    access: Cell<Option<Access>>,
    // H §§7.5.6/7.6.5: row state belongs to the memory interface, not
    // to the instruction cache. SDRAM has two independently active banks.
    rows: Cell<[Option<u32>; 2]>,
    burst_words: Cell<u32>,
    // Self-refresh exit recovery only. Normal refresh time belongs to the
    // board's granted transaction, never a countdown started at the request.
    refresh_remaining: Cell<u32>,
    refresh_pending: Cell<bool>,
}
impl Default for Bsc {
    fn default() -> Self {
        Self {
            registers: [0x03F0, 0x00FC, 0xAAFF, 0, 0, 0, 0],
            slave: false,
            sdram_mode: None,
            observed: Cell::new(false),
            phase: 0,
            access: Cell::new(None),
            rows: Cell::new([None; 2]),
            burst_words: Cell::new(0),
            refresh_remaining: Cell::new(0),
            refresh_pending: Cell::new(false),
        }
    }
}
impl Bsc {
    /// H §7.5.8/fig.7.27. A0 is not a word-address pin; the MRS
    /// cycle is explicitly exempt from the CPU address-error checks (table4.6).
    pub fn mode_states(&self, address: u32) -> Result<u32, BusError> {
        let code = (address & !1).wrapping_sub(0xFFFF8000);
        let valid = if self.registers[3] & 0x40 == 0 {
            matches!(code, 0x426 | 0x446 | 0x466)
        } else {
            matches!(code, 0x848 | 0x888 | 0x8C8)
        };
        if !valid {
            return Err(BusError::UnsupportedDevice { address, width: 2 });
        }
        Ok(2 + u32::from(self.registers[3] >> 15))
    }
    pub fn write_mode(&mut self, address: u32) -> Result<(), BusError> {
        self.mode_states(address)?;
        self.sdram_mode = Some((address & !1).wrapping_sub(0xFFFF8000) as u16);
        // PALL precedes MRS even if the value did not change. The next access
        // must activate its row anew, independently in both SDRAM banks.
        self.rows.set([None; 2]);
        self.burst_words.set(0);
        Ok(())
    }
    pub fn set_slave(&mut self, slave: bool) {
        self.slave = slave;
    }
    pub fn read(&self, address: u32, width: usize) -> Result<u32, BusError> {
        let offset = address.wrapping_sub(BSC);
        if offset >= 0x1C {
            return Err(BusError::Unmapped { address, width });
        }
        if offset > 0x1A || !matches!((width, offset & 3), (4, 0) | (2, 2)) {
            return Err(BusError::UnsupportedDevice { address, width });
        }
        let i = (offset / 4) as usize;
        let mut value = self.registers[i];
        if i == 0 && self.slave {
            value |= 0x8000;
        }
        if i == 4 {
            self.observed.set(value & 0x80 != 0);
        }
        Ok(u32::from(value))
    }
    pub fn write(&mut self, address: u32, value: u32) -> Result<(), BusError> {
        let offset = address.wrapping_sub(BSC);
        if offset >= 0x1C {
            return Err(BusError::Unmapped { address, width: 4 });
        }
        if offset > 0x18 || offset & 3 != 0 {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        // Invalid key has no effect, including on the read-before-clear latch.
        if value >> 16 != 0xA55A {
            return Ok(());
        }
        let i = (offset / 4) as usize;
        let mask = [0x1FF7, 0x00FC, 0xFFFF, 0xFEFC, 0x00F8, 0x00FF, 0x00FF][i];
        let mut word = value as u16 & mask;
        // H §7.2.1: 110/111 do not select a memory interface.
        if i == 0 && word & 7 >= 6 {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 1 && [2, 4, 6].iter().any(|shift| word >> shift & 3 == 0) {
            // H §7.2.2: 00 is reserved, not an alternate bus width.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 2 && [8, 10, 12, 14].iter().any(|shift| word >> shift & 3 == 3) {
            // H §7.2.3: IWn=11 is reserved, not three idle cycles.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 3 && word >> 11 & 3 == 3 {
            // H §7.2.4: TRAS=11 is reserved.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 3 {
            let amx = ((word >> 5) & 4) | ((word >> 4) & 3);
            let mode = self.registers[0] & 7;
            if (4..=6).contains(&amx)
                || (matches!(mode, 1 | 4 | 5) && word & 0x40 == 0 && matches!(amx, 1 | 2))
                || (mode == 2 && amx == 7)
                || (word & 12 == 4)
                || (mode == 2 && word & 4 != 0)
            {
                return Err(BusError::UnsupportedDevice { address, width: 4 });
            }
        }
        if i == 2 && matches!(self.registers[0] & 7, 2 | 3) && word >> 6 & 3 == 3 {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        let bcr1 = if i == 0 { word } else { self.registers[0] };
        let bcr2 = if i == 1 { word } else { self.registers[1] };
        if i <= 1 && bcr1 & 0x1000 != 0 && !matches!(bcr1 & 7, 4 | 5) && bcr2 >> 4 & 3 == 1 {
            // H §7.3.2: little-endian mode does not support an 8-bit bus.
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        if i == 4 {
            let flag = self.registers[4] & 0x80;
            // CMF cannot be set by software, and clears only after a flagged read.
            word = (word & !0x80)
                | (flag
                    & if self.observed.replace(false) {
                        word
                    } else {
                        0x80
                    });
        }
        if (i == 0 && (word ^ self.registers[0]) & 7 != 0)
            || (i == 3 && ((word ^ self.registers[3]) & !12 != 0 || word & 12 == 12))
        {
            self.rows.set([None; 2]);
            self.burst_words.set(0);
        }
        if i == 3 && self.registers[3] & 12 == 12 && word & 12 != 12 {
            // H §§7.5.7/7.7.5: leaving self-refresh inhibits commands
            // for a refresh recovery interval. No guest RAM is destroyed.
            self.refresh_remaining
                .set(if matches!(self.registers[0] & 7, 2 | 3) {
                    self.refresh_clocks()
                } else {
                    3 + u32::from(self.registers[3] >> 11 & 3)
                });
        }
        self.registers[i] = word;
        Ok(())
    }
    fn interface(&self, address: u32) -> Interface {
        match (address >> 25, self.registers[0] & 7) {
            (2, 4 | 5) | (3, 1 | 5) => Interface::Sdram,
            (3, 2) => Interface::Dram,
            (3, 3) => Interface::Pseudo,
            _ => Interface::Ordinary,
        }
    }
    fn row(&self, address: u32, sdram: bool) -> (usize, u32) {
        let mcr = self.registers[3];
        let amx = ((mcr >> 5) & 4) | ((mcr >> 4) & 3);
        let long = u32::from(mcr & 0x40 != 0);
        if sdram {
            // H table7.4: device bank is A21/22/23, A19/A18 on a
            // long bus, or A20/A18/A17 on a word bus. Row excludes it.
            let bank_bit = match amx {
                0..=2 => 20 + long + u32::from(amx),
                3 => 18 + long,
                _ => 17 + long,
            };
            let row_shift = if amx < 3 {
                9 + long + u32::from(amx)
            } else {
                9 + long
            };
            (
                ((address >> bank_bit) & 1) as usize,
                (address & !(1 << bank_bit)) >> row_shift,
            )
        } else {
            (0, address >> (9 + long + u32::from(amx & 3)))
        }
    }
    fn refresh_clocks(&self) -> u32 {
        let mcr = self.registers[3];
        let ras = 2 + u32::from(mcr >> 11 & 3);
        let precharge = 1 + u32::from(mcr >> 15);
        // H figs7.26/7.35/7.42: REF recovery, or Tp/Trr/TRAS/Tre.
        if self.registers[0] & 7 == 2 || self.registers[0] & 7 == 3 {
            precharge + ras + 2
        } else {
            precharge + ras + 3
        }
    }
    /// H §§7.2.1/7.2.3/7.4.2, fig.7.11: inserted Tw clocks per
    /// ordinary-space bus transaction, independently for CS0-CS3. These are
    /// additional waits, not a claim about instruction/pipeline base cycles.
    pub fn software_waits(&self, address: u32) -> u32 {
        if address >= 0x0800_0000 {
            return 0;
        }
        let area = address >> 25;
        let select = (self.registers[2] >> (area * 2)) & 3;
        if select != 3 {
            u32::from(select)
        } else {
            let shift = match area {
                0 => 4,
                1 => 6,
                _ => 8,
            };
            3 + u32::from((self.registers[0] >> shift) & 3)
        }
    }
    /// H §7.9, fig.7.47: the preceding READ's area selects IWn.
    /// Writes never require turnaround on their own. An immediate read/write
    /// pair needs at least one cycle even with IWn=0. Existing empty bus cycles
    /// satisfy the requirement. Only actual external beats enter this history.
    #[cfg(test)]
    pub fn access_waits(&self, address: u32, write: bool) -> u32 {
        self.access_timing(address, write, true, true).1
    }
    /// (base states, inserted states) for one physical data beat. `first`
    /// and `last` delimit an operand or an entire cache-line read. SDRAM
    /// always completes its 16-byte burst, including discarded read data.
    pub fn access_timing(&self, address: u32, write: bool, first: bool, last: bool) -> (u32, u32) {
        if address >= 0x0800_0000 {
            return (0, 0);
        }
        let area = address >> 25;
        let prior = self.access.get();
        let idle = prior.map_or(0, |last| {
            if last.read && (last.area != area || write) {
                let specified = u32::from(self.registers[2] >> (8 + last.area * 2) & 3);
                specified.max(u32::from(write)).saturating_sub(last.gap)
            } else {
                0
            }
        });
        let interface = self.interface(address);
        let mcr = self.registers[3];
        let select = u32::from(self.registers[2] >> (area * 2) & 3);
        let trp = 1 + u32::from(mcr >> 15);
        let rcd = 1 + u32::from(mcr >> 14 & 1);
        let mut clocks = match interface {
            Interface::Ordinary => 2 + self.software_waits(address),
            Interface::Sdram => {
                let (bank, row) = self.row(address, true);
                let mut rows = self.rows.get();
                let active = area == 3 && mcr & 0x200 != 0;
                let hit = active && rows[bank] == Some(row);
                let activation = if hit {
                    0
                } else {
                    rcd + if area == 3 && rows[bank].is_some() {
                        trp
                    } else {
                        0
                    }
                };
                if write {
                    if area == 3 {
                        rows[bank] = active.then_some(row);
                        self.rows.set(rows);
                    }
                    activation
                        + 1
                        + if active {
                            0
                        } else {
                            1 + u32::from(mcr >> 13 & 1) + trp
                        }
                } else {
                    let latency = select + 1;
                    let head = if first {
                        self.burst_words.set(0);
                        if area == 3 {
                            rows[bank] = active.then_some(row);
                            self.rows.set(rows);
                        }
                        activation + latency + u32::from(hit && latency == 1)
                    } else {
                        0
                    };
                    let words = self.burst_words.get() + 1;
                    self.burst_words.set(words);
                    let tail = if last {
                        (16 / self.bus_width(address) as u32).saturating_sub(words)
                            + if active {
                                0
                            } else if latency == 1 {
                                trp
                            } else {
                                trp - 1
                            }
                    } else {
                        0
                    };
                    head + 1 + tail
                }
            }
            Interface::Dram | Interface::Pseudo => {
                let (_, row) = self.row(address, false);
                let mut rows = self.rows.get();
                let hit = mcr & 0x400 != 0 && rows[0] == Some(row);
                // H figs7.31/7.34/7.38/7.41: page/static-column
                // cycles omit Tp/Tr; consecutive ordinary cycles overlap Tp.
                let overlap =
                    rows[0].is_none() && prior.is_some_and(|a| a.area == area || a.gap != 0);
                let states = if hit {
                    2 + select
                } else {
                    trp + rcd + 2 + select - u32::from(overlap)
                };
                rows[0] = (mcr & 0x400 != 0
                    && (!last || mcr & 0x200 != 0 && interface == Interface::Dram))
                    .then_some(row);
                self.rows.set(rows);
                states
            }
        };
        let refresh = self
            .refresh_remaining
            .get()
            .saturating_sub(prior.map_or(0, |a| a.busy));
        clocks = clocks.saturating_add(idle).saturating_add(refresh);
        let base = clocks.min(2);
        let waits = clocks - base;
        self.access.set(Some(Access {
            area,
            read: !write,
            // H fig.7.7: ordinary beats occupy T1/T2 plus waits. The
            // interpreter accounts for base cycles separately; keep their bus
            // occupancy here so time spent transferring is never empty time.
            busy: prior.map_or(0, |last| last.busy).saturating_add(clocks),
            gap: 0,
        }));
        (base, waits)
    }
    pub fn extend_access(&self, clocks: u32) {
        if let Some(mut access) = self.access.get() {
            access.busy = access.busy.saturating_add(clocks);
            self.access.set(Some(access));
        }
    }
    pub fn advance_bus(&self, clocks: u32) {
        if let Some(mut last) = self.access.get() {
            let empty = clocks.saturating_sub(last.busy);
            last.busy = last.busy.saturating_sub(clocks);
            last.gap = last.gap.saturating_add(empty).min(2);
            self.access.set(Some(last));
        }
    }
    pub fn cancel_burst(&self) {
        self.burst_words.set(0);
    }
    pub fn reset_bus(&self) {
        self.access.set(None);
        self.rows.set([None; 2]);
        self.burst_words.set(0);
    }
    pub fn release_shared_bus(&self) {
        // A different shared owner supplies no empty cycles on a partial
        // master's private bus. Preserve private turnaround history (§7.10.3).
        if self
            .access
            .get()
            .is_none_or(|last| self.shared_access(last.area << 25))
        {
            self.reset_bus();
        }
    }
    /// H §§7.2.1/7.10.3: partial-share master mode requests ownership only
    /// for CS2. The MD5 slave pin takes precedence over the software PSHR bit.
    pub fn shared_access(&self, address: u32) -> bool {
        self.slave || self.registers[0] & 0x400 == 0 || address >> 25 == 2
    }
    pub fn monitors_external_bus(&self) -> bool {
        self.slave || self.registers[0] & 0x400 == 0
    }
    /// H §§7.2.2/7.3.1: ordinary CS1-CS3 narrow buses divide larger
    /// operand/refill requests into separate external byte/word transactions.
    /// CS0 retains this board model's existing 32-bit external adapter.
    pub fn bus_width(&self, address: u32) -> usize {
        if self.interface(address) != Interface::Ordinary {
            return if self.registers[3] & 0x40 == 0 { 2 } else { 4 };
        }
        match address >> 25 {
            area @ 1..=3 => 1 << (((self.registers[1] >> (area * 2)) & 3) - 1),
            _ => 4,
        }
    }
    pub fn little_endian(&self, address: u32) -> bool {
        address >> 25 == 2 && self.registers[0] & 0x1000 != 0
    }
    pub fn interrupt(&self) -> bool {
        self.registers[4] & 0xC0 == 0xC0
    }
    pub fn can_request_interrupt(&self) -> bool {
        self.registers[4] & 0x40 != 0 && self.registers[4] & 0x38 != 0
    }
    pub fn inhibits_external_bus(&self) -> bool {
        // H §7.5.7: self-refresh holds the bus even against slave requests;
        // exiting it prohibits commands for the recovery interval.
        self.registers[3] & 12 == 12 || self.refresh_remaining.get() != 0
    }
    pub fn clocks_to_event(&self) -> u32 {
        let recovery = match self.refresh_remaining.get() {
            0 => u32::MAX,
            n => n,
        };
        let divisor =
            [0, 4, 16, 64, 256, 1024, 2048, 4096][((self.registers[4] >> 3) & 7) as usize];
        if divisor == 0 {
            return recovery;
        }
        let ticks = self.registers[6].wrapping_sub(self.registers[5]) & 255;
        (u32::from(if ticks == 0 { 256 } else { ticks }) * divisor)
            .saturating_sub(self.phase)
            .max(1)
            .min(recovery)
    }
    /// H §§7.2.7/7.5.7/7.10: a compare latches a request, not an
    /// overlapping refresh cycle. The board arbiter calls this only after
    /// the current owner's indivisible transfer has relinquished the bus.
    pub fn grant_refresh(&self) -> Option<u32> {
        if !self.refresh_pending.replace(false) {
            return None;
        }
        self.reset_bus(); // PALL invalidates both open-bank rows.
        Some(self.refresh_clocks())
    }
    pub fn advance(&mut self, clocks: u32) {
        let select = (self.registers[4] >> 3) as usize & 7;
        let divisor = [0, 4, 16, 64, 256, 1024, 2048, 4096][select];
        if divisor == 0 {
            self.advance_bus(clocks);
            self.refresh_remaining
                .set(self.refresh_remaining.get().saturating_sub(clocks));
            return;
        }
        self.phase %= divisor as u32;
        let mut remaining = clocks;
        while remaining != 0 {
            let delta = remaining.min(divisor as u32 - self.phase);
            self.advance_bus(delta);
            self.refresh_remaining
                .set(self.refresh_remaining.get().saturating_sub(delta));
            self.phase += delta;
            remaining -= delta;
            if self.phase < divisor as u32 {
                continue;
            }
            self.phase = 0;
            // §7.5.7: RTCOR-1 causes refresh on the next count edge.
            self.registers[5] = (self.registers[5] + 1) & 255;
            if self.registers[5] == self.registers[6] {
                self.registers[5] = 0;
                self.registers[4] |= 0x80;
                if !self.slave && self.registers[3] & 12 == 8 && self.registers[0] & 7 != 0 {
                    // A later match replaces the unserved request. Neither
                    // rows nor elapsed bus time change before it is granted.
                    self.refresh_pending.set(true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn burst_rom_and_partial_sharing_are_valid_initializations_with_pin_specific_effects() {
        // H §§7.8/7.10.3: a 32-bit CS0 adapter needs no nibble cycles;
        // PSHR affects master-mode arbitration/monitoring, never the MD5 pin.
        for slave in [false, true] {
            for bits in [0, 0x400, 0x800, 0xC00] {
                let mut b = Bsc::default();
                b.set_slave(slave);
                b.write(BSC, 0xA55A_03F0 | bits).unwrap();
                assert_eq!(
                    b.read(BSC, 4),
                    Ok(0x3F0 | bits | if slave { 0x8000 } else { 0 })
                );
                assert_eq!(b.monitors_external_bus(), slave || bits & 0x400 == 0);
                for area in 0..4 {
                    assert_eq!(
                        b.shared_access(area << 25),
                        slave || bits & 0x400 == 0 || area == 2
                    );
                    assert_eq!(
                        b.access_waits(area << 25, false),
                        if area == 0 { 6 } else { 8 }
                    );
                    assert_eq!(b.bus_width(area << 25), 4);
                }
            }
        }
    }
    #[test]
    fn partial_private_turnaround_survives_other_shared_bus_owners() {
        let mut b = Bsc::default();
        b.write(BSC, 0xA55A_07F0).unwrap();
        b.write(BSC + 8, 0xA55A_AA00).unwrap();
        assert_eq!(b.access_waits(0x0600_0000, false), 0);
        b.release_shared_bus();
        assert_eq!(b.access_waits(0x0600_0000, true), 2);
        assert_eq!(b.access_waits(0x0400_0000, false), 0);
        b.release_shared_bus();
        assert_eq!(b.access_waits(0x0400_0000, true), 0);
        b.reset_bus();
        assert_eq!(b.access_waits(0x0600_0000, true), 0);
    }
    #[test]
    fn keyed_registers_processor_pins_and_word_lanes() {
        for slave in [false, true] {
            let mut b = Bsc::default();
            b.set_slave(slave);
            assert_eq!(
                b.read(BSC, 4).unwrap(),
                0x3F0 | if slave { 0x8000 } else { 0 }
            );
            for (i, reset) in [0x3F0, 0xFC, 0xAAFF, 0, 0, 0, 0].into_iter().enumerate() {
                let a = BSC + i as u32 * 4;
                let pin = if slave && i == 0 { 0x8000 } else { 0 };
                assert_eq!(b.read(a + 2, 2), Ok(reset | pin));
                b.write(a, 0xFFFF).unwrap();
                assert_eq!(b.read(a, 4), Ok(reset | pin));
                assert!(b.read(a, 2).is_err());
                assert!(b.read(a + 1, 1).is_err());
            }
            b.write(BSC, 0xA55A_83F0).unwrap();
            assert_eq!(b.read(BSC, 4), Ok(0x3F0 | if slave { 0x8000 } else { 0 }));
            assert!(b.write(BSC, 0xA55A_FFFF).is_err());
        }
    }
    #[test]
    fn ordinary_software_waits_decode_every_area_and_long_wait_selection() {
        // H §7.2.3: W=00/01/10 -> 0/1/2 Tw; W=11 selects
        // 3/4/5/6 Tw in the appropriate BCR1 AnLW field.
        for area in 0..4 {
            let mut b = Bsc::default();
            let address = area << 25;
            assert_eq!(b.software_waits(address), 6);
            for select in 0..3 {
                b.write(BSC + 8, 0xA55A_AA00 | select << (area * 2))
                    .unwrap();
                assert_eq!(b.software_waits(address), select);
                for other in 0..4 {
                    if other != area {
                        assert_eq!(b.software_waits(other << 25), 0);
                    }
                }
            }
            b.write(BSC + 8, 0xA55A_AAFF).unwrap();
            let shift = [4, 6, 8, 8][area as usize];
            for long in 0..4 {
                let word = (0x3F0 & !(3 << shift)) | long << shift;
                b.write(BSC, 0xA55A_0000 | word).unwrap();
                assert_eq!(b.software_waits(address), 3 + long);
                // Areas 2 and 3 share AHLW, unlike their WCR fields.
                if area >= 2 {
                    assert_eq!(b.software_waits((5 - area) << 25), 3 + long);
                }
            }
            assert_eq!(b.software_waits(0x0800_0000), 0);
        }
    }
    #[test]
    fn reserved_interface_encodings_and_wrong_keys_are_atomic() {
        let mut b = Bsc::default();
        b.write(BSC, 0xA55A_0120).unwrap();
        b.write(BSC + 8, 0xA55A_AA39).unwrap();
        b.write(BSC, 0x1234_03F0).unwrap();
        b.write(BSC + 8, 0x1234_AAFF).unwrap();
        assert_eq!(b.read(BSC, 4), Ok(0x120));
        assert_eq!(b.read(BSC + 8, 4), Ok(0xAA39));
        for (address, value) in [
            (BSC, 0xA55A_0126),      // reserved interface
            (BSC + 4, 0xA55A_00F0),  // reserved bus width
            (BSC + 8, 0xA55A_AB39),  // reserved idle encoding
            (BSC + 12, 0xA55A_1800), // reserved TRAS
        ] {
            assert!(b.write(address, value).is_err());
        }
        assert_eq!(b.read(BSC, 4), Ok(0x120));
        assert_eq!(b.read(BSC + 8, 4), Ok(0xAA39));
    }
    #[test]
    fn turnaround_uses_prior_read_area_minimum_write_gap_and_existing_empty_cycles() {
        // H §§7.2.3/7.9, fig.7.47: independent direction/CS/IW/gap
        // cross-product, with zero software waits to isolate turnaround.
        for prior_area in 0..4 {
            for next_area in 0..4 {
                for prior_write in [false, true] {
                    for next_write in [false, true] {
                        for idle in 0u32..=2 {
                            for empty in 0u32..=4 {
                                let mut b = Bsc::default();
                                let wcr = (0xAA00 & !(3 << (8 + prior_area * 2)))
                                    | idle << (8 + prior_area * 2);
                                b.write(BSC + 8, 0xA55A_0000 | wcr).unwrap();
                                assert_eq!(b.access_waits(prior_area << 25, prior_write), 0);
                                b.advance_bus(2 + empty); // T1/T2, then empty cycles.
                                let required =
                                    if !prior_write && (prior_area != next_area || next_write) {
                                        idle.max(u32::from(next_write))
                                    } else {
                                        0
                                    };
                                assert_eq!(
                                    b.access_waits(next_area << 25, next_write),
                                    required.saturating_sub(empty),
                                    "CS{prior_area} write={prior_write} -> CS{next_area} write={next_write}, IW={idle}, empty={empty}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn software_waits_and_prior_beat_occupancy_are_not_empty_time_and_reset_drops_history() {
        for chunk in [1, 2, 3, 8] {
            let b = Bsc::default();
            assert_eq!(b.access_waits(0, false), 6);
            let mut clocks = 8; // T1/T2 plus six Tw; still no empty cycle.
            while clocks != 0 {
                let n = clocks.min(chunk);
                b.advance_bus(n);
                clocks -= n;
            }
            assert_eq!(b.access_waits(0x0600_0000, false), 8);
            // Two back-to-back beats queue their full occupancy.
            assert_eq!(b.access_waits(0x0600_0004, true), 8);
            b.advance_bus(19); // 10+10 clocks queued; one busy clock remains.
            assert_eq!(b.access_waits(0, false), 6); // write->read, no idle
            b.reset_bus();
            assert_eq!(b.access_waits(0x0600_0000, true), 6);
        }
    }
    #[test]
    fn ordinary_bus_widths_are_independent_and_reserved_selections_do_not_publish() {
        let mut b = Bsc::default();
        for area in 1..=3 {
            for (select, width) in [(1, 1), (2, 2), (3, 4)] {
                b.write(
                    BSC + 4,
                    0xA55A_0000 | (0xFC & !(3 << (area * 2))) | select << (area * 2),
                )
                .unwrap();
                assert_eq!(b.bus_width(area << 25), width);
                for other in 1..=3 {
                    if area != other {
                        assert_eq!(b.bus_width(other << 25), 4);
                    }
                }
            }
            assert!(
                b.write(BSC + 4, 0xA55A_0000 | (0xFC & !(3 << (area * 2))))
                    .is_err()
            );
            assert_eq!(b.read(BSC + 4, 4), Ok(0xFC));
        }
    }
    #[test]
    fn endian_is_cs2_only_and_byte_bus_combinations_are_rejected_in_either_order() {
        let mut b = Bsc::default();
        b.write(BSC, 0xA55A_13F0).unwrap();
        for area in 0..4 {
            assert_eq!(b.little_endian(area << 25), area == 2);
        }
        assert!(b.write(BSC + 4, 0xA55A_00DC).is_err());
        assert_eq!(b.read(BSC + 4, 4), Ok(0xFC));
        b.write(BSC, 0xA55A_03F0).unwrap();
        b.write(BSC + 4, 0xA55A_00DC).unwrap();
        assert!(b.write(BSC, 0xA55A_13F0).is_err());
        assert_eq!(b.read(BSC, 4), Ok(0x3F0));
    }
    #[test]
    fn refresh_compare_interrupt_and_read_before_clear_across_split_clocks() {
        for chunk in [1, 3, 4, 7, 32] {
            let mut b = Bsc::default();
            b.write(BSC + 24, 0xA55A_0002).unwrap();
            b.write(BSC + 16, 0xA55A_0048).unwrap(); // /4, CMIE
            let mut left = 7;
            while left != 0 {
                let n = left.min(chunk);
                b.advance(n);
                left -= n;
            }
            assert!(!b.interrupt());
            b.advance(1);
            assert!(b.interrupt());
            b.write(BSC + 16, 0xA55A_0048).unwrap();
            assert!(b.interrupt()); // not read yet
            assert_eq!(b.read(BSC + 16, 4), Ok(0xC8));
            b.write(BSC + 16, 0x1234_0048).unwrap(); // wrong key cannot consume latch
            b.write(BSC + 16, 0xA55A_0048).unwrap();
            assert!(!b.interrupt());
        }
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;

    fn configured(mode: u32, mcr: u32, wcr: u32) -> Bsc {
        let mut b = Bsc::default();
        b.write(BSC + 8, 0xA55A_0000 | wcr).unwrap();
        b.write(BSC, 0xA55A_0000 | mode).unwrap();
        b.write(BSC + 12, 0xA55A_0000 | mcr).unwrap();
        b
    }
    fn cycle(b: &Bsc, address: u32, write: bool, first: bool, last: bool) -> u32 {
        let (base, waits) = b.access_timing(address, write, first, last);
        base + waits
    }

    #[test]
    fn each_documented_memory_selection_uses_mcr_width_only_in_its_selected_areas() {
        // H §§7.2.1/7.2.2/7.2.4. BCR2 selects ordinary 8-bit buses;
        // MCR SZ overrides it for SDRAM/DRAM/pseudo-SRAM, on either CPU.
        for (mode, special) in [
            (0, [false, false]),
            (1, [false, true]),
            (2, [false, true]),
            (3, [false, true]),
            (4, [true, false]),
            (5, [true, true]),
        ] {
            for (sz, width) in [(0, 2), (0x40, 4)] {
                for slave in [false, true] {
                    let mut b = configured(mode, sz, 0);
                    b.set_slave(slave);
                    b.write(BSC + 4, 0xA55A_0054).unwrap();
                    assert_eq!(b.bus_width(0), 4);
                    assert_eq!(b.bus_width(0x0200_0000), 1);
                    for (area, selected) in [(2, special[0]), (3, special[1])] {
                        assert_eq!(b.bus_width(area << 25), if selected { width } else { 1 });
                    }
                }
            }
        }
    }

    #[test]
    fn sdram_read_latency_and_empty_burst_tail_match_figures_7_15_through_7_17() {
        // Literal totals count Tr, Tc/Tw, Td1..Td4/8 and Tap. A cache
        // fill fetches all data; an uncached byte discards its remaining beats.
        for (sz, beats, totals) in [(0x40, 4, [7, 7, 8, 9]), (0, 8, [11, 11, 12, 13])] {
            for (cas, expected) in totals.into_iter().enumerate() {
                for consumed in [1, beats] {
                    let b = configured(1, sz, (cas as u32) << 6);
                    let actual: u32 = (0..consumed)
                        .map(|i| {
                            cycle(
                                &b,
                                0x0600_0000 + i * (16 / beats),
                                false,
                                i == 0,
                                i + 1 == consumed,
                            )
                        })
                        .sum();
                    assert_eq!(
                        actual,
                        expected,
                        "SZ={sz:X} CL={} read beats={consumed}",
                        cas + 1
                    );
                }
                let b = configured(1, sz | 0xC000, (cas as u32) << 6);
                assert_eq!(cycle(&b, 0x0600_0000, false, true, true), expected + 2);
            }
        }
    }

    #[test]
    fn sdram_banks_hold_independent_rows_and_cs2_is_always_auto_precharged() {
        // H table7.4, figs7.19-7.24: active write=Tr+Tc, hit=Tc,
        // row change=Tp+Tr+Tc. CS2 and other spaces leave CS3 rows alone.
        let b = configured(5, 0x240, 0);
        assert_eq!(cycle(&b, 0x0600_0000, true, true, true), 2);
        assert_eq!(cycle(&b, 0x0600_0004, true, true, true), 1);
        assert_eq!(cycle(&b, 0x0620_0000, true, true, true), 2); // bank 1
        assert_eq!(cycle(&b, 0x0600_0008, true, true, true), 1); // bank 0 retained
        assert_eq!(cycle(&b, 0x0400_0000, true, true, true), 4); // CS2 ignores RASD
        assert_eq!(cycle(&b, 0x0600_000C, true, true, true), 1);
        assert_eq!(cycle(&b, 0x0600_0400, true, true, true), 3); // bank 0, next row
        assert_eq!(cycle(&b, 0x0620_0004, true, true, true), 1);
        b.release_shared_bus();
        assert_eq!(cycle(&b, 0x0620_0008, true, true, true), 2);
        // A read hit with CL=1 inserts Tnop for the DQM pipeline.
        assert_eq!(cycle(&b, 0x0620_000C, false, true, true), 6);
    }

    #[test]
    fn dram_page_and_pseudo_static_column_modes_observe_group_and_row_boundaries() {
        // H figs7.31/7.34/7.38/7.41. Normal Tp/Tr/Tc1/Tc2=4;
        // a consecutive closed-row cycle overlaps Tp. A page hit has Tc1/Tc2.
        for mode in [2, 3] {
            let b = configured(mode, 0x440, 0);
            assert_eq!(cycle(&b, 0x0600_0000, true, true, false), 4);
            assert_eq!(cycle(&b, 0x0600_0004, true, false, true), 2);
            assert_eq!(cycle(&b, 0x0600_0008, true, true, true), 3);
            let b = configured(mode, 0xC440, 0x80);
            assert_eq!(cycle(&b, 0x0600_0000, true, true, false), 8);
            assert_eq!(cycle(&b, 0x0600_0004, true, false, true), 4);
        }
        let b = configured(2, 0x640, 0);
        assert_eq!(cycle(&b, 0x0600_0000, true, true, true), 4);
        assert_eq!(cycle(&b, 0x0600_0004, true, true, true), 2);
        assert_eq!(cycle(&b, 0x0600_0400, true, true, true), 4);
        assert_eq!(cycle(&b, 0x0600_0404, true, true, true), 2);
    }

    #[test]
    fn refresh_owns_real_bus_states_closes_rows_and_slave_never_drives_refresh() {
        // H §7.5.7/fig7.25: Tp + Trr + (TRAS+2) = six clocks.
        for slave in [false, true] {
            let mut b = configured(1, 0x248, 0);
            b.set_slave(slave);
            assert_eq!(cycle(&b, 0x0600_0000, true, true, true), 2);
            b.advance(2);
            b.write(BSC + 24, 0xA55A_0001).unwrap();
            b.write(BSC + 16, 0xA55A_0008).unwrap(); // /4
            b.advance(4);
            assert_eq!(b.read(BSC + 16, 4).unwrap(), 0x88);
            b.write(BSC + 16, 0xA55A_0000).unwrap(); // no second refresh
            assert_eq!(b.grant_refresh(), if slave { None } else { Some(6) });
            b.advance(if slave { 0 } else { 6 });
            assert_eq!(
                cycle(&b, 0x0600_0004, true, true, true),
                if slave { 1 } else { 2 }
            );
        }
        // Compare during a busy transfer leaves one request, even if the
        // interval is shorter than the unfinished external transaction.
        let mut b = configured(1, 0x48, 0);
        b.write(BSC + 24, 0xA55A_0001).unwrap();
        b.write(BSC + 16, 0xA55A_0008).unwrap();
        assert_eq!(cycle(&b, 0x0600_0000, false, true, true), 7);
        assert_eq!(cycle(&b, 0x0600_0004, false, true, true), 7);
        b.advance(4);
        assert!(b.refresh_pending.get());
        b.advance(4);
        assert!(b.refresh_pending.get());
        b.write(BSC + 16, 0xA55A_0000).unwrap();
        b.advance(100); // An ungranted request never expires with time.
        assert_eq!(b.grant_refresh(), Some(6));
        assert_eq!(b.grant_refresh(), None); // one pending request
        b.advance(6);
        assert_eq!(cycle(&b, 0x0600_0000, false, true, true), 7);
    }

    #[test]
    fn self_refresh_suppresses_timer_requests_and_exit_has_documented_recovery() {
        for (mode, mcr, expected) in [(1, 0x4c, 3), (3, 0x4c, 5)] {
            let mut b = configured(mode, mcr, 0);
            b.write(BSC + 24, 0xA55A_0001).unwrap();
            b.write(BSC + 16, 0xA55A_0008).unwrap();
            b.advance(64);
            assert_eq!(b.refresh_remaining.get(), 0);
            b.write(BSC + 16, 0xA55A_0000).unwrap();
            b.write(BSC + 12, 0xA55A_0048).unwrap();
            assert_eq!(b.refresh_remaining.get(), expected);
            b.advance(expected);
            assert_eq!(b.refresh_remaining.get(), 0);
        }
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;
    #[test]
    fn all_six_documented_mrs_addresses_latch_address_ignore_data_and_precharge_both_banks() {
        for slave in [false, true] {
            for (size, codes) in [(0, [0x426, 0x446, 0x466]), (0x40, [0x848, 0x888, 0x8C8])] {
                for trp in [0, 0x8000] {
                    let mut b = Bsc::default();
                    b.set_slave(slave);
                    b.write(BSC, 0xA55A0001).unwrap();
                    b.write(BSC + 8, 0xA55A0000).unwrap();
                    b.write(BSC + 12, 0xA55A0200 | size | trp).unwrap();
                    for code in codes {
                        for a0 in [0, 1] {
                            b.access_timing(0x06000000, true, true, true);
                            assert!(b.rows.get().iter().any(Option::is_some));
                            let address = 0xFFFF8000 | code | a0;
                            assert_eq!(b.mode_states(address), Ok(2 + (trp >> 15)));
                            b.write_mode(address).unwrap();
                            assert_eq!(b.sdram_mode, Some(code as u16));
                            assert_eq!(b.rows.get(), [None; 2]);
                            assert_eq!(b.burst_words.get(), 0);
                        }
                    }
                    let before = b.sdram_mode;
                    assert!(b.write_mode(0xFFFF8000).is_err());
                    assert_eq!(b.sdram_mode, before);
                }
            }
        }
    }
}
