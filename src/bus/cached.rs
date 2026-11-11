//! Event-bounded cached CPU bursts, with real buffered-store acceptance.
//! A peer must have a proven private polling period. Interfering bus masters,
//! UBC comparisons and CPU-visible events bound each burst. Register blocks
//! share the interpreter's ALU semantics. Other instructions use ordinary
//! step(). Local clock/cache/prefetch changes remain
//! private until validated. A final store reuses the actual bus protocol.
use super::poll_cycle::PollCycle;
use super::*;
use crate::cache::CachedLines;
use crate::cpu::{Cpu, CpuStatus};

#[derive(Clone)]
struct Local {
    clock: u64,
    lru: [u8; 64],
    retained: Option<(u32, u16)>,
    epochs: u64,
    reads: Option<crate::onchip::ReadSequence>,
    write_len: usize,
    fill_len: usize,
    write_pc: u32,
}
struct PassiveRead<'a> {
    address: u32,
    bytes: [u8; 4],
    little: bool,
    rom: Option<&'a [u8]>,
    sequence: RefCell<crate::onchip::ReadSequence>,
}
impl PassiveRead<'_> {
    fn permits(&self, address: u32) -> bool {
        self.rom.map_or(address == self.address, |rom| {
            (address as usize) < rom.len()
        })
    }
}
struct CycleAnchor {
    cpu: Cpu,
    local: Local,
    ready: u64,
    retired_at: u64,
    retired: u32,
}
/// Keep a proven periodic instruction sequence compact. Sound catch-up needs
/// retirement positions, not a host allocation and PLL conversion per skipped
/// instruction. There is at most one repeated cycle in each bounded burst.
#[derive(Default)]
struct Retirements {
    points: Vec<u64>,
    repeated: Option<(usize, usize, u64, usize)>, // prefix, pattern, period, copies
    last_reference: u32,
}
impl Retirements {
    fn recycle_into(mut self, reuse: &mut Vec<u64>) {
        self.points.clear();
        *reuse = self.points;
    }
    fn len(&self) -> usize {
        self.points.len() + self.repeated.map_or(0, |(_, n, _, copies)| n * copies)
    }
    fn at(&self, index: usize) -> u64 {
        if let Some((prefix, n, period, copies)) = self.repeated
            && index >= prefix
        {
            let relative = index - prefix;
            if relative < n * copies {
                return self.points[prefix - n + relative % n] + period * (1 + relative / n) as u64;
            }
            return self.points[index - n * copies];
        }
        self.points[index]
    }
    fn last(&self) -> u64 {
        self.at(self.len() - 1)
    }
    fn through(&self, clock: u64) -> usize {
        let (mut lo, mut hi) = (0, self.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.at(mid) <= clock {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }
    fn aligned_prefix(&self, limit: usize, origin: u64, period: Option<u64>) -> usize {
        let Some(period) = period else { return limit };
        (0..limit)
            .rev()
            .find(|&i| {
                let clocks = self.at(i) - origin;
                clocks != 0 && clocks.is_multiple_of(period)
            })
            .map_or(0, |i| i + 1)
    }
}
struct WramInstructions<'a> {
    low: &'a [u8],
    high: &'a [u8],
    bus: &'a crate::onchip::OnChip,
}
impl WramInstructions<'_> {
    fn line(&self, address: u32) -> Option<[u8; 16]> {
        let base = address & !15;
        let bytes = if (LOW_WRAM..LOW_WRAM + WRAM_SIZE as u32).contains(&base) {
            self.low
                .get((base - LOW_WRAM) as usize..(base - LOW_WRAM) as usize + 16)?
        } else if (HIGH_WRAM..HIGH_WRAM + WRAM_SIZE as u32).contains(&base) {
            self.high
                .get((base - HIGH_WRAM) as usize..(base - HIGH_WRAM) as usize + 16)?
        } else {
            return None;
        };
        let mut result: [u8; 16] = bytes.try_into().unwrap();
        if self.bus.external_little_endian(base) {
            for word in result.as_chunks_mut::<4>().0 {
                word.reverse();
            }
        }
        Some(result)
    }
}
struct Cached<'a> {
    cache: &'a Cache,
    code: Option<WramInstructions<'a>>,
    fills: RefCell<CachedLines>,
    fill_count: Cell<usize>,
    fill_reads: RefCell<Option<crate::onchip::ReadSequence>>,
    scalar_active: Cell<bool>,
    clock: Cell<u64>,
    lru: RefCell<[u8; 64]>,
    retained: Cell<Option<(u32, u16)>>,
    epochs: Cell<u64>,
    end: Cell<u64>,
    fill_end: u64,
    store: Cell<Option<PlannedStore>>,
    store_ready: u64,
    store_origin: u64,
    passive: Option<PassiveRead<'a>>,
    write_bus: Option<&'a crate::onchip::OnChip>,
    write_protocol: Option<crate::onchip::ReadSequence>,
    idle_vdp1_vram: bool,
    ram_stores: Vec<RamStore>,
    write_bounds: (u32, u32),
    write_state: Option<WriteState>,
    write_pc: Cell<u32>,
}
#[derive(Clone, Copy)]
struct PlannedStore {
    address: u32,
    width: usize,
    value: u32,
    at: u64,
}
#[derive(Clone, Copy)]
struct RamStore {
    store: PlannedStore,
    pc: u32,
    cached: bool,
}
struct WriteState {
    sequence: crate::onchip::ReadSequence,
    before_last: crate::onchip::ReadSequence,
    ready: u64,
}
impl WriteState {
    fn append(&mut self, store: PlannedStore) {
        self.sequence.advance(store.at);
        self.before_last = self.sequence.clone();
        let physical = Bus::physical(store.address);
        self.ready = if vdp1_vram(physical) {
            self.sequence.write_video(physical, store.width, store.at)
        } else {
            self.sequence.write(physical, store.width, store.at)
        };
    }
}
struct RamPlan {
    stores: Vec<RamStore>,
    before_last: crate::onchip::ReadSequence,
}
pub(super) fn ordinary_memory(address: u32) -> bool {
    address < 0x2000_0000
        && (address < BIOS_SIZE as u32
            || (LOW_WRAM..LOW_WRAM + WRAM_SIZE as u32).contains(&address)
            || (HIGH_WRAM..HIGH_WRAM + WRAM_SIZE as u32).contains(&address))
}
impl Cached<'_> {
    fn snapshot(&self) -> Local {
        debug_assert_eq!(self.fill_count.get(), self.fills.borrow().len());
        Local {
            clock: self.clock.get(),
            lru: *self.lru.borrow(),
            retained: self.retained.get(),
            epochs: self.epochs.get(),
            reads: self
                .passive
                .as_ref()
                .map(|p| p.sequence.borrow().clone())
                .or_else(|| {
                    (self.fill_count.get() != 0)
                        .then(|| self.fill_reads.borrow().clone())
                        .flatten()
                }),
            write_len: self.ram_stores.len(),
            fill_len: self.fill_count.get(),
            write_pc: self.write_pc.get(),
        }
    }
    fn restore(&mut self, state: Local) {
        self.store.set(None);
        self.write_pc.set(state.write_pc);
        if self.ram_stores.len() != state.write_len {
            self.ram_stores.truncate(state.write_len);
            self.write_state = None;
            self.write_bounds = (u32::MAX, 0);
            for i in 0..self.ram_stores.len() {
                let ram = self.ram_stores[i];
                self.append_ram_timing(ram.store);
                if ram.cached {
                    self.extend_write_bounds(ram.store);
                }
            }
        }
        self.clock.set(state.clock);
        *self.lru.get_mut() = state.lru;
        self.retained.set(state.retained);
        self.epochs.set(state.epochs);
        self.fills.get_mut().truncate(state.fill_len);
        self.fill_count.set(state.fill_len);
        if let Some(passive) = &mut self.passive {
            *passive.sequence.get_mut() = state.reads.expect("passive protocol snapshot");
        } else {
            *self.fill_reads.get_mut() = state.reads;
        }
    }
    fn into_parts(self) -> (Local, Option<RamPlan>, CachedLines) {
        let reads = match self.passive {
            Some(passive) => Some(passive.sequence.into_inner()),
            None => self.fill_reads.into_inner(),
        };
        let fills = self.fills.into_inner();
        let local = Local {
            clock: self.clock.get(),
            lru: self.lru.into_inner(),
            retained: self.retained.get(),
            epochs: self.epochs.get(),
            reads,
            write_len: self.ram_stores.len(),
            fill_len: fills.len(),
            write_pc: self.write_pc.get(),
        };
        let plan = self.write_state.map(|state| RamPlan {
            stores: self.ram_stores,
            before_last: state.before_last,
        });
        (local, plan, fills)
    }
    fn append_ram_timing(&mut self, store: PlannedStore) {
        let state = self.write_state.get_or_insert_with(|| {
            let mut sequence = self.write_protocol.clone().unwrap_or_else(|| {
                self.write_bus
                    .expect("quiet data bus")
                    .plan_cached_reads(self.store_origin)
            });
            sequence.advance(self.store_ready);
            WriteState {
                before_last: sequence.clone(),
                sequence,
                ready: self.store_ready,
            }
        });
        state.append(store);
    }
    fn extend_write_bounds(&mut self, store: PlannedStore) {
        self.write_bounds.0 = self.write_bounds.0.min(store.address);
        self.write_bounds.1 = self.write_bounds.1.max(store.address + store.width as u32);
    }
    fn overlay<const N: usize>(&self, address: u32, bytes: &mut [u8; N]) {
        if address >= self.write_bounds.1 || address.saturating_add(N as u32) <= self.write_bounds.0
        {
            return;
        }
        for ram in &self.ram_stores {
            if ram.cached {
                let store = ram.store;
                let value = store.value.to_be_bytes();
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let at = address.wrapping_add(i as u32);
                    if (store.address..store.address + store.width as u32).contains(&at) {
                        *byte = value[4 - store.width + (at - store.address) as usize];
                    }
                }
            }
        }
    }
    fn read<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        if let Some(passive) = &self.passive
            && let Some(rom) = passive.rom
            && let Some(bytes) = rom.get(address as usize..(address as usize).saturating_add(N))
            && address.is_multiple_of(N as u32)
        {
            let at = passive
                .sequence
                .borrow_mut()
                .read(address, N, self.clock.get());
            if at > self.end.get() {
                return Err(BusError::Pending);
            }
            self.clock.set(at);
            return Ok(bytes.try_into().unwrap());
        }
        // A forged address-array tag must not bypass peripheral lane/width
        // validation that CpuBus performs even before a cache hit.
        if !ordinary_memory(address) {
            if let Some(passive) = &self.passive
                && passive.rom.is_none()
                && address == passive.address
                && matches!(N, 2 | 4)
            {
                let at =
                    passive
                        .sequence
                        .borrow_mut()
                        .read(address & 0x1FFF_FFFF, N, self.clock.get());
                if at > self.end.get() {
                    return Err(BusError::Pending);
                }
                self.clock.set(at);
                let mut bytes = [0; N];
                bytes.copy_from_slice(&passive.bytes[..N]);
                if passive.little {
                    bytes.reverse();
                }
                return Ok(bytes);
            }
            return Err(BusError::Pending);
        }
        let mut bytes = if self.fill_count.get() == 0 {
            self.cache.cached_only(address, &mut self.lru.borrow_mut())
        } else {
            self.fills
                .borrow()
                .read(self.cache, address, &mut self.lru.borrow_mut())
        }
        .ok_or(BusError::Pending)?;
        if let Some(store) = self.store.get()
            && store.address < 0x2000_0000
            && self.cache.read_hit(store.address)
        {
            let value = store.value.to_be_bytes();
            for (i, byte) in bytes.iter_mut().enumerate() {
                let at = address.wrapping_add(i as u32);
                if (store.address..store.address + store.width as u32).contains(&at) {
                    *byte = value[4 - store.width + (at - store.address) as usize];
                }
            }
        }
        self.overlay(address, &mut bytes);
        Ok(bytes)
    }
    fn opcode(&self, address: u32) -> Option<u16> {
        if let Some((at, word)) = self.retained.get()
            && at == address
        {
            return Some(word);
        }
        if let Some(rom) = self.passive.as_ref().and_then(|p| p.rom) {
            let bytes = rom.get(address as usize..(address as usize).saturating_add(2))?;
            return Some(u16::from_be_bytes(bytes.try_into().unwrap()));
        }
        let bytes = if self.fill_count.get() == 0 {
            self.cache.peek_cached::<2>(address)
        } else {
            self.fills.borrow().peek::<2>(self.cache, address)
        };
        bytes.map(|mut bytes| {
            self.overlay(address, &mut bytes);
            u16::from_be_bytes(bytes)
        })
    }
    fn hit(&self, address: u32) -> bool {
        if self.fill_count.get() == 0 {
            self.cache.read_hit(address)
        } else {
            self.fills.borrow().read_hit(self.cache, address)
        }
    }
    fn readable(&self, address: u32) -> bool {
        self.hit(address)
            || (ordinary_memory(address)
                && self.cache.control() & 5 == 1
                && self.ram_stores.is_empty()
                && self.store.get().is_none()
                && self
                    .code
                    .as_ref()
                    .is_some_and(|code| code.line(address).is_some()))
    }
    fn read_data<const N: usize>(&self, address: u32) -> Result<[u8; N], BusError> {
        match self.read(address) {
            Err(BusError::Pending)
                if ordinary_memory(address) && address.is_multiple_of(N as u32) =>
            {
                self.refill(address, false)?;
                self.read(address)
            }
            result => result,
        }
    }
    fn refill(&self, address: u32, instruction: bool) -> Result<(), BusError> {
        if self.scalar_active.get()
            || self.cache.control() & (if instruction { 3 } else { 5 }) != 1
            || !self.ram_stores.is_empty()
            || self.store.get().is_some()
        {
            return Err(BusError::Pending);
        }
        let code = self.code.as_ref().ok_or(BusError::Pending)?;
        let bytes = code.line(address).ok_or(BusError::Pending)?;
        let mut sequence = self
            .fill_reads
            .borrow()
            .clone()
            .unwrap_or_else(|| code.bus.plan_cached_reads(self.store_origin));
        let at = sequence.read_cache_line(address, self.clock.get());
        if at > self.end.get().min(self.fill_end) {
            return Err(BusError::Pending);
        }
        self.end.set(self.end.get().min(self.fill_end));
        let mut fills = self.fills.borrow_mut();
        fills.fill(self.cache, address, bytes, &self.lru.borrow());
        self.fill_count.set(fills.len());
        *self.fill_reads.borrow_mut() = Some(sequence);
        self.clock.set(at);
        Ok(())
    }
    fn write<const N: usize>(&mut self, address: u32, bytes: [u8; N]) -> Result<(), BusError> {
        if self.fill_count.get() != 0
            || !buffered_memory(address)
            || self.passive.as_ref().is_some_and(|p| p.rom.is_some())
            || !address.is_multiple_of(N as u32)
            || self.store.get().is_some()
            || self
                .passive
                .as_ref()
                .is_some_and(|p| p.sequence.borrow().beats != 0)
        {
            return Err(BusError::Pending);
        }
        let physical = Bus::physical(address);
        let ram = self.write_bus.is_some()
            && (work_ram(physical) || (self.idle_vdp1_vram && vdp1_vram(physical)));
        if (!ram && !self.ram_stores.is_empty()) || self.ram_stores.len() == 16 {
            return Err(BusError::Pending);
        }
        let ready = self
            .write_state
            .as_ref()
            .map_or(self.store_ready, |s| s.ready);
        self.sh2_pipeline_wait(ready.saturating_sub(self.clock.get()) as u32)?;
        let at = self.clock.get();
        // The real write buffer accepts at the end of its one internal state.
        // Only isolated WRAM stores may be followed by another instruction.
        self.sh2_pipeline_wait(1)?;
        self.cache
            .plan_written_replacement(address, &mut self.lru.borrow_mut());
        let store = PlannedStore {
            address,
            width: N,
            value: bytes
                .into_iter()
                .fold(0, |v, byte| v << 8 | u32::from(byte)),
            at,
        };
        if ram {
            self.append_ram_timing(store);
            let cached = address < 0x2000_0000 && self.cache.read_hit(address);
            if cached {
                self.extend_write_bounds(store);
            }
            self.ram_stores.push(RamStore {
                store,
                pc: self.write_pc.get(),
                cached,
            });
        } else {
            self.store.set(Some(store));
        }
        Ok(())
    }
}
impl Memory for Cached<'_> {
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock.get())
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_schedule_memory_access(&self, address: u32, write: bool, _completion: u64) -> bool {
        write || !self.hit(address) || self.passive.as_ref().is_some_and(|p| p.permits(address))
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        if self.clock.get() + u64::from(clocks) > self.end.get() {
            return Err(BusError::Pending);
        }
        self.clock.set(self.clock.get() + u64::from(clocks));
        Ok(())
    }
    fn sh2_speculative_fetch(&self, address: u32) -> bool {
        let physical = Bus::physical(address);
        address & 1 == 0
            && (physical < BIOS_SIZE as u32
                || (LOW_WRAM..LOW_WRAM + WRAM_SIZE as u32).contains(&physical)
                || (HIGH_WRAM..HIGH_WRAM + WRAM_SIZE as u32).contains(&physical))
    }
    fn sh2_discard_prefetch(&self) {
        self.retained.set(None);
    }
    fn instruction_break(&self, pc: u32, after: bool) -> bool {
        if !after {
            self.write_pc.set(pc);
            self.epochs.set(self.epochs.get() + 1);
        }
        false
    }
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        let retained = self.retained.get();
        if let Some((at, word)) = retained
            && at == address
        {
            self.retained.set(None);
            return Ok(word);
        }
        let words: [u8; 4] = match self.read(address & !3) {
            Ok(words) => words,
            Err(BusError::Pending) => {
                self.refill(address & !3, true)?;
                self.read(address & !3)?
            }
            Err(error) => return Err(error),
        };
        if address & 2 == 0 {
            self.retained.set(Some((
                address + 2,
                u16::from_be_bytes([words[2], words[3]]),
            )));
            Ok(u16::from_be_bytes([words[0], words[1]]))
        } else {
            self.retained.set(None);
            Ok(u16::from_be_bytes([words[2], words[3]]))
        }
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.read_data::<1>(a).map(|b| b[0])
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.read_data(a).map(u16::from_be_bytes)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.read_data(a).map(u32::from_be_bytes)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.write(a, [v])
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.write(a, v.to_be_bytes())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.write(a, v.to_be_bytes())
    }
}
fn vdp1_vram(address: u32) -> bool {
    (crate::vdp1::VRAM..crate::vdp1::VRAM + crate::vdp1::VRAM_SIZE as u32).contains(&address)
}
fn work_ram(address: u32) -> bool {
    (LOW_WRAM..LOW_WRAM + WRAM_SIZE as u32).contains(&address)
        || (HIGH_WRAM..HIGH_WRAM + WRAM_SIZE as u32).contains(&address)
}
fn buffered_memory(address: u32) -> bool {
    if address >= 0x4000_0000 {
        return false;
    }
    let physical = Bus::physical(address);
    work_ram(physical)
        || (crate::vdp1::VRAM..crate::vdp1::REGISTERS + 0x18).contains(&physical)
        || (VRAM..VRAM + 0x80000).contains(&physical)
        || (crate::vdp2::CRAM..crate::vdp2::CRAM + 0x1000).contains(&physical)
        || (VDP2..VDP2 + 0x120).contains(&physical)
        || (SCU..SCU + 0x100).contains(&physical)
        || (SCSP..SCSP + 0x1000).contains(&physical)
}
// ISA classification is independent of guest memory. The literal decoders
// generate this table and remain the exhaustive test oracle.
const fn make_opcodes() -> [u8; 65536] {
    let mut table = [0; 65536];
    let mut op = 0;
    while op < table.len() {
        let width = match store_width_literal(op as u16) {
            Some(width) => width as u8,
            None => 0,
        };
        table[op] = candidate_literal(op as u16) as u8 | width << 1;
        op += 1;
    }
    table
}
static OPCODES: [u8; 65536] = make_opcodes();
fn accepted_opcode(op: u16) -> bool {
    OPCODES[usize::from(op)] != 0
}
fn store_width(op: u16) -> Option<usize> {
    let width = OPCODES[usize::from(op)] >> 1;
    (width != 0).then_some(usize::from(width))
}

