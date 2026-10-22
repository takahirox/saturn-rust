//! Original operand-cycle vectors. M68000PRM pp.4-13, 4-125, 4-173,
//! ABCD/SBCD/SUBX entries; MC68000UM §§5.1/5.4.1/6.3.9.1, fig.6-7.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cycle {
    Acknowledge(u8),
    Read(u32, usize),
    Write(u32, usize, u32),
}
struct Spy {
    ram: tests::Ram,
    cycles: RefCell<Vec<Cycle>>,
    fail: Option<(u32, usize)>,
    acknowledgement: Option<std::result::Result<u8, BusError>>,
    waits: RefCell<Vec<(usize, u32)>>,
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
                    Cycle::Acknowledge(_) => return false,
                    Cycle::Read(a, _) | Cycle::Write(a, _, _) => a,
                };
                (0x2000..0x4000).contains(address)
            })
            .collect()
    }
}
impl Memory for Spy {
    fn m68k_internal_wait(&self, clocks: u32) -> std::result::Result<(), BusError> {
        self.waits
            .borrow_mut()
            .push((self.cycles.borrow().len(), clocks));
        Ok(())
    }
    fn m68k_interrupt_acknowledge(&mut self, level: u8) -> std::result::Result<u8, BusError> {
        self.cycles.get_mut().push(Cycle::Acknowledge(level));
        self.acknowledgement.unwrap_or(Ok(24 + level))
    }
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
            acknowledgement: None,
            waits: RefCell::default(),
        },
    )
}

#[test]
fn indexed_and_predecrement_calculations_precede_operand_cycles_across_instruction_families() {
    // UM table8-1: -(An) adds2 beyond (An); d8(An,Xn) adds2 beyond
    // d16(An). Both widths still use ordinary four-clock operand cycles.
    for reg in 0..8 {
        for indexed in [false, true] {
            for family in [
                0x1000, 0x3000, 0x2000, 0xD000, 0xD040, 0xD080, 0x9000, 0x9040, 0x9080, 0x8000,
                0x8040, 0x8080, 0xC000, 0xC040, 0xC080, 0xB000, 0xB040, 0xB080, 0x4A00, 0x4A40,
                0x4A80, 0x4200, 0x4240, 0x4280, 0x5200, 0x5240, 0x5280, 0x56C0, 0xE0C0, 0xC0C0,
                0x80C0,
            ] {
                let op = family | if indexed { 0x30 } else { 0x20 } | reg;
                let (mut c, mut m) = fixture(op);
                for a in &mut c.state.a {
                    *a = 0x2004;
                }
                c.state.d[1] = 0; // word-sized index
                m.ram.write16(0x102, 0x1000).unwrap();
                m.ram.write32(0x2000, 0x00100002).unwrap();
                m.ram.write32(0x2004, 0x00200003).unwrap();
                c.step(&mut m);
                assert_eq!(c.state.exceptions, 0, "{op:04X}");
                let waits = m.waits.borrow();
                assert_eq!(waits.len(), 1, "{op:04X}");
                let (position, clocks) = waits[0];
                assert_eq!(clocks, 2);
                let trace = m.cycles.borrow();
                assert!(
                    trace[..position]
                        .iter()
                        .all(|event| matches!(event, Cycle::Read(a, 2) if *a < 0x2000)),
                    "{op:04X}: {trace:?}"
                );
                assert!(
                    matches!(trace[position], Cycle::Read(0x2000..=0x2004, _)),
                    "{op:04X}: {trace:?}"
                );
            }
        }
    }
}

#[test]
fn paired_predecrements_share_one_wait_and_move_destination_has_no_source_wait() {
    // UM tables8-2 and8-11. A7's byte decrement is two, including alias.
    for src in 0..8 {
        for dst in 0..8 {
            for family in [
                0xD108, 0xD148, 0xD188, 0x9108, 0x9148, 0x9188, 0xC108, 0x8108,
            ] {
                let op = family | dst << 9 | src;
                let (mut c, mut m) = fixture(op);
                c.state.a.fill(0x2008);
                c.step(&mut m);
                assert_eq!(c.state.exceptions, 0, "{op:04X}");
                assert_eq!(m.waits.borrow().len(), 1, "{op:04X}");
                assert_eq!(m.waits.borrow()[0].1, 2);
            }
            for family in [0x1100, 0x3100, 0x2100] {
                let (mut c, mut m) = fixture(family | dst << 9 | src);
                c.state.a.fill(0x2008);
                c.step(&mut m);
                assert_eq!(c.state.exceptions, 0);
                assert!(m.waits.borrow().is_empty());
            }
        }
    }
}

