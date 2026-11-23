//! Live-origin WRAM write plans validated by the canonical bus arbiter.
//! Captured CPU inputs and store acceptance times must match; physical byte
//! completions and final BSC state are computed from the current board.
//! The admitted horizon precedes non-bus peripheral transitions. Prediction
//! advances only BSC/A-bus state; real peripheral clocks run during catch-up.
use super::*;
use crate::bus::loop_probe::trace::CapturedLoop;

pub(in crate::bus) struct Opportunity {
    pub first: usize,
    pub legacy_last: usize,
    pub shift: u64,
    pub count: u32,
    pub end: u64,
}

pub(in crate::bus) struct PreparedWrites {
    pub last: usize,
    pub ways: [u8; 256],
    pub lru: [u8; 64],
    pub plan: LiveWritePlan,
}

pub(in crate::bus) struct LiveWritePlan {
    origin: u64,
    pending_origin: bool,
    end: u64,
    bsc: [crate::onchip::LiveWriteBsc; 2],
    active: Option<Active>,
    write: Option<BufferedWrite>,
    group: Option<Processor>,
    previous: Option<Processor>,
    external: Option<Processor>,
    dmac_yield: bool,
    abus: crate::scu::Abus,
    physical: Vec<PhysicalWrite>,
    stores: Vec<Store>,
}

