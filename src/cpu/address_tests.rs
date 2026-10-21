//! Original vectors derived from SH7604 §§4.3/4.6/4.7, Tables 4.6/4.10/4.11.
use super::*;
use crate::bus::{Bus, HIGH_WRAM};

const HANDLER: u32 = HIGH_WRAM + 0x400;
const STACK: u32 = HIGH_WRAM + 0x1000;

fn setup(opcodes: &[u16]) -> (Cpu, Bus) {
    let mut bus = Bus::default();
    for (i, opcode) in opcodes.iter().enumerate() {
        bus.write16(HIGH_WRAM + i as u32 * 2, *opcode).unwrap();
    }
    let mut cpu = Cpu::start(HIGH_WRAM, STACK);
    cpu.state.vbr = HIGH_WRAM + 0x200;
    bus.write32(cpu.state.vbr + 9 * 4, HANDLER).unwrap();
    (cpu, bus)
}

fn assert_entry(cpu: &Cpu, bus: &Bus, saved_pc: u32, sr: u32, retired: u64) {
    assert_eq!(cpu.state.pc, HANDLER);
    assert_eq!(cpu.state.r[15], STACK - 8);
    assert_eq!(bus.read32(STACK - 8), Ok(saved_pc));
    assert_eq!(bus.read32(STACK - 4), Ok(sr));
    assert_eq!(cpu.state.sr, sr); // Address errors do not raise the IRQ mask.
    assert_eq!(cpu.state.instructions, retired);
    assert_eq!(cpu.state.exceptions, 1);
    assert_eq!(cpu.state.delay_target, None);
    assert!(!cpu.state.address_error_pending);
    assert_eq!(cpu.state.status, CpuStatus::Running);
    assert_eq!(
        cpu.state.last_exception,
        Some(Exception {
            vector: 9,
            saved_pc
        })
    );
}

#[test]
fn odd_and_peripheral_fetches_dispatch_vector_nine_without_retiring() {
    for pc in [
        HIGH_WRAM + 1,
        0xFFFF_FE00,
        0xFFFF_FEFE,
        0xFFFF_FF00,
        u32::MAX,
    ] {
        let (mut cpu, mut bus) = setup(&[]);
        cpu.state.pc = pc;
        cpu.state.sr = 0x263;
        cpu.step(&mut bus).unwrap();
        assert_entry(&cpu, &bus, pc, 0x263, 0);
    }
}

