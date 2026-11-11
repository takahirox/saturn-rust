//! All ordinary MOV load encodings retain their architectural addressing rules.
use super::tests::FlatMemory;
use super::*;
use std::cell::{Cell, RefCell};

struct Clocked {
    data: FlatMemory,
    clock: Cell<u64>,
    reads: RefCell<Vec<(u32, u64)>>,
    events: RefCell<Vec<(bool, u32, u64)>>,
}
impl Memory for Clocked {
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.data.write16(a, v)
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.data.write32(a, v)
    }
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock.get())
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_speculative_fetch(&self, _address: u32) -> bool {
        true
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        self.clock.set(self.clock.get() + u64::from(clocks));
        Ok(())
    }
    fn fetch16(&self, a: u32) -> Result<u16, BusError> {
        self.events.borrow_mut().push((true, a, self.clock.get()));
        self.data.read16(a)
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
        self.events.borrow_mut().push((false, a, self.clock.get()));
        self.data.read8(a)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
        self.events.borrow_mut().push((false, a, self.clock.get()));
        self.data.read16(a)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
        self.events.borrow_mut().push((false, a, self.clock.get()));
        self.data.read32(a)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.data.write8(a, v)
    }
}

#[test]
fn every_mov_load_addressing_form_samples_at_ma_end_and_preserves_sign_and_updates() {
    for pc in [0x100, 0x102] {
        for (op, address, width, destination, update) in [
            (0x6210, 0x800, 1, 2, 0),
            (0x6211, 0x800, 2, 2, 0),
            (0x6212, 0x800, 4, 2, 0),
            (0x6214, 0x800, 1, 2, 1),
            (0x6215, 0x800, 2, 2, 2),
            (0x6216, 0x800, 4, 2, 4),
            (0x6114, 0x800, 1, 1, 0),
            (0x6115, 0x800, 2, 1, 0),
            (0x6116, 0x800, 4, 1, 0),
            (0x5213, 0x80C, 4, 2, 0),
            (0x8413, 0x803, 1, 0, 0),
            (0x8513, 0x806, 2, 0, 0),
            (0x021C, 0x804, 1, 2, 0),
            (0x021D, 0x804, 2, 2, 0),
            (0x021E, 0x804, 4, 2, 0),
            (0x001E, 0x804, 4, 0, 0), // R0 indexed destination alias
            (0xC403, 0x903, 1, 0, 0),
            (0xC503, 0x906, 2, 0, 0),
            (0xC603, 0x90C, 4, 0, 0),
            (0x9203, pc + 10, 2, 2, 0),
            (0xD203, ((pc + 4) & !3) + 12, 4, 2, 0),
        ] {
            let mut memory = Clocked {
                data: FlatMemory(vec![0; 65536]),
                clock: Cell::new(32),
                reads: RefCell::default(),
                events: RefCell::default(),
            };
            memory.data.write16(pc, op).unwrap();
            let expected = match width {
                1 => {
                    memory.data.write8(address, 0x81).unwrap();
                    0xFFFFFF81
                }
                2 => {
                    memory.data.write16(address, 0x8123).unwrap();
                    0xFFFF8123
                }
                _ => {
                    memory.data.write32(address, 0x81234567).unwrap();
                    0x81234567
                }
            };
            let mut c = Cpu::start(pc, 0xF000);
            c.prime_fetch(&memory);
            c.state.r[0] = 4;
            c.state.r[1] = 0x800;
            c.state.gbr = 0x900;
            c.step(&mut memory).unwrap();
            assert_eq!(&*memory.reads.borrow(), &[(address, 34)], "{op:04x}");
            assert_eq!(c.state.r[destination], expected, "{op:04x}");
            if destination != 1 {
                assert_eq!(c.state.r[1], 0x800 + update);
            }
            assert_eq!(c.last_clocks(), if pc & 2 == 0 { 1 } else { 2 });
            assert_eq!(c.state.exceptions, 0);
        }
    }
}

