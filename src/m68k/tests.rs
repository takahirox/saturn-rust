//! Original instruction words and expected architectural results, derived from
//! M68000PRM §§2 (addressing), 3–4 (integer operations), appendix B (frames).
use super::*;

struct Ram(Vec<u8>);
impl Default for Ram {
    fn default() -> Self {
        Self(vec![0; 65536])
    }
}
impl Memory for Ram {
    fn read8(&self, a: u32) -> std::result::Result<u8, BusError> {
        self.0.get(a as usize).copied().ok_or(BusError::Unmapped {
            address: a,
            width: 1,
        })
    }
    fn read16(&self, a: u32) -> std::result::Result<u16, BusError> {
        if a & 1 != 0 {
            return Err(BusError::Unaligned {
                address: a,
                width: 2,
            });
        }
        Ok(u16::from(self.read8(a)?) << 8 | u16::from(self.read8(a + 1)?))
    }
    fn read32(&self, a: u32) -> std::result::Result<u32, BusError> {
        // Deliberately reject 2-mod-4 alignment: the core must split long cycles.
        if a & 3 != 0 {
            return Err(BusError::Unaligned {
                address: a,
                width: 4,
            });
        }
        Ok(u32::from(self.read16(a)?) << 16 | u32::from(self.read16(a + 2)?))
    }
    fn write8(&mut self, a: u32, v: u8) -> std::result::Result<(), BusError> {
        *self.0.get_mut(a as usize).ok_or(BusError::Unmapped {
            address: a,
            width: 1,
        })? = v;
        Ok(())
    }
    fn write16(&mut self, a: u32, v: u16) -> std::result::Result<(), BusError> {
        if a & 1 != 0 {
            return Err(BusError::Unaligned {
                address: a,
                width: 2,
            });
        }
        self.write8(a, (v >> 8) as u8)?;
        self.write8(a + 1, v as u8)
    }
    fn write32(&mut self, a: u32, v: u32) -> std::result::Result<(), BusError> {
        if a & 3 != 0 {
            return Err(BusError::Unaligned {
                address: a,
                width: 4,
            });
        }
        self.write16(a, (v >> 16) as u16)?;
        self.write16(a + 2, v as u16)
    }
}
fn fixture(words: &[u16]) -> (M68k, Ram) {
    let mut ram = Ram::default();
    ram.write32(0, 0xF000).unwrap();
    ram.write32(4, 0x100).unwrap();
    for v in 2..48 {
        ram.write32(v * 4, 0x800 + v * 8).unwrap();
    }
    for (i, op) in words.iter().enumerate() {
        ram.write16(0x100 + i as u32 * 2, *op).unwrap();
    }
    let mut cpu = M68k::default();
    cpu.reset(&ram).unwrap();
    (cpu, ram)
}
fn run(cpu: &mut M68k, ram: &mut Ram, n: usize) {
    for _ in 0..n {
        cpu.step(ram);
        assert!(!cpu.state.halted);
    }
}

#[test]
fn move_bus_clocks_match_primary_tables_for_every_source_and_destination_mode() {
    // MC68000UM tables8-2/8-3, literal rows in destination order:
    // Dn,An,(An),(An)+,-(An),d16(An),d8(An,Xn),abs.W,abs.L.
    let rows = [
        (
            0x00,
            vec![],
            [4, 4, 8, 8, 8, 12, 14, 12, 16],
            [4, 4, 12, 12, 12, 16, 18, 16, 20],
        ),
        (
            0x08,
            vec![],
            [4, 4, 8, 8, 8, 12, 14, 12, 16],
            [4, 4, 12, 12, 12, 16, 18, 16, 20],
        ),
        (
            0x10,
            vec![],
            [8, 8, 12, 12, 12, 16, 18, 16, 20],
            [12, 12, 20, 20, 20, 24, 26, 24, 28],
        ),
        (
            0x18,
            vec![],
            [8, 8, 12, 12, 12, 16, 18, 16, 20],
            [12, 12, 20, 20, 20, 24, 26, 24, 28],
        ),
        (
            0x20,
            vec![],
            [10, 10, 14, 14, 14, 18, 20, 18, 22],
            [14, 14, 22, 22, 22, 26, 28, 26, 30],
        ),
        (
            0x28,
            vec![4],
            [12, 12, 16, 16, 16, 20, 22, 20, 24],
            [16, 16, 24, 24, 24, 28, 30, 28, 32],
        ),
        (
            0x30,
            vec![0],
            [14, 14, 18, 18, 18, 22, 24, 22, 26],
            [18, 18, 26, 26, 26, 30, 32, 30, 34],
        ),
        (
            0x38,
            vec![0x2000],
            [12, 12, 16, 16, 16, 20, 22, 20, 24],
            [16, 16, 24, 24, 24, 28, 30, 28, 32],
        ),
        (
            0x39,
            vec![0, 0x2000],
            [16, 16, 20, 20, 20, 24, 26, 24, 28],
            [20, 20, 28, 28, 28, 32, 34, 32, 36],
        ),
        (
            0x3A,
            vec![16],
            [12, 12, 16, 16, 16, 20, 22, 20, 24],
            [16, 16, 24, 24, 24, 28, 30, 28, 32],
        ),
        (
            0x3B,
            vec![16],
            [14, 14, 18, 18, 18, 22, 24, 22, 26],
            [18, 18, 26, 26, 26, 30, 32, 30, 34],
        ),
        (
            0x3C,
            vec![0x1234],
            [8, 8, 12, 12, 12, 16, 18, 16, 20],
            [12, 12, 20, 20, 20, 24, 26, 24, 28],
        ),
    ];
    let destinations = [
        (0, vec![]),
        (9, vec![]),
        (0x11, vec![]),
        (0x19, vec![]),
        (0x21, vec![]),
        (0x29, vec![4]),
        (0x31, vec![0]),
        (0x38, vec![0x3000]),
        (0x39, vec![0, 0x3000]),
    ];
    for (src, extensions, word, long) in rows {
        for (column, (dst, dest_extensions)) in destinations.iter().enumerate() {
            for size in [1, 2, 3] {
                if size == 1 && (src == 8 || *dst == 9) {
                    continue;
                }
                let mut code = vec![(size << 12) | ((dst & 7) << 9) | ((dst & 0x38) << 3) | src];
                code.extend(&extensions);
                if src == 0x3C && size == 2 {
                    code.push(0x5678);
                }
                code.extend(dest_extensions);
                let (mut cpu, mut ram) = fixture(&code);
                cpu.state.a[0] = 0x2000;
                cpu.state.a[1] = 0x3000;
                cpu.state.d[0] = 0x12345678;
                assert_eq!(
                    cpu.step(&mut ram),
                    if size == 2 {
                        long[column]
                    } else {
                        word[column]
                    },
                    "MOVE size{size} {src:02X}->{dst:02X}"
                );
                assert_eq!(cpu.state.exceptions, 0);
            }
        }
    }
}