#[test]
fn unlk_loads_the_frame_register_before_incrementing_the_selected_stack() {
    // M68000PRM p.4-194: all eight An encodings are legal, including A7.
    // Source-authored operation-order vectors; wrapping and 2-mod-4 longwords
    // distinguish an address-register load from a generic stack pop.
    for reg in 0..8 {
        for supervisor in [false, true] {
            for ccr in 0..32 {
                for loaded in [0x3000u32, 0x3002, 0xFFFF_FFFC] {
                    let (mut c, mut m) = fixture(0x4E58 | reg as u16);
                    c.state.sr = ccr | if supervisor { S } else { 0 };
                    c.state.a[7] = 0x4000;
                    c.state.a[reg] = 0x2002;
                    m.ram.write16(0x2002, (loaded >> 16) as u16).unwrap();
                    m.ram.write16(0x2004, loaded as u16).unwrap();
                    assert_eq!(c.step(&mut m), 12);
                    assert_eq!(c.state.sr, ccr | if supervisor { S } else { 0 });
                    assert_eq!(c.state.instructions, 1);
                    assert_eq!(c.state.exceptions, 0);
                    assert_eq!(
                        c.state.a[7],
                        if reg == 7 {
                            loaded.wrapping_add(4)
                        } else {
                            0x2006
                        }
                    );
                    if reg != 7 {
                        assert_eq!(c.state.a[reg], loaded);
                    }
                    assert_eq!(
                        m.operands(),
                        [Cycle::Read(0x2002, 2), Cycle::Read(0x2004, 2)]
                    );
                }
            }
        }
    }
}

#[test]
fn unlk_operand_fault_keeps_the_frame_pointer_and_does_not_increment_sp() {
    // MC68000UM §6.3.9.1/fig6-7: the second word can fault after the
    // first was read. UNLK cannot publish a partially loaded An or SP+4.
    for reg in 0..8 {
        for failed in [0x2002, 0x2004] {
            for supervisor in [false, true] {
                let op = 0x4E58 | reg as u16;
                let (mut c, mut m) = fixture(op);
                c.state.ssp = 0x4000;
                c.state.sr = X | Z | if supervisor { S } else { 0 };
                c.state.a[7] = 0x3000;
                c.state.a[reg] = 0x2002;
                m.ram.write32(0x2000, 0xDEADCAFE).unwrap();
                m.fail = Some((failed, 2));
                c.step(&mut m);
                assert_eq!(c.state.last_vector, Some(2));
                assert_eq!(c.state.instructions, 0);
                assert!(!c.state.halted);
                let sp = if supervisor { 0x1FF4 } else { 0x3FF2 };
                assert_eq!(c.state.a[7], sp);
                if reg != 7 {
                    assert_eq!(c.state.a[reg], 0x2002);
                }
                if !supervisor {
                    assert_eq!(c.state.usp, 0x2002);
                }
                assert_eq!(m.ram.read16(sp), Ok(if supervisor { 0x15 } else { 0x11 }));
                assert_eq!(m.ram.read16(sp + 2), Ok(0));
                assert_eq!(m.ram.read16(sp + 4), Ok(failed as u16));
                assert_eq!(m.ram.read16(sp + 6), Ok(op));
                assert_eq!(
                    m.ram.read16(sp + 8),
                    Ok(X | Z | if supervisor { S } else { 0 })
                );
                assert_eq!(
                    m.operands()
                        .iter()
                        .filter(|cycle| matches!(cycle, Cycle::Read(_, _)))
                        .count(),
                    if failed == 0x2002 { 1 } else { 2 }
                );
            }
        }
    }
}