#[test]
fn pc_relative_delay_slot_uses_branch_target_and_register_transfers_have_no_ma() {
    for (op, address) in [(0x9203, 0x408), (0xD203, 0x40C)] {
        let mut memory = Clocked {
            data: FlatMemory(vec![0; 65536]),
            clock: Cell::new(32),
            reads: RefCell::default(),
            events: RefCell::default(),
        };
        memory.data.write16(0x102, op).unwrap();
        memory.data.write32(address, 0x12345678).unwrap();
        let mut c = Cpu::start(0x102, 0xF000);
        c.prime_fetch(&memory);
        c.state.delay_target = Some(0x400);
        c.step(&mut memory).unwrap();
        assert_eq!(&*memory.reads.borrow(), &[(address, 34)]);
        assert_eq!(c.state.pc, 0x400);
    }
    for op in [0x6213, 0xE281, 0xC703, 0x0229] {
        let mut memory = Clocked {
            data: FlatMemory(vec![0; 65536]),
            clock: Cell::new(32),
            reads: RefCell::default(),
            events: RefCell::default(),
        };
        memory.data.write16(0x102, op).unwrap();
        let mut c = Cpu::start(0x102, 0xF000);
        c.prime_fetch(&memory);
        c.step(&mut memory).unwrap();
        assert!(memory.reads.borrow().is_empty());
        assert_eq!(memory.clock.get(), 32);
        assert_eq!(c.last_clocks(), 1);
    }
}

#[test]
fn prepared_indirect_load_matches_complete_cpu_and_ma_events_for_all_registers_and_wb_states() {
    for pc in [0x100u32, 0x102] {
        for op in 0x6000u16..=0x6FFF {
            if !matches!(op & 15, 0..=2 | 4..=6) {
                continue;
            }
            for mode in 0..6 {
                let make_bus = || {
                    let mut data = FlatMemory(vec![0; 4096]);
                    data.write16(pc, op).unwrap();
                    data.write32(0x800, 0x81234567).unwrap();
                    Clocked {
                        data,
                        clock: Cell::new(100),
                        reads: RefCell::default(),
                        events: RefCell::default(),
                    }
                };
                let (mut a_bus, mut b_bus) = (make_bus(), make_bus());
                let mut a = Cpu::start(pc, 0x800);
                a.state.r.fill(0x800);
                a.load_pipeline
                    .issue(0x6002 | if mode & 1 == 0 { 0 } else { 0xF00 });
                a.load_pipeline.load_split(mode & 2 != 0);
                if mode & 4 != 0 {
                    a.load_pipeline.retain_ex(99);
                }
                let mut b = a.clone();
                a.prime_fetch(&a_bus);
                b.prime_fetch(&b_bus);
                let plan = a.prepare_cached_instruction(op, 100).unwrap();
                a.execute_cached_instruction(plan, &mut a_bus);
                assert!(b.step_cached(&mut b_bus));
                assert_eq!(a, b, "op={op:04X} pc={pc:X} mode={mode}");
                assert_eq!(a_bus.clock.get(), b_bus.clock.get());
                assert_eq!(*a_bus.reads.borrow(), *b_bus.reads.borrow());
                assert_eq!(*a_bus.events.borrow(), *b_bus.events.borrow());
            }
        }
    }
}