#[test]
fn opcode_branch_register_count_and_multiply_costs_use_primary_clock_vectors() {
    // MC68000UM tables8-4–8-14; all values are 16-bit-bus clock periods.
    for (words, ccr, d0, expected) in [
        (vec![0x6602], 0, 0, 10),
        (vec![0x6602], 4, 0, 8),
        (vec![0x6600, 2], 4, 0, 12),
        (vec![0x51C8, 0xFFFE], 0, 1, 10),
        (vec![0x51C8, 0xFFFE], 0, 0, 14),
        (vec![0x50C8, 0], 0, 0, 12),
        (vec![0x57C0], 0, 0, 4),
        (vec![0x57C0], 4, 0, 6),
        (vec![0xD041], 0, 0, 4),
        (vec![0xD081], 0, 0, 8),
        (vec![0xD090], 0, 0, 14),
        (vec![0xB081], 0, 0, 6),
        (vec![0x4ED0], 0, 0, 8),
        (vec![0x4EF8, 0x2000], 0, 0, 10),
        (vec![0x4EF9, 0, 0x2000], 0, 0, 12),
        (vec![0x4E90], 0, 0, 16),
        (vec![0x41D0], 0, 0, 4),
        (vec![0x41F0, 0], 0, 0, 12),
        (vec![0x4879, 0, 0x2000], 0, 0, 20),
        (vec![0x48D0, 3], 0, 0, 24),
        (vec![0x4CD0, 3], 0, 0, 28),
        (vec![0x48E0, 3], 0, 0, 24),
        (vec![0x48E8, 3, 4], 0, 0, 28),
        (vec![0xE388], 0, 0, 10),
        (vec![0xE348], 0, 0, 8),
        (vec![0xE3D0], 0, 0, 12),
        (vec![0x4280], 0, 0, 6),
        (vec![0x4290], 0, 0, 20),
        (vec![0x4A90], 0, 0, 12),
        (vec![0x40C0], 0, 0, 6),
        (vec![0x46FC, 0x2700], 0, 0, 16),
        (vec![0x0280, 0, 0], 0, 0, 14),
        (vec![0x06A8, 0, 1, 4], 0, 0, 32),
        (vec![0xC101], 0, 0, 6),
        (vec![0xC109], 0, 0, 18),
        (vec![0xD101], 0, 0, 4),
        (vec![0xD189], 0, 0, 30),
        (vec![0xB388], 0, 0, 20),
        (vec![0x80C1], 0, 0, 38),
        (vec![0x4190], 0, 1, 44),
    ] {
        let (mut cpu, mut ram) = fixture(&words);
        cpu.state.sr = 0x2700 | ccr;
        cpu.state.d[0] = d0;
        cpu.state.a[0] = 0x2000;
        cpu.state.a[1] = 0x3000;
        assert_eq!(cpu.step(&mut ram), expected, "opcode {:04X}", words[0]);
    }
    for (source, unsigned, signed) in [
        (0, 38, 38),
        (1, 40, 42),
        (0x8000, 40, 40),
        (0xFFFF, 70, 40),
        (0x5555, 54, 70),
        (0xAAAA, 54, 68),
    ] {
        for (opcode, clocks) in [(0xC0C1, unsigned), (0xC1C1, signed)] {
            let (mut cpu, mut ram) = fixture(&[opcode]);
            cpu.state.d[1] = source;
            assert_eq!(
                cpu.step(&mut ram),
                clocks,
                "MUL {opcode:04X} source {source:04X}"
            );
        }
    }
}
#[test]
fn reset_move_movea_moveq_and_big_endian_word_aligned_longs() {
    let (mut c, mut m) = fixture(&[
        0x70FF, 0x123C, 0x0080, 0x347C, 0xFFFE, 0x23C0, 0x0000, 0x2002, 0x2639, 0x0000, 0x2002,
    ]);
    assert_eq!(
        (c.state.pc, c.state.a[7], c.state.sr),
        (0x100, 0xF000, 0x2700)
    );
    c.state.d[1] = 0x12345600;
    run(&mut c, &mut m, 5);
    assert_eq!(c.state.d[0], u32::MAX);
    assert_eq!(c.state.d[1], 0x12345680);
    assert_eq!(c.state.a[2], 0xFFFF_FFFE);
    assert_eq!(c.state.d[3], u32::MAX);
    assert_eq!(&m.0[0x2002..0x2006], &[255; 4]);
    assert_eq!(c.state.sr & 15, N);
}
#[test]
fn every_effective_address_mode_and_pc_relative_extension_base() {
    // MOVE.W <ea>,D0. Expected independently from literal register/displacement setup.
    for (ea, ext, address, expected_a) in [
        (0x10, vec![], 0x2000, 0x2000),
        (0x18, vec![], 0x2000, 0x2002),
        (0x20, vec![], 0x1FFE, 0x1FFE),
        (0x28, vec![0xFFFC], 0x1FFC, 0x2000),
        (0x30, vec![0x1004], 0x2002, 0x2000), // D1.W = -2, displacement +4
        (0x38, vec![0x2100], 0x2100, 0x2000),
        (0x39, vec![0, 0x2200], 0x2200, 0x2000),
        (0x3A, vec![0x2000], 0x2102, 0x2000),
        (0x3B, vec![0x1804], 0x106, 0x2000), // D1.L = 0, base 102 +4
    ] {
        let mut words = vec![0x3000 | ea];
        words.extend(ext);
        let (mut c, mut m) = fixture(&words);
        c.state.a[0] = 0x2000;
        c.state.d[1] = if ea == 0x3B { 0 } else { 0xFFFF_FFFE };
        m.write16(address, 0xBEEF).unwrap();
        c.step(&mut m);
        assert_eq!(c.state.d[0], 0xBEEF, "ea={ea:02x}");
        assert_eq!(c.state.a[0], expected_a, "ea={ea:02x}");
    }
    let (mut c, mut m) = fixture(&[0x3010, 0x3011, 0x3012]);
    c.state.a[0] = 0x01002000;
    m.write16(0x2000, 0x6789).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x6789); // upper eight address bits never reach pins
}
#[test]
fn byte_stack_postincrement_and_predecrement_use_two_bytes() {
    let (mut c, mut m) = fixture(&[0x101F, 0x1F00]);
    m.write8(0xF000, 0x81).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.a[7], 0xF002);
    c.step(&mut m);
    assert_eq!(c.state.a[7], 0xF000);
    assert_eq!(m.0[0xF000], 0x81);
}
#[test]
fn arithmetic_flags_all_widths_and_extend_zero_chains() {
    // ADD, SUB, CMP independent sign/carry boundaries for byte, word and long.
    for (size_bits, mask, sign) in [
        (0, 255, 128),
        (0x40, 65535, 32768),
        (0x80, u32::MAX, 0x80000000),
    ] {
        for (dst, src, result, flags) in [
            (mask, 1, 0, X | Z | C),
            (sign - 1, 1, sign, N | V),
            (sign, sign, 0, X | Z | V | C),
        ] {
            let (mut c, mut m) = fixture(&[0xD001 | size_bits]);
            c.state.d[0] = dst;
            c.state.d[1] = src;
            c.step(&mut m);
            assert_eq!(c.state.d[0] & mask, result);
            assert_eq!(c.state.sr & 31, flags);
        }
        let (mut c, mut m) = fixture(&[0x9001 | size_bits, 0xB001 | size_bits]);
        c.state.d[0] = 0;
        c.state.d[1] = 1;
        c.step(&mut m);
        assert_eq!(c.state.d[0] & mask, mask);
        assert_eq!(c.state.sr & 31, X | N | C);
        let d = c.state.d;
        c.step(&mut m);
        assert_eq!(c.state.d, d);
        assert_eq!(c.state.sr & X, X);
    }
    let (mut c, mut m) = fixture(&[0xD101, 0xD101, 0x9101]);
    c.state.sr |= Z | X;
    c.state.d[0] = 255;
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, X | Z | C);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 1);
    assert_eq!(c.state.sr & Z, 0);
    c.state.d[1] = 1;
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0);
    assert_eq!(c.state.sr & Z, 0);
}
#[test]
fn immediate_quick_and_address_arithmetic_preserves_flags_and_sign_extends() {
    let (mut c, mut m) = fixture(&[
        0x0680, 0x7FFF, 0xFFFF, 0x5280, 0x0440, 0, 0x0A00, 0x00FF, 0xD0FC, 0xFFFF, 0x5248, 0xB0FC,
        0xFFFF,
    ]);
    run(&mut c, &mut m, 2);
    assert_eq!(c.state.d[0], 0x80000000);
    assert_eq!(c.state.sr & 31, N | V);
    run(&mut c, &mut m, 2);
    assert_eq!(c.state.d[0], 0x800000FF);
    let flags = c.state.sr;
    c.state.a[0] = 1;
    c.step(&mut m);
    assert_eq!(c.state.a[0], 0);
    assert_eq!(c.state.sr, flags);
    c.step(&mut m);
    assert_eq!(c.state.a[0], 1);
    assert_eq!(c.state.sr, flags);
    c.step(&mut m);
    assert_eq!(c.state.sr & 15, C); // 1 - FFFFFFFF = 2 with unsigned borrow
}
#[test]
fn logic_bit_operations_movep_and_tas() {
    let (mut c, mut m) = fixture(&[
        0x0080, 0x8000, 0x0010, 0x0280, 0x8000, 0x0010, 0x0A80, 0x8000, 0, 0x08C0, 0x001F, 0x0880,
        0x0004, 0x0840, 0x001F, 0x01C8, 0x0001, 0x0348, 0x0001, 0x4AC1,
    ]);
    c.state.a[0] = 0x2000;
    run(&mut c, &mut m, 3);
    assert_eq!(c.state.d[0], 16);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x80000010);
    assert_ne!(c.state.sr & Z, 0);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x80000000);
    assert_eq!(c.state.sr & Z, 0);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0);
    assert_eq!(c.state.sr & Z, 0);
    c.state.d[0] = 0x12345678;
    c.step(&mut m);
    assert_eq!(
        [m.0[0x2001], m.0[0x2003], m.0[0x2005], m.0[0x2007]],
        [0x12, 0x34, 0x56, 0x78]
    );
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0x12345678);
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0x123456F8);
    assert_eq!(c.state.sr & 15, 0);
}
#[test]
fn control_flow_branches_dbcc_scc_jsr_rts_and_pc_relative_lea() {
    let (mut c, mut m) = fixture(&[
        0x7002, 0x51C8, 0xFFFE, 0x57C1, 0x41FA, 0x0010, 0x4E90, 0x4E71,
    ]);
    run(&mut c, &mut m, 4);
    assert_eq!(c.state.pc, 0x106);
    assert_eq!(c.state.d[0], 0xFFFF);
    c.step(&mut m);
    assert_eq!(c.state.d[1] & 255, 0); // false EQ
    c.step(&mut m);
    assert_eq!(c.state.a[0], 0x11A);
    m.write16(0x11A, 0x4E75).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.pc, 0x11A);
    assert_eq!(m.read32(0xEFFC).unwrap(), 0x10E);
    c.step(&mut m);
    assert_eq!(c.state.pc, 0x10E);
    let (mut c, mut m) = fixture(&[0x6104, 0x4E71, 0x4E71, 0x4E75]);
    c.step(&mut m);
    assert_eq!(c.state.pc, 0x106);
    c.step(&mut m);
    assert_eq!(c.state.pc, 0x102);
    for (op, flags, expected_pc) in [
        (0x6704, Z, 0x106),
        (0x6704, 0, 0x102),
        (0x6600, 0, 0x108),
        (0x6600, Z, 0x104),
    ] {
        let (mut c, mut m) = fixture(&[op, 6]);
        c.state.sr |= flags;
        c.step(&mut m);
        assert_eq!(c.state.pc, expected_pc, "branch {op:04x}");
        assert_eq!(c.state.a[7], 0xF000);
    }
    let (mut c, mut m) = fixture(&[0x4EF9, 0, 0x2200]);
    c.step(&mut m);
    assert_eq!(c.state.pc, 0x2200);
    assert_eq!(c.state.a[7], 0xF000);
    let (mut c, mut m) = fixture(&[0x4E71]);
    c.state.sr |= X | N | Z | V | C;
    let sr = c.state.sr;
    c.step(&mut m);
    assert_eq!(
        (c.state.pc, c.state.sr, c.state.instructions),
        (0x102, sr, 1)
    );
}

