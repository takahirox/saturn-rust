use super::*;
use crate::bus::{HIGH_WRAM, Memory};
use crate::cpu::Cpu;

#[test]
fn actual_if_id_alu_and_load_ma_edges_match_production_without_instruction_replay() {
    assert_eq!(super::verify_board()[0], 4360);
}

#[test]
fn owned_stores_match_buffer_acceptance_cache_forwarding_aliases_and_byte_publication() {
    assert_eq!(super::verify_store_board()[0], 5120);
}

#[test]
fn branch_slot_store_preserves_the_target_bytes_already_fetched_before_acceptance() {
    for owner in [Processor::Master, Processor::Slave] {
        for alias in [0, 0x20000000] {
            for offset in [0, 2] {
                for waits in [0, 0xC0] {
                    let (mut bus, mut state) = fixture::board(owner, 1, 3, waits, alias, offset);
                    let start = HIGH_WRAM + offset;
                    for (i, word) in [0xA000, 0x2321, 0xE607, 9, 9, 9].into_iter().enumerate() {
                        bus.write16(start + i as u32 * 2, word).unwrap();
                    }
                    state.r[2] = 0xE62A;
                    state.r[3] = state.pc + 4;
                    let mut cpu = TimedExecutor::new(owner, state);
                    cpu.set_cache_control(1).unwrap();
                    for _ in 0..400 {
                        cpu.poll(&mut bus).unwrap();
                        if cpu.state().instructions == 3 {
                            break;
                        }
                        bus.grant_bus_transaction();
                        bus.advance_clocks(1).unwrap();
                    }
                    assert_eq!(cpu.state().instructions, 3);
                    assert_eq!(
                        cpu.state().r[6],
                        7,
                        "target IF preceded the self-modifying slot store"
                    );
                    bus.advance_clocks(64).unwrap();
                    assert_eq!(bus.read16(start + 4), Ok(0xE62A));
                }
            }
        }
    }
}

#[test]
fn early_refill_words_and_fetch_halves_keep_their_original_sampling_edges() {
    let owner = Processor::Master;
    let (mut bus, state) = fixture::board(owner, 1, 3, 0, 0, 0);
    let mut cpu = TimedExecutor::new(owner, state);
    cpu.set_cache_control(1).unwrap();
    let base = HIGH_WRAM + 0x2400;
    let early = bus.read32(base + 4).unwrap();
    let mut changed = false;
    for _ in 0..200 {
        if cpu.poll(&mut bus).unwrap() {
            break;
        }
        // The first refill word has completed when its continuation requests
        // the second. Later writes cannot replace the already sampled bytes.
        if !changed && cpu.transfer.is_some_and(|t| t.address == base + 8) {
            bus.write32(base + 4, 0x99887766).unwrap();
            bus.write32(base, 0xABCDEF12).unwrap();
            changed = true;
        }
        bus.grant_bus_transaction();
        bus.advance_clocks(1).unwrap();
    }
    assert!(changed);
    assert_eq!(cpu.state().r[0], 0xABCDEF12, "requested word arrives last");
    assert_eq!(cpu.state().r[15], base + 4);
    assert_eq!(
        cpu.port.request(Read::Data {
            address: base + 4,
            width: 4
        }),
        Ok(ReadPoll::Ready(early))
    );
    // The successor was decoded before the write, so it must keep ADD #1.
    bus.write16(HIGH_WRAM + 2, 0x7007).unwrap();
    for _ in 0..100 {
        if cpu.poll(&mut bus).unwrap() {
            break;
        }
        bus.grant_bus_transaction();
        bus.advance_clocks(1).unwrap();
    }
    assert_eq!(cpu.state().instructions, 2);
    assert_eq!(cpu.state().r[0], 0xABCDEF13);
}

#[test]
fn cold_and_cache_through_nop_streams_match_inherited_literal_retirement_edges() {
    for owner in [Processor::Master, Processor::Slave] {
        for (control, alias, expected) in [
            (
                1,
                0,
                vec![
                    (9, 1),
                    (10, 2),
                    (11, 3),
                    (12, 4),
                    (13, 5),
                    (14, 6),
                    (23, 7),
                    (23, 8),
                ],
            ),
            (
                0,
                0x20000000,
                vec![
                    (5, 1),
                    (5, 2),
                    (8, 3),
                    (8, 4),
                    (11, 5),
                    (11, 6),
                    (14, 7),
                    (14, 8),
                ],
            ),
        ] {
            let mut bus = Bus::default();
            bus.set_slave_enabled(true);
            bus.for_cpu(owner).write32(0xFFFFFFE8, 0xA55A0000).unwrap();
            for at in (HIGH_WRAM..HIGH_WRAM + 64).step_by(2) {
                bus.write16(at, 9).unwrap();
            }
            let state = Cpu::start(HIGH_WRAM | alias, HIGH_WRAM + 0x1000)
                .state()
                .clone();
            let mut cpu = TimedExecutor::new(owner, state);
            cpu.set_cache_control(control).unwrap();
            let mut retired = Vec::new();
            for _ in 0..50 {
                while cpu.state().instructions < 8 && cpu.poll(&mut bus).unwrap() {
                    retired.push((bus.main_clocks, cpu.state().instructions));
                }
                if retired.len() == 8 {
                    break;
                }
                bus.grant_bus_transaction();
                bus.advance_clocks(1).unwrap();
            }
            assert_eq!(retired, expected);
        }
    }
}

#[test]
fn unsupported_decode_is_terminal_and_cannot_execute_the_promoted_successor() {
    let mut bus = Bus::default();
    bus.write16(HIGH_WRAM, 0x001B).unwrap(); // SLEEP is outside this slice.
    bus.write16(HIGH_WRAM + 2, 0xE12A).unwrap();
    let state = Cpu::start(HIGH_WRAM, HIGH_WRAM + 0x1000).state().clone();
    let mut cpu = TimedExecutor::new(Processor::Master, state.clone());
    for _ in 0..100 {
        match cpu.poll(&mut bus) {
            Ok(false) => {
                bus.grant_bus_transaction();
                bus.advance_clocks(1).unwrap();
            }
            Err(ReadError::State(Error::Unsupported {
                pc: HIGH_WRAM,
                opcode: 0x001B,
            })) => break,
            result => panic!("unexpected decode result: {result:?}"),
        }
    }
    assert!(cpu.failed);
    assert_eq!(cpu.state(), &state);
    for _ in 0..3 {
        assert_eq!(cpu.poll(&mut bus), Err(ReadError::State(Error::Protocol)));
        assert_eq!(cpu.set_cache_control(1), Err(Error::Protocol));
        assert_eq!(cpu.state(), &state);
    }
}
