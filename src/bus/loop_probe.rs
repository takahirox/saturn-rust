//! Verified resident master-loop templates and diagnostic controls.
#[cfg(any(test, feature = "kernel-experiment"))]
use super::Memory as _;
use crate::{Cpu, Machine, MachineError};
use std::collections::{BTreeMap, BTreeSet};
mod learning;
pub(super) mod trace;
pub(super) type Mismatches = std::cell::RefCell<BTreeMap<[u32; 5], u64>>;

pub(super) struct Record {
    pub ordinal: u32,
    pub at: u64,
    pub cpu: Cpu,
    pub cache: Vec<u8>,
    pub retained: Option<(u32, u16)>,
    pub stores: u64,
    pub gaps: u64,
}
#[derive(Default)]
pub(super) struct LoopProbe {
    pub(super) observing: bool,
    tracing: std::cell::Cell<bool>,
    pub(super) replaying: bool,
    pub(super) learning: bool,
    pub(super) replayed: std::cell::Cell<u64>,
    pub(super) shortened: std::cell::Cell<u64>,
    pub(super) sound_elisions: std::cell::Cell<u64>,
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub(super) refresh_clamps: std::cell::Cell<u64>,
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub(super) longest_replay: std::cell::Cell<u32>,
    retry: u64,
    last_training_count: u64,
    pub(super) diagnosing: bool,
    pub(super) mismatches: Mismatches,
    headers: BTreeSet<u32>,
    origins: BTreeMap<u32, Record>,
    last_instruction: Option<u64>,
    stores: u64,
    stats: [u64; 10],
    periods: BTreeMap<(u32, u32, bool, bool), u64>,
    best: Option<(u32, u64)>,
    capture: std::cell::RefCell<Option<trace::Capture>>,
    pub(super) captured: Option<trace::CapturedLoop>,
}
impl LoopProbe {
    pub(super) fn automatic() -> Self {
        Self {
            replaying: true,
            learning: true,
            ..Self::default()
        }
    }
    pub fn discover(&mut self, pc: u32, op: u16) {
        let displacement = if op >> 12 == 0xA {
            i32::from((op << 4) as i16 >> 4)
        } else if matches!(op >> 8, 0x89 | 0x8B | 0x8D | 0x8F) {
            i32::from(op as i8)
        } else {
            return;
        };
        if displacement < -16 {
            let target = pc.wrapping_add(4).wrapping_add((displacement * 2) as u32);
            if self.replaying && self.headers.len() == 128 && !self.headers.contains(&target) {
                self.headers.clear();
            }
            if self.headers.len() < 128 {
                self.headers.insert(target);
            }
        }
    }
    pub fn is_header(&self, pc: u32) -> bool {
        self.headers.contains(&pc)
    }
    pub fn commit(
        &mut self,
        records: Vec<Record>,
        initial: u64,
        final_instruction: u64,
        selected: u32,
        stores: u64,
        eligible: bool,
    ) {
        if !self.observing {
            return;
        }
        if self.last_instruction.is_some_and(|n| n != initial) {
            // A repeated endpoint remains an observation across unrecorded
            // instructions, but it is not a complete cached-only trace.
            self.stats[6] += 1;
        }
        self.last_instruction = Some(final_instruction);
        if !eligible {
            self.origins.clear();
            self.stats[7] += 1;
            self.stores += stores;
            return;
        }
        for mut record in records.into_iter().filter(|r| r.ordinal <= selected) {
            record.stores += self.stores;
            record.gaps = self.stats[6];
            let pc = record.cpu.state().pc;
            self.stats[0] += 1;
            if let Some(prior) = self.origins.get(&pc)
                && let Some(period) = record.at.checked_sub(prior.at)
                && period != 0
                && record.cpu.state().instructions > prior.cpu.state().instructions
                && record.cpu.same_cached_cycle_origin(&prior.cpu, period)
            {
                self.stats[1] += 1;
                let instructions = record.cpu.state().instructions - prior.cpu.state().instructions;
                let same_cache = record.cache == prior.cache && record.retained == prior.retained;
                let no_gap = record.gaps == prior.gaps;
                *self
                    .periods
                    .entry((instructions as u32, period as u32, same_cache, no_gap))
                    .or_default() += 1;
                if same_cache {
                    if instructions >= 32 && self.best.is_none_or(|(_, n)| instructions > n) {
                        self.best = Some((pc, instructions));
                    }
                    self.stats[2] += 1;
                    self.stats[3] += instructions;
                    self.stats[4] += record.stores - prior.stores;
                    self.stats[5] += period;
                    if no_gap {
                        self.stats[8] += 1;
                        self.stats[9] += instructions;
                    }
                }
            }
            self.origins.insert(pc, record);
        }
        self.stores += stores;
    }
}
impl super::Bus {
    pub(crate) fn has_master_loop_phase(&self, pc: u32) -> bool {
        self.loop_probe.as_ref().is_some_and(|probe| {
            probe.replaying
                && probe.captured.as_ref().is_some_and(|data| {
                    data.complete && !data.compiled.is_empty() && data.by_pc.contains_key(&pc)
                })
        })
    }
}