#[test]
fn conditions_match_independent_truth_table_for_all_ccr_values() {
    for flags in 0..32 {
        let mut c = M68k::default();
        c.state.sr = flags;
        let n = flags & 8 != 0;
        let z = flags & 4 != 0;
        let v = flags & 2 != 0;
        let carry = flags & 1 != 0;
        let expected = [
            true,
            false,
            !carry && !z,
            carry || z,
            !carry,
            carry,
            !z,
            z,
            !v,
            v,
            !n,
            n,
            n == v,
            n != v,
            !z && n == v,
            z || n != v,
        ];
        for (condition, value) in expected.into_iter().enumerate() {
            assert_eq!(c.condition(condition as u16), value);
        }
    }
}
#[test]
fn movem_predecrement_reverse_mask_and_postincrement_base_override() {
    let (mut c, mut m) = fixture(&[0x48E0, 0xC080, 0x4CD8, 0x0103]);
    c.state.a[0] = 0x3000;
    c.state.d[0] = 0x11223344;
    c.state.d[1] = 0x55667788;
    c.step(&mut m);
    assert_eq!(c.state.a[0], 0x2FF4);
    assert_eq!(m.read32(0x2FF4).unwrap(), 0x11223344);
    assert_eq!(m.read32(0x2FF8).unwrap(), 0x55667788);
    assert_eq!(m.read32(0x2FFC).unwrap(), 0x3000);
    c.state.d = [0; 8];
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x11223344);
    assert_eq!(c.state.d[1], 0x55667788);
    assert_eq!(c.state.a[0], 0x3000);
}
#[test]
fn multiply_divide_signed_unsigned_and_zero_overflow_exceptions() {
    for (op, a, b, result) in [
        (0xC0C1, 65535, 65535, 0xFFFE0001),
        (0xC1C1, 0xFFFF, 2, 0xFFFFFFFE),
        (0x80C1, 0x10003, 2, 0x00018001),
        (0x81C1, (-7i32) as u32, 2, 0xFFFFFFFD),
    ] {
        let (mut c, mut m) = fixture(&[op]);
        c.state.d[0] = a;
        c.state.d[1] = b;
        c.step(&mut m);
        assert_eq!(c.state.d[0], result);
        assert_eq!(c.state.exceptions, 0);
    }
    let (mut c, mut m) = fixture(&[0x81C1]);
    c.state.d[0] = 0x80000000;
    c.state.d[1] = 0xFFFF;
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x80000000);
    assert_ne!(c.state.sr & V, 0);
    let (mut c, mut m) = fixture(&[0x80C1]);
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(5));
    assert_eq!(
        m.read32(0xEFFE).unwrap_err(),
        BusError::Unaligned {
            address: 0xEFFE,
            width: 4
        }
    );
    assert_eq!(m.read16(0xEFFC).unwrap(), 0);
    assert_eq!(m.read16(0xEFFE).unwrap(), 0x102);
}
#[test]
fn decimal_extend_and_chained_zero() {
    for (op, a, b, x, result, carry) in [
        (0xC101, 0x49, 0x50, 0, 0x99, false),
        (0xC101, 0x99, 0, 1, 0, true),
        (0x8101, 0, 1, 0, 0x99, true),
        (0x8101, 0, 0, 1, 0x99, true),
    ] {
        let (mut c, mut m) = fixture(&[op]);
        c.state.d[0] = a;
        c.state.d[1] = b;
        c.state.sr |= Z | if x != 0 { X } else { 0 };
        c.step(&mut m);
        assert_eq!(c.state.d[0], result);
        assert_eq!(c.state.sr & C != 0, carry);
        assert_eq!(c.state.sr & Z != 0, result == 0);
    }
    let (mut c, mut m) = fixture(&[0x4800]);
    c.state.d[0] = 1;
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x99);
}
#[test]
fn shifts_rotates_zero_counts_carry_overflow_and_extend() {
    for (op, initial, flags, result, expected) in [
        (0xE300, 0x40, 0, 0x80, N | V),
        (0xE200, 0x81, 0, 0xC0, N | X | C),
        (0xE308, 0x80, 0, 0, X | C | Z),
        (0xE210, 1, X, 0x80, N | X | C),
        (0xE318, 0x80, X, 1, X | C),
        (0xE220, 0x80, X, 0x80, N | X),
        (0xE230, 0, X, 0, Z | X | C),
    ] {
        let (mut c, mut m) = fixture(&[op]);
        c.state.d[0] = initial;
        c.state.d[1] = 0;
        c.state.sr |= flags;
        c.step(&mut m);
        assert_eq!(c.state.d[0], result, "{op:04x}");
        assert_eq!(c.state.sr & 31, expected, "{op:04x}");
    }
}
#[test]
fn trap_privilege_user_supervisor_stacks_rte_stop_and_irq() {
    let (mut c, mut m) = fixture(&[0x4E40, 0x4E71]);
    m.write16(0x900, 0x4E73).unwrap(); // TRAP handler must exist before vector prefetch
    c.state.usp = 0xD000;
    c.sr(0);
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(32));
    assert_eq!(c.state.a[7], 0xEFFA);
    assert_eq!(c.state.usp, 0xD000);
    assert_eq!(m.read16(0xEFFA).unwrap(), 0);
    m.write16(0x102, 0x4E72).unwrap(); // set before RTE refills the target stream
    c.step(&mut m);
    assert_eq!((c.state.pc, c.state.a[7], c.state.sr), (0x102, 0xD000, 0));
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(8));
    // STOP with mask zero wakes on interrupt, including from a user SR.
    let (mut c, mut m) = fixture(&[0x4E72, 0x2000, 0x4E71]);
    c.step(&mut m);
    assert!(c.state.stopped);
    c.set_interrupt(3);
    assert_eq!(c.step(&mut m), 44);
    assert!(!c.state.stopped);
    assert_eq!(c.state.last_vector, Some(27));
    assert_eq!(c.state.sr & 0x700, 0x300);
    // Level 7 accepted once per rising transition even at interrupt mask 7.
    c.state.sr |= 0x700;
    c.set_interrupt(7);
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(31));
    let n = c.state.exceptions;
    m.write16(c.state.pc, 0x4E71).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.exceptions, n);
}
#[test]
fn address_error_bus_error_seven_word_frames_trace_and_double_fault_halt() {
    let (mut c, mut m) = fixture(&[0x3010]);
    c.state.a[0] = 0x2001;
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(3));
    assert_eq!(c.state.a[7], 0xEFF2);
    assert_eq!(m.read16(0xEFF2).unwrap(), 0x15); // read, processing instruction, supervisor data FC=5
    assert_eq!(m.read16(0xEFF6).unwrap(), 0x2001);
    assert_eq!(m.read16(0xEFF8).unwrap(), 0x3010);
    assert_eq!(m.read16(0xEFFA).unwrap(), 0x2700);
    let (mut c, mut m) = fixture(&[0x3010]);
    c.state.a[0] = 0x10000;
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(2));
    let (mut c, mut m) = fixture(&[0x4E71]);
    c.state.sr |= T;
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(9));
    assert_eq!(c.state.sr & T, 0);
    let (mut c, mut m) = fixture(&[0x3010]);
    c.state.a[0] = 1;
    c.state.a[7] = 1;
    c.step(&mut m);
    assert!(c.state.halted);
}
#[test]
fn illegal_later_model_and_line_emulator_instructions_dispatch_vectors() {
    for (op, vector) in [
        (0x4AFC, 4),
        (0x4E74, 4),
        (0x4848, 4),
        (0x42C0, 4),
        (0x7100, 4),
        (0xA000, 10),
        (0xF000, 11),
    ] {
        let (mut c, mut m) = fixture(&[op]);
        c.step(&mut m);
        assert_eq!(c.state.last_vector, Some(vector), "{op:04x}");
        assert_eq!(m.read16(0xEFFE).unwrap(), 0x100);
    }
}
#[test]
fn untested_encoding_never_panics_or_runs_unbounded() {
    // Classification safety only; semantic vectors above carry acceptance evidence.
    let (base, mut m) = fixture(&[]);
    for op in 0..=u16::MAX {
        let mut c = M68k {
            state: base.state.clone(),
            ..M68k::default()
        };
        m.write16(0x100, op).unwrap();
        m.write16(0x102, 0).unwrap();
        m.write16(0x104, 0).unwrap();
        c.step(&mut m);
        assert!(!c.state.halted, "{op:04x}");
    }
}

