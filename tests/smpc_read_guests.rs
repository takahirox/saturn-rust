//! Original reset-vector firmware exercises scheduled DMAC and cache reads.
use saturn_rust::{
    Boot, Machine, MachineError,
    bus::{BIOS_SIZE, Bus, BusError, Memory},
    cpu::CpuError,
    smpc::diagnostics::{ReadUnit, Rejection},
};
#[path = "support/smpc_progress.rs"]
mod progress;

fn rom(text: &str) -> Vec<u8> {
    let mut image = vec![0; BIOS_SIZE];
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}

#[test]
fn scheduled_dma_read_guest_preserves_error_context() {
    check_read_guest("dma", include_str!("fixtures/smpc-read-dma.hex"));
}

#[test]
fn scheduled_cache_read_guest_preserves_operand_context() {
    check_read_guest("cache", include_str!("fixtures/smpc-read-cache.hex"));
}

#[test]
fn scheduled_read_controls_complete_their_transfers() {
    for (name, source) in [
        ("dma-ram", include_str!("fixtures/smpc-read-dma-ram.hex")),
        (
            "cache-through",
            include_str!("fixtures/smpc-read-cache-through.hex"),
        ),
        (
            "cache-disabled",
            include_str!("fixtures/smpc-read-cache-disabled.hex"),
        ),
    ] {
        check_read_guest(name, source);
    }
}

fn check_read_guest(name: &str, source: &str) {
    let image = rom(source);
    let mut reference = None;
    for chunk in [1, 3, 127, 10000] {
        for observe in [false, true] {
            let mut machine = Machine::new();
            machine.reset(Boot::ExternalBios(&image)).unwrap();
            let mut error = None;
            let mut left = 10000;
            while left > 0 {
                let n = chunk.min(left);
                let result = machine.run_steps(n);
                left -= n;
                if observe {
                    let _ = (machine.cpu_state(), machine.smpc_fault_diagnostics());
                    machine.debug_read32(0x0601f000).unwrap();
                    // Debug uses the nonobserving on-chip path: these
                    // reads must not acknowledge AE/TE or change replay.
                    machine.debug_read32(0xffffffb0).unwrap();
                    machine.debug_read32(0xffffff8c).unwrap();
                }
                if let Err(e) = result {
                    error = Some(e);
                    break;
                }
            }
            let mut expected_state = Bus::default().smpc_command_state();
            expected_state.sf = true;
            expected_state.input = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde];
            assert_eq!(machine.bus().smpc_command_state(), expected_state);
            let trace: Vec<_> = (0..20)
                .map(|i| machine.bus().read32(0x0601f000 + i * 4).unwrap())
                .collect();
            assert_eq!(&trace[..3], &[1, 0, 0]);
            let Some(MachineError::Cpu(CpuError::Bus {
                pc,
                source: BusError::Smpc(fault),
            })) = &error
            else {
                panic!("{name}: expected bounded SMPC snapshot, got {error:?}");
            };
            assert_eq!(fault.instruction, Some(0x6010));
            assert_eq!(fault.main_clock, Some(machine.clocks()));
            assert_eq!(fault.state, expected_state);
            assert_eq!(machine.debug_read32(0x0601f080), Ok(0));
            if name == "cache" {
                assert_eq!(*pc, 0x20001000);
                assert_eq!(
                    (fault.address, fault.width, fault.write),
                    (0x00100021, 1, None)
                );
                assert_eq!(fault.reason, Rejection::ByteOnly);
                assert_eq!(
                    fault.refill,
                    Some(ReadUnit {
                        address: 0x00100024,
                        width: 4
                    })
                );
                assert_eq!(&trace[3..], &[0; 17]);
                assert_eq!(machine.cpu_state().exceptions, 0);
            } else {
                // The transfer finished; this deliberate byte read of
                // COMREG supplies a side-effect-free hidden-state snapshot.
                // DMA rejection is handled by vector10, never a host fault.
                assert_eq!(
                    (fault.address, fault.width, fault.write),
                    (0x2010001f, 1, None)
                );
                assert_eq!(fault.reason, Rejection::WriteOnly);
                assert_eq!(fault.refill, None);
                assert_eq!(
                    *pc,
                    if name.starts_with("dma") {
                        0x200018e8
                    } else {
                        0x20001078
                    }
                );
                if name == "dma" {
                    assert_eq!(trace[3], 10);
                    let exception = machine.cpu_state().last_exception.as_ref().unwrap();
                    assert_eq!((exception.vector, exception.saved_pc), (10, trace[4]));
                    assert_eq!(trace[4], 0x20000444);
                    assert_eq!(trace[5], 0xf0);
                    assert_eq!(
                        &trace[6..19],
                        &[
                            5, 0x00100020, 0x2601f100, 4, 0x4e11, 1, 0, 0, 0xa5a5a5a5, 0xa5a5a5a5,
                            0xa5a5a5a5, 0xa5a5a5a5, 0x600d,
                        ]
                    );
                    assert_eq!(machine.cpu_state().exceptions, 1);
                } else if name == "dma-ram" {
                    assert_eq!(
                        &trace[3..17],
                        &[
                            0, 1, 0x2601f210, 0x2601f110, 0, 0x4e13, 1, 0, 0, 0x12345678,
                            0x89abcdef, 0x13579bdf, 0x2468ace0, 0x600d,
                        ]
                    );
                    assert_eq!(machine.cpu_state().exceptions, 0);
                } else {
                    assert_eq!(&trace[3..8], &[0, 1, 0, 0, 0x600d]);
                    assert_eq!(machine.cpu_state().exceptions, 0);
                }
            }
            progress::assert_progress(&format!("smpc-read-{name}"), 0, &machine);
            let clocks = machine.clocks();
            assert_eq!(machine.run_steps(1).unwrap_err(), *error.as_ref().unwrap());
            assert_eq!(machine.clocks(), clocks);
            assert_eq!(machine.bus().smpc_command_state(), expected_state);
            let result = (
                trace,
                machine.clocks(),
                machine.cpu_state().clone(),
                machine.smpc_fault_diagnostics(),
                error,
            );
            assert_eq!(
                *reference.get_or_insert(result.clone()),
                result,
                "{name}, chunk {chunk}, observe {observe}"
            );
        }
    }
}
