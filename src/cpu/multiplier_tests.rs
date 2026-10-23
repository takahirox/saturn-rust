//! Independent literal vectors from docs/sh2-multiplier.md, not timing helpers.
use super::*;
use crate::bus::Memory;
use crate::cpu::tests::FlatMemory;

fn program(ops: &[u16]) -> (Cpu, FlatMemory) {
    let mut m = FlatMemory(vec![0; 65536]);
    for (i, &op) in ops.iter().enumerate() {
        m.write16(0x100 + i as u32 * 2, op).unwrap();
    }
    m.write32(0x1000, 3).unwrap();
    m.write32(0x2000, 7).unwrap();
    let mut c = Cpu::start(0x100, 0x8000);
    c.state.r[1] = 3;
    c.state.r[2] = 7;
    c.state.r[3] = 0x1000;
    c.state.r[4] = 0x2000;
    c.state.r[5] = 0x3000;
    c.state.r[6] = 0x12345678;
    c.state.r[8] = 0x4000;
    c.state.mach = 0x13579BDF;
    c.state.sr = SR_MASK & !S;
    (c, m)
}

#[test]
fn multiplier_all_producer_consumer_families_and_progressive_spacing() {
    // (opcode, baseline, delay for single MA, delay for second MA).
    for (producer, base, single, second) in [
        (0x212E, 1, [1, 0, 0, 0], [0, 0, 0, 0]),
        (0x212F, 1, [1, 0, 0, 0], [0, 0, 0, 0]),
        (0x0127, 2, [3, 2, 1, 0], [2, 1, 0, 0]),
        (0x3125, 2, [3, 2, 1, 0], [2, 1, 0, 0]),
        (0x312D, 2, [3, 2, 1, 0], [2, 1, 0, 0]),
        (0x434F, 2, [1, 0, 0, 0], [0, 0, 0, 0]),
        (0x034F, 2, [3, 2, 1, 0], [2, 1, 0, 0]),
        (0x0028, 1, [0; 4], [0; 4]),
        (0x460A, 1, [0; 4], [0; 4]),
        (0x461A, 1, [0; 4], [0; 4]),
        (0x070A, 1, [0; 4], [0; 4]),
        (0x071A, 1, [0; 4], [0; 4]),
        (0x4802, 1, [0; 4], [0; 4]),
        (0x4812, 1, [0; 4], [0; 4]),
        (0x4806, 1, [0; 4], [0; 4]),
        (0x4816, 1, [0; 4], [0; 4]),
    ] {
        for (consumer, cost, two) in [
            (0x212E, 1, false),
            (0x212F, 1, false),
            (0x0127, 2, true),
            (0x3125, 2, true),
            (0x312D, 2, true),
            (0x434F, 2, true),
            (0x034F, 2, true),
            (0x0028, 1, false),
            (0x460A, 1, false),
            (0x461A, 1, false),
            (0x050A, 1, false),
            (0x051A, 1, false),
            (0x4502, 1, false),
            (0x4512, 1, false),
            (0x4506, 1, false),
            (0x4516, 1, false),
        ] {
            for spacing in 0..4 {
                let mut ops = vec![producer];
                ops.extend(std::iter::repeat_n(9, spacing));
                ops.push(consumer);
                let (mut c, mut m) = program(&ops);
                let mut clocks = 0;
                for i in 0..ops.len() {
                    c.step(&mut m).unwrap();
                    let expected = if i == 0 {
                        base
                    } else if i <= spacing {
                        1
                    } else {
                        cost + if two {
                            second[spacing]
                        } else {
                            single[spacing]
                        }
                    };
                    assert_eq!(
                        c.last_clocks(),
                        expected,
                        "{producer:04x} -> {consumer:04x}, spacing {spacing}, step {i}"
                    );
                    clocks += c.last_clocks();
                }
                assert_eq!(
                    clocks,
                    base + spacing as u32
                        + cost
                        + if two {
                            second[spacing]
                        } else {
                            single[spacing]
                        }
                );
                assert_eq!(c.state.sr, SR_MASK & !S);
                assert_eq!(c.state.r[1], 3);
                assert_eq!(c.state.r[2], 7);
                assert_eq!(c.state.r[6], 0x12345678);
                assert_eq!(c.state.exceptions, 0);
            }
        }
    }
}

#[test]
fn multiplier_results_signed_unsigned_preservation_and_transfer_directions() {
    for (op, lo, hi) in [
        (0x212E, 0x0002FFFA, 0x13579BDF),
        (0x212F, 0xFFFFFFFA, 0x13579BDF),
        (0x0127, 0xFFFFFFFA, 0x13579BDF),
        (0x3125, 0xFFFFFFFA, 2),
        (0x312D, 0xFFFFFFFA, 0xFFFFFFFF),
    ] {
        let short = matches!(op, 0x212E | 0x212F);
        for (transfer, register) in [(0x050A, hi), (0x051A, lo)] {
            let (mut c, mut m) = program(&[op, transfer, 0x7501]);
            c.state.r[1] = (-2i32) as u32;
            c.state.r[2] = 3;
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), if short { 1 } else { 2 });
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), if short { 2 } else { 4 });
            assert_eq!(
                (c.state.mach, c.state.macl, c.state.r[5]),
                (hi, lo, register)
            );
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), 2); // preserve STS WB -> GPR EX hazard
            assert_eq!(c.state.r[5], register.wrapping_add(1));
            assert_eq!(c.state.sr, SR_MASK & !S);
        }
        for (transfer, expected) in [(0x4502, hi), (0x4512, lo)] {
            let (mut c, mut m) = program(&[op, transfer, 0x4506]);
            c.state.r[1] = (-2i32) as u32;
            c.state.r[2] = 3;
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), if short { 1 } else { 2 });
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), if short { 2 } else { 4 });
            assert_eq!((c.state.r[5], m.read32(0x2FFC)), (0x2FFC, Ok(expected)));
            c.step(&mut m).unwrap();
            assert_eq!((c.state.r[5], c.state.mach), (0x3000, expected));
        }
    }
}