#[test]
fn unary_ext_swap_exg_and_address_effects_have_independent_results() {
    let (mut c, mut m) = fixture(&[
        0x4400, 0x4600, 0x4200, 0x4A00, 0x4881, 0x48C1, 0x4841, 0xC141, 0xC149, 0xC189,
    ]);
    c.state.d[0] = 1;
    c.state.d[1] = 0x80;
    c.state.a[0] = 0x1234;
    c.state.a[1] = 0x5678;
    c.step(&mut m);
    assert_eq!(c.state.d[0], 255);
    assert_eq!(c.state.sr & 31, N | X | C);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0);
    assert_eq!(c.state.sr & 31, Z | X);
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, Z | X);
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, Z | X);
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0xFF80);
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0xFFFFFF80);
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0xFF80FFFF);
    c.step(&mut m);
    assert_eq!((c.state.d[0], c.state.d[1]), (0xFF80FFFF, 0));
    c.step(&mut m);
    assert_eq!((c.state.a[0], c.state.a[1]), (0x5678, 0x1234));
    c.step(&mut m);
    assert_eq!((c.state.d[0], c.state.a[1]), (0x1234, 0xFF80FFFF));
}
#[test]
fn link_unlk_pea_usp_and_status_register_operations() {
    let (mut c, mut m) = fixture(&[
        0x4E56, 0xFFF0, 0x4856, 0x205F, 0x4E5E, 0x4E60, 0x4E69, 0x003C, 0x0011, 0x023C, 0x0014,
        0x0A3C, 0x0004, 0x40C2, 0x44FC, 0x001F, 0x46FC, 0x2004,
    ]);
    c.state.a[6] = 0xABCD;
    c.step(&mut m);
    assert_eq!((c.state.a[6], c.state.a[7]), (0xEFFC, 0xEFEC));
    c.step(&mut m);
    assert_eq!(c.state.a[7], 0xEFE8);
    c.step(&mut m);
    assert_eq!(c.state.a[0], 0xEFFC);
    c.step(&mut m);
    assert_eq!((c.state.a[6], c.state.a[7]), (0xABCD, 0xF000));
    c.step(&mut m);
    c.step(&mut m);
    assert_eq!(c.state.a[1], 0xEFFC);
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, 0x11);
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, 0x10);
    c.step(&mut m);
    assert_eq!(c.state.sr & 31, 0x14);
    c.step(&mut m);
    assert_eq!(c.state.d[2], 0x2714);
    c.step(&mut m);
    assert_eq!(c.state.sr, 0x271F);
    c.step(&mut m);
    assert_eq!(c.state.sr, 0x2004);
}
#[test]
fn chk_trapv_rtr_and_reset_output() {
    for (value, bound, negative) in [(0xFFFF, 10, true), (11, 10, false)] {
        let (mut c, mut m) = fixture(&[0x4181]);
        c.state.d[0] = value;
        c.state.d[1] = bound;
        c.step(&mut m);
        assert_eq!(c.state.last_vector, Some(6));
        assert_eq!(c.state.sr & N != 0, negative);
        assert_eq!(m.read16(0xEFFE).unwrap(), 0x102);
    }
    let (mut c, mut m) = fixture(&[0x4E76]);
    c.state.sr |= V;
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(7));
    let (mut c, mut m) = fixture(&[0x4E77]);
    m.write16(0xF000, 0x1F).unwrap();
    m.write16(0xF002, 0).unwrap();
    m.write16(0xF004, 0x200).unwrap();
    c.step(&mut m);
    assert_eq!(
        (c.state.pc, c.state.sr, c.state.a[7]),
        (0x200, 0x271F, 0xF006)
    );
    let (mut c, mut m) = fixture(&[0x4E70]);
    c.state.d[0] = 37;
    assert_eq!(c.step(&mut m), 132);
    assert!(c.reset_output);
    assert_eq!(c.state.d[0], 37);
    assert_eq!(c.state.pc, 0x102);
}
#[test]
fn logical_register_memory_and_memory_extend_compare_forms() {
    let (mut c, mut m) = fixture(&[
        0xC001, 0x8001, 0xB110, 0xD110, 0x9110, 0xB308, 0xD109, 0x9109,
    ]);
    c.state.d[0] = 0x5A;
    c.state.d[1] = 0x0F;
    c.state.a[0] = 0x2000;
    c.state.a[1] = 0x2001;
    m.write8(0x2000, 0x55).unwrap();
    m.write8(0x2001, 0x5A).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.d[0], 10);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 15);
    c.step(&mut m);
    assert_eq!(m.0[0x2000], 0x5A);
    c.step(&mut m);
    assert_eq!(m.0[0x2000], 0x69);
    c.step(&mut m);
    assert_eq!(m.0[0x2000], 0x5A);
    c.step(&mut m);
    assert_eq!(c.state.sr & Z, Z);
    assert_eq!((c.state.a[0], c.state.a[1]), (0x2001, 0x2002));
    c.state.sr |= X | Z;
    c.step(&mut m);
    assert_eq!(m.0[0x2000], 0xB5);
    assert_eq!(c.state.sr & Z, 0);
    c.step(&mut m);
    assert_eq!((c.state.a[0], c.state.a[1]), (0x1FFF, 0x2000));
}
#[test]
fn deterministic_alu_property_vectors_use_wide_signed_reference_arithmetic() {
    let mut seed = 0x12345678u32;
    for (size_bits, bits) in [(0, 8), (0x40, 16), (0x80, 32)] {
        let mask = (1u64 << bits) - 1;
        let sign = 1i64 << (bits - 1);
        for sub in [false, true] {
            for extend in [false, true] {
                for _ in 0..1000 {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let a = u64::from(seed) & mask;
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let b = u64::from(seed) & mask;
                    let x = i64::from(extend && seed & 0x10000 != 0);
                    let signed = |n: u64| {
                        if n & (sign as u64) != 0 {
                            n as i64 - ((mask + 1) as i64)
                        } else {
                            n as i64
                        }
                    };
                    let mathematical = if sub {
                        signed(a) - signed(b) - x
                    } else {
                        signed(a) + signed(b) + x
                    };
                    let result = mathematical as u64 & mask;
                    let carry = if sub {
                        a < b + x as u64
                    } else {
                        a + b + x as u64 > mask
                    };
                    let overflow = mathematical < -sign || mathematical >= sign;
                    let op = (if sub { 0x9001 } else { 0xD001 })
                        | size_bits
                        | if extend { 0x100 } else { 0 };
                    let (mut c, mut m) = fixture(&[op]);
                    c.state.d[0] = a as u32;
                    c.state.d[1] = b as u32;
                    c.state.sr |= Z | if x != 0 { X } else { 0 };
                    c.step(&mut m);
                    assert_eq!(
                        u64::from(c.state.d[0]) & mask,
                        result,
                        "{op:04x} {a:x} {b:x}"
                    );
                    let flags = if carry { X | C } else { 0 }
                        | if overflow { V } else { 0 }
                        | if result == 0 { Z } else { 0 }
                        | if result & sign as u64 != 0 { N } else { 0 };
                    assert_eq!(c.state.sr & 31, flags, "{op:04x} {a:x} {b:x} x={x}");
                }
            }
        }
    }
}

