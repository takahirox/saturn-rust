//! H Rev4 table5.8 and figs5.7/5.10; ideal single-state reference memory.
use super::*;
use std::cell::{Cell, RefCell};
#[derive(Debug, PartialEq, Eq)]
enum Event {
    Write(u64, u32, u32),
    Read(u64, u32),
    Fetch(u64, u32),
    Clear(u64),
    Vector(u64, u8),
}
struct Ideal {
    clock: Cell<u64>,
    events: RefCell<Vec<Event>>,
}
impl Ideal {
    fn access(&self, e: Event) {
        self.events.borrow_mut().push(e);
        self.clock.set(self.clock.get() + 1);
    }
}
impl Memory for Ideal {
    fn sh2_clock(&self) -> Option<u64> {
        Some(self.clock.get())
    }
    fn sh2_pipeline_wait(&mut self, n: u32) -> Result<(), BusError> {
        self.clock.set(self.clock.get() + u64::from(n));
        Ok(())
    }
    fn sh2_clear_nmi(&mut self) {
        self.events
            .borrow_mut()
            .push(Event::Clear(self.clock.get()));
    }
    fn sh2_interrupt_vector(&mut self, level: u8, v: u8) -> Result<u8, BusError> {
        self.events
            .borrow_mut()
            .push(Event::Vector(self.clock.get(), level));
        Ok(v)
    }
    fn read8(&self, _: u32) -> Result<u8, BusError> {
        unreachable!()
    }
    fn read16(&self, _: u32) -> Result<u16, BusError> {
        unreachable!()
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.access(Event::Read(self.clock.get(), a));
        Ok(0x400)
    }
    fn fetch16(&self, a: u32) -> Result<u16, BusError> {
        self.access(Event::Fetch(self.clock.get(), a));
        Ok(9)
    }
    fn write8(&mut self, _: u32, _: u8) -> Result<(), BusError> {
        unreachable!()
    }
    fn write16(&mut self, _: u32, _: u16) -> Result<(), BusError> {
        unreachable!()
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.access(Event::Write(self.clock.get(), a, v));
        Ok(())
    }
}
#[test]
fn manual_reference_response_is_ten_or_thirteen_states_with_intervening_phases() {
    for (nmi, level, vector) in [(true, 15, 11), (false, 6, 0x60), (false, 13, 0x46)] {
        let a = if nmi { 2 } else { 5 };
        let mut b = Ideal {
            clock: Cell::new(a),
            events: RefCell::new(vec![]),
        };
        let mut c = Cpu::start(0x200, 0x1000);
        c.state.sr = 0x203;
        assert!(
            if nmi {
                c.accept_nmi(&mut b)
            } else {
                c.accept_interrupt(&mut b, level, vector)
            }
            .unwrap()
        );
        let mut expected = vec![];
        if nmi {
            expected.push(Event::Clear(3));
        }
        expected.extend([
            Event::Write(a + 3, 0xFFC, 0x203),
            Event::Write(a + 4, 0xFF8, 0x200),
        ]);
        if !nmi {
            expected.push(Event::Vector(11, level));
        }
        expected.extend([
            Event::Read(a + 6, u32::from(vector) * 4),
            Event::Fetch(if nmi { 10 } else { 13 }, 0x400),
        ]);
        assert_eq!(*b.events.borrow(), expected);
        assert_eq!(c.state.sr, 0x203 | u32::from(level) << 4);
        assert_eq!(c.state.instructions, 0);
        assert_eq!(
            c.state.last_exception,
            Some(Exception {
                vector,
                saved_pc: 0x200
            })
        );
    }
}