#[test]
fn movem_load_reads_the_terminal_word_before_fetching_the_next_instruction() {
    // MC68000UM table8-10: 3+n reads (word), 3+2n (long), independent
    // of list contents. The initial reset/start opcode fetch adds one here.
    for (long, stride) in [(false, 2), (true, 4)] {
        for mask in [0, 1, 2, 3, 0x0101, 0x5555, 0xAAAA, 0xFFFF] {
            let (mut c, mut m) = fixture(0x4C90 | if long { 0x40 } else { 0 });
            m.ram.write16(0x102, mask).unwrap();
            c.state.a[0] = 0x2000;
            for i in 0..33 {
                m.ram.write16(0x2000 + i * 2, 0x8010 + i as u16).unwrap();
            }
            m.cycles.borrow_mut().clear();
            let count = mask.count_ones();
            assert_eq!(c.step(&mut m), 12 + count * if long { 8 } else { 4 });
            let mut expected = vec![Cycle::Read(0x100, 2), Cycle::Read(0x102, 2)];
            for i in 0..count * stride / 2 + 1 {
                expected.push(Cycle::Read(0x2000 + i * 2, 2));
            }
            expected.push(Cycle::Read(0x104, 2));
            assert_eq!(*m.cycles.borrow(), expected, "long={long}, mask={mask:04X}");
            assert_eq!(c.state.sr, 0x2700);
            assert_eq!(c.state.instructions, 1);
            assert_eq!(c.state.exceptions, 0);
        }
    }
}

#[test]
fn movem_terminal_read_fault_preserves_every_loaded_register_and_base_progress() {
    // The final read is a data access, even for an empty list. It must
    // propagate BERR, with no speculative following instruction read.
    for (long, stride) in [(false, 2), (true, 4)] {
        for reg in 0..8 {
            for mask in [0, 1, 3, 0x0101, 0x7FFF] {
                let op = 0x4C98 | reg as u16 | if long { 0x40 } else { 0 };
                let (mut c, mut m) = fixture(op);
                m.ram.write16(0x102, mask).unwrap();
                c.state.a[reg] = 0x2000;
                for i in 0..32 {
                    m.ram.write16(0x2000 + i * 2, 0x8010 + i as u16).unwrap();
                }
                let end = 0x2000 + mask.count_ones() * stride;
                m.fail = Some((end, 2));
                m.cycles.borrow_mut().clear();
                c.step(&mut m);
                let mut index = 0;
                for n in 0..15 {
                    if mask & (1 << n) == 0 {
                        continue;
                    }
                    let value = if long {
                        (0x8010 + index * 2) << 16 | (0x8011 + index * 2)
                    } else {
                        (0x8010 + index) as u16 as i16 as i32 as u32
                    };
                    if n < 8 {
                        assert_eq!(c.state.d[n], value);
                    } else if n - 8 != reg {
                        assert_eq!(c.state.a[n - 8], value);
                    }
                    index += 1;
                }
                assert_eq!(c.state.a[reg], if reg == 7 { end - 14 } else { end });
                assert_eq!(c.state.instructions, 0);
                assert_eq!(c.state.last_vector, Some(2));
                let sp = c.state.a[7];
                assert_eq!(m.ram.read16(sp), Ok(0x15)); // supervisor data read
                let frame_long = |at| {
                    u32::from(m.ram.read16(at).unwrap()) << 16
                        | u32::from(m.ram.read16(at + 2).unwrap())
                };
                assert_eq!(frame_long(sp + 2), end);
                assert_eq!(m.ram.read16(sp + 6), Ok(op));
                assert_eq!(frame_long(sp + 10), 0x104);
                assert!(!m.cycles.borrow().contains(&Cycle::Read(0x104, 2)));
            }
        }
    }
}