#[test]
fn unfetchable_delayed_successor_latches_once_and_finishes_the_branch_boundary() {
    // An original synthetic instruction at the boundary isolates the fetch
    // policy from the still-unimplemented SH7604 I/O-area bus mapping.
    struct BoundaryFetch(Bus);
    impl Memory for BoundaryFetch {
        fn read8(&self, a: u32) -> Result<u8, BusError> {
            self.0.read8(a)
        }
        fn read16(&self, a: u32) -> Result<u16, BusError> {
            if a == 0xFFFF_FDFE {
                Ok(0x412B)
            } else {
                self.0.read16(a)
            }
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
    let (mut cpu, bus) = setup(&[]);
    let mut bus = BoundaryFetch(bus);
    cpu.state.pc = 0xFFFF_FDFE;
    cpu.state.r[1] = HIGH_WRAM + 0x100;
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.pc, 0xFFFF_FE00);
    cpu.step(&mut bus).unwrap();
    assert!(cpu.state.address_error_pending);
    assert_eq!(cpu.state.pc, HIGH_WRAM + 0x100);
    assert_eq!(cpu.state.exceptions, 0);
    cpu.step(&mut bus).unwrap();
    assert_entry(&cpu, &bus.0, HIGH_WRAM + 0x100, 0xF0, 1);
}

#[test]
fn misaligned_operand_cycles_complete_then_stack_the_next_pc() {
    // MOV.W/L indirect, postincrement, predecrement, indexed and displaced;
    // LDC.L/LDS.L/STC.L/STS.L and MAC.W/L. R15 stays aligned for entry.
    let forms = [
        (0x6211, 2),
        (0x6212, 4),
        (0x6215, 2),
        (0x6216, 4),
        (0x2121, 2),
        (0x2122, 4),
        (0x2125, 2),
        (0x2126, 4),
        (0x0125, 2),
        (0x0126, 4),
        (0x021D, 2),
        (0x021E, 4),
        (0x5120, 4),
        (0x1120, 4),
        (0x8110, 2),
        (0x8510, 2),
        (0xC100, 2),
        (0xC200, 4),
        (0xC500, 2),
        (0xC600, 4),
        (0x4117, 4),
        (0x4116, 4),
        (0x4113, 4),
        (0x4112, 4),
        (0x421F, 2),
        (0x021F, 4),
    ];
    for (opcode, width) in forms {
        for misalignment in 1..width {
            let (mut cpu, mut bus) = setup(&[opcode]);
            let address = HIGH_WRAM + 0x801 + misalignment - 1;
            cpu.state.r[1] = address;
            cpu.state.r[2] = address;
            cpu.state.gbr = address;
            cpu.state.sr = 0x63;
            // Sentinels prove invalid writes don't partially touch neighbors.
            bus.write32(HIGH_WRAM + 0x7FC, 0x1234_5678).unwrap();
            bus.write32(HIGH_WRAM + 0x800, 0x89AB_CDEF).unwrap();
            bus.write32(HIGH_WRAM + 0x804, 0x7654_3210).unwrap();
            cpu.step(&mut bus)
                .unwrap_or_else(|e| panic!("{opcode:04X}: {e}"));
            assert!(
                cpu.state.address_error_pending,
                "{opcode:04X}/{misalignment}"
            );
            assert_eq!(cpu.state.pc, HIGH_WRAM + 2);
            assert_eq!(cpu.state.instructions, 1);
            assert_eq!(cpu.state.exceptions, 0);
            assert_eq!(bus.read32(HIGH_WRAM + 0x7FC), Ok(0x1234_5678));
            assert_eq!(bus.read32(HIGH_WRAM + 0x800), Ok(0x89AB_CDEF));
            assert_eq!(bus.read32(HIGH_WRAM + 0x804), Ok(0x7654_3210));
            let sr = cpu.state.sr;
            assert!(!cpu.accept_nmi(&mut bus).unwrap());
            assert!(!cpu.accept_interrupt(&mut bus, 15, 64).unwrap());
            cpu.step(&mut bus).unwrap();
            assert_entry(&cpu, &bus, HIGH_WRAM + 2, sr, 1);
        }
    }
}

#[test]
fn invalid_reads_have_documented_zero_data_and_preserve_auto_updates() {
    for (opcode, r1, r2) in [
        (0x6215, HIGH_WRAM + 0x803, 0),  // MOV.W @R1+,R2
        (0x6216, HIGH_WRAM + 0x805, 0),  // MOV.L @R1+,R2
        (0x6116, 0, 99),                 // aliased destination suppresses increment
        (0x2125, HIGH_WRAM + 0x7FF, 99), // MOV.W R2,@-R1
        (0x2126, HIGH_WRAM + 0x7FD, 99), // MOV.L R2,@-R1
    ] {
        let (mut cpu, mut bus) = setup(&[opcode]);
        cpu.state.r[1] = HIGH_WRAM + 0x801;
        cpu.state.r[2] = 99;
        cpu.step(&mut bus).unwrap();
        assert_eq!((cpu.state.r[1], cpu.state.r[2]), (r1, r2), "{opcode:04X}");
        assert!(cpu.state.address_error_pending);
    }
}

#[test]
fn delayed_successors_complete_before_address_error_acceptance() {
    for (branch, taken) in [
        (0xA002, true),
        (0x8D02, true),
        (0x8D02, false),
        (0x8F02, true),
        (0x8F02, false),
    ] {
        let (mut cpu, mut bus) = setup(&[branch, 0x6216, 0x7363, 0x0009]);
        cpu.state.r[1] = HIGH_WRAM + 0x801;
        cpu.state.sr = u32::from((branch == 0x8D02) == taken);
        let sr = cpu.state.sr;
        cpu.step(&mut bus).unwrap();
        cpu.step(&mut bus).unwrap(); // Faulting load in the successor.
        assert_eq!(cpu.state.r[1], HIGH_WRAM + 0x805);
        assert_eq!(cpu.state.r[2], 0);
        assert!(cpu.state.address_error_pending);
        assert_eq!(cpu.state.exceptions, 0);
        assert!(!cpu.accept_nmi(&mut bus).unwrap());
        cpu.step(&mut bus).unwrap();
        assert_entry(&cpu, &bus, HIGH_WRAM + if taken { 8 } else { 4 }, sr, 2);
        assert_eq!(cpu.state.r[3], 0); // No instruction past the successor ran.
    }
}

#[test]
fn rte_operand_error_waits_for_its_slot_but_control_transfers_do_not() {
    let (mut cpu, mut bus) = setup(&[0x002B, 0x7401]);
    cpu.state.r[15] = STACK - 7; // Invalid reads deterministically restore PC/SR=0.
    cpu.step(&mut bus).unwrap();
    assert!(cpu.state.address_error_pending);
    assert_eq!(cpu.state.delay_target, Some(0));
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.r[4], 1);
    assert_eq!(cpu.state.exceptions, 0);
    // Restore alignment solely to isolate acceptance from undefined stack writes.
    cpu.state.r[15] = STACK;
    cpu.step(&mut bus).unwrap();
    assert_entry(&cpu, &bus, 0, 0, 2);

    let (mut cpu, mut bus) = setup(&[0x4116, 0x7463]); // LDS.L @R1+,MACL
    cpu.state.r[1] = HIGH_WRAM + 0x801;
    cpu.step(&mut bus).unwrap();
    assert!(cpu.interrupt_inhibited);
    cpu.step(&mut bus).unwrap(); // Table 4.10: address error is still accepted.
    assert_entry(&cpu, &bus, HIGH_WRAM + 2, 0xF0, 1);
    assert_eq!(cpu.state.r[4], 0);
}