#[test]
fn dynamic_btst_accepts_immediate_data_but_static_immediate_destination_is_illegal() {
    // M68000PRM BTST pp.4-62/63 distinguishes the two EA tables.
    let (mut c, mut m) = fixture(&[0x013C, 0x0080, 0x083C, 0, 0]);
    c.state.d[0] = 7;
    c.step(&mut m);
    assert_eq!(c.state.sr & Z, 0);
    assert_eq!(c.state.pc, 0x104);
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(4));
}

#[test]
fn remaining_arithmetic_rotate_and_condition_family_vectors() {
    for (words, initial, sr, result, flags) in [
        (vec![0x0480, 0, 1], 0, 0, 0xFFFF_FFFF, X | N | C),
        (vec![0x5380], 1, 0, 0, Z),
        (vec![0x4000], 0, X | Z, 255, X | N | C),
        (vec![0xE218], 1, X, 128, X | N | C),
        (vec![0xE310], 0, X, 1, 0),
        (vec![0xE208], 0x81, 0, 0x40, X | C),
        (vec![0x50C0], 0x12340000, X, 0x123400FF, X),
    ] {
        let (mut c, mut m) = fixture(&words);
        c.state.d[0] = initial;
        c.state.sr |= sr;
        c.step(&mut m);
        assert_eq!(c.state.d[0], result, "{:04x}", words[0]);
        assert_eq!(c.state.sr & 31, flags, "{:04x}", words[0]);
    }
    let (mut c, mut m) = fixture(&[0x90FC, 0xFFFF, 0x0C40, 0x8000, 0x50C8, 0xFFFE]);
    c.state.d[0] = 0x7FFF;
    c.state.sr |= X;
    c.step(&mut m);
    assert_eq!(c.state.a[0], 1);
    assert_eq!(c.state.sr & 31, X);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x7FFF);
    assert_eq!(c.state.sr & 31, X | N | V | C);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x7FFF);
    assert_eq!(c.state.pc, 0x10C);
    let (mut c, mut m) = fixture(&[0xE1D0]);
    c.state.a[0] = 0x2000;
    m.write16(0x2000, 0x4000).unwrap();
    c.step(&mut m);
    assert_eq!(m.read16(0x2000).unwrap(), 0x8000);
    assert_eq!(c.state.sr & 31, N | V);
}
#[test]
fn word_movem_sign_extends_loads_and_movep_transfers_only_selected_byte_lanes() {
    let (mut c, mut m) = fixture(&[0x4890, 0x0101, 0x4C91, 3, 0x0188, 1, 0x0308, 1]);
    c.state.a[0] = 0x3000;
    c.state.a[1] = 0x3000;
    c.state.d[0] = 0xFFFF8000;
    c.state.sr |= X | Z;
    c.step(&mut m);
    assert_eq!(&m.0[0x3000..0x3004], &[0x80, 0, 0x30, 0]);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0xFFFF8000);
    assert_eq!(c.state.d[1], 0x3000);
    c.step(&mut m);
    assert_eq!(&m.0[0x3000..0x3005], &[0x80, 0x80, 0x30, 0, 0]);
    c.step(&mut m);
    assert_eq!(c.state.d[1], 0x8000);
    assert_eq!(c.state.sr & 31, X | Z);
}

