//! Original operand-cycle vectors. M68000PRM pp.4-13, 4-125, 4-173,
//! ABCD/SBCD/SUBX entries; MC68000UM §§5.1/5.4.1/6.3.9.1, fig.6-7.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cycle {
    Read(u32, usize),
    Write(u32, usize, u32),
}
struct Spy {
    ram: tests::Ram,
    cycles: RefCell<Vec<Cycle>>,
    fail: Option<(u32, usize)>,
}
impl Spy {
    fn read(&self, address: u32, width: usize) -> std::result::Result<(), BusError> {
        self.cycles.borrow_mut().push(Cycle::Read(address, width));
        if self.fail == Some((address, width)) {
            Err(BusError::Unmapped { address, width })
        } else {
            Ok(())
        }
    }
    fn operands(&self) -> Vec<Cycle> {
        self.cycles
            .borrow()
            .iter()
            .copied()
            .filter(|cycle| {
                let address = match cycle {
                    Cycle::Read(a, _) | Cycle::Write(a, _, _) => a,
                };
                (0x2000..0x4000).contains(address)
            })
            .collect()
    }
}
impl Memory for Spy {
    fn read8(&self, a: u32) -> std::result::Result<u8, BusError> {
        self.read(a, 1)?;
        self.ram.read8(a)
    }
    fn read16(&self, a: u32) -> std::result::Result<u16, BusError> {
        self.read(a, 2)?;
        self.ram.read16(a)
    }
    fn read32(&self, _: u32) -> std::result::Result<u32, BusError> {
        panic!("MC68EC000 must split longword cycles")
    }
    fn write8(&mut self, a: u32, v: u8) -> std::result::Result<(), BusError> {
        self.cycles.get_mut().push(Cycle::Write(a, 1, u32::from(v)));
        self.ram.write8(a, v)
    }
    fn write16(&mut self, a: u32, v: u16) -> std::result::Result<(), BusError> {
        self.cycles.get_mut().push(Cycle::Write(a, 2, u32::from(v)));
        self.ram.write16(a, v)
    }
    fn write32(&mut self, _: u32, _: u32) -> std::result::Result<(), BusError> {
        panic!("MC68EC000 must split longword cycles")
    }
}
fn fixture(op: u16) -> (M68k, Spy) {
    let (cpu, ram) = tests::fixture(&[op, 0x4E71, 0x4E71]);
    (
        cpu,
        Spy {
            ram,
            cycles: RefCell::default(),
            fail: None,
        },
    )
}

#[test]
fn scc_and_move_sr_read_the_destination_before_writing_with_one_ea_update() {
    for (op, size, clocks, value) in [
        (0x50D0, Size::Byte, 12, 255), // ST (A0)
        (0x51D8, Size::Byte, 12, 0),   // SF (A0)+
        (0x57E0, Size::Byte, 14, 255), // SEQ -(A0)
        (0x40D0, Size::Word, 12, 0x2714),
        (0x40D8, Size::Word, 12, 0x2714),
        (0x40E0, Size::Word, 14, 0x2714),
    ] {
        for fault in [false, true] {
            let (mut c, mut m) = fixture(op);
            c.state.sr = 0x2714;
            c.state.a[0] = if op & 0x38 == 0x20 {
                0x2000 + size.bytes()
            } else {
                0x2000
            };
            m.ram.write32(0x2000, 0xABCD1234).unwrap();
            m.fail = fault.then_some((0x2000, size.bytes() as usize));
            let elapsed = c.step(&mut m);
            let mut expected = vec![Cycle::Read(0x2000, size.bytes() as usize)];
            if !fault {
                expected.push(Cycle::Write(0x2000, size.bytes() as usize, value));
                assert_eq!(elapsed, clocks);
                assert_eq!(c.state.sr, 0x2714);
                assert_eq!(c.state.instructions, 1);
            } else {
                assert_eq!(c.state.last_vector, Some(2));
                assert_eq!(c.state.instructions, 0);
                let sp = c.state.a[7];
                assert_eq!(m.ram.read16(sp), Ok(0x15)); // data read, not write
                assert_eq!(m.ram.read32(sp + 2), Ok(0x2000));
                assert_eq!(m.ram.read16(sp + 6), Ok(op));
                assert_eq!(m.ram.read16(sp + 8), Ok(0x2714));
                assert_eq!(m.ram.read32(0x2000), Ok(0xABCD1234));
            }
            assert_eq!(m.operands(), expected, "op={op:04X}, fault={fault}");
            assert_eq!(
                c.state.a[0],
                0x2000 + if op & 0x38 == 0x18 { size.bytes() } else { 0 }
            );
        }
    }
}

