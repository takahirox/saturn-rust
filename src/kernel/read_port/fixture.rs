//! Original WRAM read fixture shared by native and browser/Worker validation.
//! It compares the owned transport against the production memory interpreter,
//! including the real board clock; no synthetic transfer latency is assigned.
use super::*;
use crate::bus::Memory;
fn word(address: u32) -> u32 {
    0x8123_4567 ^ address.rotate_left(9)
}

fn advance_board(bus: &mut Bus, chunk: u32) {
    bus.grant_bus_transaction();
    let deadline = bus.bus_transaction_deadline();
    assert_ne!(
        deadline,
        u32::MAX,
        "an owned read must have real pending bus work"
    );
    bus.advance_clocks(deadline.min(chunk)).unwrap();
}

fn production_read(bus: &mut Bus, owner: Processor, read: Read, chunk: u32) -> u32 {
    for _ in 0..10000 {
        let result = {
            let memory = bus.replay_cpu(owner);
            match read {
                Read::Fetch16(address) => memory.fetch16(address).map(u32::from),
                Read::Data { address, width: 1 } => memory.read8(address).map(u32::from),
                Read::Data { address, width: 2 } => memory.read16(address).map(u32::from),
                Read::Data { address, .. } => memory.read32(address),
            }
        };
        match result {
            Ok(value) => {
                bus.finish_cpu_transaction(owner);
                return value;
            }
            Err(BusError::Pending) => advance_board(bus, chunk),
            other => panic!("production read failed: {other:?}"),
        }
    }
    panic!("production read did not finish");
}

fn owned_read(bus: &mut Bus, port: &mut ReadPort, read: Read, chunk: u32) -> u32 {
    drive_owned_read(bus, port, read, chunk, true)
}

fn drive_owned_read(
    bus: &mut Bus,
    port: &mut ReadPort,
    read: Read,
    chunk: u32,
    finish: bool,
) -> u32 {
    let initial = port.request(read).unwrap();
    if let ReadPoll::Ready(value) = initial {
        return value;
    }
    let mut transfer = port.take_transfer().unwrap();
    for _ in 0..10000 {
        match port.resume(bus, transfer).unwrap() {
            ReadPoll::Ready(value) => {
                if finish {
                    bus.finish_cpu_transaction(transfer.ticket.owner());
                }
                // A duplicate delivery must not start another real bus read.
                assert_eq!(port.resume(bus, transfer), Ok(ReadPoll::Discarded));
                return value;
            }
            ReadPoll::Pending => {
                if let Some(next) = port.take_transfer() {
                    transfer = next;
                } else {
                    advance_board(bus, chunk);
                }
            }
            other => panic!("live owned read was discarded: {other:?}"),
        }
    }
    panic!("owned read did not finish");
}

pub fn verify_board() -> Vec<u32> {
    let mut digest = vec![0u32, 2166136261];
    for owner in [Processor::Master, Processor::Slave] {
        for select in 1..=3 {
            for little in [false, true] {
                if little && select == 1 {
                    continue;
                } // Reserved BSC mode.
                for chunk in [1, 7, u32::MAX] {
                    for control in [0, 1, 3, 5, 9] {
                        let setup = || {
                            let mut b = Bus::default();
                            b.set_slave_enabled(true);
                            b.for_cpu(owner)
                                .write32(0xFFFFFFE4, 0xA55A003C | select << 6)
                                .unwrap();
                            b.for_cpu(owner)
                                .write32(0xFFFFFFE0, 0xA55A03F0 | if little { 0x1000 } else { 0 })
                                .unwrap();
                            b.for_cpu(owner).write8(crate::cache::CCR, control).unwrap();
                            for at in (HIGH_WRAM..HIGH_WRAM + 32).step_by(4) {
                                b.write32(at, word(at)).unwrap();
                            }
                            b
                        };
                        let (mut actual, mut reference) = (setup(), setup());
                        let mut port = ReadPort::new(owner);
                        port.set_control(control).unwrap();
                        for read in [
                            Read::Fetch16(HIGH_WRAM + 4),
                            Read::Fetch16(HIGH_WRAM + 6),
                            Read::Data {
                                address: HIGH_WRAM + 1,
                                width: 1,
                            },
                            Read::Data {
                                address: HIGH_WRAM + 2,
                                width: 2,
                            },
                            Read::Data {
                                address: HIGH_WRAM + 12,
                                width: 4,
                            },
                            Read::Data {
                                address: (HIGH_WRAM + 20) | 0x20000000,
                                width: 4,
                            },
                        ] {
                            let expected = production_read(&mut reference, owner, read, chunk);
                            assert_eq!(
                                owned_read(&mut actual, &mut port, read, chunk),
                                expected,
                                "{owner:?}/{select}/{little}/{control}/{read:?}"
                            );
                            assert_eq!(
                                actual.main_clocks, reference.main_clocks,
                                "completion edge {owner:?}/{select}/{little}/{control}/{read:?}"
                            );
                            digest[0] += 1;
                            digest[1] = (digest[1] ^ expected).wrapping_mul(16777619);
                            digest[1] =
                                (digest[1] ^ actual.main_clocks as u32).wrapping_mul(16777619);
                        }
                    }
                }
            }
        }
    }
    let competing = verify_competing_reads();
    digest[0] += 2;
    for value in competing {
        digest[1] = (digest[1] ^ value).wrapping_mul(16777619);
    }
    for value in verify_fetch_operand_stream() {
        digest[1] = (digest[1] ^ value).wrapping_mul(16777619);
    }
    digest[0] += 2;
    digest
}

