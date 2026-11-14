//! Complete scalar retirement versus the original memory/control dispatch tail.
use super::*;
use std::cell::{Cell, RefCell};

struct Code {
    op: u16,
    clock: Cell<u64>,
    clocked: bool,
    fetch_clocks: bool,
    events: RefCell<Vec<(u8, u32)>>,
}
impl Memory for Code {
    fn fetch16(&self, address: u32) -> Result<u16, BusError> {
        let word = self.read16(address)?;
        if self.fetch_clocks {
            self.clock.set(self.clock.get() + 1);
        }
        Ok(word)
    }
    fn read8(&self, address: u32) -> Result<u8, BusError> {
        let word = if address & !1 == 0x100 { self.op } else { 9 };
        Ok(if address & 1 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        })
    }
    fn read16(&self, address: u32) -> Result<u16, BusError> {
        self.events.borrow_mut().push((0, address));
        Ok(u16::from_be_bytes([
            self.read8(address)?,
            self.read8(address + 1)?,
        ]))
    }
    fn read32(&self, address: u32) -> Result<u32, BusError> {
        Ok(u32::from(self.read16(address)?) << 16 | u32::from(self.read16(address + 2)?))
    }
    fn write8(&mut self, _: u32, _: u8) -> Result<(), BusError> {
        panic!("scalar store");
    }
    fn write16(&mut self, _: u32, _: u16) -> Result<(), BusError> {
        panic!("scalar store");
    }
    fn write32(&mut self, _: u32, _: u32) -> Result<(), BusError> {
        panic!("scalar store");
    }
    fn sh2_clock(&self) -> Option<u64> {
        self.clocked.then_some(self.clock.get())
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_speculative_fetch(&self, _: u32) -> bool {
        true
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        self.events.borrow_mut().push((1, clocks));
        self.clock.set(self.clock.get() + u64::from(clocks));
        Ok(())
    }
    fn instruction_break(&self, pc: u32, after: bool) -> bool {
        self.events.borrow_mut().push((2 + u8::from(after), pc));
        false
    }
    fn sh2_discard_prefetch(&self) {
        self.events.borrow_mut().push((4, 0));
    }
}

#[test]
fn every_scalar_encoding_preserves_wb_overlap_delay_slots_and_fetch_order() {
    for op in 0..=u16::MAX {
        if scalar::decode_literal(op).is_none() {
            continue;
        }
        for mode in 0..7 {
            let new_code = || Code {
                op,
                clock: Cell::new(100),
                clocked: mode != 5,
                fetch_clocks: false,
                events: RefCell::default(),
            };
            let (mut fast_bus, mut literal_bus) = (new_code(), new_code());
            let mut fast = Cpu::default();
            fast.state.pc = 0x100;
            fast.state.status = CpuStatus::Running;
            fast.state.sr = 0x3F3;
            fast.state.r = std::array::from_fn(|i| {
                0x8000_0001u32.wrapping_add(0x1234_5679u32.wrapping_mul(i as u32))
            });
            if mode != 0 {
                // Real MOV writeback dependency metadata. Different target
                // registers exercise both consuming and independent scalars.
                fast.load_pipeline.issue(0x6002 | (op & 0xF00));
                fast.load_pipeline.load_split(mode == 2);
                if mode == 3 {
                    fast.load_pipeline.retain_ex(99);
                }
                if mode == 4 {
                    fast.load_pipeline.retain_ex(100);
                }
            }
            if mode >= 4 {
                fast.state.delay_target = Some(0x200);
            }
            if mode == 6 {
                fast.state.status = CpuStatus::Sleeping;
                fast.state.address_error_pending = true;
                fast.address_error_inhibited = true;
            }
            fast.fetch.prime(&fast_bus, 0x100);
            let mut literal = fast.clone();
            fast_bus.events.borrow_mut().clear();
            assert_eq!(
                fast.step(&mut fast_bus),
                literal.step_mode::<false>(&mut literal_bus)
            );
            assert_eq!(fast, literal, "op={op:04X} mode={mode}");
            assert_eq!(fast_bus.clock, literal_bus.clock);
            assert_eq!(
                fast_bus.events, literal_bus.events,
                "op={op:04X} mode={mode}"
            );
        }
    }
}

