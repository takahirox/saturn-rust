//! ST-097 §§2.1/3.2; ST-210 #1/2/4/15–25 supersedes obsolete register/table descriptions.
use super::ScuError;
use crate::bus::{BusError, Memory};
use std::collections::VecDeque;

pub(super) fn b_bus(address: u32) -> bool {
    (0x05A0_0000..0x05FC_0000).contains(&address)
}
pub(super) fn valid_source(address: u32) -> bool {
    (0x0200_0000..0x0590_0000).contains(&address)
        || (0x05A0_0000..0x05E0_0000).contains(&address)
        || (0x0600_0000..0x0610_0000).contains(&address)
}
pub(super) fn valid_destination(address: u32) -> bool {
    b_bus(address) || (0x0600_0000..0x0610_0000).contains(&address)
}

#[derive(Clone)]
pub(super) struct Channel {
    source: u32,
    destination: u32,
    count: u32,
    add: u32,
    mode: u32,
    enabled: bool,
    requested: bool,
    retrigger: bool,
    active: Option<Transfer>,
    table: Option<Table>,
}
#[derive(Clone)]
struct Table {
    address: u32,
    words: [u32; 3],
    next: usize,
}
impl Table {
    fn new(address: u32, channel: usize) -> Result<Self, ScuError> {
        // ST-210 #25: all three longwords must fit in work RAM-H.
        if address & 3 != 0 || !(0x0600_0000..=0x060F_FFF4).contains(&address) {
            return Err(ScuError::DmaParameters { channel });
        }
        Ok(Self {
            address,
            words: [0; 3],
            next: 0,
        })
    }
    fn fetch(&mut self, memory: &impl Memory) -> Result<Option<Transfer>, ScuError> {
        // Descriptor bus cycles participate in ordinary grant arbitration.
        // Preserve each sampled word across preemption; do not re-read it.
        self.words[self.next] = memory.read32(self.address + self.next as u32 * 4)?;
        self.next += 1;
        if self.next != 3 {
            return Ok(None);
        }
        let count = self.words[0] & 0xF_FFFF;
        let count = if count == 0 { 0x10_0000 } else { count };
        Ok(Some(Transfer {
            source: self.words[2] & 0x07FF_FFFF,
            destination: self.words[1] & 0x07FF_FFFF,
            unread: count,
            unwritten: count,
            table: self.address + 12,
            last: self.words[2] & 0x8000_0000 != 0,
            buffer: VecDeque::with_capacity(8),
            write_unit: 0,
        }))
    }
}
#[derive(Clone)]
struct Transfer {
    source: u32,
    destination: u32,
    unread: u32,
    unwritten: u32,
    table: u32,
    last: bool,
    buffer: VecDeque<u8>,
    // A complete SCU longword is two physical B-bus halfwords. A
    // halfword already belonging to that unit is distinct from a two-byte
    // head/tail, which must use byte cycles (figs.2.1/3.6–3.8).
    write_unit: usize,
}
impl Default for Channel {
    fn default() -> Self {
        Self {
            source: 0,
            destination: 0,
            count: 0,
            add: 0x101,
            mode: 7,
            enabled: false,
            requested: false,
            retrigger: false,
            active: None,
            table: None,
        }
    }
}
impl Channel {
    pub(super) fn read(&self, offset: u32, address: u32) -> Result<u32, BusError> {
        match offset {
            0 => Ok(self.source),
            4 => Ok(self.destination),
            _ => Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
    }
    pub(super) fn write(&mut self, offset: u32, value: u32, address: u32) -> Result<(), BusError> {
        if self.pending() {
            return Err(BusError::UnsupportedDevice { address, width: 4 });
        }
        match offset {
            0 => self.source = value & 0x07FF_FFFF,
            4 => self.destination = value & 0x07FF_FFFF,
            8 => self.count = value & if address & 0x60 == 0 { 0xF_FFFF } else { 0xFFF },
            12 => self.add = value & 0x107,
            16 => {
                self.enabled = value & 0x100 != 0;
                if self.mode & 7 == 7 && value & 1 != 0 && self.enabled {
                    self.requested = true;
                }
            }
            20 => self.mode = value & 0x0101_0107,
            _ => return Err(BusError::UnsupportedDevice { address, width: 4 }),
        }
        Ok(())
    }
    pub(super) fn armed(&self) -> bool {
        (self.enabled && self.mode & 7 != 7) || self.pending()
    }
    pub(super) fn pending(&self) -> bool {
        self.requested || self.active.is_some() || self.table.is_some()
    }
    pub(super) fn indirect(&self) -> bool {
        self.mode & (1 << 24) != 0
    }
    pub(super) fn trigger(&mut self, trigger: u8) {
        if self.enabled && self.mode & 7 == u32::from(trigger) {
            if self.pending() {
                self.retrigger = true;
            } else {
                self.requested = true;
            }
        }
    }
    pub(super) fn cancel(&mut self) {
        self.active = None;
        self.table = None;
        self.requested = false;
        self.retrigger = false;
        self.enabled = false;
    }
    pub(super) fn beat(
        &mut self,
        memory: &mut impl Memory,
        index: usize,
    ) -> Result<bool, ScuError> {
        if let Some(table) = self.table.as_mut() {
            if let Some(transfer) = table.fetch(memory)? {
                self.active = Some(transfer);
                self.table = None;
            }
            return Ok(false);
        }
        if self.active.is_none() {
            self.requested = false;
            if self.indirect() {
                self.table = Some(Table::new(self.destination, index)?);
                return Ok(false);
            }
            let transfer = {
                let count = if self.count != 0 {
                    self.count
                } else if index == 0 {
                    0x10_0000
                } else {
                    0x1000
                };
                Transfer {
                    source: self.source,
                    destination: self.destination,
                    unread: count,
                    unwritten: count,
                    table: 0,
                    last: true,
                    buffer: VecDeque::with_capacity(8),
                    write_unit: 0,
                }
            };
            self.active = Some(transfer);
            return Ok(false);
        }
        let t = self.active.as_mut().unwrap();
        let illegal = || ScuError::DmaParameters { channel: index };
        if (t.unread != 0 && !valid_source(t.source))
            || !valid_destination(t.destination)
            || (self.add & 0x100 == 0 && !(0x0580_0000..0x0590_0000).contains(&t.source))
            || (!b_bus(t.destination) && self.add & 7 != 2)
        {
            return Err(illegal());
        }
        let write_unit = if t.write_unit != 0 {
            t.write_unit
        } else {
            width(t.destination, t.unwritten)
        };
        let write_width = write_unit.min(if b_bus(t.destination) { 2 } else { 4 });
        if t.buffer.len() < write_width && t.unread != 0 {
            let read_width = width(t.source, t.unread);
            if !valid_source(t.source + read_width as u32 - 1) {
                return Err(illegal());
            }
            match read_width {
                4 => t.buffer.extend(memory.read32(t.source)?.to_be_bytes()),
                2 => t.buffer.extend(memory.read16(t.source)?.to_be_bytes()),
                _ => t.buffer.push_back(memory.read8(t.source)?),
            }
            if self.add & 0x100 != 0 {
                t.source = t.source.wrapping_add(read_width as u32);
            }
            t.unread -= read_width as u32;
            return Ok(false);
        }
        if !valid_destination(t.destination + write_width as u32 - 1) {
            return Err(illegal());
        }
        let mut bytes = [0; 4];
        for byte in &mut bytes[..write_width] {
            *byte = t.buffer.pop_front().unwrap();
        }
        match write_width {
            4 => memory.write32(t.destination, u32::from_be_bytes(bytes))?,
            2 => memory.write16(t.destination, u16::from_be_bytes([bytes[0], bytes[1]]))?,
            _ => memory.write8(t.destination, bytes[0])?,
        }
        t.write_unit = write_unit - write_width;
        let stride = match self.add & 7 {
            0 => 0,
            n => 1 << n,
        };
        t.destination = t
            .destination
            .wrapping_add(if b_bus(t.destination) && write_width != 1 {
                stride
            } else {
                write_width as u32
            });
        t.unwritten -= write_width as u32;
        if t.unwritten != 0 {
            return Ok(false);
        }
        let t = self.active.take().unwrap();
        if self.indirect() {
            if !t.last {
                self.table = Some(Table::new(t.table, index)?);
                return Ok(false);
            }
            if self.mode & 0x100 != 0 {
                self.destination = t.table;
            }
        } else {
            if self.mode & 0x10000 != 0 {
                self.source = t.source;
            }
            if self.mode & 0x100 != 0 {
                self.destination = t.destination;
            }
        }
        // External factors remain armed for repeated transfers; GO is a one-shot.
        self.requested = std::mem::take(&mut self.retrigger);
        Ok(true)
    }
}
fn width(address: u32, remaining: u32) -> usize {
    if address & 3 == 0 && remaining >= 4 {
        4
    } else {
        // ST-097 §2.1 fig.2.1: no word-sized fringe, even when aligned.
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct BytePort {
        bytes: BTreeMap<u32, u8>,
        reads: RefCell<Vec<(u32, usize)>>,
        writes: Vec<(u32, usize)>,
    }
    impl BytePort {
        fn sample(&self, address: u32, width: usize) -> u32 {
            self.reads.borrow_mut().push((address, width));
            (0..width).fold(0, |v, i| {
                v << 8 | u32::from(*self.bytes.get(&(address + i as u32)).unwrap_or(&0xEE))
            })
        }
        fn publish(&mut self, address: u32, width: usize, value: u32) {
            self.writes.push((address, width));
            for i in 0..width {
                self.bytes
                    .insert(address + i as u32, (value >> ((width - i - 1) * 8)) as u8);
            }
        }
    }
    impl Memory for BytePort {
        fn read8(&self, a: u32) -> Result<u8, BusError> {
            Ok(self.sample(a, 1) as u8)
        }
        fn read16(&self, a: u32) -> Result<u16, BusError> {
            Ok(self.sample(a, 2) as u16)
        }
        fn read32(&self, a: u32) -> Result<u32, BusError> {
            Ok(self.sample(a, 4))
        }
        fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
            self.publish(a, 1, u32::from(v));
            Ok(())
        }
        fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
            self.publish(a, 2, u32::from(v));
            Ok(())
        }
        fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
            self.publish(a, 4, v);
            Ok(())
        }
    }
    fn byte_fringe_cycles(start: u32, count: u32, halfwords: bool) -> Vec<(u32, usize)> {
        // Independent construction: complete four-byte aligned blocks, and
        // every other byte separately. Never call the production selector.
        let end = start + count;
        let full_start = (start + 3) & !3;
        let full_end = end & !3;
        let mut cycles = Vec::new();
        for address in start..end {
            if address >= full_start && address < full_end {
                if address & 3 == 0 {
                    if halfwords {
                        cycles.extend([(address, 2), (address + 2, 2)]);
                    } else {
                        cycles.push((address, 4));
                    }
                }
            } else {
                cycles.push((address, 1));
            }
        }
        cycles
    }
    #[test]
    fn all_channels_direct_and_indirect_use_byte_fringes_for_every_alignment_and_short_count() {
        // ST-097 §2.1 fig.2.1: even a two-byte aligned fringe is byte
        // access, not a word. Equal copied data cannot establish bus width.
        for channel in 0..3 {
            for indirect in [false, true] {
                for source_base in [0x06001000, 0x05a01000] {
                    for (destination_base, dst_halfwords) in
                        [(0x06002000, false), (0x05a02000, true)]
                    {
                        for src_offset in 0..4 {
                            for dst_offset in 0..4 {
                                for count in 1..=19 {
                                    let source = source_base + src_offset;
                                    let destination = destination_base + dst_offset;
                                    let mut c = Channel::default();
                                    let mut p = BytePort::default();
                                    for i in 0..32 {
                                        p.bytes.insert(source_base + i, (i * 7 + 3) as u8);
                                    }
                                    if indirect {
                                        for (a, v) in [
                                            (0x06000000, count),
                                            (0x06000004, destination),
                                            (0x06000008, source | 0x80000000),
                                        ] {
                                            p.publish(a, 4, v);
                                        }
                                        p.writes.clear();
                                    }
                                    let base = 0x05fe0000 + channel as u32 * 32;
                                    for (o, v) in [
                                        (0, source),
                                        (4, if indirect { 0x06000000 } else { destination }),
                                        (8, count),
                                        (12, if dst_halfwords { 0x101 } else { 0x102 }),
                                        (20, 0x00010107 | if indirect { 1 << 24 } else { 0 }),
                                        (16, 0x101),
                                    ] {
                                        c.write(o, v, base + o).unwrap();
                                    }
                                    let mut completed = 0;
                                    for _ in 0..100 {
                                        if !c.pending() {
                                            break;
                                        }
                                        completed += usize::from(c.beat(&mut p, channel).unwrap());
                                    }
                                    assert!(!c.pending());
                                    assert_eq!(completed, 1);
                                    let reads = p.reads.borrow();
                                    assert_eq!(
                                        &reads[if indirect { 3 } else { 0 }..],
                                        byte_fringe_cycles(source, count, false)
                                    );
                                    assert_eq!(
                                        p.writes,
                                        byte_fringe_cycles(destination, count, dst_halfwords)
                                    );
                                    for i in 0..count {
                                        assert_eq!(
                                            p.bytes[&(destination + i)],
                                            p.bytes[&(source + i)]
                                        );
                                    }
                                    assert!(!p.bytes.contains_key(&(destination - 1)));
                                    assert!(!p.bytes.contains_key(&(destination + count)));
                                    if !indirect {
                                        assert_eq!(
                                            (c.source, c.destination),
                                            (source + count, destination + count)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    #[derive(Default)]
    struct Port {
        words: BTreeMap<u32, u32>,
        reads: RefCell<Vec<u32>>,
        writes: Vec<(u32, u32)>,
        fail: Option<u32>,
    }
    impl Memory for Port {
        fn read8(&self, _: u32) -> Result<u8, BusError> {
            panic!("expected a longword")
        }
        fn read16(&self, _: u32) -> Result<u16, BusError> {
            panic!("expected a longword")
        }
        fn read32(&self, address: u32) -> Result<u32, BusError> {
            self.reads.borrow_mut().push(address);
            if self.fail == Some(address) {
                return Err(BusError::Unmapped { address, width: 4 });
            }
            Ok(*self.words.get(&address).unwrap_or(&0))
        }
        fn write8(&mut self, _: u32, _: u8) -> Result<(), BusError> {
            panic!("expected a longword")
        }
        fn write16(&mut self, _: u32, _: u16) -> Result<(), BusError> {
            panic!("expected a longword")
        }
        fn write32(&mut self, address: u32, value: u32) -> Result<(), BusError> {
            self.writes.push((address, value));
            self.words.insert(address, value);
            Ok(())
        }
    }
    fn fixture(table: u32, index: usize, count: u32) -> (Channel, Port) {
        let mut c = Channel::default();
        let a = 0x05FE0000 + index as u32 * 32;
        for (off, value) in [(4, table), (12, 0x102), (20, 0x01000107), (16, 0x101)] {
            c.write(off, value, a + off).unwrap();
        }
        let p = Port {
            words: [
                (table, count),
                (table + 4, 0x06002000),
                (table + 8, 0x86001000),
                (0x06001000, 0x12345678),
            ]
            .into_iter()
            .collect(),
            ..Port::default()
        };
        (c, p)
    }
    #[test]
    fn indirect_descriptor_fetches_one_word_per_grant_and_never_resamples_completed_words() {
        // ST-210 #25, ST-097 §3.2: count/write/read, followed by source
        // and destination transactions. The initial request issues no read.
        for index in 0..3 {
            let (mut c, mut p) = fixture(0x06000000, index, 4);
            assert!(!c.beat(&mut p, index).unwrap());
            assert!(p.reads.borrow().is_empty());
            for (n, at) in [0x06000000, 0x06000004, 0x06000008].into_iter().enumerate() {
                assert!(!c.beat(&mut p, index).unwrap());
                assert_eq!(p.reads.borrow().len(), n + 1);
                assert_eq!(p.reads.borrow()[n], at);
                assert!(p.writes.is_empty());
                if n == 0 {
                    p.words.insert(0x06000000, 8);
                } // already sampled
            }
            assert!(!c.beat(&mut p, index).unwrap());
            assert_eq!(
                *p.reads.borrow(),
                [0x06000000, 0x06000004, 0x06000008, 0x06001000]
            );
            assert!(p.writes.is_empty());
            assert!(c.beat(&mut p, index).unwrap());
            assert_eq!(p.writes, [(0x06002000, 0x12345678)]);
            assert!(!c.pending());
            assert_eq!(c.destination, 0x0600000C);
        }
    }
    #[test]
    fn indirect_table_alignment_and_whole_descriptor_bounds_fail_before_bus_access() {
        for table in [
            0x05FFFFFC, 0x06000001, 0x06000002, 0x06000003, 0x060FFFF8, 0x060FFFFC, 0x06100000,
        ] {
            for index in 0..3 {
                let (mut c, mut p) = fixture(table, index, 4);
                assert_eq!(
                    c.beat(&mut p, index),
                    Err(ScuError::DmaParameters { channel: index })
                );
                assert!(p.reads.borrow().is_empty());
                assert!(p.writes.is_empty());
            }
        }
        let (mut c, mut p) = fixture(0x060FFFF4, 0, 4);
        for _ in 0..5 {
            assert!(!c.beat(&mut p, 0).unwrap());
        }
        assert!(c.beat(&mut p, 0).unwrap());
        assert_eq!(c.destination, 0x06100000); // legal end; no wrapped table fetch
        assert_eq!(
            &p.reads.borrow()[..3],
            &[0x060FFFF4, 0x060FFFF8, 0x060FFFFC]
        );
    }
    #[test]
    fn partial_table_fault_and_cancel_never_publish_a_transfer_or_leak_latched_fields() {
        for failed in 0..3 {
            let (mut c, mut p) = fixture(0x06000000, 0, 4);
            p.fail = Some(0x06000000 + failed * 4);
            c.beat(&mut p, 0).unwrap();
            for _ in 0..failed {
                c.beat(&mut p, 0).unwrap();
            }
            assert!(matches!(c.beat(&mut p, 0), Err(ScuError::Bus(_))));
            assert_eq!(p.reads.borrow().len(), failed as usize + 1);
            assert!(c.active.is_none());
            assert!(p.writes.is_empty());
            c.cancel();
            assert!(!c.pending());
            assert!(c.table.is_none());
        }
    }
    #[test]
    fn every_indirect_channel_masks_twenty_bit_count_and_zero_means_one_megabyte() {
        // ST-210 #25: descriptor count width is twenty bits, including
        // channels whose direct count register is only twelve bits wide.
        for index in 0..3 {
            for (count, expected) in [
                (0, 0x100000),
                (1, 1),
                (0xFFF, 0xFFF),
                (0x1000, 0x1000),
                (0xFFFFF, 0xFFFFF),
                (0x100000, 0x100000),
                (u32::MAX, 0xFFFFF),
            ] {
                let (mut c, mut p) = fixture(0x06000000, index, count);
                for _ in 0..4 {
                    c.beat(&mut p, index).unwrap();
                }
                let t = c.active.as_ref().unwrap();
                assert_eq!((t.unread, t.unwritten), (expected, expected));
                assert_eq!((t.source, t.destination), (0x06001000, 0x06002000));
                assert!(t.last);
            }
        }
    }
}

#[cfg(test)]
mod arbitration_tests {
    use super::*;
    use crate::bus::{Bus, HIGH_WRAM};
    use crate::scu::{SCU, Scu};
    #[test]
    fn higher_channel_preempts_a_partial_table_and_low_channel_retains_sampled_count() {
        let mut s = Scu::default();
        let mut b = Bus::default();
        for (i, v) in [4, HIGH_WRAM + 0x200, 0x80000000 | (HIGH_WRAM + 0x100)]
            .into_iter()
            .enumerate()
        {
            b.write32(HIGH_WRAM + i as u32 * 4, v).unwrap();
        }
        b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
        for (off, v) in [(4, HIGH_WRAM), (12, 0x102), (20, 0x01000107), (16, 0x101)] {
            s.write32(SCU + off, v).unwrap();
        }
        s.advance_dma(&mut b).unwrap(); // request
        s.advance_dma(&mut b).unwrap(); // count
        b.write32(HIGH_WRAM, 8).unwrap(); // later writes cannot replace sampled count
        for (off, v) in [
            (0, HIGH_WRAM + 0x100),
            (4, HIGH_WRAM + 0x300),
            (8, 4),
            (12, 0x102),
            (20, 7),
            (16, 0x101),
        ] {
            s.write32(SCU + 0x40 + off, v).unwrap();
        }
        for _ in 0..3 {
            s.advance_dma(&mut b).unwrap();
            assert_eq!(s.channels[0].table.as_ref().unwrap().next, 1);
        }
        assert_eq!(b.read32(HIGH_WRAM + 0x300), Ok(0x12345678));
        assert_eq!(s.read32(SCU + 0xA4).unwrap(), 0x200);
        for _ in 0..4 {
            s.advance_dma(&mut b).unwrap();
        }
        assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x12345678));
        assert_eq!(b.read32(HIGH_WRAM + 0x204), Ok(0));
        assert_eq!(s.read32(SCU + 0xA4).unwrap(), 0xA00);
    }
}