#[test]
fn scc_all_conditions_and_register_forms_preserve_ccr_and_never_read_memory_data() {
    for sr in 0..32 {
        for condition in 0..16 {
            for memory in [false, true] {
                let op = 0x50C0 | condition << 8 | if memory { 0x10 } else { 0 };
                let (mut c, mut m) = fixture(op);
                c.state.sr = 0x2700 | sr;
                c.state.d[0] = 0xABCD1200;
                c.state.a[0] = 0x2000;
                // Independent truth table, M68000PRM table3-19.
                let carry = sr & 1 != 0;
                let overflow = sr & 2 != 0;
                let zero = sr & 4 != 0;
                let negative = sr & 8 != 0;
                let truth = [
                    true,
                    false,
                    !carry && !zero,
                    carry || zero,
                    !carry,
                    carry,
                    !zero,
                    zero,
                    !overflow,
                    overflow,
                    !negative,
                    negative,
                    negative == overflow,
                    negative != overflow,
                    !zero && negative == overflow,
                    zero || negative != overflow,
                ][condition as usize];
                c.step(&mut m);
                let expected = if truth { 255 } else { 0 };
                if memory {
                    assert_eq!(m.ram.read8(0x2000), Ok(expected));
                    assert_eq!(
                        m.operands(),
                        [
                            Cycle::Read(0x2000, 1),
                            Cycle::Write(0x2000, 1, u32::from(expected))
                        ]
                    );
                } else {
                    assert_eq!(c.state.d[0], 0xABCD1200 | u32::from(expected));
                    assert!(m.operands().is_empty());
                }
                assert_eq!(c.state.sr, 0x2700 | sr);
            }
        }
    }
}

#[test]
fn extended_memory_arithmetic_reads_source_before_destination_and_preserves_fault_progress() {
    for (op, width, result) in [
        (0xD109, 1, 8),
        (0xD149, 2, 8),
        (0xD189, 4, 8), // ADDX -(A1),-(A0)
        (0x9109, 1, 2),
        (0x9149, 2, 2),
        (0x9189, 4, 2), // SUBX
        (0xC109, 1, 8),
        (0x8109, 1, 2), // ABCD/SBCD
    ] {
        let failures = if width == 4 {
            vec![None, Some(0x2000), Some(0x2002), Some(0x3000), Some(0x3002)]
        } else {
            vec![None, Some(0x2000), Some(0x3000)]
        };
        for fail in failures {
            let (mut c, mut m) = fixture(op);
            c.state.a[0] = 0x3000 + width;
            c.state.a[1] = 0x2000 + width;
            let size = match width {
                1 => Size::Byte,
                2 => Size::Word,
                _ => Size::Long,
            };
            c.write(&mut m.ram, 0x2000, size, 3).unwrap();
            c.write(&mut m.ram, 0x3000, size, 5).unwrap();
            m.fail = fail.map(|a| (a, width.min(2) as usize));
            c.step(&mut m);
            let mut cycles = if width == 4 {
                vec![
                    Cycle::Read(0x2000, 2),
                    Cycle::Read(0x2002, 2),
                    Cycle::Read(0x3000, 2),
                    Cycle::Read(0x3002, 2),
                    Cycle::Write(0x3002, 2, result),
                    Cycle::Write(0x3000, 2, 0),
                ]
            } else {
                vec![
                    Cycle::Read(0x2000, width as usize),
                    Cycle::Read(0x3000, width as usize),
                    Cycle::Write(0x3000, width as usize, result),
                ]
            };
            if let Some(address) = fail {
                let failed_cycle = cycles
                    .iter()
                    .position(|c| matches!(c, Cycle::Read(a, _) if *a == address))
                    .unwrap();
                cycles.truncate(failed_cycle + 1);
            }
            assert_eq!(m.operands(), cycles, "op={op:04X}, fail={fail:?}");
            assert_eq!(c.state.a[1], 0x2000);
            assert_eq!(
                c.state.a[0],
                if fail.is_some_and(|address| address < 0x3000) {
                    0x3000 + width
                } else {
                    0x3000
                }
            );
            if let Some(address) = fail {
                assert_eq!(c.state.last_vector, Some(2));
                assert_eq!(m.ram.read32(c.state.a[7] + 2), Ok(address));
                assert_eq!(c.state.instructions, 0);
            } else {
                assert_eq!(c.state.instructions, 1);
                assert_eq!(c.get(&m.ram, Ea::Memory(0x3000), size).unwrap(), result);
            }
        }
    }
}

#[test]
fn extended_memory_arithmetic_aliases_every_address_register_including_a7_byte_stride() {
    for (base, size, result) in [
        (0xD108, Size::Byte, 8),
        (0xD148, Size::Word, 8),
        (0xD188, Size::Long, 8),
        (0x9108, Size::Byte, 2),
        (0x9148, Size::Word, 2),
        (0x9188, Size::Long, 2),
        (0xC108, Size::Byte, 8),
        (0x8108, Size::Byte, 2),
    ] {
        for reg in 0..8 {
            let (mut c, mut m) = fixture(base | reg << 9 | reg);
            let stride = if reg == 7 && matches!(size, Size::Byte) {
                2
            } else {
                size.bytes()
            };
            c.state.a[reg as usize] = 0x2000 + stride * 2;
            c.write(&mut m.ram, 0x2000 + stride, size, 3).unwrap();
            c.write(&mut m.ram, 0x2000, size, 5).unwrap();
            c.step(&mut m);
            assert_eq!(c.state.a[reg as usize], 0x2000);
            assert_eq!(c.state.instructions, 1);
            assert_eq!(c.get(&m.ram, Ea::Memory(0x2000), size).unwrap(), result);
            assert_eq!(
                m.operands()[0],
                Cycle::Read(0x2000 + stride, size.bytes().min(2) as usize)
            );
        }
    }
}