#[test]
fn movem_following_program_fetch_fault_occurs_after_all_operand_reads() {
    for (long, stride) in [(false, 2), (true, 4)] {
        for mask in [0, 1, 3, 0x0101] {
            let op = 0x4C98 | if long { 0x40 } else { 0 };
            let (mut c, mut m) = fixture(op);
            m.ram.write16(0x102, mask).unwrap();
            c.state.a[0] = 0x2000;
            m.ram.write32(0x2000, 0x81234567).unwrap();
            m.ram.write32(0x2004, 0x89ABCDEF).unwrap();
            m.fail = Some((0x104, 2));
            m.cycles.borrow_mut().clear();
            c.step(&mut m);
            let count = mask.count_ones();
            let end = 0x2000 + count * stride;
            let cycles = m.cycles.borrow();
            assert_eq!(
                cycles[count as usize * stride as usize / 2 + 2],
                Cycle::Read(end, 2)
            );
            assert_eq!(
                cycles[count as usize * stride as usize / 2 + 3],
                Cycle::Read(0x104, 2)
            );
            assert_eq!(c.state.a[0], end);
            if mask & 1 != 0 {
                assert_eq!(c.state.d[0], if long { 0x81234567 } else { 0xFFFF8123 });
            }
            if mask & 2 != 0 {
                assert_eq!(c.state.d[1], if long { 0x89ABCDEF } else { 0x4567 });
            }
            let sp = c.state.a[7];
            assert_eq!(m.ram.read16(sp), Ok(0x16)); // supervisor program read
            assert_eq!(m.ram.read32(sp + 2), Ok(0x104));
            assert_eq!(m.ram.read16(sp + 6), Ok(op));
            assert_eq!(m.ram.read32(sp + 10), Ok(0x106));
            assert_eq!(c.state.last_vector, Some(2));
            assert_eq!(c.state.instructions, 0);
        }
    }
}