// MC68000UM §§2.4, 5.1/5.4, 6.3.9. Original bus-cycle and fault vectors:
// completed writes must remain visible and the frame identifies the failed cycle.
struct CycleRam {
    ram: Ram,
    writes: Vec<(u32, u16)>,
    fail_read: Option<u32>,
    fail_write: Option<u32>,
}
impl Memory for CycleRam {
    fn read8(&self, a: u32) -> std::result::Result<u8, BusError> {
        self.ram.read8(a)
    }
    fn read16(&self, a: u32) -> std::result::Result<u16, BusError> {
        if self.fail_read == Some(a) {
            return Err(BusError::Unmapped {
                address: a,
                width: 2,
            });
        }
        self.ram.read16(a)
    }
    fn read32(&self, _: u32) -> std::result::Result<u32, BusError> {
        panic!("68k must issue word cycles")
    }
    fn write8(&mut self, a: u32, v: u8) -> std::result::Result<(), BusError> {
        self.ram.write8(a, v)
    }
    fn write16(&mut self, a: u32, v: u16) -> std::result::Result<(), BusError> {
        self.writes.push((a, v));
        if self.fail_write == Some(a) {
            return Err(BusError::Unmapped {
                address: a,
                width: 2,
            });
        }
        self.ram.write16(a, v)
    }
    fn write32(&mut self, _: u32, _: u32) -> std::result::Result<(), BusError> {
        panic!("68k must issue word cycles")
    }
}
fn cycle_fixture(words: &[u16]) -> (M68k, CycleRam) {
    let (cpu, ram) = fixture(words);
    (
        cpu,
        CycleRam {
            ram,
            writes: Vec::new(),
            fail_read: None,
            fail_write: None,
        },
    )
}
#[test]
fn long_predecrement_bus_order_and_partial_write_fault_frames() {
    // MOVE.L D0,-(A0), successful and either word cycle failing.
    for fail in [None, Some(0x2002), Some(0x2000)] {
        let (mut c, mut m) = cycle_fixture(&[0x2100]);
        c.state.d[0] = 0x1234ABCD;
        c.state.a[0] = 0x2004;
        m.fail_write = fail;
        c.step(&mut m);
        assert_eq!(c.state.a[0], 0x2000);
        assert_eq!(m.writes[0], (0x2002, 0xABCD));
        if fail != Some(0x2002) {
            assert_eq!(m.writes[1], (0x2000, 0x1234));
        }
        assert_eq!(
            m.ram.read16(0x2002).unwrap(),
            if fail == Some(0x2002) { 0 } else { 0xABCD }
        );
        if let Some(address) = fail {
            assert_eq!(c.state.last_vector, Some(2));
            assert_eq!(m.ram.read32(c.state.a[7] + 2).unwrap(), address);
            assert_eq!(m.ram.read16(c.state.a[7]).unwrap(), 5); // processing instruction, supervisor data write
            assert_eq!(c.state.instructions, 0);
        } else {
            assert_eq!(m.ram.read32(0x2000).unwrap(), 0x1234ABCD);
            assert_eq!(c.state.instructions, 1);
        }
    }
}
#[test]
fn second_longword_cycle_fault_records_its_own_address_and_preserves_first_write() {
    for write in [false, true] {
        let (mut c, mut m) = cycle_fixture(&[if write { 0x2080 } else { 0x2010 }]);
        c.state.a[0] = 0x2000;
        c.state.d[0] = 0x1234ABCD;
        if write {
            m.fail_write = Some(0x2002);
        } else {
            m.fail_read = Some(0x2002);
        }
        c.step(&mut m);
        assert_eq!(c.state.last_vector, Some(2));
        assert_eq!(m.ram.read32(c.state.a[7] + 2).unwrap(), 0x2002);
        assert_eq!(
            m.ram.read16(c.state.a[7]).unwrap(),
            if write { 5 } else { 0x15 }
        );
        assert_eq!(c.state.d[0], 0x1234ABCD);
        assert_eq!(
            m.ram.read16(0x2000).unwrap(),
            if write { 0x1234 } else { 0 }
        );
    }
}
#[test]
fn movem_and_exception_pushes_issue_predecrement_word_cycles_and_retain_progress() {
    let (mut c, mut m) = cycle_fixture(&[0x48E0, 0xC000]); // MOVEM.L D0-D1,-(A0)
    c.state.a[0] = 0x3000;
    c.state.d[0] = 0x12345678;
    c.state.d[1] = 0x89ABCDEF;
    m.fail_write = Some(0x2FF8);
    c.step(&mut m);
    assert_eq!(
        &m.writes[..4],
        &[
            (0x2FFE, 0xCDEF),
            (0x2FFC, 0x89AB),
            (0x2FFA, 0x5678),
            (0x2FF8, 0x1234)
        ]
    );
    assert_eq!(c.state.a[0], 0x2FF8);
    assert_eq!(m.ram.read32(c.state.a[7] + 2).unwrap(), 0x2FF8);
    let (mut c, mut m) = cycle_fixture(&[0x4E40]); // TRAP #0
    c.step(&mut m);
    assert_eq!(
        &m.writes[..3],
        &[(0xEFFE, 0x0102), (0xEFFC, 0), (0xEFFA, 0x2700)]
    );
    assert_eq!(c.state.last_vector, Some(32));
}