const fn store_width_literal(op: u16) -> Option<usize> {
    match op >> 12 {
        0 if matches!(op & 15, 4..=6) => Some(1 << (op & 3)),
        1 => Some(4),
        2 if matches!(op & 15, 0..=2 | 4..=6) => Some(1 << (op & 3)),
        4 if matches!(op & 255, 0x02 | 0x12 | 0x22 | 0x03 | 0x13 | 0x23) => Some(4),
        8 if matches!(op >> 8 & 15, 0 | 1) => Some(1 << (op >> 8 & 1)),
        0xC if matches!(op >> 8 & 15, 0..=2) => Some(1 << (op >> 8 & 3)),
        _ => None,
    }
}
const fn candidate_literal(op: u16) -> bool {
    match op >> 12 {
        0 => {
            matches!(
                op,
                0x0009 | 0x0008 | 0x0018 | 0x0019 | 0x0028 | 0x000B | 0x002B
            ) || matches!(
                op & 0xF0FF,
                0x0002 | 0x0012 | 0x0022 | 0x0003 | 0x0023 | 0x000A | 0x001A | 0x002A | 0x0029
            ) || matches!(op & 15, 7 | 0xC..=0xF)
        }
        2 => op & 15 >= 7,
        3 => !matches!(op & 15, 1 | 9),
        4 => matches!(
            op & 255,
            0x00 | 0x01
                | 0x04
                | 0x05
                | 0x08
                | 0x09
                | 0x10
                | 0x11
                | 0x15
                | 0x18
                | 0x19
                | 0x20
                | 0x21
                | 0x24
                | 0x25
                | 0x28
                | 0x29
                | 0x0B
                | 0x2B
                | 0x0A
                | 0x1A
                | 0x2A
                | 0x06
                | 0x16
                | 0x26
                | 0x07
                | 0x17
                | 0x27
        ),
        5..=7 | 9..=0xB | 0xD | 0xE => true,
        8 => matches!(op >> 8, 0x84 | 0x85 | 0x89 | 0x8B | 0x8D | 0x8F),
        0xC => matches!(op >> 8, 0xC4..=0xCB),
        _ => false,
    }
}
fn cached_operand(op: u16, cpu: &Cpu, memory: &Cached<'_>, store_available: bool) -> bool {
    if op & 0xF00F == 0x000F {
        let n = ((op >> 8) & 15) as usize;
        let m = ((op >> 4) & 15) as usize;
        let first = cpu.state().r[n];
        let second = cpu.state().r[m].wrapping_add(if n == m { 4 } else { 0 });
        return [first, second].into_iter().all(|address| {
            ordinary_memory(address) && address.is_multiple_of(4) && memory.readable(address)
        });
    }
    if let Some(width) = store_width(op) {
        return store_available
            && memory.fill_count.get() == 0
            && cpu.cached_store_address(op).is_some_and(|address| {
                buffered_memory(address)
                    && address.is_multiple_of(width as u32)
                    && (op >> 12 != 4 || work_ram(Bus::physical(address)))
            });
    }
    cpu.cached_load_address(op)
        .is_none_or(|address| memory.readable(address))
}
pub(crate) struct CachedBatch {
    pub cpu: Cpu,
    pub ready: u64,
    pub reference: u64,
    pub retired: u32,
}
impl Bus {
    /// Retire one register instruction at the current hardware boundary.
    /// A missing IF must fall back before changing CPU or cache state.
    pub(crate) fn issue_cached_scalar(&mut self, processor: Processor, cpu: &mut Cpu) -> bool {
        let index = processor as usize;
        if !self.onchip[index].cached_batch_allowed() {
            return false;
        }
        let clock = self.main_clocks;
        let count = cpu.cached_scalar_block_with_fetch(clock, 1, |_, address| {
            self.fetch_resident_instruction(processor, address)
        });
        self.onchip[index].count_cached_instructions(u64::from(count));
        count != 0
    }
    pub(super) fn fetch_resident_instruction(
        &mut self,
        processor: Processor,
        address: u32,
    ) -> Option<u16> {
        if !ordinary_memory(address) {
            return None;
        }
        let memory = self.for_cpu(processor);
        memory
            .fetch_instruction_word(address, |a| {
                memory.bus.cache[processor as usize]
                    .borrow_mut()
                    .read(a, true, |_| Err(BusError::Pending))?
                    .ok_or(BusError::Pending)
            })
            .ok()
    }