#[test]
fn scalar_blocks_consume_only_spent_writeback_and_preserve_retained_ex() {
    for op in 0..=u16::MAX {
        if scalar::decode_literal(op).is_none() {
            continue;
        }
        for mode in 0..15 {
            for limit in [1, 3] {
                let new_code = || Code {
                    op,
                    clock: Cell::new(100),
                    clocked: true,
                    fetch_clocks: true,
                    events: RefCell::default(),
                };
                let (mut fast_bus, mut literal_bus) = (new_code(), new_code());
                let mut fast = Cpu::default();
                fast.state.pc = 0x100;
                fast.state.status = CpuStatus::Running;
                fast.state.sr = 0x3F3;
                fast.state.r = std::array::from_fn(|i| {
                    0x8000_0001u32.wrapping_add(0x1234_5679u32.wrapping_mul(i as u32))
                });
                if mode >= 2 {
                    let register = if mode & 1 == 0 { 0 } else { op >> 8 & 15 };
                    fast.load_pipeline.issue(0x6002 | register << 8);
                    fast.load_pipeline.load_split(mode >= 4);
                }
                if mode & 1 != 0 {
                    fast.load_pipeline.issue_overlap = 1;
                }
                if mode >= 6 {
                    fast.load_pipeline.retain_ex(99 + (mode & 1) as u64);
                }
                if mode >= 12 {
                    fast.load_pipeline = timing::LoadPipeline::default();
                    fast.load_pipeline.retain_ex(98 + (mode - 12) as u64);
                }
                if (8..12).contains(&mode) {
                    fast.state.delay_target = Some(0x200);
                }
                if (10..12).contains(&mode) {
                    fast.state.address_error_pending = true;
                }
                fast.fetch.finish_cached_pair(0x100, (op, 9), 100);
                let mut literal = fast.clone();
                let before = fast.clone();
                let count = fast.cached_scalar_block(&mut fast_bus, limit);
                let expected = if mode < 2 || (4..6).contains(&mode) {
                    limit
                } else {
                    0
                };
                assert_eq!(count, expected, "op={op:04X} mode={mode}");
                if count == 0 {
                    assert_eq!(fast, before);
                    assert_eq!(fast_bus.clock.get(), 100);
                    assert!(fast_bus.events.borrow().is_empty());
                    continue;
                }
                for _ in 0..count {
                    literal.step_mode::<false>(&mut literal_bus).unwrap();
                }
                assert_eq!(fast, literal, "op={op:04X} mode={mode} limit={limit}");
                assert_eq!(fast_bus.clock, literal_bus.clock);
                let physical = |code: &Code| {
                    code.events
                        .borrow()
                        .iter()
                        .copied()
                        .filter(|&(kind, _)| kind == 0)
                        .collect::<Vec<_>>()
                };
                assert_eq!(physical(&fast_bus), physical(&literal_bus));
            }
        }
    }
}

#[test]
fn prepared_scalar_matches_complete_cpu_fetch_events_and_clocks_after_load_writeback() {
    for op in 0..=u16::MAX {
        if scalar::decode(op).is_none() {
            continue;
        }
        for mode in 0..6 {
            let make_bus = || Code {
                op,
                clock: Cell::new(100),
                clocked: true,
                fetch_clocks: false,
                events: RefCell::default(),
            };
            let (mut a_bus, mut b_bus) = (make_bus(), make_bus());
            let mut a = Cpu::start(0x100, 0x800);
            a.state.r = std::array::from_fn(|i| (i as u32).wrapping_mul(0x1357_9BDF));
            a.load_pipeline
                .issue(0x6002 | if mode & 1 == 0 { 0 } else { 0xF00 });
            a.load_pipeline.load_split(mode & 2 != 0);
            if mode & 4 != 0 {
                a.load_pipeline.retain_ex(99);
            }
            let mut b = a.clone();
            a.prime_fetch(&a_bus);
            b.prime_fetch(&b_bus);
            let plan = a
                .prepare_cached_instruction(op, 100)
                .expect("admitted scalar");
            a.execute_cached_instruction(plan, &mut a_bus);
            assert!(b.step_cached(&mut b_bus));
            assert_eq!(a, b, "op={op:04X} mode={mode}");
            assert_eq!(a_bus.clock.get(), b_bus.clock.get());
            assert_eq!(*a_bus.events.borrow(), *b_bus.events.borrow());
        }
    }
}