impl Machine {
    pub fn loop_state_hashes(&self) -> Vec<u32> {
        fn digest(bytes: impl IntoIterator<Item = u8>) -> [u32; 2] {
            let hash = bytes.into_iter().fold(0xCBF29CE484222325u64, |h, b| {
                (h ^ u64::from(b)).wrapping_mul(0x100000001B3)
            });
            [hash as u32, (hash >> 32) as u32]
        }
        let mut hashes = Vec::new();
        for cpu in [&self.cpu, &self.slave] {
            hashes.extend(digest(format!("{cpu:?}").bytes()));
        }
        for cache in &self.bus.cache {
            let cache = cache.borrow();
            let image = cache.loop_probe_snapshot(cache.replacement_state(), |address| {
                cache.peek_cached_touch::<16>(address)
            });
            hashes.extend(digest(image.unwrap_or_default()));
        }
        for index in 0..2 {
            hashes.extend(digest(
                format!("{:?}", self.bus.instruction_word[index].get()).bytes(),
            ));
            hashes.extend(digest(
                self.bus.onchip[index].private_poll_epoch().to_le_bytes(),
            ));
        }
        hashes.extend(digest(self.bus.high.iter().copied()));
        hashes.extend(digest(self.bus.low.iter().copied()));
        hashes.extend(digest(
            self.cpu_ready.iter().flat_map(|at| at.to_le_bytes()),
        ));
        hashes
    }
    pub fn enable_loop_replay(&mut self, enabled: bool) {
        let probe = self.bus.loop_probe.get_or_insert_with(Default::default);
        probe.replaying = enabled;
        probe.learning = enabled;
    }
    pub fn enable_loop_training(&mut self, enabled: bool) {
        self.bus
            .loop_probe
            .get_or_insert_with(Default::default)
            .learning = enabled;
    }
    pub fn loop_replayed_instructions(&self) -> u64 {
        self.bus.loop_probe.as_ref().map_or(0, |p| p.replayed.get())
    }
    pub fn loop_shortened_batches(&self) -> u64 {
        self.bus
            .loop_probe
            .as_ref()
            .map_or(0, |p| p.shortened.get())
    }
    #[cfg(any(test, feature = "kernel-experiment"))]
    pub fn sound_gap_elisions(&self) -> u64 {
        self.bus.cached_sound_gap_elisions
    }
    pub fn loop_sound_elisions(&self) -> u64 {
        self.bus
            .loop_probe
            .as_ref()
            .map_or(0, |p| p.sound_elisions.get())
    }
    pub fn diagnose_loop_replay(&mut self, enabled: bool) {
        self.bus
            .loop_probe
            .get_or_insert_with(Default::default)
            .diagnosing = enabled;
    }
    pub fn loop_replay_mismatches(&self) -> Vec<u32> {
        self.bus.loop_probe.as_ref().map_or_else(Vec::new, |p| {
            p.mismatches
                .borrow()
                .iter()
                .flat_map(|(key, &count)| {
                    key.iter()
                        .copied()
                        .chain([count as u32, (count >> 32) as u32])
                })
                .collect()
        })
    }
    pub fn advance_frame_probe(&mut self) -> Result<(), MachineError> {
        self.bus
            .loop_probe
            .get_or_insert_with(Default::default)
            .observing = true;
        let result = self.advance_frame();
        self.bus.loop_probe.as_mut().unwrap().observing = false;
        result
    }
    pub fn loop_probe_counts(&self) -> Vec<u32> {
        self.bus.loop_probe.as_ref().map_or_else(Vec::new, |probe| {
            probe
                .stats
                .into_iter()
                .chain([probe.headers.len() as u64])
                .flat_map(|n| [n as u32, (n >> 32) as u32])
                .collect()
        })
    }
    pub fn loop_probe_periods(&self) -> Vec<u32> {
        self.bus.loop_probe.as_ref().map_or_else(Vec::new, |probe| {
            probe
                .periods
                .iter()
                .flat_map(|(&(instructions, clocks, cache, no_gap), &n)| {
                    [
                        instructions,
                        clocks,
                        u32::from(cache),
                        u32::from(no_gap),
                        n as u32,
                        (n >> 32) as u32,
                    ]
                })
                .collect()
        })
    }
}

