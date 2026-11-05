//! Original ST-169 command-contract guest, including explicit unsupported paths.
#[path = "support/smpc_progress.rs"]
mod progress;

use saturn_rust::{
    Boot, Machine, MachineError,
    bus::{Bus, BusError, Memory, Processor},
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
fn dual_sh2_firmware_preserves_command_issuer_and_permitted_slave_results() {
    let master_expected: Vec<_> = include_str!("fixtures/smpc-issuer-allowed.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for (name, command, source) in [
        (
            "allowed",
            None,
            include_str!("fixtures/smpc-issuer-allowed.hex"),
        ),
        ("02", Some(2), include_str!("fixtures/smpc-issuer-02.hex")),
        ("03", Some(3), include_str!("fixtures/smpc-issuer-03.hex")),
        (
            "0e",
            Some(0x0e),
            include_str!("fixtures/smpc-issuer-0e.hex"),
        ),
        (
            "0f",
            Some(0x0f),
            include_str!("fixtures/smpc-issuer-0f.hex"),
        ),
        (
            "10",
            Some(0x10),
            include_str!("fixtures/smpc-issuer-10.hex"),
        ),
    ] {
        let mut reference = None;
        for chunk in [1, 3, 127, 10000] {
            for observe in [false, true] {
                let mut m = Machine::new();
                m.reset(Boot::ExternalBios(&rom(source))).unwrap();
                let mut error = None;
                for start in (0..250000).step_by(chunk) {
                    let result = m.run_steps((250000 - start).min(chunk) as u32);
                    if observe {
                        let _ = (m.cpu_state(), m.slave_state());
                        let _ = m.debug_read32(0x0601e100).unwrap();
                        let _ = m.smpc_fault_diagnostics();
                    }
                    if let Err(e) = result {
                        error = Some(e);
                        break;
                    }
                }
                if let Some(command) = command {
                    let Some(MachineError::SlaveCpu(CpuError::Bus {
                        source: BusError::Smpc(fault),
                        ..
                    })) = &error
                    else {
                        panic!("{name}: expected slave fault, got {error:?}");
                    };
                    assert_eq!(fault.reason, Rejection::ProhibitedIssuer);
                    assert_eq!(
                        (fault.address, fault.width, fault.write),
                        (0x2010001f, 1, Some(command))
                    );
                    assert!(fault.state.sf);
                    assert!(!fault.state.busy);
                    assert_eq!(
                        (
                            fault.state.command,
                            fault.state.pending,
                            fault.state.remaining
                        ),
                        (0x17, None, None)
                    );
                    assert_eq!(&fault.state.input[..3], &[1, 0, 0xf0]);
                    assert_eq!(fault.state.command_report, 0x17);
                    assert_eq!(m.bus().smpc_command_state(), fault.state);
                    assert!(
                        m.smpc_fault_diagnostics()
                            .unwrap()
                            .contains("Slave SH-2 PC=0x")
                    );
                    assert_eq!(m.bus().read32(0x0601e204), Ok(0));
                    // Guest observed permitted SETSMEM completion before fault.
                    let actual: Vec<_> = (0..8)
                        .map(|i| m.bus().read32(0x0601e100 + i * 4).unwrap())
                        .collect();
                    assert_eq!(actual, [1, 1, 8, 0, 0x17, 1, 1, 8]);
                    let clocks = m.clocks();
                    assert_eq!(
                        m.run_steps(1).unwrap_err().to_string(),
                        error.as_ref().unwrap().to_string()
                    );
                    assert_eq!(m.clocks(), clocks);
                } else {
                    assert!(error.is_none(), "{error:?}");
                    let actual: Vec<_> = (0..master_expected.len())
                        .map(|i| m.bus().read32(0x0601e000 + i as u32 * 4).unwrap())
                        .collect();
                    assert_eq!(actual, master_expected);
                    let actual: Vec<_> = (0..11)
                        .map(|i| m.bus().read32(0x0601e100 + i * 4).unwrap())
                        .collect();
                    assert_eq!(actual, [1, 1, 8, 0, 0x17, 1, 1, 8, 0, 0x16, 0x600d]);
                    assert_eq!(m.bus().read32(0x0601e204), Ok(1));
                }
                assert!(m.slave_state().instructions > 0);
                assert_eq!(
                    (m.cpu_state().exceptions, m.slave_state().exceptions),
                    (0, 0)
                );
                let result = (
                    m.clocks(),
                    m.cpu_state().clone(),
                    m.slave_state().clone(),
                    error.map(|e| e.to_string()),
                );
                let row = include_str!("fixtures/smpc-issuer-progress.tsv")
                    .lines()
                    .find_map(|line| {
                        let (guest, counts) = line.split_once('\t')?;
                        (guest == name).then_some(counts)
                    })
                    .unwrap();
                let expected: Vec<u64> =
                    row.split_whitespace().map(|v| v.parse().unwrap()).collect();
                assert_eq!(
                    [
                        m.clocks(),
                        m.cpu_state().instructions,
                        m.slave_state().instructions
                    ]
                    .as_slice(),
                    expected,
                    "{name}: native must match shared Wasm progress"
                );
                assert_eq!(
                    *reference.get_or_insert(result.clone()),
                    result,
                    "{name}, chunk {chunk}, observe {observe}"
                );
            }
        }
    }
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
fn comreg_read_guests_preserve_operand_instead_of_legacy_byte_signature() {
    // Only inherited public APIs: run this exact test and rom() against main
    // as a negative control. Both guests formerly reported a one-byte access
    // at 0010001f, despite their distinct instruction and effective address.
    for (source, instruction, address, expected) in [
        (
            include_str!("fixtures/smpc-fault-comreg-word-read.hex"),
            0x6011_u16,
            0x0010001e,
            "2-byte read at 0x0010001E: ByteOnly",
        ),
        (
            include_str!("fixtures/smpc-fault-read.hex"),
            0x6010,
            0x0010001f,
            "1-byte read at 0x0010001F: WriteOnly",
        ),
    ] {
        let bios = rom(source);
        assert_eq!(&bios[0x20c..0x20e], &instruction.to_be_bytes());
        let mut reference = None;
        for chunk in [1, 3, 127, 10000] {
            for observe in [false, true] {
                let mut machine = Machine::new();
                machine.reset(Boot::ExternalBios(&bios)).unwrap();
                let error = (0..10000)
                    .find_map(|_| {
                        let result = machine.run_steps(chunk);
                        if observe {
                            let _ = machine.cpu_state();
                            assert_eq!(machine.debug_read32(0x0601a200), Ok(0));
                        }
                        result.err()
                    })
                    .expect("bounded COMREG fault");
                let message = error.to_string();
                assert!(message.contains("PC 0x0000020C"), "{message}");
                assert!(message.contains(expected), "{message}");
                assert_eq!(machine.cpu_state().r[1], address);
                assert_eq!(machine.cpu_state().exceptions, 0);
                assert_eq!(machine.bus().read8(0x00100063), Ok(0));
                assert_eq!(machine.bus().read32(0x0601a200), Ok(0));
                let result = (machine.clocks(), machine.cpu_state().clone(), message);
                assert_eq!(machine.run_steps(1).unwrap_err().to_string(), result.2);
                assert_eq!(machine.clocks(), result.0);
                assert_eq!(
                    *reference.get_or_insert(result.clone()),
                    result,
                    "instruction {instruction:04x}, chunk {chunk}, observe {observe}"
                );
            }
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
fn exception_operands_reject_smpc_before_byte_decomposition_but_preserve_ram_cycles() {
    fn check(memory: &mut impl Memory) {
        for base in [0x00100000, 0x00100100, 0x20100000, 0x2017ff80] {
            // Include operands beginning before the select boundary.
            for address in base - 3..base + 4 {
                let Err(BusError::Smpc(fault)) = memory.exception_write32(address, 0x12003400)
                else {
                    panic!("exception store must reject the complete SMPC longword");
                };
                assert_eq!(
                    (fault.address, fault.width, fault.write, fault.reason),
                    (address, 4, Some(0x12003400), Rejection::ByteOnly)
                );
                assert_eq!(fault.state.input, [0; 7]);
            }
            // SR, disconnected lane, SF and disconnected lane are individually
            // readable: the +61 longword needs the width guard to stop it.
            for address in base + 0x60..base + 0x64 {
                let Err(BusError::Smpc(fault)) = memory.exception_read32(address) else {
                    panic!("exception read must reject the complete SMPC longword");
                };
                assert_eq!(
                    (fault.address, fault.width, fault.write, fault.reason),
                    (address, 4, None, Rejection::ByteOnly)
                );
            }
        }
        // H §§4.8.2–3's exact-address data policy for invalid RAM cycles is
        // unchanged; regular operands still raise the existing alignment error.
        for offset in 1..4 {
            let address = 0x26010000 + offset;
            memory.exception_write32(address, 0x12345678).unwrap();
            assert_eq!(memory.exception_read32(address), Ok(0x12345678));
            assert!(matches!(
                memory.read32(address),
                Err(BusError::Unaligned { .. })
            ));
        }
    }
    let mut bus = Bus::default();
    check(&mut bus);
    for processor in [Processor::Master, Processor::Slave] {
        check(&mut bus.for_cpu(processor));
    }
    assert_eq!(bus.smpc_command_state().input, [0; 7]);
}

#[test]
fn unaligned_exception_stack_and_vector_guests_stop_at_the_smpc_longword_boundary() {
    for (name, source, address, write) in [
        (
            "stack",
            include_str!("fixtures/smpc-exception-stack.hex"),
            0x20100005,
            Some(0xf0),
        ),
        (
            "vector",
            include_str!("fixtures/smpc-exception-vector.hex"),
            0x20100061,
            None,
        ),
    ] {
        let mut reference = None;
        for chunk in [1, 3, 127, 10000] {
            for observe in [false, true] {
                let mut machine = Machine::new();
                machine.reset(Boot::ExternalBios(&rom(source))).unwrap();
                let mut error = None;
                for _ in 0..1000 {
                    let result = machine.run_steps(chunk);
                    if observe {
                        let _ = (machine.cpu_state(), machine.smpc_fault_diagnostics());
                        assert_eq!(machine.debug_read32(0x0601a200), Ok(0));
                    }
                    if let Err(e) = result {
                        error = Some(e);
                        break;
                    }
                }
                let Some(MachineError::Cpu(CpuError::ExceptionEntry {
                    vector: 32,
                    source: BusError::Smpc(fault),
                    ..
                })) = &error
                else {
                    panic!("{name}: expected bounded exception SMPC fault, got {error:?}");
                };
                assert_eq!(
                    (fault.address, fault.width, fault.write, fault.reason),
                    (address, 4, write, Rejection::ByteOnly)
                );
                assert_eq!(fault.state.input, [0; 7]);
                assert_eq!(machine.bus().smpc_command_state(), fault.state);
                assert_eq!(machine.bus().read32(0x0601a200), Ok(0));
                assert_eq!(machine.cpu_state().exceptions, 0);
                if name == "vector" {
                    // RAM stacking completed before the rejected vector fetch.
                    assert_eq!(machine.bus().read32(0x060feffc), Ok(0xf0));
                    assert_eq!(machine.bus().read32(0x060feff8), Ok(0x210));
                }
                let result = (
                    machine.clocks(),
                    machine.cpu_state().clone(),
                    machine.smpc_fault_diagnostics().unwrap(),
                    error.unwrap().to_string(),
                );
                progress::assert_progress(&format!("smpc-exception-{name}"), 0, &machine);
                assert_eq!(machine.run_steps(1).unwrap_err().to_string(), result.3);
                assert_eq!(machine.clocks(), result.0);
                assert_eq!(
                    *reference.get_or_insert(result.clone()),
                    result,
                    "{name}, chunk {chunk}, observe {observe}"
                );
            }
        }
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
            progress::assert_progress("smpc-timed-peripherals", 500000, &m);
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
                for observe in [false, true] {
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
                        if observe {
                            let _ = m.cpu_state();
                            let _ = m.debug_read32(0x0601c000).unwrap();
                            assert!(m.smpc_fault_diagnostics().is_none());
                        }
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
                    progress::assert_progress("smpc-reset-observer", 100000, &m);
                    let result = (m.clocks(), m.cpu_state().clone());
                    assert_eq!(*reference.get_or_insert(result.clone()), result);
                }
            }
        }
    }
}