#[test]
fn movem_postincrement_preserves_completed_loads_when_a_later_bus_cycle_faults() {
    let (mut c, mut m) = cycle_fixture(&[0x4CD8, 0x0003]); // MOVEM.L (A0)+,D0-D1
    c.state.a[0] = 0x2000;
    c.state.d[1] = 0xDEADBEEF;
    m.ram.write32(0x2000, 0x12345678).unwrap();
    m.ram.write32(0x2004, 0x89ABCDEF).unwrap();
    m.fail_read = Some(0x2006);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 0x12345678);
    assert_eq!(c.state.d[1], 0xDEADBEEF);
    assert_eq!(c.state.a[0], 0x2004);
    assert_eq!(c.state.instructions, 0);
    assert_eq!(c.state.last_vector, Some(2));
    assert_eq!(m.ram.read32(c.state.a[7] + 2).unwrap(), 0x2006);
}

#[test]
fn prefetch_retains_next_opcode_but_not_unfetched_extension_and_branch_flushes_it() {
    // UM §§5.1/6.3.9.1: executing IR and lookahead IRC are independent of
    // subsequent RAM writes. A branch, even to the sequential address, refills.
    let (mut c, mut m) = fixture(&[0x30BC, 0x7009, 0x7003, 0x4E71]);
    c.state.a[0] = 0x104;
    c.step(&mut m); // MOVE.W #$7009,(A0) overwrites the prefetched MOVEQ #3.
    assert_eq!(m.read16(0x104), Ok(0x7009));
    c.step(&mut m);
    assert_eq!(c.state.d[0], 3);
    let (mut c, mut m) = fixture(&[0x30BC, 9, 0x303C, 3]);
    c.state.a[0] = 0x106;
    c.step(&mut m); // next opcode is prefetched, its extension is not yet fetched.
    c.step(&mut m);
    assert_eq!(c.state.d[0], 9);
    let (mut c, mut m) = fixture(&[0x4ED0, 0x7003, 0x4E71]);
    c.state.a[0] = 0x102;
    // A fresh target word replaces an already fetched one at the same address.
    c.next_word(&m, true).unwrap();
    m.write16(0x102, 0x7009).unwrap();
    c.execute(&mut m, 0x4ED0).unwrap();
    c.step(&mut m);
    assert_eq!(c.state.d[0], 9);
}

#[test]
fn fault_frames_save_internal_prefetch_pc_sr_ir_and_access_context() {
    // Literal source-authored expectations, UM fig.6-7, §§5.4.1/6.3.9.1.
    for (code, fail, saved_pc, ir, address, ssw) in [
        (vec![0x2010], 0x2002, 0x104, 0x2010, 0x2002, 0x15),
        (vec![0x2039, 0, 0x2000], 0x2002, 0x108, 0x2039, 0x2002, 0x15),
        (vec![0x7003], 0x102, 0x104, 0x7003, 0x102, 0x16),
        (vec![0x4EF9, 0, 0x2000], 0x2000, 0x108, 0x4EF9, 0x2000, 0x16),
    ] {
        let (mut c, mut m) = cycle_fixture(&code);
        c.state.a[0] = 0x2000;
        c.state.sr = 0x271B;
        m.fail_read = Some(fail);
        c.step(&mut m);
        assert_eq!(c.state.last_vector, Some(2));
        let sp = c.state.a[7];
        assert_eq!(m.ram.read16(sp), Ok(ssw));
        assert_eq!(m.ram.read32(sp + 2), Ok(address));
        assert_eq!(m.ram.read16(sp + 6), Ok(ir));
        assert_eq!(m.ram.read16(sp + 8), Ok(0x271B));
        assert_eq!(m.ram.read16(sp + 10), Ok((saved_pc >> 16) as u16));
        assert_eq!(m.ram.read16(sp + 12), Ok(saved_pc as u16));
        assert_eq!(c.state.instructions, 0);
        assert_eq!(c.state.d[0], 0);
        assert!(!c.state.halted);
    }
}

#[test]
fn reset_and_irq_discard_prefetched_words_and_restore_program_function_code() {
    let (mut c, mut m) = fixture(&[0x4E71, 0x7003, 0x4E71]);
    c.step(&mut m);
    m.write16(0x102, 0x7009).unwrap();
    c.reset(&m).unwrap();
    c.step(&mut m);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 9);
    c.state.sr = 0x2000;
    let handler = m.read32(27 * 4).unwrap();
    m.write16(handler, 0x700B).unwrap();
    c.set_interrupt(3);
    c.step(&mut m);
    c.set_interrupt(0);
    c.step(&mut m);
    assert_eq!(c.state.d[0], 11);
    assert_eq!(c.state.exceptions, 1);
}

