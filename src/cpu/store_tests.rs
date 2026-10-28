//! Independent ordinary MOV store vectors; docs/sh2-store-pipeline.md.
use super::tests::FlatMemory;
use super::*;
use std::cell::{Cell, RefCell};

struct Clocked {
    data: FlatMemory,
    clock: Cell<u64>,
    scheduled: bool,
    latency: u64,
    phases: RefCell<Vec<(u32, bool, u64)>>,
    writes: Vec<(u32, u32, u32, u64)>,
}
impl Clocked {
    fn write(&mut self, a: u32, width: u32, v: u32) -> Result<(), BusError> {
        if self.scheduled {
            self.clock.set(self.clock.get() + self.latency);
        }
        self.writes.push((a, width, v, self.clock.get()));
        write_operand(&mut self.data, a, width, v)
    }
}
impl Memory for Clocked {
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock.get())
    }
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn sh2_schedule_memory_access(&self, a: u32, write: bool, end: u64) -> bool {
        self.phases.borrow_mut().push((a, write, end));
        self.scheduled
    }
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        self.clock.set(self.clock.get() + u64::from(clocks));
        Ok(())
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.data.read8(a)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.data.read16(a)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.data.read32(a)
    }
    fn fetch16(&self, a: u32) -> Result<u16, BusError> {
        self.data.read16(a)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.write(a, 1, u32::from(v))
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.write(a, 2, u32::from(v))
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.write(a, 4, v)
    }
}
fn memory(pc: u32, op: u16, scheduled: bool, latency: u64) -> Clocked {
    let mut data = FlatMemory(vec![0xAA; 65536]);
    data.write16(pc, op).unwrap();
    Clocked {
        data,
        clock: Cell::new(32),
        scheduled,
        latency,
        phases: RefCell::default(),
        writes: vec![],
    }
}

#[test]
fn every_store_form_retains_source_lanes_flags_and_predecrement_alias_at_ma() {
    // Opcode, independently calculated EA, width, source, final R1.
    for (op, address, width, source, r1) in [
        (0x2120, 0x800, 1, 0x81234567, 0x800),
        (0x2121, 0x800, 2, 0x81234567, 0x800),
        (0x2122, 0x800, 4, 0x81234567, 0x800),
        (0x2124, 0x7FF, 1, 0x81234567, 0x7FF),
        (0x2125, 0x7FE, 2, 0x81234567, 0x7FE),
        (0x2126, 0x7FC, 4, 0x81234567, 0x7FC),
        (0x2114, 0x7FF, 1, 0x800, 0x7FF),
        (0x2115, 0x7FE, 2, 0x800, 0x7FE),
        (0x2116, 0x7FC, 4, 0x800, 0x7FC),
        (0x0124, 0x804, 1, 0x81234567, 0x800),
        (0x0125, 0x804, 2, 0x81234567, 0x800),
        (0x0126, 0x804, 4, 0x81234567, 0x800),
        (0x0004, 8, 1, 4, 0x800),
        (0x0005, 8, 2, 4, 0x800),
        (0x0006, 8, 4, 4, 0x800),
        (0x1123, 0x80C, 4, 0x81234567, 0x800),
        (0x8013, 0x803, 1, 4, 0x800),
        (0x8113, 0x806, 2, 4, 0x800),
        (0xC003, 0x903, 1, 4, 0x800),
        (0xC103, 0x906, 2, 4, 0x800),
        (0xC203, 0x90C, 4, 4, 0x800),
    ] {
        for pc in [0x100, 0x102] {
            for (scheduled, latency, completion) in [(false, 1, 34), (true, 1, 34), (true, 4, 37)] {
                let mut memory = memory(pc, op, scheduled, latency);
                let mut c = Cpu::start(pc, 0xF000);
                c.prime_fetch(&memory);
                c.state.r[0] = 4;
                c.state.r[1] = 0x800;
                c.state.r[2] = 0x81234567;
                c.state.gbr = 0x900;
                c.state.sr = SR_MASK;
                c.step(&mut memory).unwrap();
                let mask = match width {
                    1 => 0xFF,
                    2 => 0xFFFF,
                    _ => u32::MAX,
                };
                assert_eq!(&*memory.phases.borrow(), &[(address, true, 34)], "{op:04x}");
                assert_eq!(
                    memory.writes,
                    [(address, width, source & mask, completion)],
                    "{op:04x}"
                );
                assert_eq!(
                    &memory.data.0[address as usize..(address + width) as usize],
                    &source.to_be_bytes()[(4 - width) as usize..]
                );
                assert_eq!(memory.data.read8(address - 1), Ok(0xAA));
                assert_eq!(memory.data.read8(address + width), Ok(0xAA));
                assert_eq!(c.state.r[1], r1);
                assert_eq!(c.state.r[0], 4);
                assert_eq!(c.state.r[2], 0x81234567);
                assert_eq!(c.state.sr, SR_MASK);
                assert_eq!(c.state.instructions, 1);
                assert_eq!(c.state.exceptions, 0);
                assert_eq!(c.last_clocks(), if pc & 2 == 0 { 1 } else { 2 });
            }
        }
    }
}

#[test]
fn register_mov_is_ex_only_and_store_in_delay_slot_publishes_target_after_ma() {
    for op in [0x6213, 0xE281, 0x0229, 0xC703] {
        let mut memory = memory(0x102, op, false, 1);
        let mut c = Cpu::start(0x102, 0xF000);
        c.prime_fetch(&memory);
        c.step(&mut memory).unwrap();
        assert!(memory.phases.borrow().is_empty());
        assert!(memory.writes.is_empty());
        assert_eq!(memory.clock.get(), 32);
    }
    let mut memory = memory(0x102, 0x2FF6, true, 4);
    let mut c = Cpu::start(0x102, 0x800);
    c.prime_fetch(&memory);
    c.state.delay_target = Some(0x400);
    c.step(&mut memory).unwrap();
    assert_eq!(memory.writes, [(0x7FC, 4, 0x800, 37)]);
    assert_eq!(c.state.r[15], 0x7FC);
    assert_eq!(c.state.pc, 0x400);
    assert_eq!(c.state.delay_target, None);
}