#[test]
fn resident_control_matches_timed_reference_fetches_for_every_branch_encoding() {
    // The oracle executes the original IF/ID helpers and pipeline waits. Check
    // every encoding, both conditional outcomes, WB hazards and retained EX.
    let timeline = |events: &[(u8, u32)]| {
        let mut clock = 100u64;
        let mut trace = Vec::new();
        for &(kind, value) in events {
            if kind == 1 {
                clock += u64::from(value);
            } else {
                trace.push((clock, kind, value));
            }
        }
        trace
    };
    for op in 0..=u16::MAX {
        if !Branch::recognized(op) {
            continue;
        }
        for t in [0, T] {
            for mode in 0..9 {
                let new_code = || Code {
                    op,
                    clock: Cell::new(100),
                    clocked: true,
                    fetch_clocks: false,
                    events: RefCell::default(),
                };
                let (mut fast_bus, mut literal_bus) = (new_code(), new_code());
                let mut fast = Cpu::start(0x100, 0x1000);
                fast.state.sr = t;
                fast.state.pr = 0x300;
                fast.state.r = std::array::from_fn(|i| 0x200 + i as u32 * 4);
                match mode {
                    1 | 3 | 4 | 5 | 6 | 7 => {
                        fast.load_pipeline.issue(0x6002 | (op & 0xF00));
                    }
                    2 | 8 => {
                        fast.load_pipeline.issue(0x4F26);
                    }
                    _ => {}
                }
                fast.load_pipeline.load_split(mode == 3);
                if mode >= 4 {
                    fast.load_pipeline.retain_ex(match mode {
                        5 => 100,
                        6 => 98,
                        _ => 99,
                    });
                }
                fast.fetch.prime(&fast_bus, 0x100);
                fast_bus.events.borrow_mut().clear();
                let mut literal = fast.clone();
                let expected = literal.prepare_cached_control(&mut literal_bus).unwrap();
                literal.retire_cached_control(expected);
                let plan = fast.prepare_resident_control(100).unwrap();
                fast_bus.instruction_break(fast.state.pc, false);
                fast_bus
                    .sh2_pipeline_wait((plan.issue_at - fast_bus.clock.get()) as u32)
                    .unwrap();
                let mut values = [0; 3];
                for (i, &(address, at, discard)) in plan.fetches.iter().flatten().enumerate() {
                    if discard {
                        fast_bus.sh2_discard_prefetch();
                    }
                    fast_bus
                        .sh2_pipeline_wait((at - fast_bus.clock.get()) as u32)
                        .unwrap();
                    values[i] = fast_bus.fetch16(address).unwrap();
                }
                fast_bus
                    .sh2_pipeline_wait((plan.at - fast_bus.clock.get()) as u32)
                    .unwrap();
                fast.retire_cached_control(plan.finish(values));
                assert_eq!(fast, literal, "op={op:04X} t={t} mode={mode}");
                assert_eq!(
                    fast_bus.clock, literal_bus.clock,
                    "op={op:04X} t={t} mode={mode}"
                );
                assert_eq!(
                    timeline(&fast_bus.events.borrow()),
                    timeline(&literal_bus.events.borrow()),
                    "op={op:04X} t={t} mode={mode}"
                );
            }
        }
    }
}