#[test]
fn unaligned_exception_stacking_reaches_vector_nine_without_recursive_errors() {
    // H §4.8.3: complete the original exception, then dispatch address error;
    // do not recursively latch another error while stacking vector 9 itself.
    for offset in 1..=3 {
        let (mut cpu, mut bus) = setup(&[0xC320]); // TRAPA #32
        bus.write32(cpu.state.vbr + 32 * 4, HANDLER + 0x20).unwrap();
        bus.write16(HANDLER, 0x0009).unwrap();
        cpu.state.r[15] = STACK + offset;
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HANDLER + 0x20);
        assert_eq!(cpu.state.r[15], STACK + offset - 8);
        assert!(cpu.state.address_error_pending);
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.pc, HANDLER);
        assert_eq!(cpu.state.exceptions, 2);
        assert_eq!(
            cpu.state.last_exception.as_ref().unwrap().saved_pc,
            HANDLER + 0x20
        );
        assert_eq!(cpu.state.r[15], STACK + offset - 16);
        assert!(!cpu.state.address_error_pending);
        cpu.step(&mut bus).unwrap();
        assert_eq!(cpu.state.exceptions, 2);
    }
}

#[test]
fn unavailable_exception_stack_remains_a_bounded_host_diagnostic() {
    let (mut cpu, mut bus) = setup(&[0x6212]);
    cpu.state.r[1] = HIGH_WRAM + 0x801;
    cpu.state.r[15] = 0x04000001;
    cpu.step(&mut bus).unwrap();
    let error = cpu.step(&mut bus).unwrap_err();
    assert!(matches!(
        error,
        CpuError::ExceptionEntry {
            vector: 9,
            source: BusError::Unmapped { .. },
            ..
        }
    ));
    let failed = cpu.state.clone();
    for _ in 0..10 {
        assert_eq!(cpu.step(&mut bus), Err(error.clone()));
        assert_eq!(cpu.state, failed);
    }
    assert_eq!(cpu.state.instructions, 1);
    assert_eq!(cpu.state.exceptions, 0);
}

#[test]
fn a_deferred_error_wakes_sleep_in_a_branch_successor() {
    let (mut cpu, mut bus) = setup(&[0x002B, 0x001B]); // RTE; SLEEP
    cpu.state.r[15] = STACK - 7;
    cpu.step(&mut bus).unwrap();
    cpu.step(&mut bus).unwrap();
    assert_eq!(cpu.state.status, CpuStatus::Sleeping);
    assert!(cpu.state.address_error_pending);
    cpu.state.r[15] = STACK;
    cpu.step(&mut bus).unwrap();
    assert_entry(&cpu, &bus, 0, 0, 2);
}

#[test]
fn peripheral_width_pc_relative_and_tas_checks_precede_device_access() {
    // Every invalid cycle must reach vector 9 even though the underlying Bus
    // has no implementation for these on-chip registers/cache partitions.
    for (opcode, address) in [
        (0x6210, 0xFFFF_FF00),
        (0x2120, 0xFFFF_FFFF),
        (0x6212, 0xFFFF_FE00),
        (0x2122, 0xFFFF_FEFC),
        (0x411B, 0x4000_0000),
        (0x411B, 0x6000_0000),
        (0x411B, 0xC000_0000),
        (0x411B, 0xFFFF_FE00),
    ] {
        let (mut cpu, mut bus) = setup(&[opcode]);
        cpu.state.r[1] = address;
        cpu.step(&mut bus).unwrap();
        assert!(
            cpu.state.address_error_pending,
            "{opcode:04X}/{address:08X}"
        );
        let sr = cpu.state.sr;
        cpu.step(&mut bus).unwrap();
        assert_entry(&cpu, &bus, HIGH_WRAM + 2, sr, 1);
    }
    // A slot's PC-relative operand uses the pending destination + 2.
    for target in [0x3FFF_FFFE, 0x5FFF_FFFE, 0xFFFF_FDFE] {
        for opcode in [0x9200, 0xD200] {
            let (mut cpu, mut bus) = setup(&[0x412B, opcode]);
            cpu.state.r[1] = target;
            cpu.step(&mut bus).unwrap();
            cpu.step(&mut bus).unwrap();
            assert!(cpu.state.address_error_pending);
            cpu.step(&mut bus).unwrap();
            assert_entry(&cpu, &bus, target, 0xF0, 2);
        }
    }
    // Correct-width ordinary accesses still diagnose missing implementations.
    for (opcode, address) in [(0x6211, 0xFFFF_FF00), (0x6210, 0x0400_0000)] {
        let (mut cpu, mut bus) = setup(&[opcode]);
        cpu.state.r[1] = address;
        assert!(matches!(cpu.step(&mut bus), Err(CpuError::Bus { .. })));
        assert!(!cpu.state.address_error_pending);
        assert_eq!(cpu.state.instructions, 0);
    }
}
