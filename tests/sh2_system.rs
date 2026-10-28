//! The same original guest ROM is executed by the Chromium/Wasm smoke test.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::{CpuStatus, Exception},
};

fn original_rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/sh2-system.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let mut address = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for word in parts {
            let word = u16::from_str_radix(word, 16).unwrap();
            bytes[address..address + 2].copy_from_slice(&word.to_be_bytes());
            address += 2;
        }
    }
    bytes
}

#[test]
fn synthetic_rom_executes_guest_trap_illegal_slot_handlers_and_sleep() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&original_rom())).unwrap();
    // Hand-calculated fetch, retired count, R0, exception count after each step.
    let trace = [
        (0x102, 1, 5, 0),
        (0x180, 2, 5, 1),
        (0x182, 3, 20, 1),
        (0x184, 4, 20, 1),
        (0x104, 5, 22, 1),
        (0x106, 6, 22, 1),
        (0x108, 7, 22, 1),
        (0x10C, 8, 23, 1),
        (0x200, 8, 23, 2),
        (0x202, 9, 23, 2),
        (0x204, 10, 23, 2),
        (0x206, 11, 23, 2),
        (0x208, 12, 23, 2),
        (0x10E, 13, 27, 2),
        (0x110, 14, 27, 2),
        (0x220, 14, 27, 3),
        (0x222, 15, 27, 3),
        (0x114, 16, 35, 3),
        (0x116, 17, 35, 3),
        (0x118, 18, 35, 3),
        (0x11A, 19, 35, 3),
    ];
    for (pc, retired, r0, exceptions) in trace {
        machine.step().unwrap();
        let state = machine.cpu_state();
        assert_eq!(
            (state.pc, state.instructions, state.r[0], state.exceptions),
            (pc, retired, r0, exceptions)
        );
    }
    let sleeping = machine.cpu_state().clone();
    assert_eq!(sleeping.status, CpuStatus::Sleeping);
    assert_eq!(sleeping.r[15], 0x06100000);
    assert_eq!(
        sleeping.last_exception,
        Some(Exception {
            vector: 6,
            saved_pc: 0x114
        })
    );
    // H §7.11.2: store acceptance precedes external completion. SLEEP's
    // retained successor supplies no real IF to fence the preceding write.
    // One sleeping step finishes the remaining part of the high-RAM write
    // (two bus states plus six default Tw), without changing CPU state.
    let slept_at = machine.clocks();
    machine.step().unwrap();
    assert!(machine.clocks() - slept_at <= 8);
    assert_eq!(machine.cpu_state(), &sleeping);
    assert_eq!(machine.bus().read32(0x06001000), Ok(35));
    machine.run_steps(10_000).unwrap();
    machine.advance_frame().unwrap();
    assert_eq!(machine.cpu_state(), &sleeping);
    machine.reset(Boot::ExternalBios(&original_rom())).unwrap();
    assert_eq!(machine.cpu_state().exceptions, 0);
    assert_eq!(machine.cpu_state().last_exception, None);
    assert_eq!(machine.cpu_state().delay_target, None);
}
