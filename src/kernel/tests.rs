use super::*;
use crate::{
    bus::{BusError, Memory},
    cpu::CpuStatus,
};

struct Ram(Vec<u8>);
impl Ram {
    fn read(&self, address: u32, width: usize) -> u32 {
        let offset = (address - RAM_BASE) as usize;
        self.0[offset..offset + width]
            .iter()
            .fold(0, |v, b| v << 8 | u32::from(*b))
    }
    fn write(&mut self, address: u32, width: usize, value: u32) {
        let offset = (address - RAM_BASE) as usize;
        self.0[offset..offset + width].copy_from_slice(&value.to_be_bytes()[4 - width..]);
    }
}
impl Memory for Ram {
    fn read8(&self, a: u32) -> Result<u8, BusError> {
        Ok(self.read(a, 1) as u8)
    }
    fn read16(&self, a: u32) -> Result<u16, BusError> {
        Ok(self.read(a, 2) as u16)
    }
    fn read32(&self, a: u32) -> Result<u32, BusError> {
        Ok(self.read(a, 4))
    }
    fn write8(&mut self, a: u32, v: u8) -> Result<(), BusError> {
        self.write(a, 1, u32::from(v));
        Ok(())
    }
    fn write16(&mut self, a: u32, v: u16) -> Result<(), BusError> {
        self.write(a, 2, u32::from(v));
        Ok(())
    }
    fn write32(&mut self, a: u32, v: u32) -> Result<(), BusError> {
        self.write(a, 4, v);
        Ok(())
    }
}
fn board(words: &[u16], latency: u32) -> Kernel {
    let mut state = Cpu::start(RAM_BASE, RAM_BASE + 0xF000).state().clone();
    state.r[1] = PROBE;
    state.r[2] = RAM_BASE + 0x1000;
    state.r[3] = 0x8123_4567;
    let mut stopped = state.clone();
    stopped.status = CpuStatus::Halted;
    let mut k = Kernel::new([state, stopped], latency);
    for (i, word) in words.iter().enumerate() {
        k.initialize(RAM_BASE + i as u32 * 2, 2, u32::from(*word))
            .unwrap();
    }
    k.enable_trace();
    k
}

#[test]
fn completion_samples_once_and_retirement_keeps_the_sample() {
    // MOV.L @R1,R0; SLEEP. Request at 5, grant at 5, complete at 10, retire at 11.
    let mut k = board(&[0x6012, 0x001B], 5);
    k.initialize(PROBE, 4, 7).unwrap();
    k.schedule(
        10,
        Event::Write {
            address: PROBE,
            width: 4,
            value: 19,
        },
    )
    .unwrap();
    k.schedule(
        11,
        Event::Write {
            address: PROBE,
            width: 4,
            value: 41,
        },
    )
    .unwrap();
    k.run_until(10).unwrap();
    assert_eq!(k.probe_counts(), [0, 0]);
    assert_eq!(k.states()[0].r[0], 0);
    k.run_until(11).unwrap();
    assert_eq!(k.probe_counts(), [1, 0]);
    assert_eq!(k.states()[0].r[0], 0);
    k.run_until(12).unwrap();
    assert_eq!(k.states()[0].r[0], 19);
    k.run_until(100).unwrap();
    assert_eq!(k.probe_counts(), [1, 0]);
    assert_eq!(k.states()[0].instructions, 2);
}

#[test]
fn fetched_opcode_is_sampled_at_completion_and_retained_until_retirement() {
    let mut k = board(&[0xE001, 0x001B], 5);
    k.schedule(
        5,
        Event::Write {
            address: RAM_BASE,
            width: 2,
            value: 0xE007,
        },
    )
    .unwrap();
    k.schedule(
        6,
        Event::Write {
            address: RAM_BASE,
            width: 2,
            value: 0xE009,
        },
    )
    .unwrap();
    k.run_until(7).unwrap();
    assert_eq!(k.states()[0].r[0], 7);
    assert_eq!(k.retired(), [1, 0]);
    k.run_until(100).unwrap();
    assert_eq!(k.states()[0].r[0], 7);
}

#[test]
fn reset_drains_granted_write_but_cancels_delivery_and_queued_requests() {
    // MOV.L R3,@R1; SLEEP; NOP; SLEEP (reset target).
    let mut k = board(&[0x2132, 0x001B, 0x0009, 0x001B], 5);
    k.schedule(
        8,
        Event::Reset {
            owner: 0,
            pc: RAM_BASE + 4,
            sp: 0,
        },
    )
    .unwrap();
    k.run_until(11).unwrap();
    assert_eq!(k.probe_counts(), [0, 1]);
    assert_eq!(k.fabric.probe_value, 0x8123_4567);
    assert_eq!(k.states()[0].instructions, 0);
    k.run_until(100).unwrap();
    assert_eq!(k.states()[0].instructions, 2);
    assert_eq!(k.probe_counts(), [0, 1]);
    assert_eq!(k.states()[0].r[1], 0);
    // Slave's first fetch is queued behind the master when reset cancels it.
    let mut q = board(&[0x0009, 0x001B], 5);
    q.cpus[1] = Executor::new(Cpu::start(RAM_BASE, 0).state().clone());
    q.schedule(
        1,
        Event::Reset {
            owner: 1,
            pc: RAM_BASE + 2,
            sp: 0,
        },
    )
    .unwrap();
    q.run_until(100).unwrap();
    assert!(!q.trace().iter().any(|v| matches!(
        v,
        Trace::Granted {
            owner: 1,
            epoch: 0,
            ..
        }
    )));
    assert_eq!(q.states()[1].instructions, 1);
}