    fn passive_read<'a>(&self, initial: &Cpu) -> Option<PassiveRead<'a>> {
        if self.cache[0].borrow().control() & 1 == 0
            && initial.state().pc < BIOS_SIZE as u32
            && self.cached_reads_quiet()
        {
            return Some(PassiveRead {
                address: u32::MAX,
                bytes: [0; 4],
                little: false,
                rom: None,
                sequence: RefCell::new(self.onchip[0].plan_cached_reads(self.main_clocks)),
            });
        }
        let op = initial.decoded_opcode()?;
        let address = initial.cached_load_address(op)?;
        // EDSR/LOPR are stable until drawing completion or a field change.
        // Both remain strict horizons; no peer or other bus master intervenes.
        let physical = Bus::physical(address);
        if !(0x2000_0000..0x4000_0000).contains(&address)
            || op >> 12 == 4
            || !self.cached_reads_quiet()
            || self.onchip[0].external_bus_width(physical) < 2
        {
            return None;
        }
        let bytes = if physical == crate::vdp1::REGISTERS + 0x10 {
            let a = self.vdp1.read16(physical).ok()?.to_be_bytes();
            let b = self.vdp1.read16(physical + 2).ok()?.to_be_bytes();
            [a[0], a[1], b[0], b[1]]
        } else {
            return None;
        };
        Some(PassiveRead {
            address,
            bytes,
            rom: None,
            little: self.onchip[0].external_little_endian(Bus::physical(address)),
            sequence: RefCell::new(self.onchip[0].plan_cached_reads(self.main_clocks)),
        })
    }
    pub(crate) fn cached_wait_allowed(&self) -> bool {
        self.onchip[0].cached_batch_allowed()
            && (!self.slave_enabled || self.onchip[1].cached_batch_allowed())
            && !self.scu.dma_pending()
    }
    pub(crate) fn coalesced_main_deadline(&self) -> u32 {
        self.observable_main_deadline()
            .min(self.bus_main_deadline())
    }
    pub(super) fn observable_main_deadline(&self) -> u32 {
        self.observable_deadline_with_onchip(self.onchip[0].clocks_to_event())
    }
    fn observable_deadline_with_onchip(&self, onchip: u32) -> u32 {
        onchip
            .min(if self.slave_enabled {
                self.onchip[1].clocks_to_event()
            } else {
                u32::MAX
            })
            .min(self.interrupt_deadline())
            .min(self.raster.clocks_to_event())
            .min(self.scu.clocks_to_event())
            // Drawing updates the back buffer; rotation/sprite-window reads
            // use the other buffer. VDP1 retains the exceptional deadline for
            // a pixel captured before a swap that still targets the front.
            .min(self.vdp1.clocks_to_observable_event())
    }
    pub(super) fn try_coalesced_wait(
        &mut self,
        requested: u32,
    ) -> Option<Result<(u32, Option<u8>), ScuError>> {
        self.grant_bus_transaction();
        if self.sound_ram_request().is_some() {
            return None;
        }
        let deadline = self
            .cd
            .clocks_to_event()
            .min(self.smpc.clocks_to_event())
            .min(if self.cd.audio_quiet() && !self.cd.interrupt() {
                self.sound.cached_cpu_deadline()
            } else {
                self.sound.clocks_to_event()
            });
        let fixed = self
            .system_clock
            .preview(deadline.saturating_sub(1))
            .saturating_add(self.pending_main_clocks);
        let main = requested
            .min(self.observable_main_deadline().saturating_sub(1))
            .min(self.bus_main_deadline())
            .min(fixed);
        if main == 0 {
            return None;
        }
        Some(
            self.advance_isolated_segment::<false>(main, false, true)
                .map(|r| (r as u32, None)),
        )
    }
    /// No peer CPU or observable device edge intervenes. The display sequencer
    /// and fixed sound microcycles commute with cached work. Buffered data
    /// transfers still publish at their real edges between local segments.
    fn advance_isolated_clocks(
        &mut self,
        main: u32,
        sound_advanced: bool,
        transactions: bool,
    ) -> Result<u64, ScuError> {
        // Buffered data writes may finish inside the interval. Publish each at
        // its real edge before subsequent display/texture acquisition, and
        // recompute VDP1 ownership for the following segment.
        let mut remaining = main;
        let mut reference = 0;
        while remaining != 0 {
            if transactions {
                self.grant_cached_write_continuation();
            }
            let delta = remaining.min(if transactions {
                self.bus_main_deadline().max(1)
            } else {
                u32::MAX
            });
            reference +=
                self.advance_isolated_segment::<false>(delta, sound_advanced, transactions)?;
            remaining -= delta;
        }
        Ok(reference)
    }
    fn advance_readonly_refresh_clocks(
        &mut self,
        main: u32,
        sound_advanced: bool,
    ) -> Result<u64, ScuError> {
        // Admission proved the bus empty with no pending refresh. The prefix
        // before the first BSC event needs neither arbitration nor IRQ polling.
        let first = main.min(self.onchip[0].clocks_to_event());
        let mut reference = self.advance_isolated_segment::<false>(first, sound_advanced, false)?;
        let mut remaining = main - first;
        while remaining != 0 {
            // Refresh requests/grants still run on the actual arbiter. Neither
            // a refresh grant nor completion changes the disabled IRQ source.
            self.grant_bus_transaction();
            let active = self.cached_transfer_active();
            let delta = remaining
                .min(self.onchip[0].clocks_to_event())
                .min(self.bus_main_deadline())
                .max(1);
            reference += self.advance_isolated_segment::<false>(delta, sound_advanced, false)?;
            if active {
                self.advance_bus_transaction(delta)?;
            }
            remaining -= delta;
        }
        Ok(reference)
    }
    fn advance_isolated_segment<const CLOCK_ADVANCED: bool>(
        &mut self,
        main: u32,
        sound_advanced: bool,
        transactions: bool,
    ) -> Result<u64, ScuError> {
        if main == 0 {
            return Ok(0);
        }
        #[cfg(debug_assertions)]
        let interrupt_sources = (!transactions).then(|| {
            [
                self.interrupt_request(Processor::Master),
                self.interrupt_request(Processor::Slave),
            ]
        });
        let reference = self
            .system_clock
            .deadline(main.saturating_sub(self.pending_main_clocks));
        self.pending_main_clocks =
            self.pending_main_clocks + self.system_clock.advance(reference) - main;
        self.onchip[0].advance_to_event(main);
        if self.slave_enabled {
            self.onchip[1].advance_to_event(main);
        }
        self.scu.advance_clocks(main)?;
        let (vram_busy, framebuffer_busy) = self.vdp1_bus_ownership();
        let ended = self
            .vdp1
            .advance_with_bus(main, vram_busy, framebuffer_busy)?;
        assert!(!ended, "cached catch-up crossed VDP1 completion");
        let command = self.advance_fixed_with_sound(reference, sound_advanced)?;
        assert!(command.is_none(), "cached catch-up crossed SMPC completion");
        let mut remaining = main;
        self.acquire_display();
        while remaining != 0 {
            let display = if self.vdp2.display_active() {
                self.raster
                    .clocks_to_display_slot()
                    .min(if self.vdp2.rotation_display_active() {
                        self.raster.clocks_to_rotation_slot()
                    } else {
                        u32::MAX
                    })
            } else {
                u32::MAX
            };
            let delta = remaining.min(display);
            if !CLOCK_ADVANCED {
                self.main_clocks += u64::from(delta);
            }
            assert!(
                self.raster.advance(delta).is_none(),
                "cached catch-up crossed blanking"
            );
            self.acquire_display();
            remaining -= delta;
        }
        let (h, v) = self.raster.counters();
        self.vdp2.counters(h, v);
        self.vdp2.scan_field(self.raster.odd());
        if transactions {
            self.advance_bus_transaction(main)?;
            self.sample_interrupts();
        }
        // With no transfer or observable device edge, neither IRQ input can
        // change. Retain its original recognition start instead of polling
        // the same pins again at every independent catch-up segment.
        #[cfg(debug_assertions)]
        if let Some(before) = interrupt_sources {
            debug_assert_eq!(
                before,
                [
                    self.interrupt_request(Processor::Master),
                    self.interrupt_request(Processor::Slave)
                ]
            );
        }
        Ok(u64::from(reference))
    }
    pub(crate) fn cached_batch(
        &mut self,
        initial: &Cpu,
        count: u32,
        poll: Option<&PollCycle>,
        master_start: u64,
    ) -> Result<Option<CachedBatch>, crate::MachineError> {
        let poll_period = poll.map(|cycle| cycle.period);
        if initial
            .decoded_opcode()
            .is_some_and(|op| !accepted_opcode(op))
        {
            return Ok(None);
        }
        if (self.slave_enabled && poll_period.is_none())
            || !ordinary_memory(initial.state().pc)
            || !self.cached_bus_compatible()
            || !self.onchip[0].cached_batch_allowed()
            || self.scu.dma_pending()
            || self.cpu_dma_errors.iter().any(|&e| e)
            || initial.state().status != CpuStatus::Running
            || initial.state().address_error_pending
            || (self.cache[0].borrow().control() & 1 == 0 && initial.state().pc >= BIOS_SIZE as u32)
        {
            return Ok(None);
        }
        // An accepted buffered write, or refresh, can overlap cache hits.
        // Data writes may finish inside a burst; control/refresh edges remain
        // horizons, and no later cached read bypasses an uncached operand.
        self.grant_cached_transaction();
        if !self.cached_bus_compatible() || !self.onchip[0].cached_batch_allowed() {
            return Ok(None);
        }
        let transfer_active = self.cached_transfer_active();
        let store_window = self.cached_store_window();
        let store_available = store_window.is_some();
        let store_ready = self.main_clocks + u64::from(store_window.as_ref().map_or(0, |w| w.0));
        // Host input can establish a request between frame calls. Start its
        // recognition interval at this boundary, as ordinary issue_cpu does.
        self.sample_interrupts();
        if poll.is_some_and(|cycle| {
            self.recognized_interrupt(Processor::Slave)
                .is_some_and(|irq| u32::from(irq.level) > cycle.mask)
        }) {
            return Ok(None);
        }
        if self
            .recognized_interrupt(Processor::Master)
            .is_some_and(|r| u32::from(r.level) > (initial.state().sr >> 4) & 15)
        {
            return Ok(None);
        }
        // Stay strictly before the next CPU-visible event, including recognition,
        // blanking, refresh, DSP, live sound, CD and SMPC deadlines. Pure display
        // acquisition and provably silent sound still run at their normal edges
        // during board catch-up. Convert fixed-domain deadlines through the
        // current PLL phase, including an already admitted second main tick.
        let mut passive = self.passive_read(initial);
        if let Some(reads) = &mut passive
            && reads.address == u32::MAX
        {
            reads.rom = self.bios.as_deref();
        }
        if self.cache[0].borrow().control() & 1 == 0
            && passive.as_ref().and_then(|p| p.rom).is_none()
        {
            return Ok(None);
        }
        // Uncached IF can compete with a buffered store. Keep every store
        // on the real arbiter instead of modeling a read after its acceptance.
        if passive.as_ref().is_some_and(|p| p.rom.is_some())
            && initial
                .decoded_opcode()
                .is_some_and(|op| store_width(op).is_some())
        {
            return Ok(None);
        }
        // Only already cached instructions/data may cross a refresh request.
        // Do not mix external passive reads, writes or accepted bus work into
        // this interval; each still uses the original strict BSC horizon.
        let readonly_refresh = if passive.is_none()
            && !transfer_active
            && self.cached_readonly_bus_quiet()
            && initial
                .decoded_opcode()
                .is_some_and(|op| store_width(op).is_none())
        {
            self.onchip[0].readonly_refresh_deadline()
        } else {
            None
        };
        // Cache-only work cannot access CD registers. The CD deadline below
        // keeps the whole span before a possible change of audio source.
        let cd_quiet = self.cd.audio_quiet() && !self.cd.interrupt();
        let sound_lookahead =
            self.sound.enabled && cd_quiet && self.sound.cached_lookahead_allowed();
        let track_retirements = sound_lookahead || poll_period.is_some();
        // External operands can span more than the ordinary sound cutoff's
        // retirement gap. A sound checkpoint permits choosing an earlier CPU
        // prefix without publishing future RAM, synthesis or control effects.
        let mut checkpoint_sound = (passive.is_some() || poll_period.is_some()) && sound_lookahead;
        let reference_span = self
            .cd
            .clocks_to_event()
            .min(self.smpc.clocks_to_event())
            .min(if sound_lookahead {
                u32::MAX
            } else if cd_quiet {
                self.sound.cached_cpu_deadline()
            } else {
                self.sound.clocks_to_event()
            })
            .saturating_sub(1);
        let fixed_span = self
            .system_clock
            .preview(reference_span)
            .saturating_add(self.pending_main_clocks);
        let span = self
            .observable_main_deadline()
            .min(if store_available {
                u32::MAX
            } else {
                self.bus_main_deadline()
            })
            .saturating_sub(1)
            .min(fixed_span)
            .min(1024);
        let mut readonly_refresh = readonly_refresh
            .map(|local| {
                self.observable_deadline_with_onchip(local)
                    .saturating_sub(1)
                    .min(fixed_span)
                    .min(1024)
            })
            .filter(|&wider| wider > span);
        let normal_end = self.main_clocks + u64::from(span);
        let span = readonly_refresh.unwrap_or(span);
        if span < 2 {
            return Ok(None);
        }
        let mut end = self.main_clocks + u64::from(span);
        if master_start > end {
            return Ok(None);
        }
        let code = (passive.is_none() && self.cached_reads_quiet()).then_some(WramInstructions {
            low: &self.low,
            high: &self.high,
            bus: &self.onchip[0],
        });
        let cache = self.cache[0].borrow();
        let mut memory = Cached {
            cache: &cache,
            code,
            fills: RefCell::default(),
            fill_count: Cell::new(0),
            fill_reads: RefCell::new(None),
            scalar_active: Cell::new(false),
            clock: Cell::new(master_start),
            lru: RefCell::new(cache.replacement_state()),
            retained: Cell::new(self.instruction_word[0].get()),
            epochs: Cell::new(0),
            end: Cell::new(end),
            fill_end: normal_end,
            store: Cell::new(None),
            store_ready,
            store_origin: self.main_clocks,
            write_bus: (store_window.is_some()
                && self.external_owner.get() == Some(Processor::Master)
                && self.onchip[1].cached_batch_allowed()
                && passive.is_none())
            .then_some(&self.onchip[0]),
            write_protocol: store_window.and_then(|w| w.1),
            idle_vdp1_vram: !self.vdp1.busy(),
            ram_stores: Vec::new(),
            write_bounds: (u32::MAX, 0),
            write_state: None,
            write_pc: Cell::new(0),
            passive,
        };
        let Some(op) = initial
            .decoded_opcode()
            .or_else(|| memory.opcode(initial.state().pc))
        else {
            return Ok(None);
        };
        if (!accepted_opcode(op))
            || !(cached_operand(op, initial, &memory, store_available)
                || memory.passive.as_ref().is_some_and(|p| {
                    initial
                        .cached_load_address(op)
                        .is_some_and(|a| p.permits(a))
                }))
        {
            return Ok(None);
        }
        let origin = memory.snapshot();
        let mut points = Retirements {
            points: std::mem::take(&mut self.cached_retirement_points),
            ..Retirements::default()
        };
        debug_assert!(points.points.is_empty());
        let mut cpu = initial.clone();
        let mut ready = self.main_clocks;
        let mut retired_at = self.main_clocks;
        let mut retired = 0;
        let mut anchor = Some(CycleAnchor {
            cpu: initial.clone(),
            local: origin.clone(),
            ready,
            retired_at,
            retired,
        });
        let mut cycle_attempts = 0;
        let mut skipped = 0;
        let mut store_pc = 0;
        while retired < count && memory.clock.get() < end {
            let Some(op) = cpu
                .decoded_opcode()
                .or_else(|| memory.opcode(cpu.state().pc))
            else {
                break;
            };
            if readonly_refresh.is_some() && store_width(op).is_some() {
                if memory.clock.get() >= normal_end {
                    break;
                }
                // A store discovered before the original refresh horizon can
                // still use the existing mixed read/write path. Narrow both
                // CPU scheduling and bus planning before attempting its edge.
                readonly_refresh = None;
                end = normal_end;
                memory.end.set(end);
            }
            if anchor.as_ref().is_some_and(|a| retired - a.retired > 16) {
                anchor = Some(CycleAnchor {
                    cpu: cpu.clone(),
                    local: memory.snapshot(),
                    ready,
                    retired_at,
                    retired,
                });
            }
            if let Some(previous) = &anchor
                && previous.cpu.state().pc == cpu.state().pc
                && retired > previous.retired
            {
                let instructions = retired - previous.retired;
                let dt = if instructions == 2
                    && op & 0xF0FF == 0x4010
                    && memory.opcode(cpu.state().pc + 2) == Some(0x8BFD)
                {
                    Some(op)
                } else if instructions == 2 && op == 0x8BFD {
                    memory
                        .opcode(cpu.state().pc.wrapping_sub(2))
                        .filter(|op| op & 0xF0FF == 0x4010)
                } else {
                    None
                };
                let countdown = dt.map(|op| ((op >> 8) & 15) as usize);
                let local = memory.snapshot();
                let period = local.clock - previous.local.clock;
                let stable_local = instructions <= 16
                    && period != 0
                    && local.write_len == previous.local.write_len
                    && local.fill_len == previous.local.fill_len
                    && local.lru == previous.local.lru
                    && local.retained == previous.local.retained
                    && match (&local.reads, &previous.local.reads) {
                        (Some(a), Some(b)) => a.repeats(b, period),
                        (None, None) => true,
                        _ => false,
                    }
                    && ready - previous.ready == period
                    && retired_at - previous.retired_at == period;
                if stable_local {
                    let counter_limit = countdown.map_or(u64::MAX, |register| {
                        u64::from(cpu.state().r[register].saturating_sub(1))
                    });
                    let mut repetitions = ((end - local.clock) / period)
                        .min(u64::from((count - retired) / instructions))
                        .min(counter_limit);
                    if sound_lookahead && !checkpoint_sound && repetitions != 0 {
                        let first = previous.retired as usize;
                        let last = retired as usize - 1;
                        let mut gap = points.at(first) + period - points.at(last);
                        for i in first + 1..=last {
                            gap = gap.max(points.at(i) - points.at(i - 1));
                        }
                        if self.system_clock.reference_span_bound(gap as u32) >= 9 {
                            repetitions = 0;
                        }
                    }
                    if repetitions != 0
                        && cpu.repeat_cached_cycle(&previous.cpu, period, repetitions, countdown)
                    {
                        let clocks = period * repetitions;
                        memory.clock.set(memory.clock.get() + clocks);
                        if let (Some(passive), Some(before)) =
                            (&memory.passive, &previous.local.reads)
                        {
                            passive
                                .sequence
                                .borrow_mut()
                                .repeat(before, period, repetitions);
                        }
                        memory.epochs.set(
                            memory.epochs.get()
                                + (memory.epochs.get() - previous.local.epochs) * repetitions,
                        );
                        ready += clocks;
                        retired_at += clocks;
                        let extra = instructions * repetitions as u32;
                        retired += extra;
                        skipped += extra;
                        if track_retirements {
                            assert!(points.repeated.is_none());
                            points.repeated = Some((
                                points.points.len(),
                                instructions as usize,
                                period,
                                repetitions as usize,
                            ));
                            points.last_reference = self.system_clock.deadline(
                                ((points.last() - self.main_clocks) as u32)
                                    .saturating_sub(self.pending_main_clocks),
                            );
                        }
                        anchor = None;
                        if retired == count || memory.clock.get() == end {
                            break;
                        }
                    } else {
                        cycle_attempts += 1;
                        anchor = if cycle_attempts < 2 {
                            Some(CycleAnchor {
                                cpu: cpu.clone(),
                                local: memory.snapshot(),
                                ready,
                                retired_at,
                                retired,
                            })
                        } else {
                            None
                        };
                    }
                } else {
                    cycle_attempts += 1;
                    anchor = if cycle_attempts < 2 {
                        Some(CycleAnchor {
                            cpu: cpu.clone(),
                            local: memory.snapshot(),
                            ready,
                            retired_at,
                            retired,
                        })
                    } else {
                        None
                    };
                }
            }
            // A straight register sequence shares ALU semantics with step(),
            // but needs no per-instruction CPU/local snapshot or pipeline setup.
            let scalar_start = memory.clock.get();
            if memory.passive.as_ref().is_none_or(|p| p.rom.is_none())
                && (checkpoint_sound
                    || !sound_lookahead
                    || self
                        .system_clock
                        .deadline(
                            ((scalar_start - self.main_clocks) as u32)
                                .saturating_sub(self.pending_main_clocks),
                        )
                        .saturating_sub(points.last_reference)
                        < 9)
            {
                memory.scalar_active.set(true);
                let scalar_count = cpu.cached_scalar_block(
                    &mut memory,
                    (count - retired).min((end - scalar_start) as u32),
                );
                memory.scalar_active.set(false);
                if scalar_count != 0 {
                    retired_at = scalar_start + u64::from(scalar_count) - 1;
                    ready = retired_at + 1;
                    memory.clock.set(ready);
                    retired += scalar_count;
                    if track_retirements {
                        points.points.extend(scalar_start..=retired_at);
                        points.last_reference = self.system_clock.deadline(
                            ((retired_at - self.main_clocks) as u32)
                                .saturating_sub(self.pending_main_clocks),
                        );
                    }
                    continue;
                }
            }
            // Prepared register/load plans preserve retained WB/EX and legal
            // delay slots. Prove every actual operand and future IF hit before
            // executing a plan without full CPU/local rollback copies.
            if memory.passive.is_none()
                && let Some(plan) = cpu.prepare_cached_instruction(op, memory.clock.get())
            {
                let future = plan.future;
                let reference = self.system_clock.deadline(
                    ((plan.at - self.main_clocks) as u32).saturating_sub(self.pending_main_clocks),
                );
                if plan
                    .operand
                    .is_none_or(|(a, _)| ordinary_memory(a) && memory.hit(a))
                    && ordinary_memory(future)
                    && (memory.retained.get().is_some_and(|(a, _)| a == future)
                        || memory.hit(future))
                    && plan.at <= memory.end.get()
                    && (checkpoint_sound
                        || !sound_lookahead
                        || reference.saturating_sub(points.last_reference) < 9)
                {
                    let at = plan.at;
                    #[cfg(test)]
                    let loading = plan.operand.is_some();
                    #[cfg(test)]
                    let slot = cpu.state().delay_target.is_some();
                    cpu.execute_cached_instruction(plan, &mut memory);
                    #[cfg(test)]
                    {
                        if slot {
                            self.cached_prepared_slots += 1;
                        }
                        if loading {
                            self.cached_prepared_loads += 1;
                        } else {
                            self.cached_wb_scalars += 1;
                        }
                    }
                    debug_assert_eq!(memory.clock.get(), at);
                    retired_at = at;
                    ready = (cpu.execution_clock().unwrap() + u64::from(cpu.last_clocks())).max(at);
                    memory.clock.set(ready.min(end));
                    retired += 1;
                    if track_retirements {
                        points.points.push(at);
                        points.last_reference = reference;
                    }
                    continue;
                }
            }
            // The reference interpreter supplies all remaining pipeline semantics.
            // Keep failed attempts' cache/prefetch/epoch changes private.
            // Scalar blocks and prepared plans have already proved their own
            // legal opcode, operands, real IFs and observer bounds. Only the
            // general replay path needs this broader admission check.
            if (!accepted_opcode(op))
                || !(cached_operand(op, &cpu, &memory, store_available)
                    || memory
                        .passive
                        .as_ref()
                        .is_some_and(|p| cpu.cached_load_address(op).is_some_and(|a| p.permits(a))))
            {
                break;
            }
            let before = memory.snapshot();
            let started = before.clock;
            let instruction_pc = cpu.state().pc;
            let interrupt_mask = cpu.state().sr & 0xF0;
            let mut branch = None;
            let mut interpreted = None;
            if cpu.has_cached_control(op) {
                let Some(plan) = cpu.prepare_cached_control(&mut memory) else {
                    memory.restore(before);
                    break;
                };
                branch = Some(plan);
            } else {
                let mut next = cpu.clone();
                if !next.step_cached(&mut memory)
                    || next.state().instructions != cpu.state().instructions + 1
                {
                    memory.restore(before);
                    break;
                }
                interpreted = Some(next);
            }
            let at = memory.clock.get();
            if memory.fill_count.get() != 0 {
                end = memory.end.get();
                readonly_refresh = None;
            }
            checkpoint_sound |= sound_lookahead && memory.fill_count.get() != 0;
            if track_retirements {
                let reference = self.system_clock.deadline(
                    ((at - self.main_clocks) as u32).saturating_sub(self.pending_main_clocks),
                );
                if sound_lookahead
                    && !checkpoint_sound
                    && reference.saturating_sub(points.last_reference) >= 9
                {
                    memory.restore(before);
                    break;
                }
                points.points.push(at);
                points.last_reference = reference;
            }
            if let Some(branch) = branch {
                cpu.retire_cached_control(branch);
            } else {
                cpu = interpreted.unwrap();
            }
            retired_at = at;
            ready = (cpu.execution_clock().unwrap_or(started)
                + u64::from(cpu.last_clocks().max(1)))
            .max(memory.clock.get());
            memory.clock.set(ready.min(end));
            retired += 1;
            if memory.store.get().is_some() {
                store_pc = instruction_pc;
                break;
            }
            if cpu.state().address_error_pending {
                break;
            }
            if cpu.state().sr & 0xF0 != interrupt_mask {
                break;
            }
        }
        let mut selected = points.aligned_prefix(retired as usize, self.main_clocks, poll_period);
        if selected == 0 {
            points.recycle_into(&mut self.cached_retirement_points);
            return Ok(None);
        }
        if sound_lookahead {
            self.sound.main_ram_request(None);
            let desired = self.system_clock.deadline(
                ((points.at(selected - 1) - self.main_clocks) as u32)
                    .saturating_sub(self.pending_main_clocks),
            );
            let (advanced, checkpoint) = if checkpoint_sound {
                let (advanced, checkpoint) = self
                    .sound
                    .cached_lookahead_checkpoint(desired)
                    .map_err(ScuError::from)
                    .map_err(crate::MachineError::Scu)?;
                (advanced, Some(checkpoint))
            } else {
                (
                    self.sound
                        .cached_lookahead(desired)
                        .map_err(ScuError::from)
                        .map_err(crate::MachineError::Scu)?,
                    None,
                )
            };
            let through =
                advanced.saturating_add(self.sound.cached_cpu_deadline().saturating_sub(1));
            let main_through = self.main_clocks
                + u64::from(self.pending_main_clocks)
                + u64::from(self.system_clock.preview(through));
            selected = points.aligned_prefix(
                points.through(main_through).min(selected),
                self.main_clocks,
                poll_period,
            );
            if selected == 0
                && let Some(checkpoint) = checkpoint
            {
                self.sound.restore_cached_lookahead(checkpoint);
                points.recycle_into(&mut self.cached_retirement_points);
                return Ok(None);
            }
            let selected_reference = self.system_clock.deadline(
                ((points.at(selected.saturating_sub(1)) - self.main_clocks) as u32)
                    .saturating_sub(self.pending_main_clocks),
            );
            assert!(
                selected > 0 && (selected_reference >= advanced || checkpoint.is_some()),
                "sound control write preceded the next cache-only retirement"
            );
            let restored = selected_reference < advanced;
            let remainder = if restored {
                self.sound
                    .restore_cached_lookahead(checkpoint.expect("long-read sound checkpoint"));
                selected_reference
            } else {
                selected_reference - advanced
            };
            if remainder != 0 || restored || !self.sound.cached_lookahead_settled() {
                self.sound
                    .advance(remainder)
                    .map_err(ScuError::from)
                    .map_err(crate::MachineError::Scu)?;
            }
        }
        if selected < retired as usize {
            // Replay the prevalidated prefix at the shared retirement horizon.
            memory.restore(origin);
            cpu = initial.clone();
            for _ in 0..selected {
                let start = memory.clock.get();
                cpu.step(&mut memory)
                    .expect("prevalidated cache-only prefix");
                retired_at = memory.clock.get();
                ready = (cpu.execution_clock().unwrap_or(start)
                    + u64::from(cpu.last_clocks().max(1)))
                .max(retired_at);
                memory.clock.set(ready.min(end));
            }
            retired = selected as u32;
        }
        points.recycle_into(&mut self.cached_retirement_points);
        let store = memory.store.get();
        let (local, ram_plan, fills) = memory.into_parts();
        debug_assert!(fills.is_empty() || (ram_plan.is_none() && store.is_none()));
        drop(cache);
        self.instruction_word[0].set(local.retained);
        self.onchip[0].count_cached_instructions(local.epochs);
        // Display acquisition and silent sound microcycles cannot observe
        // cached CPU registers or cache hits. The ordinary board scheduler
        // still executes every such edge, including all memory acquisitions.
        let reference = if let Some(plan) = ram_plan {
            self.commit_ram_plan(plan, retired_at, sound_lookahead, transfer_active)
        } else if let Some(store) = store {
            self.commit_planned_store(
                store,
                store_pc,
                retired_at,
                sound_lookahead,
                transfer_active,
            )
        } else if readonly_refresh.is_some() {
            self.advance_readonly_refresh_clocks(
                (retired_at - self.main_clocks) as u32,
                sound_lookahead,
            )
        } else {
            self.advance_isolated_clocks(
                (retired_at - self.main_clocks) as u32,
                sound_lookahead,
                transfer_active,
            )
        }
        .map_err(crate::MachineError::Scu)?;
        if let Some(reads) = local.reads
            && reads.beats != 0
        {
            #[cfg(test)]
            {
                self.cached_read_beats += reads.beats;
            }
            self.commit_cached_read_sequence(reads);
        }
        #[cfg(test)]
        {
            self.cached_code_fills += fills.len() as u64;
        }
        fills.commit(self.cache[0].get_mut());
        self.cache[0].get_mut().commit_replacement_state(local.lru);
        #[cfg(test)]
        {
            self.cached_cycle_skipped += u64::from(skipped);
        }
        #[cfg(not(test))]
        let _ = skipped;
        Ok(Some(CachedBatch {
            cpu,
            ready,
            reference,
            retired,
        }))
    }
    fn publish_ram_store<const N: usize>(&mut self, store: PlannedStore, mut bytes: [u8; N]) {
        self.cache[0].get_mut().written(store.address, bytes);
        if self.onchip[0].external_little_endian(Bus::physical(store.address)) {
            bytes.reverse();
        }
        self.write(store.address, bytes)
            .expect("validated isolated store");
    }
    fn commit_ram_plan(
        &mut self,
        mut plan: RamPlan,
        retired_at: u64,
        sound_advanced: bool,
        transfer_active: bool,
    ) -> Result<u64, ScuError> {
        let last = plan.stores.pop().expect("nonempty WRAM plan");
        let prefix = self.advance_isolated_clocks(
            (last.store.at - self.main_clocks) as u32,
            sound_advanced,
            transfer_active,
        )?;
        // Prior single-entry writes have drained before the last acceptance.
        // No admitted observer can read WRAM during this isolated interval.
        // VDP1 command/texture RAM joins this set only while drawing is idle;
        // blanking/CPU-visible events end the burst before drawing can start.
        // Cache hits and retained IF consumed the private ordered overlay.
        for ram in plan.stores {
            let store = ram.store;
            match store.width {
                1 => self.publish_ram_store(store, [store.value as u8]),
                2 => self.publish_ram_store(store, (store.value as u16).to_be_bytes()),
                _ => self.publish_ram_store(store, store.value.to_be_bytes()),
            }
            #[cfg(test)]
            {
                self.cached_stores += 1;
            }
        }
        self.commit_cached_read_sequence(plan.before_last);
        self.commit_planned_store(last.store, last.pc, retired_at, sound_advanced, false)
            .map(|reference| prefix + reference)
    }
    fn issue_planned_store(&mut self, store: PlannedStore, pc: u32) -> Result<(), BusError> {
        let mut memory = self.cached_store_memory(pc, store.address, store.width, store.value);
        match store.width {
            1 => memory.write8(store.address, store.value as u8),
            2 => memory.write16(store.address, store.value as u16),
            _ => memory.write32(store.address, store.value),
        }
    }
    fn commit_planned_store(
        &mut self,
        store: PlannedStore,
        pc: u32,
        retired_at: u64,
        sound_advanced: bool,
        transfer_active: bool,
    ) -> Result<u64, ScuError> {
        assert!(retired_at > store.at);
        let mut reference = self.advance_isolated_clocks(
            (store.at - self.main_clocks) as u32,
            sound_advanced,
            transfer_active,
        )?;
        let physical = Self::physical(store.address);
        let wram = [LOW_WRAM, HIGH_WRAM].into_iter().any(|base| {
            physical >= base && physical + store.width as u32 <= base + WRAM_SIZE as u32
        });
        if wram && self.cached_reads_quiet() {
            // All peer/DMA/refresh/interrupt and raster events are outside
            // this admitted interval. WRAM has no display or sound reader.
            // Keep each bus acceptance/publication at its absolute edge,
            // then advance the independent devices once over the same span.
            let origin = self.main_clocks;
            assert_eq!(self.issue_planned_store(store, pc), Err(BusError::Pending));
            self.grant_bus_transaction();
            self.main_clocks += 1;
            self.advance_bus_transaction(1)?;
            assert_eq!(self.issue_planned_store(store, pc), Ok(()));
            let mut tail = (retired_at - self.main_clocks) as u32;
            while tail != 0 {
                self.grant_cached_transaction();
                let delta = tail.min(self.bus_main_deadline().max(1));
                self.main_clocks += u64::from(delta);
                self.advance_bus_transaction(delta)?;
                tail -= delta;
            }
            self.finish_cpu_transaction(Processor::Master);
            reference += self.advance_isolated_segment::<true>(
                (retired_at - origin) as u32,
                sound_advanced,
                false,
            )?;
            #[cfg(test)]
            {
                self.cached_stores += 1;
            }
            return Ok(reference);
        }
        // Reuse the actual acceptance, endian/width splitting, cache update,
        // external owner and buffered publication protocol. Only the cached
        // instruction header avoids the replay journal.
        assert_eq!(self.issue_planned_store(store, pc), Err(BusError::Pending));
        self.grant_bus_transaction();
        reference += self.advance_isolated_clocks(1, sound_advanced, true)?;
        assert_eq!(self.issue_planned_store(store, pc), Ok(()));
        let mut tail = (retired_at - store.at - 1) as u32;
        while tail != 0 {
            self.grant_cached_transaction();
            let delta = tail.min(self.bus_main_deadline().max(1));
            reference += self.advance_isolated_clocks(delta, sound_advanced, true)?;
            tail -= delta;
        }
        self.finish_cpu_transaction(Processor::Master);
        #[cfg(test)]
        {
            self.cached_stores += 1;
        }
        Ok(reference)
    }
}

#[cfg(test)]
#[path = "cached_tests.rs"]
mod tests;
