//! All ordinary MOV load encodings retain their architectural addressing rules.
use super::tests::FlatMemory;
use super::*;
use std::cell::{Cell, RefCell};

struct Clocked {
    data: FlatMemory,
    clock: Cell<u64>,
    reads: RefCell<Vec<(u32, u64)>>,
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
    fn sh2_pipeline_wait(&mut self, clocks: u32) -> Result<(), BusError> {
        self.clock.set(self.clock.get() + u64::from(clocks));
        Ok(())
    }
    fn fetch16(&self, a: u32) -> Result<u16, BusError> {
        self.data.read16(a)
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
        self.data.read8(a)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
        self.data.read16(a)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.reads.borrow_mut().push((a, self.clock.get()));
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
