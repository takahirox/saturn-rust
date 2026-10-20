//! Executes an original BIOS fixture; no firmware or host exception-handler hook.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::{CpuStatus, Exception},
};

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/sh2-address-errors.hex").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let mut offset = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for word in parts {
            let word = u16::from_str_radix(word, 16).unwrap();
            bytes[offset..offset + 2].copy_from_slice(&word.to_be_bytes());
            offset += 2;
        }
    }
    bytes
}

#[test]
fn synthetic_bios_handles_data_slot_and_fetch_address_errors_and_returns() {
    // SH7604 §§4.3.2/4.6/4.7: data errors save the next PC, slot data errors
    // save the branch destination, a bad fetch saves the unfetched address.
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    machine.run_steps(1000).unwrap();
    let s = machine.cpu_state();
    assert_eq!(s.status, CpuStatus::Sleeping);
    assert_eq!(s.pc, 0x120);
    assert_eq!(s.instructions, 55); // 11 main + 14 + 14 + 16 handler instructions.
    assert_eq!(s.exceptions, 3);
    assert_eq!(s.r[1], 0x0600_1003); // Both invalid cycles completed auto-update.
    assert_eq!(s.r[2], 85);
    assert_eq!(s.r[6], 0); // Documented deterministic invalid-read data policy.
    assert_eq!((s.r[4], s.r[5], s.r[9]), (1, 7, 3));
    assert_eq!(s.r[15], 0x0610_0000);
    assert_eq!(s.sr, 0xF0);
    assert_eq!(
        s.last_exception,
        Some(Exception {
            vector: 9,
            saved_pc: 0x11B
        })
    );
    for (i, expected) in [0x108, 0xF0, 0x112, 0xF0, 0x11B, 0xF0]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            machine.bus().read32(0x0600_2000 + i as u32 * 4),
            Ok(expected)
        );
    }
    // Invalid store didn't reach either halfword of work RAM.
    assert_eq!(machine.bus().read32(0x0600_1000), Ok(0));
    assert_eq!(machine.bus().read32(0x0600_1004), Ok(0));
    let sleeping = machine.cpu_state().clone();
    machine.run_steps(10000).unwrap();
    assert_eq!(machine.cpu_state(), &sleeping);

    // Reset discards exception state and volatile trace. Re-execution is identical.
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    assert!(!machine.cpu_state().address_error_pending);
    assert_eq!(machine.bus().read32(0x0600_2000), Ok(0));
    machine.run_steps(1000).unwrap();
    assert_eq!(machine.cpu_state(), &sleeping);
}

#[test]
fn reset_discards_a_latched_address_error_before_another_boot_mode_runs() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    machine.run_steps(4).unwrap();
    assert!(machine.cpu_state().address_error_pending);
    assert_eq!(machine.cpu_state().exceptions, 0);
    machine.reset(Boot::HlePrototype).unwrap();
    assert!(!machine.cpu_state().address_error_pending);
    machine.step().unwrap();
    assert_eq!(machine.cpu_state().exceptions, 0);
    assert_eq!(machine.cpu_state().r[0], 5);
}