impl Bus {
    #[cfg(test)]
    pub(in crate::bus) fn live_write_buffer_pending(&self) -> bool {
        self.transactions.writes[0].is_some()
    }
    #[cfg(test)]
    pub(in crate::bus) fn assert_live_write_active_matches(&self, other: &Self) {
        assert!(self.transactions.active == other.transactions.active);
        assert!(self.transactions.internal == other.transactions.internal);
        assert!(self.transactions.cpu_group == other.transactions.cpu_group);
        assert!(self.transactions.previous == other.transactions.previous);
        assert_eq!(self.transactions.dmac_yield, other.transactions.dmac_yield);
        assert_eq!(self.external_owner.get(), other.external_owner.get());
        assert_eq!(self.scu.plan_cached_abus(), other.scu.plan_cached_abus());
        for i in 0..2 {
            assert_eq!(self.cpu_bus_clocks[i].get(), other.cpu_bus_clocks[i].get());
            match (&self.transactions.writes[i], &other.transactions.writes[i]) {
                (Some(a), Some(b)) => {
                    assert!(a.dmac == b.dmac && a.beats == b.beats && a.context == b.context)
                }
                (None, None) => {}
                _ => panic!("live write buffer differs"),
            }
        }
    }
    pub(in crate::bus) fn prepare_live_write_runtime(
        &self,
        data: &CapturedLoop,
        window: Opportunity,
    ) -> Option<PreparedWrites> {
        let owner = self.loop_probe.as_ref()?;
        if !owner.live_write_runtime {
            return None;
        }
        #[cfg(any(test, feature = "kernel-experiment"))]
        owner.live_write_runtime_counts[0].set(owner.live_write_runtime_counts[0].get() + 1);
        let last = data.timeline.as_ref()?.prefix_end(
            window.first,
            window.count,
            window.end,
            window.shift,
        )?;
        if last <= window.legacy_last {
            return None;
        }
        let cache = self.cache[0].borrow();
        let mut ways = [u8::MAX; 256];
        for guard in data.compiled.get(window.first)? {
            if guard.needed_by > last as u32 {
                continue;
            }
            let actual = cache.loop_guard_matches(
                guard.address,
                guard.ways,
                guard.input_mask(last as u32),
                &guard.bytes,
            )?;
            if guard.ways != 0 {
                let set = (guard.address >> 4 & 63) as usize;
                ways[set * 4 + guard.ways.trailing_zeros() as usize] =
                    actual.trailing_zeros() as u8;
            }
        }
        let mut lru = cache.replacement_state();
        for (_, touch) in data.replay_touches(window.first, last) {
            if !touch.apply_mapped(&mut lru, &ways) {
                return None;
            }
        }
        let mut stores = Vec::new();
        for store in data
            .stores
            .iter()
            .filter(|s| s.owner > window.first as u64 && s.owner <= last as u64)
        {
            if self.onchip[0].external_little_endian(Bus::physical(store.address)) {
                return None;
            }
            stores.push(Store {
                requested: store.requested?.checked_add(window.shift)?,
                accepted: store.accepted.checked_add(window.shift)?,
                address: store.address,
                width: store.width as u32,
                value: store.value,
                pc: store.pc,
            });
        }
        let end = data.phases[last].at.checked_add(window.shift)?;
        data.phases[last].ready.checked_add(window.shift)?;
        if end <= self.main_clocks {
            return None;
        }
        let pending = owner.pending_write_runtime;
        let pending_origin =
            self.transactions.active.is_some() || self.transactions.writes[0].is_some();
        #[cfg(any(test, feature = "kernel-experiment"))]
        if pending && pending_origin {
            owner.pending_write_counts[0].set(owner.pending_write_counts[0].get() + 1);
        }
        let mut scratch = owner.live_write_scratch.borrow_mut();
        let probe = scratch.get_or_insert_with(|| Box::new(Probe::new()));
        if !probe.run_with_pending(self, &stores, end, pending) {
            return None;
        }
        if probe.board.transactions.internal[0].is_some() {
            return None;
        }
        let plan = LiveWritePlan {
            origin: self.main_clocks,
            pending_origin,
            end,
            bsc: std::array::from_fn(|i| probe.board.onchip[i].live_write_bsc()),
            active: probe.board.transactions.active.clone(),
            write: probe.board.transactions.writes[0].clone(),
            group: probe.board.transactions.cpu_group,
            previous: probe.board.transactions.previous,
            external: probe.board.external_owner.get(),
            dmac_yield: probe.board.transactions.dmac_yield,
            abus: probe.board.scu.plan_cached_abus(),
            physical: probe.report.physical.clone(),
            stores,
        };
        #[cfg(any(test, feature = "kernel-experiment"))]
        {
            owner.live_write_runtime_counts[1].set(owner.live_write_runtime_counts[1].get() + 1);
            owner.live_write_runtime_counts[2]
                .set(owner.live_write_runtime_counts[2].get() + (last - window.first) as u64);
            owner.live_write_runtime_counts[3]
                .set(owner.live_write_runtime_counts[3].get() + (last - window.legacy_last) as u64);
        }
        #[cfg(any(test, feature = "kernel-experiment"))]
        if pending && pending_origin {
            owner.pending_write_counts[1].set(owner.pending_write_counts[1].get() + 1);
        }
        Some(PreparedWrites {
            last,
            ways,
            lru,
            plan,
        })
    }