#[cfg(any(test, feature = "kernel-experiment"))]
fn original_machine(changing_cpu: bool, changing_cache: bool, dense_stores: bool) -> Machine {
    let mut words = vec![
        0x60F2,
        if changing_cache { 0x7001 } else { 0x7000 },
        0x2F02,
        0xE000,
        if changing_cpu { 0x7201 } else { 9 },
    ];
    if dense_stores {
        words.insert(3, 0x2F02);
    }
    words.resize(32, 9);
    let displacement = -(words.len() as i32 + 2);
    words.extend([0xA000 | (displacement as u16 & 0xFFF), 9]);
    let slave = [0x60F0, 0xC880, 0x89FC, 0xAFFB, 9];
    let mut machine = Machine::new();
    machine.enable_loop_replay(false);
    let mut bios = vec![0; super::BIOS_SIZE];
    bios[44..48].copy_from_slice(&(super::HIGH_WRAM + 0x400).to_be_bytes());
    machine.bus.load_bios(&bios).unwrap();
    machine
        .bus
        .write16(super::HIGH_WRAM + 0x400, 0x0227)
        .unwrap();
    machine
        .bus
        .write16(super::HIGH_WRAM + 0x402, 0x002B)
        .unwrap();
    machine.bus.write16(super::HIGH_WRAM + 0x404, 9).unwrap();
    machine.bus.set_slave_enabled(true);
    for (owner, pc, code) in [
        (super::Processor::Master, super::HIGH_WRAM, words.as_slice()),
        (
            super::Processor::Slave,
            super::HIGH_WRAM + 0x100,
            slave.as_slice(),
        ),
    ] {
        for (i, op) in code.iter().enumerate() {
            machine.bus.write16(pc + i as u32 * 2, *op).unwrap();
        }
        let mut bus = machine.bus.for_cpu(owner);
        super::Memory::write32(
            &mut bus,
            0xFFFFFFE8,
            if dense_stores { 0xA55A00FF } else { 0xA55A0000 },
        )
        .unwrap();
        super::Memory::write8(&mut bus, crate::cache::CCR, 0x11).unwrap();
        for at in (pc..pc + code.len() as u32 * 2 + 4).step_by(16) {
            super::Memory::fetch16(&bus, at).unwrap();
        }
    }
    machine
        .bus
        .write32(super::HIGH_WRAM + 0x1000, 0x12345678)
        .unwrap();
    super::Memory::read32(
        &machine.bus.for_cpu(super::Processor::Master),
        super::HIGH_WRAM + 0x1000,
    )
    .unwrap();
    for owner in [super::Processor::Master, super::Processor::Slave] {
        machine.bus.cpu_main_delay(owner, 0);
    }
    let (clocks, command) = machine.bus.advance_cpu_clocks(32).unwrap();
    assert!(command.is_none());
    machine.clocks = u64::from(clocks);
    machine.cpu_ready = [machine.bus.main_clocks; 2];
    machine.cpu = Cpu::start(super::HIGH_WRAM, super::HIGH_WRAM + 0x1000);
    machine.slave = Cpu::start(super::HIGH_WRAM + 0x100, crate::onchip::FRT + 1);
    machine
}
#[cfg(any(test, feature = "kernel-experiment"))]
fn relocate_cache_lines(cache: &mut crate::cache::Cache) {
    let control = cache.control();
    assert_eq!(control & 8, 0, "original fixture uses all four ways");
    for set in 0..64u32 {
        let mut tags = [[0; 4]; 4];
        let mut lines = [[[0; 4]; 4]; 4];
        for way in 0..4u32 {
            cache.set_control((control & 0x3F) | (way as u8) << 6);
            tags[way as usize] = cache.array_read(0x60000000 | set << 4).unwrap().unwrap();
            for word in 0..4u32 {
                lines[way as usize][word as usize] = cache
                    .array_read(0xC0000000 | way << 10 | set << 4 | word << 2)
                    .unwrap()
                    .unwrap();
            }
        }
        for source in 0..4u32 {
            let target = (source + 1) % 4;
            cache.set_control((control & 0x3F) | (target as u8) << 6);
            let tag = u32::from_be_bytes(tags[source as usize]);
            assert!(
                cache
                    .array_write(
                        0x60000000 | (tag & 0x1FFFFC04) | set << 4,
                        tags[source as usize],
                    )
                    .unwrap()
            );
            for word in 0..4u32 {
                assert!(
                    cache
                        .array_write(
                            0xC0000000 | target << 10 | set << 4 | word << 2,
                            lines[source as usize][word as usize],
                        )
                        .unwrap()
                );
            }
        }
    }
    cache.set_control(control);
}
// Original control for the refresh-clamped entry with live BSC refresh.
#[cfg(any(test, feature = "kernel-experiment"))]
fn verify_refresh_prefix() -> Vec<u32> {
    let mut automatic = original_machine(false, false, false);
    let mut reference = original_machine(false, false, false);
    for machine in [&mut automatic, &mut reference] {
        // Use the same SDRAM protocol when training and after refresh starts.
        machine.bus.onchip[0]
            .write32(0xFFFFFFE0, 0xA55A0001)
            .unwrap();
        machine.bus.onchip[0]
            .write32(0xFFFFFFEC, 0xA55A0048)
            .unwrap();
    }
    automatic.enable_loop_replay(true);
    for _ in 0..3 {
        automatic.advance_frame().unwrap();
        reference.advance_frame().unwrap();
        assert_eq!(automatic.cpu, reference.cpu);
        assert_eq!(automatic.slave, reference.slave);
        assert_eq!(automatic.cpu_ready, reference.cpu_ready);
        assert_eq!(automatic.clocks, reference.clocks);
        assert_eq!(automatic.sound_state(), reference.sound_state());
        assert_eq!(automatic.bus.high, reference.bus.high);
    }
    assert!(automatic.loop_replayed_instructions() > 0);
    automatic.enable_loop_training(false);
    let before = automatic.loop_replayed_instructions();
    let before_clamps = automatic
        .bus
        .loop_probe
        .as_ref()
        .unwrap()
        .refresh_clamps
        .get();
    for machine in [&mut automatic, &mut reference] {
        machine.bus.onchip[0]
            .write32(0xFFFFFFF8, 0xA55A0010)
            .unwrap();
        machine.bus.onchip[0]
            .write32(0xFFFFFFF4, 0xA55A0000)
            .unwrap();
        machine.bus.onchip[0]
            .write32(0xFFFFFFF0, 0xA55A0008)
            .unwrap();
    }
    for _ in 0..3 {
        automatic.advance_frame().unwrap();
        reference.advance_frame().unwrap();
        assert_eq!(automatic.cpu, reference.cpu, "refresh-clamped Master");
        assert_eq!(automatic.slave, reference.slave);
        assert_eq!(automatic.cpu_ready, reference.cpu_ready);
        assert_eq!(automatic.clocks, reference.clocks);
        assert_eq!(automatic.sound_state(), reference.sound_state());
        assert_eq!(automatic.bus.high, reference.bus.high);
        assert_eq!(automatic.bus.low, reference.bus.low);
        #[cfg(test)]
        for i in 0..2 {
            automatic.bus.cache[i]
                .borrow()
                .assert_same_contents(&reference.bus.cache[i].borrow());
            automatic.bus.onchip[i].assert_cached_read_bus_matches(&reference.bus.onchip[i]);
        }
    }
    let clamps = automatic
        .bus
        .loop_probe
        .as_ref()
        .unwrap()
        .refresh_clamps
        .get()
        - before_clamps;
    assert!(
        clamps > 0,
        "exercise the entry while readonly-refresh mode is available"
    );
    let replayed = automatic.loop_replayed_instructions() - before;
    assert!(
        replayed > 0,
        "reuse the original proof with live refresh enabled"
    );
    vec![replayed as u32, clamps as u32, automatic.clocks as u32]
}

