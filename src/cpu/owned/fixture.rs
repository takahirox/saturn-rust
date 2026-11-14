use super::*;
use crate::bus::{BusError, HIGH_WRAM, Memory};
use crate::cpu::{Cpu, CpuError};

const CODE: [u16; 12] = [
    0x60F6, // MOV.L @R15+,R0
    0x7001, // ADD #1,R0: WB dependency
    0x62F1, // MOV.W @R15,R2 (negative)
    0x322C, // ADD R2,R2
    0x64F0, // MOV.B @R15,R4 (negative)
    0x6543, // MOV R4,R5
    0x66F2, // MOV.L @R15,R6
    0xE707, // independent immediate
    0x367C, // ADD R7,R6
    0x0009, 0x0009, 0x0009,
];

pub(super) fn board(
    owner: Processor,
    control: u8,
    select: u32,
    waits: u32,
    alias: u32,
    offset: u32,
) -> (Bus, CpuState) {
    let mut bus = Bus::default();
    bus.set_slave_enabled(true);
    bus.for_cpu(owner)
        .write8(crate::cache::CCR, control)
        .unwrap();
    bus.for_cpu(owner)
        .write32(0xFFFFFFE4, 0xA55A003C | select << 6)
        .unwrap();
    bus.for_cpu(owner)
        .write32(0xFFFFFFE8, 0xA55A0000 | waits)
        .unwrap();
    for (index, word) in CODE.into_iter().enumerate() {
        bus.write16(HIGH_WRAM + offset + index as u32 * 2, word)
            .unwrap();
    }
    for at in (HIGH_WRAM + 0x2400..HIGH_WRAM + 0x2440).step_by(4) {
        bus.write32(at, 0x8123FEDC ^ at.rotate_left(4)).unwrap();
    }
    let mut state = Cpu::start(HIGH_WRAM + offset + alias, HIGH_WRAM + 0x2400 + alias)
        .state()
        .clone();
    state.r[1] = 0x12345678;
    (bus, state)
}

// Architectural and per-instruction completion/next-issue comparisons. The
// reference intentionally uses the production clone/replay path; the candidate
// never does. One-clock observations cannot skip an internal pipeline edge.
fn compare(owner: Processor, control: u8, select: u32, waits: u32, alias: u32, offset: u32) -> u32 {
    let (actual, state) = board(owner, control, select, waits, alias, offset);
    let (reference, _) = board(owner, control, select, waits, alias, offset);
    compare_pair(actual, reference, state, owner, control)
}

fn compare_pair(
    mut actual: Bus,
    mut reference: Bus,
    state: CpuState,
    owner: Processor,
    control: u8,
) -> u32 {
    let mut cpu = TimedExecutor::new(owner, state.clone());
    cpu.set_cache_control(control).unwrap();
    let mut old = super::super::resumable::reference_from_state(state);
    let mut old_ready = 0;
    let mut candidate_done = Vec::new();
    let mut reference_done = Vec::new();
    for _ in 0..4000 {
        assert_eq!(actual.main_clocks, reference.main_clocks);
        while cpu.state().instructions < 10 && cpu.poll(&mut actual).unwrap() {
            candidate_done.push((actual.main_clocks, cpu.ready, cpu.state().clone()));
        }
        while old.state().instructions < 10 && reference.main_clocks >= old_ready {
            let mut attempt = old.clone();
            match attempt.step(&mut reference.replay_cpu(owner)) {
                Ok(()) => {
                    old_ready = (attempt.execution_clock().unwrap()
                        + u64::from(attempt.last_clocks()))
                    .max(reference.main_clocks);
                    old = attempt;
                    reference.finish_cpu_transaction(owner);
                    reference_done.push((reference.main_clocks, old_ready, old.state().clone()));
                }
                Err(CpuError::Bus {
                    source: BusError::Pending,
                    ..
                }) => break,
                other => panic!("unexpected production result {other:?}"),
            }
        }
        if candidate_done.len() == 10 && reference_done.len() == 10 {
            break;
        }
        actual.grant_bus_transaction();
        reference.grant_bus_transaction();
        actual.advance_clocks(1).unwrap();
        reference.advance_clocks(1).unwrap();
    }
    assert_eq!(candidate_done.len(), 10);
    assert_eq!(reference_done.len(), 10);
    assert_eq!(candidate_done, reference_done, "{owner:?}/{control}");
    candidate_done
        .iter()
        .fold(2166136261u32, |mut hash, (at, ready, state)| {
            for value in [*at as u32, *ready as u32, state.pc]
                .into_iter()
                .chain(state.r)
            {
                hash = (hash ^ value).wrapping_mul(16777619);
            }
            hash
        })
}

