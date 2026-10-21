//! The same original ROM executes through Wasm in browser-smoke.mjs.
//! H tables 4.3/5.4, §§4.8.3/9.2/10.3/12.2–12.3/13.2–13.3.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};

fn rom() -> Vec<u8> {
    load(include_str!("fixtures/sh7604.hex"))
}
fn load(text: &str) -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
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
fn dack_guest_waits_for_real_external_requests_and_reset_aborts_armed_channels() {
    let rom = load(include_str!("fixtures/dack.hex"));
    let mut machine = Machine::new();
    for _ in 0..2 {
        machine.reset(Boot::ExternalBios(&rom)).unwrap();
        assert_eq!(machine.bus().read32(0xFFFFFF8C), Ok(0));
        machine.run_steps(1000).unwrap();
        assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(machine.cpu_state().exceptions, 0);
        for (i, value) in [3, 3, 1, 0x600D].into_iter().enumerate() {
            assert_eq!(machine.bus().read32(0x06003000 + i as u32 * 4), Ok(value));
        }
        assert_eq!(machine.bus().read32(0x06002000), Ok(0));
        machine.run_steps(1000).unwrap();
        assert_eq!(machine.bus().read32(0xFFFFFF88), Ok(3));
        assert_eq!(machine.bus().read32(0xFFFFFF98), Ok(3));
    }
}

#[test]
fn guest_executes_division_dma_serial_timer_irq_manual_reset_and_stacking_error() {
    let mut machine = Machine::new();
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    let backup = vec![0xA5; 32768];
    machine.import_backup_ram(&backup).unwrap();
    let mut observed_reset = false;
    for _ in 0..10000 {
        machine.step().unwrap();
        if machine.reset_in_progress() {
            observed_reset = true;
            assert!(machine.can_wake_master());
            assert_eq!(machine.backup_ram(), backup);
        }
        if machine.bus().read32(0x06003028).unwrap() == 0x600D {
            break;
        }
    }
    assert!(observed_reset);
    let expected = [
        0xFFFF_FFF2,
        0xFFFF_FFFE,
        0x5555_5555,
        1,
        1,
        1,
        0xFFFF_FF84,
        1,
        0xFFFF_FFFF,
        1,
        0x600D,
        0,
        0,
        0x12345678,
        0,
        1,
    ];
    for (i, value) in expected.into_iter().enumerate() {
        assert_eq!(
            machine.bus().read32(0x06003000 + i as u32 * 4).unwrap(),
            value,
            "mailbox {i}, pc={:08X}",
            machine.cpu_state().pc
        );
    }
    for i in 0..4 {
        assert_eq!(
            machine.bus().read32(0x06005000 + i * 4).unwrap(),
            0x12345678 + i
        );
    }
    machine.step().unwrap();
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 2); // Reset discards earlier DMA/WDT exception counts.
    assert_eq!(
        machine.cpu_state().last_exception.as_ref().unwrap().vector,
        9
    );
    assert_eq!(
        machine
            .cpu_state()
            .last_exception
            .as_ref()
            .unwrap()
            .saved_pc,
        0x1C00
    );
    assert_eq!(machine.cpu_state().r[15], 0x060FF000);
    assert!(!machine.cpu_state().address_error_pending);
    assert_eq!(machine.backup_ram(), backup);
    // A second cold boot executes all the hardware again with identical outputs.
    machine.reset(Boot::ExternalBios(&rom())).unwrap();
    machine.run_steps(10000).unwrap();
    for (i, value) in expected.into_iter().enumerate() {
        assert_eq!(
            machine.bus().read32(0x06003000 + i as u32 * 4).unwrap(),
            value
        );
    }
}