#[test]
fn split_runs_have_identical_dual_cpu_bus_order_and_states() {
    let make = || {
        let mut k = board(&[0x6012, 0x7001, 0x2132, 0xAFFB, 0x0009], 3);
        k.cpus[1] = Executor::new(k.states()[0].clone());
        for (at, value) in [(8, 0x55), (57, 0xAA), (109, 0xFF)] {
            k.schedule(
                at,
                Event::Write {
                    address: PROBE,
                    width: 4,
                    value,
                },
            )
            .unwrap();
        }
        k
    };
    let mut whole = make();
    let mut split = make();
    whole.run_until(500).unwrap();
    for end in 1..=500 {
        split.run_until(end).unwrap();
    }
    assert_eq!(whole.states(), split.states());
    assert_eq!(whole.trace(), split.trace());
    assert_eq!(whole.ram(), split.ram());
    assert_eq!(whole.probe_counts(), split.probe_counts());
    assert!(whole.retired().iter().all(|v| *v > 10));
}

#[test]
fn shared_bus_store_completion_precedes_the_peers_read_sample() {
    let mut k = board(&[0x2132, 0x001B], 2);
    k.initialize(RAM_BASE + 0x100, 2, 0x6012).unwrap();
    k.initialize(RAM_BASE + 0x102, 2, 0x001B).unwrap();
    let mut slave = k.states()[0].clone();
    slave.pc = RAM_BASE + 0x100;
    k.cpus[1] = Executor::new(slave);
    k.run_until(9).unwrap();
    assert_eq!(k.probe_counts(), [1, 1]);
    assert_eq!(k.states()[1].r[0], 0);
    k.run_until(10).unwrap();
    assert_eq!(k.states()[1].r[0], 0x8123_4567);
    let completions: Vec<_> = k
        .trace()
        .iter()
        .filter_map(|entry| match entry {
            Trace::Completed {
                at,
                owner,
                operation,
                ..
            } => Some((*at, *owner, *operation)),
            _ => None,
        })
        .collect();
    assert_eq!(
        completions[2],
        (
            6,
            0,
            Operation::Write {
                address: PROBE,
                width: 4,
                value: 0x8123_4567
            }
        )
    );
    assert_eq!(
        completions[3],
        (
            8,
            1,
            Operation::Read {
                address: PROBE,
                width: 4
            }
        )
    );
}

#[test]
fn reset_at_read_completion_discards_only_the_old_cpu_delivery() {
    let mut k = board(&[0x6012, 0x001B], 5);
    k.initialize(PROBE, 4, 99).unwrap();
    k.schedule(
        10,
        Event::Reset {
            owner: 0,
            pc: RAM_BASE + 2,
            sp: 0,
        },
    )
    .unwrap();
    k.run_until(100).unwrap();
    assert_eq!(k.probe_counts(), [1, 0]);
    assert_eq!(k.states()[0].r[0], 0);
    assert_eq!(k.states()[0].instructions, 1);
}

#[test]
fn original_isa_guest_matches_reference_at_each_retirement() {
    // Independent ordinary-memory/branch program; includes source/destination
    // aliasing, sign extension, indexed/displaced/GBR/PC-relative loads/stores.
    let words = [
        0xE0FF, 0xE405, 0x7403, 0x354C, 0x4528, 0x6548, 0x654D, 0x3540, 0x2236, 0x6024, 0x6024,
        0x6425, 0x8023, 0x8423, 0x8122, 0x8522, 0x1232, 0x5422, 0xC202, 0xC602, 0xE000, 0x0236,
        0x042E, 0x900A, 0xD50A, 0xC70A, 0x0018, 0x8D01, 0x7401, 0x7402, 0x0018, 0x8F01, 0x7404,
        0x7408, 0xA001, 0x740F, 0x7463, 0x001B,
    ];
    let mut k = board(&words, 2);
    let mut memory = Ram(k.ram().to_vec());
    // Keep GBR accesses in RAM (GBR defaults to zero otherwise).
    k.cpus[0] = Executor::new({
        let mut s = k.states()[0].clone();
        s.gbr = RAM_BASE + 0x2000;
        s
    });
    let mut reference = crate::cpu::resumable::reference_from_state(k.states()[0].clone());
    let mut count = 0;
    for end in 1..1000 {
        k.run_until(end).unwrap();
        if k.states()[0].instructions != count {
            reference.step(&mut memory).unwrap();
            count += 1;
            assert_eq!(k.states()[0], reference.state(), "retirement {count}");
            assert_eq!(k.ram(), memory.0, "retirement {count}");
        }
        if k.states()[0].status == CpuStatus::Sleeping {
            break;
        }
    }
    assert!(count > 25);
}