#[test]
fn movem_load_ea_extensions_precede_operands_and_terminal_read_has_no_base_update() {
    for (bits, extensions, base) in [
        (0x10, vec![], 0x2000),
        (0x28, vec![0x0010], 0x1FF0),
        (0x30, vec![0x1810], 0x1FE0), // D1.L + d8
        (0x38, vec![0x2000], 0x1234),
        (0x39, vec![0x0000, 0x2000], 0x1234),
        (0x3A, vec![0x1EFC], 0x1234), // PC = extension address 0104
        (0x3B, vec![0x1820], 0x1234), // D1.L = 1EDC, PC=0104
    ] {
        let (mut c, mut m) = fixture(0x4C80 | bits);
        c.state.a[0] = base;
        c.state.d[1] = if bits == 0x3B { 0x1EDC } else { 0x10 };
        m.ram.write16(0x102, 1).unwrap();
        for (i, word) in extensions.iter().enumerate() {
            m.ram.write16(0x104 + i as u32 * 2, *word).unwrap();
        }
        m.ram.write16(0x2000, 0x8123).unwrap();
        m.ram.write16(0x2002, 0x4567).unwrap();
        m.cycles.borrow_mut().clear();
        c.step(&mut m);
        let next = 0x104 + extensions.len() as u32 * 2;
        let mut expected = (0x100..next)
            .step_by(2)
            .map(|a| Cycle::Read(a, 2))
            .collect::<Vec<_>>();
        expected.extend([
            Cycle::Read(0x2000, 2),
            Cycle::Read(0x2002, 2),
            Cycle::Read(next, 2),
        ]);
        assert_eq!(*m.cycles.borrow(), expected, "EA={bits:02X}");
        assert_eq!(c.state.d[0], 0xFFFF8123);
        assert_eq!(c.state.a[0], base);
        assert_eq!(c.state.instructions, 1);
    }
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

#[test]
fn external_reset_preserves_general_registers_and_usp_but_reloads_vectors_and_control_state() {
    // MC68000UM §§5.5/6.3.1, fig.5-30. Reset has no exception frame;
    // the four word reads must be SSP-high/low, PC-high/low, in that order.
    for supervisor in [false, true] {
        for ccr in 0..32 {
            let (mut c, mut m) = fixture(0x4E71);
            c.step(&mut m); // create prefetch which must not survive reset
            c.state.d = std::array::from_fn(|i| 0x89AB_CDE0 + i as u32);
            c.state.a = std::array::from_fn(|i| 0x1357_9B00 + i as u32);
            c.state.sr = T | if supervisor { S } else { 0 } | ccr;
            c.state.usp = 0xABCD0124;
            c.state.ssp = 0xDEAD8764;
            c.state.stopped = true;
            c.state.halted = true;
            c.set_interrupt(7);
            let old = c.state.clone();
            m.ram.write32(0, 0x00008102).unwrap(); // word-aligned SSP
            m.ram.write32(4, 0x00000200).unwrap();
            m.cycles.borrow_mut().clear();
            c.reset(&m).unwrap();
            assert_eq!(
                *m.cycles.borrow(),
                [
                    Cycle::Read(0, 2),
                    Cycle::Read(2, 2),
                    Cycle::Read(4, 2),
                    Cycle::Read(6, 2)
                ]
            );
            assert_eq!(c.state.d, old.d);
            assert_eq!(&c.state.a[..7], &old.a[..7]);
            assert_eq!(c.state.a[7], 0x8102);
            assert_eq!(c.state.ssp, 0x8102);
            assert_eq!(c.state.usp, if supervisor { old.usp } else { old.a[7] });
            assert_eq!(c.state.pc, 0x200);
            assert_eq!(c.state.sr, 0x2700 | ccr);
            assert!(!c.state.stopped && !c.state.halted);
            assert!(c.prefetch.is_empty());
            assert!(!c.level7_edge);
            assert_eq!(c.irq, 0);
            assert_eq!(c.state.instructions, 0);
            assert_eq!(c.state.exceptions, 0);
        }
    }
}

#[test]
fn autovector_device_vector_and_iack_berr_precede_the_short_exception_frame() {
    // MC68000UM §§6.2.5/6.3.2–4 and table8-14: five reads (including
    // IACK), three writes; IACK BERR selects24 without a bus-error frame.
    for level in 1..=7 {
        for response in [
            None,
            Some(Ok(15)),
            Some(Ok(32)),
            Some(Ok(64)),
            Some(Err(BusError::Unmapped {
                address: 0xFFFFFF,
                width: 1,
            })),
        ] {
            let (mut cpu, mut memory) = fixture(0x4E71);
            memory.acknowledgement = response;
            let vector = match response {
                None => 24 + level,
                Some(Ok(v)) => v,
                Some(Err(_)) => 24,
            };
            memory.ram.write32(u32::from(vector) * 4, 0x500).unwrap();
            memory.ram.write16(0x500, 0x4E71).unwrap();
            memory.ram.write16(0x502, 0x4E71).unwrap();
            cpu.state.sr = 0x2015;
            let sp = cpu.state.a[7];
            cpu.set_interrupt(level);
            assert_eq!(cpu.step(&mut memory), 44);
            assert_eq!(
                *memory.cycles.borrow(),
                [
                    Cycle::Acknowledge(level),
                    Cycle::Write(sp - 2, 2, 0x100),
                    Cycle::Write(sp - 4, 2, 0),
                    Cycle::Write(sp - 6, 2, 0x2015),
                    Cycle::Read(u32::from(vector) * 4, 2),
                    Cycle::Read(u32::from(vector) * 4 + 2, 2),
                    Cycle::Read(0x500, 2),
                    Cycle::Read(0x502, 2),
                ]
            );
            assert_eq!(cpu.state.sr, 0x2015 | u16::from(level) << 8);
            assert_eq!(cpu.state.pc, 0x500);
            assert_eq!(cpu.state.a[7], sp - 6);
            assert_eq!(cpu.state.last_vector, Some(vector));
            assert_eq!(cpu.state.exceptions, 1);
            assert!(!cpu.state.halted);
        }
    }
}

#[test]
fn interrupt_vector_fetch_fault_describes_irq_activity_even_for_a_trap_number() {
    // UM §§6.2.5/6.3.9.1: a device-provided vector number does not
    // change an interrupt into TRAP activity in a subsequent group0 frame.
    for vector in [15u8, 32, 64] {
        let (mut cpu, mut memory) = fixture(0x4E71);
        memory.acknowledgement = Some(Ok(vector));
        memory.ram.write32(8, 0x500).unwrap();
        memory.ram.write16(0x500, 0x4E71).unwrap();
        memory.ram.write16(0x502, 0x4E71).unwrap();
        memory.fail = Some((u32::from(vector) * 4, 2));
        cpu.state.sr = 0x2015;
        let sp = cpu.state.a[7];
        cpu.set_interrupt(3);
        cpu.step(&mut memory);
        assert_eq!(cpu.state.last_vector, Some(2));
        assert_eq!(cpu.state.pc, 0x500);
        assert_eq!(cpu.state.a[7], sp - 20);
        assert_eq!(memory.ram.read16(sp - 20), Ok(0x1D)); // read / I-N / supervisor data
        assert_eq!(memory.ram.read16(sp - 18), Ok(0));
        assert_eq!(memory.ram.read16(sp - 16), Ok(u16::from(vector) * 4));
        assert!(!cpu.state.halted);
    }
}