// Original write-only loop: cache data must change at buffer acceptance,
// while physical WRAM can still be waiting for the real external bus.
#[cfg(any(test, feature = "kernel-experiment"))]
fn verify_pending_store_prefix() -> Vec<u32> {
    fn make() -> Machine {
        let mut machine = original_machine(false, false, false);
        let mut words = vec![0xE011, 0x2F02, 0xE022, 0x2F02, 0xE000];
        words.resize(32, 9);
        words.extend([0xAFDE, 9]);
        {
            let mut memory = machine.bus.for_cpu(super::Processor::Master);
            for (i, word) in words.iter().enumerate() {
                super::Memory::write16(&mut memory, super::HIGH_WRAM + i as u32 * 2, *word)
                    .unwrap();
            }
            super::Memory::write32(&mut memory, super::HIGH_WRAM + 0x1000, 0x22).unwrap();
            super::Memory::write32(&mut memory, 0xFFFFFFE8, 0xA55A00FF).unwrap();
        }
        for owner in [super::Processor::Master, super::Processor::Slave] {
            machine.bus.cpu_main_delay(owner, 0);
        }
        assert!(machine.bus.advance_clocks(256).unwrap().is_none());
        machine.clocks += 256;
        machine.cpu_ready = [machine.bus.main_clocks; 2];
        machine.bus.instruction_word[0].set(None);
        machine.cpu = Cpu::start(super::HIGH_WRAM, super::HIGH_WRAM + 0x1000);
        machine
            .cpu
            .prime_fetch(&machine.bus.for_cpu(super::Processor::Master));
        machine.bus.loop_probe.as_mut().unwrap().best = Some((super::HIGH_WRAM, 34));
        machine
    }
    let mut replayed = make();
    let mut reference = make();
    for machine in [&mut replayed, &mut reference] {
        let summary = machine.capture_loop_trace().unwrap();
        assert_eq!(&summary[..2], &[1, 34]);
    }
    replayed.enable_loop_replay(true);
    replayed.enable_loop_training(false);
    let before = replayed.loop_replayed_instructions();
    let retired = replayed.try_cached_batch(2).unwrap();
    assert_eq!(retired, 2);
    assert_eq!(replayed.loop_replayed_instructions() - before, 2);
    assert_eq!(replayed.cpu.state().r[0], 0x11);
    assert_eq!(
        replayed.bus.cache[0]
            .borrow()
            .peek_cached::<4>(super::HIGH_WRAM + 0x1000),
        Some(0x11_u32.to_be_bytes())
    );
    assert!(
        replayed.bus.cached_isolated_write_pending(),
        "write remains buffered after CPU acceptance"
    );
    for _ in 0..2 {
        replayed.advance_frame().unwrap();
        reference.advance_frame().unwrap();
        assert_eq!(replayed.cpu, reference.cpu);
        assert_eq!(replayed.slave, reference.slave);
        assert_eq!(replayed.cpu_ready, reference.cpu_ready);
        assert_eq!(replayed.clocks, reference.clocks);
        assert_eq!(replayed.sound_state(), reference.sound_state());
        assert_eq!(replayed.bus.high, reference.bus.high);
        assert_eq!(replayed.bus.low, reference.bus.low);
        #[cfg(test)]
        for i in 0..2 {
            replayed.bus.cache[i]
                .borrow()
                .assert_same_contents(&reference.bus.cache[i].borrow());
            replayed.bus.onchip[i].assert_cached_read_bus_matches(&reference.bus.onchip[i]);
        }
    }
    vec![
        retired,
        replayed.clocks as u32,
        replayed.cpu.state().instructions as u32,
    ]
}