    pub(in crate::bus) fn commit_live_write_plan(
        &mut self,
        plan: LiveWritePlan,
        retired_at: u64,
    ) -> Result<u64, ScuError> {
        assert_eq!(self.main_clocks, plan.origin);
        assert_eq!(retired_at, plan.end);
        if plan.pending_origin {
            // The private plan already owns all remaining WRAM/refresh beats.
            // Independent catch-up must not drain them a second time.
            assert!(Probe::pending_scope(self));
            self.transactions.active = None;
            self.transactions.writes[0] = None;
            self.transactions.cpu_group = None;
        } else {
            assert!(
                self.transactions.active.is_none()
                    && self.transactions.writes.iter().all(Option::is_none)
            );
        }
        // Independent domains still run normally. BSC counters are aged here
        // without bus grants, then replaced by the exact live-derived protocol.
        let reference = self.advance_isolated_segment_mode::<false, true>(
            (retired_at - self.main_clocks) as u32,
            false,
            false,
        )?;
        for write in plan.physical {
            match write.width {
                1 => self.write(write.address, [write.value as u8]),
                2 => self.write(write.address, (write.value as u16).to_be_bytes()),
                4 => self.write(write.address, write.value.to_be_bytes()),
                _ => unreachable!("validated physical WRAM beat"),
            }
            .expect("validated physical WRAM publication");
        }
        for store in &plan.stores {
            let cache = self.cache[0].get_mut();
            match store.width {
                1 => cache.written(store.address, [store.value as u8]),
                2 => cache.written(store.address, (store.value as u16).to_be_bytes()),
                4 => cache.written(store.address, store.value.to_be_bytes()),
                _ => unreachable!("validated cached WRAM store"),
            }
        }
        for (chip, state) in self.onchip.iter_mut().zip(plan.bsc) {
            chip.publish_live_write_bsc(state);
        }
        self.transactions.active = plan.active;
        self.transactions.writes[0] = plan.write;
        self.transactions.cpu_group = plan.group;
        self.transactions.previous = plan.previous;
        self.transactions.dmac_yield = plan.dmac_yield;
        self.external_owner.set(plan.external);
        self.scu.commit_cached_abus(&plan.abus);
        if let Some(last) = plan.stores.last() {
            self.finish_cpu_transaction(Processor::Master);
            let journal = self.transactions.journals[0].get_mut();
            journal.write_context = (last.pc, None);
            journal.current_operation = Some(Operation::Write(
                last.address,
                last.width as usize,
                last.value,
            ));
        }
        Ok(reference)
    }

    #[cfg(any(test, feature = "kernel-experiment"))]
    pub(in crate::bus) fn observe_live_write_probe(
        &self,
        data: &CapturedLoop,
        window: Opportunity,
    ) {
        let Some(owner) = &self.loop_probe else {
            return;
        };
        if !owner.live_write_enabled {
            return;
        }
        let count = |index: usize, n: u64| {
            let slot = &owner.live_write_stats[index];
            slot.set(slot.get() + n);
        };
        count(0, 1);
        let Some(timeline) = &data.timeline else {
            return;
        };
        let Some(last) = timeline.prefix_end(window.first, window.count, window.end, window.shift)
        else {
            return;
        };
        if last <= window.legacy_last {
            return;
        }
        count(1, 1);
        let cache = self.cache[0].borrow();
        let Some(guards) = data.compiled.get(window.first) else {
            return;
        };
        for guard in guards {
            if guard.needed_by <= last as u32
                && cache
                    .loop_guard_matches(
                        guard.address,
                        guard.ways,
                        guard.input_mask(last as u32),
                        &guard.bytes,
                    )
                    .is_none()
            {
                return;
            }
        }
        count(2, 1);
        let mut stores = Vec::new();
        for store in data
            .stores
            .iter()
            .filter(|s| s.owner > window.first as u64 && s.owner <= last as u64)
        {
            let Some(requested) = store.requested.and_then(|at| at.checked_add(window.shift))
            else {
                return;
            };
            let Some(accepted) = store.accepted.checked_add(window.shift) else {
                return;
            };
            if self.onchip[0].external_little_endian(Bus::physical(store.address)) {
                return;
            }
            stores.push(Store {
                requested,
                accepted,
                address: store.address,
                width: store.width as u32,
                value: store.value,
                pc: store.pc,
            });
        }
        let Some(end) = data.phases[last].at.checked_add(window.shift) else {
            return;
        };
        count(3, 1);
        count(4, (last - window.first) as u64);
        count(5, (window.legacy_last - window.first) as u64);
        let mut scratch = owner.live_write_scratch.borrow_mut();
        let probe = scratch.get_or_insert_with(|| Box::new(Probe::new()));
        if !probe.run(self, &stores, end) {
            count(6, 1);
            return;
        }
        count(7, 1);
        count(8, (last - window.legacy_last) as u64);
        count(9, stores.len() as u64);
        count(10, probe.report.physical.len() as u64);
        count(11, probe.report.refreshes as u64);
        count(12, end - self.main_clocks);
        let digest = probe.report.physical.iter().fold(0u64, |hash, write| {
            hash.rotate_left(5)
                ^ write.at
                ^ u64::from(write.address)
                ^ write.width as u64
                ^ u64::from(write.value)
        });
        owner.live_write_stats[13].set(owner.live_write_stats[13].get() ^ u64::from(digest as u32));
        count(14, (last - window.first) as u64);
        count(15, (window.legacy_last - window.first) as u64);
    }
}

