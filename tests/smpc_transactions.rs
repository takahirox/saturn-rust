//! Original ST-169 command-contract guest, including explicit unsupported paths.
use saturn_rust::{
    Boot, Machine, MachineError,
    bus::{Bus, BusError, Memory},
    cpu::CpuError,
    smpc::diagnostics::Rejection,
};

fn rom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in text
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}

#[test]
fn command_guest_uses_reset_vectors_real_results_irq_and_chunk_invariant_observation() {
    let bios = rom(include_str!("fixtures/smpc-transactions.hex"));
    let expected: Vec<_> = include_str!("fixtures/smpc-transactions.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut reference = None;
    for chunk in [1, 3, 127, 10000] {
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&bios)).unwrap();
            let mut left = 10000;
            while left != 0 {
                let n = left.min(chunk);
                m.run_steps(n).unwrap();
                // Debug observation must not advance clocks or acknowledge IRQs.
                let _ = m.cpu_state();
                let _ = m.debug_read32(0x0601a000).unwrap();
                assert!(m.smpc_fault_diagnostics().is_none());
                left -= n;
            }
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0601a000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(m.cpu_state().exceptions, 1);
            let result = (m.clocks(), m.cpu_state().clone());
            assert_eq!(*reference.get_or_insert(result.clone()), result);
        }
    }
}

#[test]
fn firmware_faults_distinguish_direction_operand_state_and_width_without_side_effects() {
    let cases = [
        (
            include_str!("fixtures/smpc-fault-read.hex"),
            Rejection::WriteOnly,
            None,
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-missing-sf.hex"),
            Rejection::MissingSf,
            Some(9),
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-invalid-sf.hex"),
            Rejection::InvalidSf,
            Some(0),
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-busy.hex"),
            Rejection::Busy,
            Some(8),
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-command.hex"),
            Rejection::UnsupportedCommand,
            Some(255),
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-parameters.hex"),
            Rejection::InvalidParameters,
            Some(16),
            1,
        ),
        (
            include_str!("fixtures/smpc-fault-word.hex"),
            Rejection::ByteOnly,
            Some(9),
            2,
        ),
        (
            include_str!("fixtures/smpc-fault-long.hex"),
            Rejection::ByteOnly,
            Some(0x00120034),
            4,
        ),
        (
            include_str!("fixtures/smpc-fault-word-read.hex"),
            Rejection::ByteOnly,
            None,
            2,
        ),
        (
            include_str!("fixtures/smpc-fault-long-read.hex"),
            Rejection::ByteOnly,
            None,
            4,
        ),
    ];
    for (source, reason, write, width) in cases {
        let bios = rom(source);
        for chunk in [1, 127, 10000] {
            let mut m = Machine::new();
            m.reset(Boot::ExternalBios(&bios)).unwrap();
            let error = (0..10000)
                .find_map(|_| m.run_steps(chunk).err())
                .expect("bounded fault");
            let MachineError::Cpu(CpuError::Bus {
                pc,
                source: BusError::Smpc(fault),
            }) = error
            else {
                panic!("{error:?}")
            };
            assert_eq!(
                (fault.reason, fault.write, fault.width),
                (reason, write, width)
            );
            assert_eq!(m.bus().read8(0x0601a200), Ok(0));
            if reason == Rejection::Busy {
                assert_eq!((fault.state.command, fault.state.pending), (9, Some(9)));
                assert!(fault.state.remaining.unwrap() > 0);
                assert!(fault.state.remaining.unwrap() < 1075);
            } else {
                assert!(!fault.state.busy);
                assert_eq!(fault.state.input, [0; 7]);
            }
            assert!(fault.main_clock.is_some());
            if write.is_none() || width != 1 {
                assert_eq!(
                    fault.instruction,
                    Some(u16::from_be_bytes(
                        bios[pc as usize..pc as usize + 2].try_into().unwrap()
                    ))
                );
            }
            let before = (
                m.clocks(),
                m.cpu_state().clone(),
                m.bus().smpc_command_state(),
            );
            let detail = m.smpc_fault_diagnostics().unwrap();
            assert!(detail.contains("Master SH-2 PC=0x"));
            assert!(
                detail.contains("main_clock:")
                    && detail.contains("remaining:")
                    && detail.contains("input:")
            );
            assert_eq!(
                before,
                (
                    m.clocks(),
                    m.cpu_state().clone(),
                    m.bus().smpc_command_state()
                )
            );
            // Faults are sticky and a cold reset cancels commands and fault data.
            assert!(m.step().is_err());
            m.reset(Boot::ExternalBios(&bios)).unwrap();
            assert!(m.smpc_fault_diagnostics().is_none());
            assert!(!m.bus().smpc_command_state().busy);
        }
    }
}