pub fn verify_board() -> Vec<u32> {
    let mut count = 0;
    let mut hash = 2166136261u32;
    for owner in [Processor::Master, Processor::Slave] {
        for control in [0, 1, 3, 5, 9] {
            for select in 1..=3 {
                for waits in [0, 0xC0] {
                    for alias in [0, 0x20000000] {
                        for offset in [0, 2] {
                            hash = (hash ^ compare(owner, control, select, waits, alias, offset))
                                .wrapping_mul(16777619);
                            count += 10;
                        }
                    }
                }
            }
        }
    }
    for control in [0, 1, 3, 5, 9] {
        for alias in [0, 0x20000000] {
            hash = (hash ^ verify_dual(control, alias)).wrapping_mul(16777619);
            count += 20;
        }
    }
    for owner in [Processor::Master, Processor::Slave] {
        for alias in [0, 0x20000000] {
            for offset in [0, 2] {
                for opcode in [
                    0xA004, 0xB004, 0x0123, 0x0103, 0x412B, 0x410B, 0x000B, 0x8904, 0x8B04, 0x8D04,
                    0x8F04,
                ] {
                    for t in [false, true] {
                        let setup = || {
                            let (mut bus, mut state) = board(owner, 1, 3, 0, alias, offset);
                            for at in (HIGH_WRAM..HIGH_WRAM + 0x80).step_by(2) {
                                bus.write16(at, 9).unwrap();
                            }
                            let pc = HIGH_WRAM + offset;
                            bus.write16(pc, opcode).unwrap();
                            bus.write16(pc + 2, 0x60F6).unwrap(); // MOV in a real delay slot.
                            state.r[1] = if opcode & 0x4000 == 0 {
                                8
                            } else {
                                state.pc + 12
                            };
                            state.pr = state.pc + 12;
                            state.sr = (state.sr & !1) | u32::from(t);
                            (bus, state)
                        };
                        let (actual, state) = setup();
                        let (reference, _) = setup();
                        hash = (hash ^ compare_pair(actual, reference, state, owner, 1))
                            .wrapping_mul(16777619);
                        count += 10;
                    }
                }
            }
        }
    }
    vec![count, hash]
}

fn verify_dual(control: u8, alias: u32) -> u32 {
    let owners = [Processor::Master, Processor::Slave];
    let setup = || {
        let (mut bus, master) = board(owners[0], control, 3, 0, alias, 0);
        bus.for_cpu(owners[1])
            .write8(crate::cache::CCR, control)
            .unwrap();
        bus.for_cpu(owners[1])
            .write32(0xFFFFFFE8, 0xA55A0000)
            .unwrap();
        for (index, word) in CODE.into_iter().enumerate() {
            bus.write16(HIGH_WRAM + 0x100 + index as u32 * 2, word)
                .unwrap();
        }
        let mut slave = master.clone();
        slave.pc += 0x100;
        (bus, [master, slave])
    };
    let (mut actual, states) = setup();
    let (mut reference, _) = setup();
    let mut cpus = states
        .clone()
        .into_iter()
        .enumerate()
        .map(|(i, s)| TimedExecutor::new(owners[i], s))
        .collect::<Vec<_>>();
    for cpu in &mut cpus {
        cpu.set_cache_control(control).unwrap();
    }
    let mut old = states.map(crate::cpu::resumable::reference_from_state);
    let mut ready = [0u64; 2];
    let mut candidate_done = Vec::new();
    let mut reference_done = Vec::new();
    for _ in 0..4000 {
        for (i, cpu) in cpus.iter_mut().enumerate() {
            while cpu.state().instructions < 10 && cpu.poll(&mut actual).unwrap() {
                candidate_done.push((i, actual.main_clocks, cpu.ready, cpu.state().clone()));
            }
        }
        for i in 0..2 {
            while old[i].state().instructions < 10 && reference.main_clocks >= ready[i] {
                let mut attempt = old[i].clone();
                match attempt.step(&mut reference.replay_cpu(owners[i])) {
                    Ok(()) => {
                        ready[i] = (attempt.execution_clock().unwrap()
                            + u64::from(attempt.last_clocks()))
                        .max(reference.main_clocks);
                        old[i] = attempt;
                        reference.finish_cpu_transaction(owners[i]);
                        reference_done.push((
                            i,
                            reference.main_clocks,
                            ready[i],
                            old[i].state().clone(),
                        ));
                    }
                    Err(CpuError::Bus {
                        source: BusError::Pending,
                        ..
                    }) => break,
                    result => panic!("unexpected dual reference result {result:?}"),
                }
            }
        }
        if candidate_done.len() == 20 && reference_done.len() == 20 {
            break;
        }
        actual.grant_bus_transaction();
        reference.grant_bus_transaction();
        actual.advance_clocks(1).unwrap();
        reference.advance_clocks(1).unwrap();
    }
    assert_eq!(candidate_done.len(), 20);
    assert_eq!(candidate_done, reference_done, "dual {control}/{alias:X}");
    candidate_done
        .iter()
        .fold(2166136261u32, |mut hash, (owner, at, ready, state)| {
            for value in [*owner as u32, *at as u32, *ready as u32, state.pc]
                .into_iter()
                .chain(state.r)
            {
                hash = (hash ^ value).wrapping_mul(16777619);
            }
            hash
        })
}