#[derive(Clone, Copy)]
struct Store {
    requested: u64,
    accepted: u64,
    address: u32,
    width: u32,
    value: u32,
    pc: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PhysicalWrite {
    at: u64,
    address: u32,
    width: usize,
    value: u32,
}

#[derive(Default)]
struct Report {
    accepted: Vec<u64>,
    physical: Vec<PhysicalWrite>,
    refreshes: usize,
}

pub(in crate::bus) struct Probe {
    board: Box<Bus>,
    report: Report,
}
impl Probe {
    pub(in crate::bus) fn new() -> Self {
        Self {
            board: Box::new(Bus::default()),
            report: Report::default(),
        }
    }

    fn pending_scope(live: &Bus) -> bool {
        let wram = |t: &Transfer| {
            let at = Bus::physical(t.address);
            t.write.is_some()
                && matches!(t.width, 1 | 2 | 4)
                && !live.onchip[0].external_little_endian(at)
                && [LOW_WRAM, HIGH_WRAM]
                    .into_iter()
                    .any(|base| at >= base && at + t.width as u32 <= base + WRAM_SIZE as u32)
        };
        live.transactions.active.as_ref().is_none_or(|a| {
            a.owner == Owner::Refresh
                || (a.owner == Owner::WriteBuffer(Processor::Master, false) && wram(&a.transfer))
        }) && live.transactions.writes[0]
            .as_ref()
            .is_none_or(|w| !w.dmac && !w.beats.is_empty() && w.beats.iter().all(wram))
            && live.transactions.writes[1].is_none()
            && live
                .transactions
                .cpu_group
                .is_none_or(|g| g == Processor::Master && live.transactions.writes[0].is_some())
    }
    fn reset_from(&mut self, live: &Bus, pending: bool) -> bool {
        if !live.cached_bus_compatible()
            || if pending {
                !Self::pending_scope(live)
            } else {
                live.transactions.active.is_some()
                    || live.transactions.writes.iter().any(Option::is_some)
                    || live.transactions.cpu_group.is_some()
            }
            || live.transactions.internal.iter().any(Option::is_some)
            || live.cpu_bus_clocks.iter().any(|clock| clock.get() != 0)
            || live.onchip.iter().any(|chip| !chip.cached_batch_allowed())
            || live.scu.dma_pending()
        {
            return false;
        }
        self.report = Report::default();
        self.board.transactions = Transactions::default();
        if pending {
            self.board.transactions.active = live.transactions.active.clone();
            self.board.transactions.writes[0] = live.transactions.writes[0].clone();
            self.board.transactions.cpu_group = live.transactions.cpu_group;
        }
        self.board.transactions.previous = live.transactions.previous;
        self.board.transactions.dmac_yield = live.transactions.dmac_yield;
        self.board.onchip.clone_from(&live.onchip);
        self.board.main_clocks = live.main_clocks;
        self.board.slave_enabled = live.slave_enabled;
        self.board.external_owner.set(live.external_owner.get());
        self.board.scu.copy_live_write_abus(&live.scu);
        for processor in 0..2 {
            self.board.cpu_bus_clocks[processor].set(0);
            self.board.instruction_word[processor].set(live.instruction_word[processor].get());
        }
        true
    }