#[test]
fn multiplier_consecutive_macs_saturate_and_update_both_addresses_once() {
    for (op, width, value, initial, expected, high) in [
        (0x434F, 2, 0x7FFF, 0x7FFFFF00u64, 0x7FFFFFFFu64, 0x1234),
        (
            0x034F,
            4,
            0x7FFFFFFF,
            0x00007FFFFFFFFF00,
            0x00007FFFFFFFFFFF,
            0,
        ),
    ] {
        let (mut c, mut m) = program(&[op, op, 0x051A]);
        c.state.sr |= S;
        c.state.set_accumulator(initial);
        if width == 2 {
            c.state.mach = high;
        }
        for base in [0x1000, 0x2000] {
            for off in [0, width] {
                if width == 2 {
                    m.write16(base + off, value as u16).unwrap();
                } else {
                    m.write32(base + off, value).unwrap();
                }
            }
        }
        for clocks in if width == 2 { [2, 2, 2] } else { [2, 4, 4] } {
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), clocks);
        }
        assert_eq!(c.state.macl, expected as u32);
        assert_eq!(
            c.state.mach,
            if width == 2 {
                high
            } else {
                (expected >> 32) as u32
            }
        );
        assert_eq!(
            (c.state.r[3], c.state.r[4]),
            (0x1000 + width * 2, 0x2000 + width * 2)
        );
        assert_eq!(c.state.r[5], expected as u32);
        assert_eq!(c.state.sr, SR_MASK);
    }
}

#[test]
fn irq_and_nmi_entry_drain_the_multiplier_without_clobbering_unsaved_mac_registers() {
    for nmi in [false, true] {
        let (mut c, mut m) = program(&[0x312D, 0x051A]);
        c.state.sr = 1;
        let vector = if nmi { 11 } else { 64 };
        c.state.vbr = 0x3000;
        m.write32(0x3000 + vector * 4, 0x300).unwrap();
        for (i, op) in [0x0A0A, 0x0B1A, 0x002B, 9].iter().enumerate() {
            m.write16(0x300 + i as u32 * 2, *op).unwrap();
        }
        c.step(&mut m).unwrap();
        assert_eq!(c.last_clocks(), 2);
        assert!(
            if nmi {
                c.accept_nmi(&mut m)
            } else {
                c.accept_interrupt(&mut m, 7, 64)
            }
            .unwrap()
        );
        assert_eq!(c.last_clocks(), 8);
        assert_eq!((m.read32(0x7FF8), m.read32(0x7FFC)), (Ok(0x102), Ok(1)));
        assert_eq!((c.state.mach, c.state.macl), (0, 21));
        c.step(&mut m).unwrap();
        assert_eq!(c.last_clocks(), 1);
        c.step(&mut m).unwrap();
        assert_eq!(c.last_clocks(), 1);
        assert_eq!((c.state.r[10], c.state.r[11]), (0, 21));
        c.step(&mut m).unwrap(); // RTE
        c.step(&mut m).unwrap(); // delay slot
        assert_eq!((c.state.pc, c.state.r[15], c.state.sr), (0x102, 0x8000, 1));
        c.step(&mut m).unwrap();
        assert_eq!((c.last_clocks(), c.state.r[5]), (1, 21));
        assert_eq!(c.state.exceptions, 1);
    }
}

struct FetchMemory(FlatMemory);
impl Memory for FetchMemory {
    fn sh2_fetch_slots(&self) -> bool {
        true
    }
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        self.0.read8(a)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        self.0.read16(a)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        self.0.read32(a)
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.0.write8(a, v)
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.0.write16(a, v)
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.0.write32(a, v)
    }
}
#[test]
fn multiplier_if_ma_alignment_splits_and_documented_consecutive_mac_overlap() {
    for (op, states) in [
        (0x212F, [1, 3, 1, 3]),
        (0x312D, [3, 5, 3, 5]),
        (0x434F, [3, 3, 3, 3]),
        (0x034F, [3, 5, 3, 5]),
        (0x0028, [1, 2, 1, 2]),
        (0x050A, [1, 2, 1, 2]),
        (0x051A, [1, 2, 1, 2]),
        (0x460A, [1, 2, 1, 2]),
        (0x461A, [1, 2, 1, 2]),
    ] {
        let (mut c, m) = program(&[op; 4]);
        let mut m = FetchMemory(m);
        for expected in states {
            c.step(&mut m).unwrap();
            assert_eq!(c.last_clocks(), expected, "{op:04x}");
        }
    }
}