/// Portable original positive and negative controls for tracing and replay.
#[cfg(any(test, feature = "kernel-experiment"))]
pub fn verify() -> Vec<u32> {
    let mut hash = 2166136261u32;
    for (changing_cpu, changing_cache, dense_stores) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let mut traced = original_machine(changing_cpu, changing_cache, dense_stores);
        let mut normal = original_machine(changing_cpu, changing_cache, dense_stores);
        traced.advance_frame_probe().unwrap();
        normal.advance_frame().unwrap();
        assert_eq!(traced.cpu, normal.cpu);
        assert_eq!(traced.slave, normal.slave);
        assert_eq!(traced.cpu_ready, normal.cpu_ready);
        assert_eq!(traced.clocks, normal.clocks);
        assert_eq!(traced.sound_state(), normal.sound_state());
        let data = traced.debug_read32(super::HIGH_WRAM + 0x1000).unwrap();
        assert_eq!(
            data,
            normal.debug_read32(super::HIGH_WRAM + 0x1000).unwrap()
        );
        assert_eq!(changing_cache, data != 0x12345678);
        let probe = traced.bus.loop_probe.as_ref().unwrap();
        assert!(probe.stats[0] > 0);
        if changing_cpu {
            assert_eq!(probe.stats[1], 0);
            assert_eq!(probe.stats[2], 0);
        } else if changing_cache {
            assert!(probe.stats[1] > 0);
            assert_eq!(probe.stats[2], 0);
        } else {
            assert!(probe.stats[2] > 0);
        }
        #[cfg(test)]
        for i in 0..2 {
            traced.bus.cache[i]
                .borrow()
                .assert_same_contents(&normal.bus.cache[i].borrow());
        }
        if changing_cpu || changing_cache {
            assert_eq!(traced.capture_loop_trace().unwrap(), vec![0]);
            // A proposed header is not a proof: the full collector must
            // reject changing register and changing read-input cycles too.
            traced.bus.loop_probe.as_mut().unwrap().best = Some((super::HIGH_WRAM, 34));
        }
        let summary = traced.capture_loop_trace().unwrap();
        if !changing_cpu && !changing_cache {
            assert_eq!(summary[0], 1, "complete loop trace");
            assert_eq!(summary[1], 34, "literal original loop length");
            assert_eq!(
                summary[3],
                if dense_stores { 2 } else { 1 },
                "literal buffered write count"
            );
            assert_eq!(summary[5], 0, "resident loop performs no physical reads");
            assert_eq!(summary[6], 0, "no unsupported access");
            assert_eq!(summary[7], 0, "all stores retain their first MA request");
            assert_eq!(summary[8], 34, "one instruction observation per word");
            if dense_stores {
                assert!(
                    summary[9] > 0,
                    "capture a first MA request before delayed buffer acceptance"
                );
            } else {
                assert_eq!(
                    summary[9], 0,
                    "store buffer is free before this loop's write"
                );
            }
            assert_eq!(
                summary[11], 3,
                "the first store belongs to the third instruction"
            );
            assert_eq!(
                summary[12],
                if dense_stores { 4 } else { 3 },
                "store ownership survives a blocked acceptance"
            );
            for _ in 0..200_000 {
                if normal.cpu.state().instructions >= traced.cpu.state().instructions {
                    break;
                }
                normal.step().unwrap();
            }
            assert_eq!(normal.cpu, traced.cpu);
            assert_eq!(normal.slave, traced.slave);
            assert_eq!(normal.cpu_ready, traced.cpu_ready);
            assert_eq!(normal.clocks, traced.clocks);
            assert_eq!(normal.sound_state(), traced.sound_state());
            assert_eq!(
                normal.debug_read32(super::HIGH_WRAM + 0x1000).unwrap(),
                traced.debug_read32(super::HIGH_WRAM + 0x1000).unwrap()
            );
            #[cfg(test)]
            for i in 0..2 {
                traced.bus.cache[i]
                    .borrow()
                    .assert_same_contents(&normal.bus.cache[i].borrow());
            }
        } else {
            assert_eq!(
                summary[0], 0,
                "changing inputs must not close a reusable period"
            );
            for _ in 0..200_000 {
                if normal.cpu.state().instructions >= traced.cpu.state().instructions {
                    break;
                }
                normal.step().unwrap();
            }
            assert_eq!(normal.cpu, traced.cpu);
            assert_eq!(normal.slave, traced.slave);
            assert_eq!(normal.cpu_ready, traced.cpu_ready);
            assert_eq!(normal.clocks, traced.clocks);
            assert_eq!(
                normal.debug_read32(super::HIGH_WRAM + 0x1000).unwrap(),
                traced.debug_read32(super::HIGH_WRAM + 0x1000).unwrap()
            );
        }
        if !changing_cpu && !changing_cache {
            let mut automatic = original_machine(false, false, dense_stores);
            let mut reference = original_machine(false, false, dense_stores);
            automatic.enable_loop_replay(true);
            for _ in 0..3 {
                automatic.advance_frame().unwrap();
                reference.advance_frame().unwrap();
                assert_eq!(
                    automatic.cpu, reference.cpu,
                    "private training preserves field endpoint"
                );
                assert_eq!(automatic.slave, reference.slave);
                assert_eq!(automatic.cpu_ready, reference.cpu_ready);
                assert_eq!(automatic.clocks, reference.clocks);
                assert_eq!(automatic.sound_state(), reference.sound_state());
                assert_eq!(automatic.bus.high, reference.bus.high);
                #[cfg(test)]
                for i in 0..2 {
                    automatic.bus.cache[i]
                        .borrow()
                        .assert_same_contents(&reference.bus.cache[i].borrow());
                }
            }
            assert!(
                automatic.loop_replayed_instructions() > 0,
                "normal API learns a private loop"
            );
            let learned = automatic.bus.loop_probe.as_ref().unwrap();
            assert_eq!(
                learned.captured.as_ref().unwrap().phases.len() - 1,
                68,
                "private training observes two literal 34-instruction periods"
            );
            assert!(
                learned.longest_replay.get() > 34,
                "publish a proven prefix across the first observed period end"
            );
            hash = (hash ^ automatic.loop_replayed_instructions() as u32).wrapping_mul(16777619);
            traced.enable_loop_replay(true);
            traced.enable_loop_training(false);
            for _ in 0..2 {
                traced.advance_frame().unwrap();
                normal.advance_frame().unwrap();
                assert_eq!(traced.cpu, normal.cpu, "replayed master");
                assert_eq!(traced.slave, normal.slave, "replayed private peer");
                assert_eq!(traced.cpu_ready, normal.cpu_ready);
                assert_eq!(traced.clocks, normal.clocks);
                assert_eq!(traced.sound_state(), normal.sound_state());
                assert_eq!(traced.bus.high, normal.bus.high);
                assert_eq!(traced.bus.low, normal.bus.low);
                #[cfg(test)]
                for i in 0..2 {
                    traced.bus.cache[i]
                        .borrow()
                        .assert_same_contents(&normal.bus.cache[i].borrow());
                }
            }
            assert!(
                traced.loop_replayed_instructions() > 0,
                "execute the proven loop"
            );
            let before_irq = traced.loop_replayed_instructions();
            let exceptions = traced.cpu.state().exceptions;
            traced.request_nmi();
            normal.request_nmi();
            for machine in [&mut traced, &mut normal] {
                let mut pending = false;
                for _ in 0..200_000 {
                    machine.loop_trace_turn(None).unwrap();
                    if machine.cpu.state().pc == super::HIGH_WRAM + 0x402 {
                        assert!(
                            !machine.cpu.loop_multiplier_idle(),
                            "MUL.L retains a live completion deadline"
                        );
                        pending = true;
                        break;
                    }
                }
                assert!(
                    pending,
                    "observe the real interrupt handler's pending multiply"
                );
            }
            traced.advance_frame().unwrap();
            normal.advance_frame().unwrap();
            assert_eq!(traced.cpu.state().exceptions, exceptions + 1);
            assert_eq!(
                traced.cpu, normal.cpu,
                "preserve interrupt history after return"
            );
            assert_eq!(traced.slave, normal.slave);
            assert_eq!(traced.cpu_ready, normal.cpu_ready);
            assert_eq!(traced.clocks, normal.clocks);
            assert_eq!(traced.bus.high, normal.bus.high);
            assert!(
                traced.loop_replayed_instructions() > before_irq,
                "reuse after ordinary interrupt return"
            );
            let before_relocation = traced.loop_replayed_instructions();
            for machine in [&mut traced, &mut normal] {
                relocate_cache_lines(&mut machine.bus.cache[0].borrow_mut());
            }
            traced.advance_frame().unwrap();
            normal.advance_frame().unwrap();
            assert_eq!(traced.cpu, normal.cpu, "cache ways may move after training");
            assert_eq!(traced.slave, normal.slave);
            assert_eq!(traced.cpu_ready, normal.cpu_ready);
            assert_eq!(traced.clocks, normal.clocks);
            assert_eq!(traced.sound_state(), normal.sound_state());
            assert_eq!(traced.bus.high, normal.bus.high);
            #[cfg(test)]
            for i in 0..2 {
                traced.bus.cache[i]
                    .borrow()
                    .assert_same_contents(&normal.bus.cache[i].borrow());
            }
            assert!(
                traced.loop_replayed_instructions() > before_relocation,
                "reuse the original proof with remapped LRU touches"
            );
            // A newly changed input must be loaded and stored by ordinary
            // execution, rather than overwritten with the trained constant.
            for machine in [&mut traced, &mut normal] {
                for _ in 0..200_000 {
                    if machine.cpu.state().pc == super::HIGH_WRAM {
                        break;
                    }
                    machine.loop_trace_turn(None).unwrap();
                }
                assert_eq!(machine.cpu.state().pc, super::HIGH_WRAM);
                machine
                    .bus
                    .write32(super::HIGH_WRAM + 0x1000, 0x87654321)
                    .unwrap();
                machine.bus.cache[0]
                    .borrow_mut()
                    .written(super::HIGH_WRAM + 0x1000, 0x87654321u32.to_be_bytes());
                super::Memory::write32(
                    &mut machine.bus.for_cpu(super::Processor::Master),
                    0xFFFFFFE8,
                    if dense_stores { 0xA55A0000 } else { 0xA55A00FF },
                )
                .unwrap();
            }
            traced.advance_frame().unwrap();
            normal.advance_frame().unwrap();
            assert_eq!(traced.cpu, normal.cpu, "changed inputs and BSC timing");
            assert_eq!(traced.slave, normal.slave);
            assert_eq!(traced.cpu_ready, normal.cpu_ready);
            assert_eq!(traced.clocks, normal.clocks);
            assert_eq!(traced.bus.high, normal.bus.high);
            assert_eq!(traced.sound_state(), normal.sound_state());
            assert_eq!(
                traced.debug_read32(super::HIGH_WRAM + 0x1000).unwrap(),
                0x87654321
            );
            #[cfg(test)]
            for i in 0..2 {
                traced.bus.cache[i]
                    .borrow()
                    .assert_same_contents(&normal.bus.cache[i].borrow());
            }
            let shortened_before = traced.loop_shortened_batches();
            let elisions_before = traced.loop_sound_elisions();
            for machine in [&mut traced, &mut normal] {
                let sound = &mut machine.bus.sound;
                sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
                sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                // Original repeated SCSP control writes bound CPU lookahead.
                for (i, word) in [0x33FCu16, 0x1820, 0x0010, 0x0000, 0x60F6]
                    .into_iter()
                    .enumerate()
                {
                    sound.ram[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&word.to_be_bytes());
                }
                sound.set_enabled(true).unwrap();
            }
            for _ in 0..2 {
                traced.advance_frame().unwrap();
                normal.advance_frame().unwrap();
                assert_eq!(traced.cpu, normal.cpu, "sound selects a shorter CPU prefix");
                assert_eq!(traced.slave, normal.slave);
                assert_eq!(traced.cpu_ready, normal.cpu_ready);
                assert_eq!(traced.clocks, normal.clocks);
                assert_eq!(traced.sound_state(), normal.sound_state());
                assert_eq!(traced.bus.high, normal.bus.high);
                assert_eq!(
                    traced.bus.sound.scsp.drain(8192),
                    normal.bus.sound.scsp.drain(8192)
                );
                #[cfg(test)]
                for i in 0..2 {
                    traced.bus.cache[i]
                        .borrow()
                        .assert_same_contents(&normal.bus.cache[i].borrow());
                    traced.bus.onchip[i].assert_cached_read_bus_matches(&normal.bus.onchip[i]);
                }
            }
            assert!(
                traced.loop_shortened_batches() > shortened_before,
                "exercise direct publication of a Sound-shortened prefix"
            );
            hash = (hash ^ traced.loop_shortened_batches() as u32).wrapping_mul(16777619);
            assert!(
                traced.loop_sound_elisions() > elisions_before,
                "dense private-peer retirements avoid a sound checkpoint"
            );
            hash = (hash ^ traced.loop_sound_elisions() as u32).wrapping_mul(16777619);
            hash = (hash ^ traced.loop_replayed_instructions() as u32).wrapping_mul(16777619);
        }
        for value in summary {
            hash = (hash ^ value).wrapping_mul(16777619);
        }
        for value in traced
            .loop_probe_counts()
            .into_iter()
            .chain(traced.loop_probe_periods())
            .chain(traced.cpu.state().r)
            .chain([traced.cpu.state().pc, traced.clocks as u32, data])
        {
            hash = (hash ^ value).wrapping_mul(16777619);
        }
    }
    for value in verify_pending_store_prefix() {
        hash = (hash ^ value).wrapping_mul(16777619);
    }
    for value in verify_refresh_prefix() {
        hash = (hash ^ value).wrapping_mul(16777619);
    }
    vec![4, hash]
}
#[cfg(test)]
mod tests {
    #[test]
    fn short_sparse_cycle_keeps_existing_folding() {
        use super::*;
        let make = || {
            let mut machine = original_machine(false, false, false);
            // Two distant BRA/NOP pairs execute only four instructions per
            // cycle. Their address span must not select the long-loop engine.
            for (offset, word) in [(0, 0xA01Eu16), (2, 9), (0x40, 0xAFDE), (0x42, 9)] {
                let address = crate::bus::HIGH_WRAM + offset;
                machine.bus.write16(address, word).unwrap();
                machine.bus.cache[0]
                    .borrow_mut()
                    .written(address, word.to_be_bytes());
            }
            machine
        };
        let mut automatic = make();
        let mut reference = make();
        automatic.enable_loop_replay(true);
        for _ in 0..2 {
            automatic.advance_frame().unwrap();
            reference.advance_frame().unwrap();
            assert_eq!(automatic.cpu, reference.cpu);
            assert_eq!(automatic.slave, reference.slave);
            assert_eq!(automatic.cpu_ready, reference.cpu_ready);
            assert_eq!(automatic.clocks, reference.clocks);
            assert_eq!(automatic.bus.high, reference.bus.high);
            for i in 0..2 {
                automatic.bus.cache[i]
                    .borrow()
                    .assert_same_contents(&reference.bus.cache[i].borrow());
                automatic.bus.onchip[i].assert_cached_read_bus_matches(&reference.bus.onchip[i]);
            }
        }
        assert_eq!(automatic.loop_replayed_instructions(), 0);
        assert!(
            automatic.bus.cached_cycle_skipped > 0,
            "retain the existing short-cycle folder"
        );
    }
    #[test]
    fn incomplete_second_period_keeps_the_first_observation() {
        let mut single = super::original_machine(false, false, false);
        let mut overlap = super::original_machine(false, false, false);
        for machine in [&mut single, &mut overlap] {
            machine.bus.loop_probe.as_mut().unwrap().best = Some((super::super::HIGH_WRAM, 34));
        }
        // A trace turn retires at most one Master instruction. This permits
        // the literal 34-instruction period but cannot complete two periods.
        let expected = single.capture_loop_trace_bounded(67).unwrap();
        assert_eq!(&expected[..2], &[1, 34]);
        let truncated = overlap.capture_loop_trace_with_overlap(67, true).unwrap();
        assert_eq!(truncated, expected);
        let a = single
            .bus
            .loop_probe
            .as_ref()
            .unwrap()
            .captured
            .as_ref()
            .unwrap();
        let b = overlap
            .bus
            .loop_probe
            .as_ref()
            .unwrap()
            .captured
            .as_ref()
            .unwrap();
        assert_eq!(a.phases.len(), b.phases.len());
        assert_eq!(a.touches.len(), b.touches.len());
        for (a, b) in a.phases.iter().zip(&b.phases) {
            assert_eq!(a.cpu, b.cpu);
            assert_eq!(a.cache, b.cache);
            assert_eq!(
                (a.at, a.ready, a.retained, a.epoch, a.touch_end),
                (b.at, b.ready, b.retained, b.epoch, b.touch_end)
            );
        }
    }
    #[test]
    fn shared_original_probe_and_negative_controls() {
        let result = super::verify();
        eprintln!("loop_probe={result:?}");
        assert_eq!(result[0], 4);
    }
}