    fn grant(&mut self) {
        let empty = self.board.transactions.writes[0].is_none();
        let idle = self.board.transactions.active.is_none();
        self.board.grant_bus_transaction();
        if empty && self.board.transactions.writes[0].is_some() {
            self.report.accepted.push(self.board.main_clocks);
        }
        if idle
            && self
                .board
                .transactions
                .active
                .as_ref()
                .is_some_and(|a| a.owner == Owner::Refresh)
        {
            self.report.refreshes += 1;
        }
    }

    fn advance_one(&mut self, end: u64) {
        assert!(self.board.main_clocks < end);
        self.grant();
        let mut delta = (end - self.board.main_clocks).min(u64::from(u32::MAX)) as u32;
        delta = delta.min(self.board.bus_main_deadline().max(1));
        delta = delta.min(self.board.onchip[0].live_write_bus_deadline());
        if self.board.slave_enabled {
            delta = delta.min(self.board.onchip[1].live_write_bus_deadline());
        }
        assert!(delta != 0);
        let physical = self.board.transactions.active.as_ref().and_then(|active| {
            (active.owner == Owner::WriteBuffer(Processor::Master, false)
                && active.clocks_left(self.board.main_clocks) == delta)
                .then(|| PhysicalWrite {
                    at: self.board.main_clocks + u64::from(delta),
                    address: Bus::physical(active.transfer.address),
                    width: active.transfer.width,
                    value: active.transfer.write.expect("WRAM write payload"),
                })
        });
        self.board.main_clocks += u64::from(delta);
        self.board.onchip[0].advance_live_write_bus(delta);
        if self.board.slave_enabled {
            self.board.onchip[1].advance_live_write_bus(delta);
        }
        self.board.scu.advance_live_write_abus(delta);
        self.board.advance_bus_transaction().unwrap();
        if let Some(physical) = physical {
            self.report.physical.push(physical);
        }
    }

    fn through(&mut self, end: u64) {
        while self.board.main_clocks < end {
            self.advance_one(end);
        }
    }