#[test]
fn unsupported_and_invalid_operations_do_not_become_nops_or_device_accesses() {
    let mut k = board(&[0xFFFF], 1);
    assert_eq!(
        k.run_until(100),
        Err(Error::Unsupported {
            pc: RAM_BASE,
            opcode: 0xFFFF
        })
    );
    assert_eq!(k.retired(), [0, 0]);
    assert_eq!(
        k.run_until(100),
        Err(Error::Unsupported {
            pc: RAM_BASE,
            opcode: 0xFFFF
        })
    );
    let mut k = board(&[0x6012], 1);
    k.cpus[0] = Executor::new({
        let mut s = k.states()[0].clone();
        s.r[1] = PROBE + 1;
        s
    });
    assert_eq!(
        k.run_until(100),
        Err(Error::Address {
            address: PROBE + 1,
            width: 4
        })
    );
    assert_eq!(k.probe_counts(), [0, 0]);
    // A branch in an untaken conditional delay slot is rejected explicitly too.
    let mut k = board(&[0x0018, 0x8F00, 0xA000], 1);
    assert!(matches!(
        k.run_until(100),
        Err(Error::Unsupported { opcode: 0xA000, .. })
    ));
}

#[test]
fn ordinary_memory_addressing_and_aliases_match_the_isa_reference() {
    let opcodes = [
        0x6120, 0x6121, 0x6122, 0x6124, 0x6125, 0x6126, 0x6224, 0x6225, 0x6226, 0x012C, 0x012D,
        0x012E, 0x5123, 0x8421, 0x8521, 0x9402, 0xD402, 0xC401, 0xC501, 0xC601, 0x2120, 0x2121,
        0x2122, 0x2124, 0x2125, 0x2126, 0x2224, 0x2225, 0x2226, 0x0124, 0x0125, 0x0126, 0x1123,
        0x8021, 0x8121, 0xC001, 0xC101, 0xC201,
    ];
    for opcode in opcodes {
        let mut k = board(&[opcode, 0x001B], 3);
        for (i, byte) in k.fabric.ram.iter_mut().enumerate().skip(0x1000) {
            *byte = (i as u8).wrapping_mul(37).wrapping_add(0x80);
        }
        let mut state = k.states()[0].clone();
        state.r[0] = 4;
        state.r[1] = RAM_BASE + 0x1100;
        state.r[2] = RAM_BASE + 0x1200;
        state.gbr = RAM_BASE + 0x2000;
        let mut reference = crate::cpu::resumable::reference_from_state(state.clone());
        k.cpus[0] = Executor::new(state);
        let mut ram = Ram(k.ram().to_vec());
        reference.step(&mut ram).unwrap();
        for end in 1..100 {
            k.run_until(end).unwrap();
            if k.retired()[0] == 1 {
                break;
            }
        }
        assert_eq!(k.states()[0], reference.state(), "opcode {opcode:04X}");
        assert_eq!(k.ram(), ram.0, "opcode {opcode:04X}");
    }
}

#[test]
fn branch_targets_links_and_delay_slots_match_the_isa_reference() {
    for opcode in [
        0xA001, 0xB001, 0x8901, 0x8B01, 0x8D01, 0x8F01, 0x410B, 0x412B, 0x0103, 0x0123, 0x000B,
    ] {
        for t in [0, 1] {
            let mut k = board(
                &[
                    opcode, 0xD002, 0x001B, 0x001B, 0x001B, 0x001B, 0xCAFE, 0xBEEF,
                ],
                1,
            );
            let mut state = k.states()[0].clone();
            state.sr = t;
            state.r[1] = RAM_BASE + 6;
            state.pr = RAM_BASE + 6;
            if matches!(opcode, 0x0103 | 0x0123) {
                state.r[1] = 2;
            }
            let mut reference = crate::cpu::resumable::reference_from_state(state.clone());
            k.cpus[0] = Executor::new(state);
            let mut ram = Ram(k.ram().to_vec());
            let mut retired = 0;
            for end in 1..100 {
                k.run_until(end).unwrap();
                if k.retired()[0] != retired {
                    reference.step(&mut ram).unwrap();
                    retired += 1;
                    assert_eq!(
                        k.states()[0],
                        reference.state(),
                        "opcode {opcode:04X}, T={t}, instruction {retired}"
                    );
                    if retired == 2 {
                        break;
                    }
                }
            }
            assert_eq!(retired, 2);
        }
    }
}