#[test]
fn byte_only_negative_control_rejects_wide_accesses_before_any_register_changes() {
    // This test deliberately uses only the inherited public API, so it also
    // runs unmodified against main as a genuine red/green negative control.
    let mut b = Bus::default();
    for base in [0x00100000, 0x00100100, 0x20100000, 0x2017ff80] {
        b.write8(base + 0x63, 1).unwrap();
        assert!(
            b.write16(base + 0x1e, 9).is_err(),
            "wide COMREG write must fail"
        );
        assert_eq!(b.read8(base + 0x5f), Ok(0));
        assert!(b.write32(base, 0x00120034).is_err());
        assert!(b.read16(base + 0x62).is_err());
        assert!(b.read32(base + 0x60).is_err());
        assert_eq!(b.read8(base + 0x63), Ok(1));
        assert_eq!(b.read8(base + 0x1e), Ok(255));
        b.write8(base + 0x1e, 9).unwrap(); // disconnected lane cannot issue command
        assert_eq!(b.read8(base + 0x5f), Ok(0));
    }
}

#[test]
fn raster_gated_intback_forms_publish_results_and_service_each_irq() {
    let bios = rom(include_str!("fixtures/smpc-timed-peripherals.hex"));
    let expected: Vec<_> = include_str!("fixtures/smpc-timed-peripherals.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut reference = None;
    for chunk in [1, 3, 127, 10000] {
        for observe in [false, true] {
            let mut m = Machine::new();
            m.reset(Boot::ExternalBios(&bios)).unwrap();
            let mut left = 500000;
            while left > 0 {
                let n = left.min(chunk);
                m.run_steps(n).unwrap();
                if observe {
                    let _ = m.cpu_state();
                    let _ = m.debug_read32(0x0601d000).unwrap();
                    assert!(m.smpc_fault_diagnostics().is_none());
                }
                left -= n;
            }
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0601d000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(m.cpu_state().exceptions, 6);
            let result = (m.clocks(), m.cpu_state().clone());
            assert_eq!(*reference.get_or_insert(result.clone()), result);
        }
    }
}

#[test]
fn cold_reset_cancels_guest_commands_beyond_former_deadlines_and_preserves_battery() {
    let observer = rom(include_str!("fixtures/smpc-reset-observer.hex"));
    let expected: Vec<_> = include_str!("fixtures/smpc-reset-observer.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut reference = None;
    for (command, source) in [
        (0x17, include_str!("fixtures/smpc-reset-pending-17.hex")),
        (0x16, include_str!("fixtures/smpc-reset-pending-16.hex")),
        (0x10, include_str!("fixtures/smpc-reset-pending-10.hex")),
    ] {
        let bios = rom(source);
        for extra_turns in [0, 1, 7] {
            for chunk in [1, 3, 127, 10000] {
                let mut m = Machine::new();
                m.reset(Boot::ExternalBios(&bios)).unwrap();
                for _ in 0..100000 {
                    m.step().unwrap();
                    if m.bus().read8(0x0601a200) == Ok(0x55) {
                        break;
                    }
                }
                for _ in 0..extra_turns {
                    m.step().unwrap();
                }
                assert_eq!(m.bus().read8(0x0601a200), Ok(0x55));
                assert_eq!(m.bus().read32(0x0601a00c), Ok(1));
                assert_eq!(m.bus().read32(0x0601a010), Ok(u32::from(command)));
                let pending = m.bus().smpc_command_state();
                assert_eq!(pending.pending, Some(command));
                let remaining = pending.remaining.unwrap();
                assert!(remaining > 0);
                m.reset(Boot::ExternalBios(&observer)).unwrap();
                assert_eq!(m.bus().smpc_command_state().pending, None);
                let mut left = 100000;
                while left > 0 {
                    let n = left.min(chunk);
                    m.run_steps(n).unwrap();
                    let _ = m.cpu_state();
                    let _ = m.debug_read32(0x0601c000).unwrap();
                    assert!(m.smpc_fault_diagnostics().is_none());
                    left -= n;
                }
                // The observer waits a full raster transition before sampling
                // cancelled state, well beyond every former command deadline.
                assert!(m.clocks() > u64::from(remaining) + 100000);
                let actual: Vec<_> = (0..expected.len())
                    .map(|i| m.bus().read32(0x0601c000 + i as u32 * 4).unwrap())
                    .collect();
                assert_eq!(
                    actual, expected,
                    "command {command:02x}, offset {extra_turns}, chunk {chunk}"
                );
                assert_eq!(m.cpu_state().exceptions, 0);
                let state = m.bus().smpc_command_state();
                assert!(!state.busy && !state.sf);
                let result = (m.clocks(), m.cpu_state().clone());
                assert_eq!(*reference.get_or_insert(result.clone()), result);
            }
        }
    }
}