#[test]
fn group_one_and_two_vector_faults_dispatch_bus_error_with_processor_activity_context() {
    // UM §§6.1/6.3.9.1: vector cycles use supervisor data, not program FC.
    // Group1 IRQ has I/N=1; group2 TRAP still processes an instruction (I/N=0).
    for irq in [false, true] {
        let (mut c, mut m) = cycle_fixture(&[0x4E40]);
        c.state.sr = 0x2000;
        let vector_address = if irq { 27 * 4 } else { 32 * 4 };
        m.fail_read = Some(vector_address);
        if irq {
            c.set_interrupt(3);
        }
        c.step(&mut m);
        assert!(!c.state.halted);
        assert_eq!(c.state.last_vector, Some(2));
        let sp = c.state.a[7];
        assert_eq!(m.ram.read16(sp), Ok(if irq { 0x1D } else { 0x15 }));
        assert_eq!(m.ram.read16(sp + 2), Ok(0));
        assert_eq!(m.ram.read16(sp + 4), Ok(vector_address as u16));
        assert_eq!(m.ram.read16(sp + 8), Ok(if irq { 0x2300 } else { 0x2000 }));
        assert_eq!(m.ram.read16(sp + 10), Ok(0));
        assert_eq!(m.ram.read16(sp + 12), Ok(vector_address as u16));
        assert_eq!(sp, 0xEFEC); // failed exception's six bytes + access frame
    }
}

#[test]
fn handler_prefetch_failure_records_vector_pc_and_double_bus_error_halts_until_reset() {
    // UM §6.3.9.1 also covers handler fetch failure in the final exception phase.
    let (mut c, mut m) = cycle_fixture(&[0x4E40]);
    m.fail_read = Some(0x900); // TRAP0 handler's first prefetched word
    c.step(&mut m);
    assert!(!c.state.halted);
    assert_eq!(c.state.last_vector, Some(2));
    let sp = c.state.a[7];
    assert_eq!(m.ram.read16(sp), Ok(0x16)); // supervisor program, still group2
    assert_eq!(m.ram.read16(sp + 2), Ok(0));
    assert_eq!(m.ram.read16(sp + 4), Ok(0x900));
    assert_eq!(m.ram.read16(sp + 12), Ok(0x80)); // TRAP vector's address
    let (mut c, mut m) = cycle_fixture(&[0x2010]);
    c.state.a[0] = 0x2001; // address error, then bus error fetching its vector
    m.fail_read = Some(3 * 4);
    c.step(&mut m);
    assert!(c.state.halted);
    let frozen = c.state.clone();
    assert_eq!(c.step(&mut m), 4);
    assert_eq!(c.state, frozen);
    m.fail_read = None;
    c.reset(&m).unwrap();
    assert!(!c.state.halted);
    assert_eq!(c.state.pc, 0x100);
    assert_eq!(c.state.a[7], 0xF000);
    assert!(c.prefetch.is_empty());
}

#[test]
fn bus_error_in_any_reset_vector_word_halts_without_an_exception_stack() {
    for address in [0, 2, 4, 6] {
        let (mut c, mut m) = cycle_fixture(&[0x4E71]);
        m.fail_read = Some(address);
        assert!(c.reset(&m).is_err());
        assert!(c.state.halted);
        assert_eq!(c.state.exceptions, 0);
        assert!(m.writes.is_empty());
        assert_eq!(c.step(&mut m), 4);
        m.fail_read = None;
        c.reset(&m).unwrap();
        assert!(!c.state.halted);
        assert_eq!(c.state.pc, 0x100);
        assert_eq!(c.state.a[7], 0xF000);
    }
}

#[test]
fn traced_trap_stacks_the_trap_before_trace_and_preserves_both_return_contexts() {
    // MC68000UM §6.3.8's explicit TRAP/trace ordering, six-byte frames fig.6-5.
    let (mut c, mut m) = fixture(&[0x4E40, 0x4E71]);
    c.state.sr = 0xA000;
    assert_eq!(c.step(&mut m), 68);
    assert_eq!(c.state.instructions, 1);
    assert_eq!(c.state.exceptions, 2);
    assert_eq!(c.state.last_vector, Some(9));
    assert_eq!(c.state.pc, 0x848);
    assert_eq!(c.state.a[7], 0xEFF4);
    assert_eq!(m.read16(0xEFFA), Ok(0xA000)); // original TRAP SR
    assert_eq!(m.read16(0xEFFE), Ok(0x102)); // original successor
    assert_eq!(m.read16(0xEFF4), Ok(0x2000)); // trace starts in supervisor, T clear
    assert_eq!(m.read16(0xEFF8), Ok(0x900)); // trace returns to the TRAP handler
}

#[test]
fn illegal_privilege_trace_and_line_emulator_entry_faults_use_group_one_activity() {
    // UM table6-3 and §6.3.9.1: these are not the group2 instruction traps.
    for (op, sr, vector) in [
        (0x4AFC, 0x2700, 4u8),
        (0x4E70, 0, 8),
        (0x4E71, 0xA700, 9),
        (0xA000, 0x2700, 10),
        (0xF000, 0x2700, 11),
    ] {
        let (mut c, mut m) = cycle_fixture(&[op]);
        c.sr(sr);
        m.fail_read = Some(u32::from(vector) * 4);
        c.step(&mut m);
        assert!(!c.state.halted);
        assert_eq!(c.state.last_vector, Some(2));
        let sp = c.state.a[7];
        assert_eq!(m.ram.read16(sp), Ok(0x1D)); // supervisor data + read + I/N
        assert_eq!(m.ram.read16(sp + 12), Ok(u16::from(vector) * 4));
    }
}

#[test]
fn five_word_move_fault_keeps_internal_pc_at_final_extension_boundary() {
    // MOVE.L #$12345678,$2000, maximum MC68000 length; UM §§6.3.9.1/8.2.
    let (mut c, mut m) = cycle_fixture(&[0x23FC, 0x1234, 0x5678, 0, 0x2000]);
    m.fail_write = Some(0x2002);
    c.step(&mut m);
    assert_eq!(c.state.last_vector, Some(2));
    let sp = c.state.a[7];
    assert_eq!(m.ram.read16(sp), Ok(5));
    assert_eq!(m.ram.read32(sp + 2), Ok(0x2002));
    assert_eq!(m.ram.read16(sp + 6), Ok(0x23FC));
    assert_eq!(m.ram.read16(sp + 12), Ok(0x10A));
    assert_eq!(m.ram.read16(0x2000), Ok(0x1234));
}