fn verify_competing_reads() -> Vec<u32> {
    let setup = || {
        let mut b = Bus::default();
        b.set_slave_enabled(true);
        for owner in [Processor::Master, Processor::Slave] {
            b.for_cpu(owner).write8(crate::cache::CCR, 1).unwrap();
        }
        for at in (HIGH_WRAM..HIGH_WRAM + 64).step_by(4) {
            b.write32(at, word(at)).unwrap();
        }
        b
    };
    let (mut actual, mut reference) = (setup(), setup());
    let owners = [Processor::Master, Processor::Slave];
    let addresses = [HIGH_WRAM + 8, HIGH_WRAM + 48];
    let mut ports = owners.map(ReadPort::new);
    let mut transfers = [None; 2];
    for i in 0..2 {
        ports[i].set_control(1).unwrap();
        ports[i]
            .request(Read::Data {
                address: addresses[i],
                width: 4,
            })
            .unwrap();
        transfers[i] = ports[i].take_transfer();
    }
    let mut actual_done = [None; 2];
    let mut reference_done = [None; 2];
    for _ in 0..10000 {
        for i in 0..2 {
            if actual_done[i].is_some() {
                continue;
            }
            loop {
                let transfer = transfers[i].unwrap();
                match ports[i].resume(&mut actual, transfer).unwrap() {
                    ReadPoll::Ready(value) => {
                        actual_done[i] = Some((value, actual.main_clocks));
                        actual.finish_cpu_transaction(owners[i]);
                        break;
                    }
                    ReadPoll::Pending => {
                        if let Some(next) = ports[i].take_transfer() {
                            transfers[i] = Some(next);
                        } else {
                            break;
                        }
                    }
                    other => panic!("competing owned read discarded: {other:?}"),
                }
            }
        }
        if actual_done.iter().all(Option::is_some) {
            break;
        }
        advance_board(&mut actual, 1);
    }
    for _ in 0..10000 {
        for i in 0..2 {
            if reference_done[i].is_some() {
                continue;
            }
            match reference.replay_cpu(owners[i]).read32(addresses[i]) {
                Ok(value) => {
                    reference_done[i] = Some((value, reference.main_clocks));
                    reference.finish_cpu_transaction(owners[i]);
                }
                Err(BusError::Pending) => (),
                other => panic!("competing reference read failed: {other:?}"),
            }
        }
        if reference_done.iter().all(Option::is_some) {
            break;
        }
        advance_board(&mut reference, 1);
    }
    assert!(actual_done.iter().all(Option::is_some));
    assert_eq!(
        actual_done, reference_done,
        "both CPUs must agree at each completion edge"
    );
    actual_done
        .into_iter()
        .flat_map(|r| {
            let (value, clock) = r.unwrap();
            [value, clock as u32]
        })
        .collect()
}

fn verify_fetch_operand_stream() -> Vec<u32> {
    let setup = || {
        let mut b = Bus::default();
        b.for_cpu(Processor::Master)
            .write8(crate::cache::CCR, 1)
            .unwrap();
        for base in [HIGH_WRAM, HIGH_WRAM + 0x800] {
            for at in (base..base + 16).step_by(4) {
                b.write32(at, word(at)).unwrap();
            }
        }
        b
    };
    let (mut actual, mut reference) = (setup(), setup());
    let mut port = ReadPort::new(Processor::Master);
    port.set_control(1).unwrap();
    let opcode = drive_owned_read(&mut actual, &mut port, Read::Fetch16(HIGH_WRAM), 1, false);
    let operand = drive_owned_read(
        &mut actual,
        &mut port,
        Read::Data {
            address: HIGH_WRAM + 0x808,
            width: 4,
        },
        1,
        false,
    );
    actual.finish_cpu_transaction(Processor::Master);
    let mut done = None;
    for _ in 0..10000 {
        let result = (|| {
            let memory = reference.replay_cpu(Processor::Master);
            Ok::<_, BusError>([
                u32::from(memory.fetch16(HIGH_WRAM)?),
                memory.read32(HIGH_WRAM + 0x808)?,
            ])
        })();
        match result {
            Ok(values) => {
                done = Some(values);
                reference.finish_cpu_transaction(Processor::Master);
                break;
            }
            Err(BusError::Pending) => advance_board(&mut reference, 1),
            other => panic!("reference read stream failed: {other:?}"),
        }
    }
    assert_eq!(done, Some([opcode, operand]));
    assert_eq!(
        actual.main_clocks, reference.main_clocks,
        "IF then operand must preserve the same instruction transaction"
    );
    vec![opcode, operand, actual.main_clocks as u32]
}