#[test]
fn all_prepared_mov_load_encodings_match_literal_execution_and_bus_addresses() {
    for pc in [0x100u32, 0x102] {
        for op in 0..=u16::MAX {
            if op & 0xF0FF == 0x4026 {
                continue;
            }
            let mut state = Cpu::start(pc, 0x800);
            state.state.r.fill(0x800);
            state.state.gbr = 0x900;
            let Some(address) = timing::mov_load_address(op, &state.state) else {
                continue;
            };
            for mode in 0..6 {
                let make_bus = || {
                    let mut data = FlatMemory(vec![0; 16384]);
                    data.write16(pc, op).unwrap();
                    for (i, b) in 0x81234567u32.to_be_bytes().into_iter().enumerate() {
                        data.write8(address + i as u32, b).unwrap();
                    }
                    Clocked {
                        data,
                        clock: Cell::new(100),
                        reads: RefCell::default(),
                        events: RefCell::default(),
                    }
                };
                let (mut a_bus, mut b_bus) = (make_bus(), make_bus());
                let mut a = state.clone();
                a.load_pipeline
                    .issue(0x6002 | if mode & 1 == 0 { 0 } else { 0xF00 });
                a.load_pipeline.load_split(mode & 2 != 0);
                if mode & 4 != 0 {
                    a.load_pipeline.retain_ex(99);
                }
                let mut b = a.clone();
                a.prime_fetch(&a_bus);
                b.prime_fetch(&b_bus);
                let plan = a.prepare_cached_instruction(op, 100).unwrap();
                assert_eq!(plan.operand.unwrap().0, address);
                a.execute_cached_instruction(plan, &mut a_bus);
                assert!(b.step_cached(&mut b_bus));
                assert_eq!(a, b, "op={op:04X} pc={pc:X} mode={mode}");
                assert_eq!(a_bus.clock.get(), b_bus.clock.get());
                assert_eq!(*a_bus.reads.borrow(), *b_bus.reads.borrow());
                assert_eq!(*a_bus.events.borrow(), *b_bus.events.borrow());
            }
        }
    }
}

#[test]
fn prepared_delay_slots_keep_redirected_prefetch_and_literal_register_load_results() {
    for target in [0x200u32, 0x202] {
        for op in 0..=u16::MAX {
            let mut model = Cpu::start(0x102, 0x800);
            model.state.r.fill(0x800);
            model.state.gbr = 0x900;
            model.state.delay_target = Some(target);
            let address = timing::mov_load_address(op, &model.state);
            if scalar::decode(op).is_none() && (address.is_none() || op & 0xF0FF == 0x4026) {
                continue;
            }
            for mode in 0..6 {
                let make_bus = || {
                    let mut data = FlatMemory(vec![0; 16384]);
                    data.write16(0x100, 0xA000 | ((target - 0x104) / 2) as u16)
                        .unwrap();
                    data.write16(0x102, op).unwrap();
                    data.write16(target, 9).unwrap();
                    data.write16(target + 2, 9).unwrap();
                    if let Some(a) = address {
                        for (i, b) in 0x81234567u32.to_be_bytes().into_iter().enumerate() {
                            data.write8(a + i as u32, b).unwrap();
                        }
                    }
                    Clocked {
                        data,
                        clock: Cell::new(100),
                        reads: RefCell::default(),
                        events: RefCell::default(),
                    }
                };
                let (mut a_bus, mut b_bus) = (make_bus(), make_bus());
                let mut a = Cpu::start(0x100, 0x800);
                a.state.r.fill(0x800);
                a.state.gbr = 0x900;
                let mut b = a.clone();
                a.prime_fetch(&a_bus);
                b.prime_fetch(&b_bus);
                assert!(a.step_cached(&mut a_bus));
                assert!(b.step_cached(&mut b_bus));
                a_bus.clock.set(102);
                b_bus.clock.set(102);
                for c in [&mut a, &mut b] {
                    c.load_pipeline
                        .issue(0x6002 | if mode & 1 == 0 { 0 } else { 0xF00 });
                    c.load_pipeline.load_split(mode & 2 != 0);
                    if mode & 4 != 0 {
                        c.load_pipeline.retain_ex(101);
                    }
                }
                let plan = a.prepare_cached_instruction(op, 102).unwrap();
                assert_eq!(plan.future, target + 2);
                a.execute_cached_instruction(plan, &mut a_bus);
                assert!(b.step_cached(&mut b_bus));
                assert_eq!(a, b, "op={op:04X} target={target:X} mode={mode}");
                assert_eq!(a_bus.clock.get(), b_bus.clock.get());
                assert_eq!(*a_bus.reads.borrow(), *b_bus.reads.borrow());
                assert_eq!(*a_bus.events.borrow(), *b_bus.events.borrow());
            }
        }
    }
}