    #[cfg(any(test, feature = "kernel-experiment"))]
    fn run(&mut self, live: &Bus, stores: &[Store], end: u64) -> bool {
        self.run_with_pending(live, stores, end, false)
    }
    fn run_with_pending(&mut self, live: &Bus, stores: &[Store], end: u64, pending: bool) -> bool {
        if end < live.main_clocks
            || end - live.main_clocks > 1024
            || !self.reset_from(live, pending)
        {
            return false;
        }
        for store in stores {
            let address = Bus::physical(store.address);
            if !matches!(store.width, 1 | 2 | 4)
                || !address.is_multiple_of(store.width)
                || ![LOW_WRAM, HIGH_WRAM]
                    .into_iter()
                    .any(|base| address >= base && address + store.width <= base + WRAM_SIZE as u32)
                || store.requested < self.board.main_clocks
                || store.accepted < store.requested
                || store.accepted >= end
            {
                return false;
            }
            self.through(store.requested);
            let first = self.report.accepted.len();
            loop {
                match self.board.resume_planned_write(
                    Processor::Master,
                    store.address,
                    store.width,
                    store.value,
                    store.pc,
                ) {
                    Ok(()) => {
                        if self.report.accepted.get(first) != Some(&store.accepted) {
                            return false;
                        }
                        self.board.finish_cpu_transaction(Processor::Master);
                        break;
                    }
                    Err(BusError::Pending) => {
                        self.grant();
                        if self
                            .report
                            .accepted
                            .get(first)
                            .is_some_and(|&at| at != store.accepted)
                            || self.board.main_clocks > store.accepted
                        {
                            return false;
                        }
                        self.advance_one((store.accepted + 1).min(end));
                    }
                    Err(_) => return false,
                }
            }
        }
        self.through(end);
        true
    }
}

#[cfg(test)]
fn configured(byte_bus: bool, refresh: bool) -> Bus {
    let mut bus = Bus::default();
    bus.for_cpu(Processor::Master)
        .write32(0xFFFF_FFE8, 0xA55A_00C0)
        .unwrap();
    if byte_bus {
        bus.for_cpu(Processor::Master)
            .write32(0xFFFF_FFE4, 0xA55A_007C)
            .unwrap();
    }
    if refresh {
        // CS2 SDRAM enables the shared refresh request; CS3 stays ordinary.
        bus.for_cpu(Processor::Master)
            .write32(0xFFFF_FFE0, 0xA55A_03F4)
            .unwrap();
        bus.for_cpu(Processor::Master)
            .write32(0xFFFF_FFEC, 0xA55A_0048)
            .unwrap();
        bus.for_cpu(Processor::Master)
            .write32(0xFFFF_FFF8, 0xA55A_0002)
            .unwrap();
        bus.for_cpu(Processor::Master)
            .write32(0xFFFF_FFF0, 0xA55A_0008)
            .unwrap();
    }
    bus
}

#[test]
fn live_origin_probe_retains_each_byte_edge_and_rejects_wrong_acceptance() {
    let live = configured(true, false);
    let store = Store {
        requested: 0,
        accepted: 0,
        address: HIGH_WRAM | 0x2000_0000,
        width: 4,
        value: 0x12345678,
        pc: 0,
    };
    let mut probe = Probe::new();
    for end in 1..=40 {
        assert!(probe.run(&live, &[store], end), "end={end}");
        let mut word = [0xA5; 4];
        for write in &probe.report.physical {
            let offset = (write.address - HIGH_WRAM) as usize;
            word[offset..offset + write.width]
                .copy_from_slice(&write.value.to_be_bytes()[4 - write.width..]);
        }
        let expected = match end / 8 {
            0 => 0xA5A5A5A5,
            1 => 0x12A5A5A5,
            2 => 0x1234A5A5,
            3 => 0x123456A5,
            _ => 0x12345678,
        };
        assert_eq!(u32::from_be_bytes(word), expected, "end={end}");
    }
    assert!(!probe.run(
        &live,
        &[Store {
            accepted: 1,
            ..store
        }],
        40
    ));
    assert_eq!(live.read32(HIGH_WRAM), Ok(0));
}

#[cfg(test)]
fn canonical_store(bus: &mut Bus, address: u32, width: u32, value: u32) -> Store {
    let requested = bus.main_clocks;
    let mut accepted = None;
    loop {
        let result = match width {
            1 => bus
                .replay_cpu(Processor::Master)
                .write8(address, value as u8),
            2 => bus
                .replay_cpu(Processor::Master)
                .write16(address, value as u16),
            4 => bus.replay_cpu(Processor::Master).write32(address, value),
            _ => unreachable!(),
        };
        match result {
            Ok(()) => {
                bus.finish_cpu_transaction(Processor::Master);
                return Store {
                    requested,
                    accepted: accepted.unwrap(),
                    address,
                    width,
                    value,
                    pc: 0,
                };
            }
            Err(BusError::Pending) => {
                let empty = bus.transactions.writes[0].is_none();
                bus.grant_bus_transaction();
                if empty && bus.transactions.writes[0].is_some() {
                    assert!(accepted.replace(bus.main_clocks).is_none());
                }
                bus.advance_clocks(1).unwrap();
            }
            other => panic!("canonical WRAM request failed: {other:?}"),
        }
    }
}

#[test]
fn live_origin_probe_matches_real_refresh_and_buffer_protocol_at_every_clock_cut() {
    let mut probe = Probe::new();
    for (byte_bus, sdram, seeded_row) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, true, true),
    ] {
        for refresh in [false, true] {
            for phase in [0, 1, 3, 7] {
                let setup = || {
                    let mut bus = configured(byte_bus, false);
                    if sdram {
                        bus.onchip[0].write32(0xFFFF_FFE0, 0xA55A_03F5).unwrap();
                        bus.onchip[0].write32(0xFFFF_FFEC, 0xA55A_0240).unwrap();
                        if seeded_row {
                            // Complete a real prior write, retaining its open
                            // row while starting the plan with no pending bus.
                            canonical_store(&mut bus, HIGH_WRAM + 0x400, 4, 0x89ABCDEF);
                            bus.advance_clocks(64).unwrap();
                            assert!(bus.transactions.active.is_none());
                            assert!(bus.transactions.writes.iter().all(Option::is_none));
                        }
                        bus.scu.write32(crate::scu::SCU + 0xB0, 0x93240324).unwrap();
                        bus.scu.write32(crate::scu::SCU + 0xB8, 0x1F).unwrap();
                        bus.scu.abus_access(0x0200_0000, 4, false);
                        bus.scu.refresh_abus();
                    }
                    if refresh {
                        bus.onchip[0]
                            .write32(0xFFFF_FFE0, 0xA55A_03F4 | u32::from(sdram))
                            .unwrap();
                        bus.onchip[0]
                            .write32(0xFFFF_FFEC, 0xA55A_0008 | if sdram { 0x240 } else { 0x40 })
                            .unwrap();
                        bus.onchip[0].write32(0xFFFF_FFF8, 0xA55A_0002).unwrap();
                        bus.onchip[0].write32(0xFFFF_FFF0, 0xA55A_0008).unwrap();
                    }
                    bus.onchip[0].advance_clocks(phase);
                    bus.main_clocks += u64::from(phase);
                    bus
                };
                let mut traced = setup();
                let origin = traced.main_clocks;
                let mut stores = Vec::new();
                for (i, requested) in [0, 1, 4, 18, 31, 48, 64, 80].into_iter().enumerate() {
                    let at = origin + requested;
                    if at > traced.main_clocks {
                        traced
                            .advance_clocks((at - traced.main_clocks) as u32)
                            .unwrap();
                    }
                    let width = [1, 2, 4][i % 3];
                    stores.push(canonical_store(
                        &mut traced,
                        HIGH_WRAM + i as u32 * 8,
                        width,
                        0x12345678 + i as u32,
                    ));
                }
                for cut in 1..=128 {
                    let end = origin + cut;
                    let selected: Vec<_> = stores
                        .iter()
                        .copied()
                        .filter(|s| s.accepted < end)
                        .collect();
                    let mut reference = setup();
                    for store in &selected {
                        if store.requested > reference.main_clocks {
                            reference
                                .advance_clocks((store.requested - reference.main_clocks) as u32)
                                .unwrap();
                        }
                        let actual = canonical_store(
                            &mut reference,
                            store.address,
                            store.width,
                            store.value,
                        );
                        assert_eq!(actual.accepted, store.accepted);
                    }
                    if reference.main_clocks < end {
                        reference
                            .advance_clocks((end - reference.main_clocks) as u32)
                            .unwrap();
                    }
                    let live = setup();
                    assert!(
                        probe.run(&live, &selected, end),
                        "byte={byte_bus}, refresh={refresh}, phase={phase}, cut={cut}"
                    );
                    for processor in 0..2 {
                        probe.board.onchip[processor]
                            .assert_cached_read_bus_matches(&reference.onchip[processor]);
                        assert!(
                            probe.board.transactions.internal[processor]
                                == reference.transactions.internal[processor]
                        );
                        match (
                            &probe.board.transactions.writes[processor],
                            &reference.transactions.writes[processor],
                        ) {
                            (Some(a), Some(b)) => assert!(
                                a.dmac == b.dmac && a.beats == b.beats && a.context == b.context
                            ),
                            (None, None) => {}
                            _ => panic!("buffer occupancy differs"),
                        }
                    }
                    assert!(probe.board.transactions.active == reference.transactions.active);
                    assert!(probe.board.transactions.cpu_group == reference.transactions.cpu_group);
                    assert!(probe.board.transactions.previous == reference.transactions.previous);
                    assert_eq!(
                        probe.board.external_owner.get(),
                        reference.external_owner.get()
                    );
                    assert!(probe.board.scu.plan_cached_abus() == reference.scu.plan_cached_abus());
                    let mut physical = [0; 64];
                    for write in &probe.report.physical {
                        let at = (write.address - HIGH_WRAM) as usize;
                        physical[at..at + write.width]
                            .copy_from_slice(&write.value.to_be_bytes()[4 - write.width..]);
                    }
                    assert_eq!(&physical, &reference.high[..64]);
                    if refresh && cut >= 16 {
                        assert!(probe.report.refreshes != 0);
                    }
                    assert_eq!(live.read32(HIGH_WRAM), Ok(0));
                }
            }
        }
    }
}

#[test]
fn pending_origin_keeps_prior_bytes_and_every_remaining_buffer_beat() {
    let mut probe = Probe::new();
    let mut used = 0;
    let mut refresh_origins = 0;
    for byte_bus in [false, true] {
        for refresh in [false, true] {
            for delay in [0, 3, 7, 8, 15, 23, 31, 35] {
                let setup = || {
                    let mut bus = configured(byte_bus, refresh);
                    bus.high[..64].fill(0xA5);
                    canonical_store(&mut bus, HIGH_WRAM, 4, 0x12345678);
                    bus.advance_clocks(delay).unwrap();
                    bus
                };
                let live = setup();
                if !live.cached_bus_compatible()
                    || !live.onchip.iter().all(|c| c.cached_batch_allowed())
                    || (live.transactions.active.is_none() && live.transactions.writes[0].is_none())
                {
                    continue;
                }
                let origin = live.main_clocks;
                let mut traced = setup();
                let mut stores = Vec::new();
                for (i, request) in [0, 12, 36, 64].into_iter().enumerate() {
                    let at = origin + request;
                    if at > traced.main_clocks {
                        traced
                            .advance_clocks((at - traced.main_clocks) as u32)
                            .unwrap();
                    }
                    stores.push(canonical_store(
                        &mut traced,
                        HIGH_WRAM + 8 + i as u32 * 8,
                        4,
                        0x89ABCDEF + i as u32,
                    ));
                }
                for cut in 1..=96 {
                    let end = origin + cut;
                    let selected: Vec<_> = stores
                        .iter()
                        .copied()
                        .filter(|s| s.accepted < end)
                        .collect();
                    let mut reference = setup();
                    for store in &selected {
                        if store.requested > reference.main_clocks {
                            reference
                                .advance_clocks((store.requested - reference.main_clocks) as u32)
                                .unwrap();
                        }
                        let actual = canonical_store(
                            &mut reference,
                            store.address,
                            store.width,
                            store.value,
                        );
                        assert_eq!(actual.accepted, store.accepted);
                    }
                    if reference.main_clocks < end {
                        reference
                            .advance_clocks((end - reference.main_clocks) as u32)
                            .unwrap();
                    }
                    assert!(
                        probe.run_with_pending(&live, &selected, end, true),
                        "byte={byte_bus}, refresh={refresh}, delay={delay}, cut={cut}"
                    );
                    probe.board.assert_live_write_active_matches(&reference);
                    for processor in 0..2 {
                        probe.board.onchip[processor]
                            .assert_cached_read_bus_matches(&reference.onchip[processor]);
                    }
                    let mut physical = live.high[..64].to_vec();
                    for write in &probe.report.physical {
                        let at = (write.address - HIGH_WRAM) as usize;
                        physical[at..at + write.width]
                            .copy_from_slice(&write.value.to_be_bytes()[4 - write.width..]);
                    }
                    assert_eq!(
                        physical,
                        reference.high[..64],
                        "byte={byte_bus}, refresh={refresh}, delay={delay}, cut={cut}"
                    );
                    used += 1;
                    refresh_origins += usize::from(
                        live.transactions
                            .active
                            .as_ref()
                            .is_some_and(|a| a.owner == Owner::Refresh),
                    );
                }
            }
        }
    }
    assert!(
        used != 0 && refresh_origins != 0,
        "exercise buffered writes and active refresh"
    );
}
