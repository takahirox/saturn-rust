//! ST-077 §3.2/fig3.2: DSP precedes MCPU(SCU) in sound-DRAM arbitration.
//! Accepted CPU/SCU beats retain their bus order while SCSP owns the RAM.
use super::*;
use crate::sound::{SCSP, SOUND_RAM};

fn fixture() -> Bus {
    let mut b = Bus::default();
    b.write16(SCSP + 0x400, 0x20F).unwrap();
    b.write16(SOUND_RAM + 0x1000, 0x1234).unwrap();
    b.write16(SCSP + 0x780, 0x800).unwrap();
    // Instruction1 MRD at ref10 holds DRAM through ref15; IWT at ref20
    // makes the retained value visible in MEMS0 after the contention ends.
    for (step, words) in [(1, [0u16, 0, 0xA002, 0x8000]), (3, [0, 0x20, 2, 0])] {
        for (i, word) in words.into_iter().enumerate() {
            b.write16(SCSP + 0x800 + step * 8 + i as u32 * 2, word)
                .unwrap();
        }
    }
    b
}
fn advance(b: &mut Bus, clocks: u32, chunk: u32) {
    let mut left = clocks;
    while left != 0 {
        let n = left.min(chunk);
        b.advance_clocks(n).unwrap();
        left -= n;
    }
}
#[test]
fn bridge_read_retains_each_word_across_dsp_pcm_dma_and_refresh_ownership() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for device in [0, 1, 2, 3] {
                let mut b = if device == 0 {
                    fixture()
                } else {
                    Bus::default()
                };
                b.set_slave_enabled(true);
                b.write16(SCSP + 0x400, 0x20F).unwrap();
                b.write32(SOUND_RAM + 0x1000, 0x11223344).unwrap();
                // Literal completion boundaries from the authored SCSP
                // schedules (ST-077 §3.2 fig3.2), not timing helpers.
                let (start, high, low) = match device {
                    0 => (7, 20, 29), // MRD10..15; main15..20,24..29
                    1 => {
                        for (at, value) in
                            [(2, 0x1000), (6, 4), (12, 0x100), (22, 0xE000), (0, 0x1820)]
                        {
                            b.write16(SCSP + at, value).unwrap();
                        }
                        (12, 24, 34) // PCM15..20; main20..24,29..34
                    }
                    2 => {
                        for (at, value) in [
                            (0x700, 0x1230),
                            (0x702, 0x5678),
                            (0x704, 0x9AB8),
                            (0x706, 0xDEF0),
                            (0x412, 0x2000),
                            (0x414, 0x700),
                            (0x416, 0x3008),
                        ] {
                            b.write16(SCSP + at, value).unwrap();
                        }
                        (2, 29, 39) // DMA ends24; main24..29,34..39
                    }
                    _ => (592, 605, 615), // refresh ref596..600 and605..610 (128*44.1kHz)
                };
                advance(&mut b, start, chunk);
                let at = SOUND_RAM | 0x20001000;
                assert_eq!(b.replay_cpu(p).read32(at), Err(BusError::Pending));
                b.grant_bus_transaction();
                advance(&mut b, high - start - 1, chunk);
                assert!(
                    b.transactions.journals[p as usize]
                        .borrow()
                        .beats
                        .is_empty(),
                    "device={device}"
                );
                assert_eq!(b.read32(at), Ok(0x11223344)); // intervening debugger inspection
                b.write32(at, 0x55667788).unwrap();
                advance(&mut b, 1, chunk);
                assert_eq!(
                    b.transactions.journals[p as usize].borrow().beats.len(),
                    1,
                    "device={device} high={high}"
                );
                assert_eq!(
                    b.transactions.journals[p as usize].borrow().beats[0].1,
                    Ok(0x5566),
                    "device={device}"
                );
                b.write32(at, 0x99AABBCC).unwrap();
                assert_eq!(b.replay_cpu(p).read32(at), Err(BusError::Pending));
                advance(&mut b, low - high - 1, chunk);
                assert_eq!(b.replay_cpu(p).read32(at), Err(BusError::Pending));
                advance(&mut b, 1, chunk);
                b.write32(at, 0xDDEEFF00).unwrap();
                assert_eq!(
                    b.replay_cpu(p).read32(at),
                    Ok(0x5566BBCC),
                    "device={device}"
                );
            }
        }
    }
}
#[test]
fn both_sh2_and_scu_wait_for_dsp_ram_without_publishing_or_resampling_early() {
    for chunk in [1, 3, 17, 10000] {
        for (channel, owner) in [
            (0, Owner::Cpu(Processor::Master)),
            (0, Owner::Cpu(Processor::Slave)),
            (0, Owner::Scu),
            (1, Owner::Scu),
            (2, Owner::Scu),
        ] {
            let mut b = fixture();
            if owner == Owner::Scu {
                advance(&mut b, 7, chunk);
                b.write32(HIGH_WRAM + 0x100, 0x56789ABC).unwrap();
                for (at, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, SOUND_RAM + 0x1000),
                    (8, 4),
                    (12, 0x101),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(SCU + channel * 32 + at, value).unwrap();
                }
                // Setup2 and source8 make the destination eligible17.
                // Move the DSP MRD to instruction3 at20 for this case.
                b.write16(SCSP + 0x80C, 2).unwrap();
                b.write16(SCSP + 0x81A, 0).unwrap();
                b.write16(SCSP + 0x81C, 0xA002).unwrap();
                b.write16(SCSP + 0x81E, 0x8000).unwrap();
                b.write16(SCSP + 0x82A, 0x20).unwrap();
            } else if let Owner::Cpu(p) = owner {
                if p == Processor::Slave {
                    b.set_slave_enabled(true);
                }
                advance(&mut b, 7, chunk);
                assert_eq!(
                    b.replay_cpu(p).write16(SOUND_RAM | 0x20001000, 0x5678),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
            }
            let target = if owner == Owner::Scu { 24 } else { 15 };
            // Use the actual granted beat's deadline; the SCU setup/source
            // phases are retained and may already be in progress.
            advance(&mut b, target - 7, chunk);
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1234));
            assert!(b.sound.ram_busy());
            b.write16(SOUND_RAM + 0x1000, 0xABCD).unwrap(); // independent debugger mutation
            advance(&mut b, 4, chunk);
            assert_eq!(
                b.read16(SOUND_RAM + 0x1000),
                Ok(0xABCD),
                "owner_cpu={} chunk={chunk}",
                matches!(owner, Owner::Cpu(_))
            );
            advance(&mut b, 1, chunk);
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x5678));
            advance(&mut b, 15, chunk);
            assert_eq!(b.read16(SCSP + 0xE02), Ok(0x1234)); // completed DSP read retained
            if owner == Owner::Scu {
                assert_eq!(b.read32(SOUND_RAM + 0x1000), Ok(0x56789ABC));
                assert_ne!(b.read32(SCU + 0xA4).unwrap() & (1 << (11 - channel)), 0);
            }
        }
    }
}

#[test]
fn main_interface_owns_sound_ram_before_simultaneous_sound_cpu_reset_sampling() {
    for chunk in [1, 3, 17, 10000] {
        for processor in [Processor::Master, Processor::Slave] {
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            b.write16(SCSP + 0x400, 0x20F).unwrap();
            b.write32(SOUND_RAM, 0x0007FF00).unwrap();
            b.write32(SOUND_RAM + 4, 0x200).unwrap();
            b.write32(SOUND_RAM + 0x200, 0x4E722700).unwrap();
            b.sound.set_enabled(true).unwrap();
            advance(&mut b, 40, chunk);
            assert_eq!(
                b.replay_cpu(processor).write16(SOUND_RAM | 0x20000000, 6),
                Err(BusError::Pending)
            );
            b.grant_bus_transaction();
            advance(&mut b, 8, chunk); // main completes48, SCPU grants48 and completes53
            assert_eq!(b.read16(SOUND_RAM), Ok(6));
            assert_eq!(b.sound.cpu.state.a[7], 0);
            b.write16(SOUND_RAM, 5).unwrap();
            advance(&mut b, 5, chunk); // admitted SCPU word samples5 at ref53
            b.write16(SOUND_RAM, 4).unwrap();
            advance(&mut b, 9, chunk); // independent low completes at ref62
            assert_eq!(b.sound.cpu.state.a[7], 0x0005FF00);
            advance(&mut b, 100, chunk);
            assert!(b.sound.cpu.state.stopped && !b.sound.cpu.state.halted);
            assert_eq!(b.sound.cpu.state.exceptions, 0);
        }
    }
}

#[test]
fn internal_dma_holds_both_cpu_and_all_scu_word_phases_until_its_final_word() {
    for chunk in [1, 3, 17, 10000] {
        for (channel, owner) in [
            (0, Owner::Cpu(Processor::Master)),
            (0, Owner::Cpu(Processor::Slave)),
            (0, Owner::Scu),
            (1, Owner::Scu),
            (2, Owner::Scu),
        ] {
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            b.write16(SCSP + 0x400, 0x20F).unwrap();
            b.write16(SCSP + 0x700, 0x1230).unwrap();
            b.write16(SCSP + 0x702, 0x5678).unwrap();
            b.write16(SCSP + 0x704, 0x9AB8).unwrap();
            b.write16(SCSP + 0x706, 0xDEF0).unwrap();
            b.write16(SCSP + 0x412, 0x1000).unwrap();
            b.write16(SCSP + 0x414, 0x700).unwrap();
            b.write16(SCSP + 0x416, 0x3008).unwrap();
            advance(&mut b, 2, chunk);
            if let Owner::Cpu(p) = owner {
                assert_eq!(
                    b.replay_cpu(p).write16(SOUND_RAM | 0x20001000, 0xABCD),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
            } else {
                b.write32(HIGH_WRAM + 0x100, 0xABCD9876).unwrap();
                for (at, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, SOUND_RAM + 0x1000),
                    (8, 4),
                    (12, 0x101),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(SCU + channel * 32 + at, value).unwrap();
                }
            }
            advance(&mut b, 8, chunk); // first DMA word at ref10
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1230));
            advance(&mut b, 13, chunk);
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1230));
            assert_eq!(b.read16(SCSP + 0x420).unwrap() & 16, 0);
            advance(&mut b, 1, chunk); // final DMA publication, main grant
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1230));
            advance(&mut b, 5, chunk); // admitted main publication at29
            assert_eq!(
                b.read16(SOUND_RAM + 0x1000),
                Ok(0xABCD),
                "owner_cpu={} chunk={chunk}",
                matches!(owner, Owner::Cpu(_))
            );
            assert_eq!(b.read16(SOUND_RAM + 0x1004), Ok(0x9AB8));
            assert_ne!(b.read16(SCSP + 0x420).unwrap() & 16, 0);
        }
    }
}

#[test]
fn pcm_second_read_holds_both_sh2_and_all_scu_channels_until_the_read_completes() {
    // ST-077 §3.2/fig3.2. Slot0 occupies ref5..10 and15..20.
    // Requests eligible12 contend with the second PCM grant15.
    // Main grants20 and publishes at24, in its own memory cycle.
    for chunk in [1, 3, 17, 10000] {
        for (channel, owner) in [
            (0, Owner::Cpu(Processor::Master)),
            (0, Owner::Cpu(Processor::Slave)),
            (0, Owner::Scu),
            (1, Owner::Scu),
            (2, Owner::Scu),
        ] {
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            for (at, value) in [
                (0x400, 0x20F),
                (2, 0x1000),
                (6, 4),
                (12, 0x100),
                (22, 0xE000),
                (0, 0x1820),
            ] {
                b.write16(SCSP + at, value).unwrap();
            }
            b.write16(SOUND_RAM + 0x1000, 0x1234).unwrap();
            if let Owner::Cpu(p) = owner {
                advance(&mut b, 12, chunk);
                assert_eq!(
                    b.replay_cpu(p).write16(SOUND_RAM | 0x20001000, 0x5678),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
            } else {
                advance(&mut b, 2, chunk);
                b.write32(HIGH_WRAM + 0x100, 0x56789ABC).unwrap();
                for (at, value) in [
                    (0, HIGH_WRAM + 0x100),
                    (4, SOUND_RAM + 0x1000),
                    (8, 4),
                    (12, 0x101),
                    (20, 7),
                    (16, 0x101),
                ] {
                    b.write32(SCU + channel * 32 + at, value).unwrap();
                }
                advance(&mut b, 10, chunk);
            }
            advance(&mut b, 11, chunk); // ref23, pending access not published
            assert!(b.sound.ram_busy());
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1234));
            advance(&mut b, 1, chunk); // admitted main completes at24
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x5678));
            advance(&mut b, 610, chunk);
            assert_eq!(&b.sound.scsp.drain(1), &[0x1234; 2]);
        }
    }
}

#[test]
fn both_dmacs_and_all_scu_routes_allocate_two_distinct_sound_halfwords() {
    use crate::sound::memory::Owner as Dram;
    for fast in [false, true] {
        for route in 0..6 {
            for read in [false, true] {
                let mut previous = None;
                for chunk in [1, 3, 17, 10000] {
                    let mut b = Bus::default();
                    b.system_clock.select(fast);
                    b.set_slave_enabled(true);
                    b.write16(SCSP + 0x400, 0x20F).unwrap();
                    let (source, destination) = if read {
                        (SOUND_RAM + 0x1000, HIGH_WRAM + 0x100)
                    } else {
                        (HIGH_WRAM + 0x100, SOUND_RAM + 0x1000)
                    };
                    b.write32(source, 0x12345678).unwrap();
                    if route < 2 {
                        let chip = &mut b.onchip[route];
                        for (at, value) in [
                            (0, source),
                            (4, destination),
                            (8, 1),
                            (12, 0x5A15),
                            (0x30, 1),
                        ] {
                            chip.write32(0xFFFFFF80 + at, value).unwrap();
                        }
                    } else if route < 5 {
                        for (at, value) in [
                            (0, source),
                            (4, destination),
                            (8, 4),
                            (12, if read { 0x102 } else { 0x101 }),
                            (20, 7),
                            (16, 0x101),
                        ] {
                            b.write32(SCU + (route as u32 - 2) * 32 + at, value)
                                .unwrap();
                        }
                    } else {
                        // ST-097 §4.5: load one word into M0 using RA0,
                        // then output it through WA0. Real SCU DSP DMA, not a
                        // host-injected sound request or ordinary SCU channel.
                        b.write32(SCU + 0x80, 0x8000).unwrap();
                        for instruction in [
                            0x98000000 | (source >> 2),
                            if read { 0xC0010001 } else { 0xC0008001 },
                            0xD3400002,
                            0,
                            0x00001C00,
                            0x9C000000 | (destination >> 2),
                            0xC0009001,
                            0xD3400007,
                            0,
                            0xF8000000,
                        ] {
                            b.write32(SCU + 0x84, instruction).unwrap();
                        }
                        b.write32(SCU + 0x80, 0x18000).unwrap();
                    }
                    advance(&mut b, 500, chunk);
                    assert_eq!(
                        b.read32(destination),
                        Ok(0x12345678),
                        "route{route} read{read}"
                    );
                    let trace: Vec<_> = b
                        .sound
                        .memory_trace()
                        .iter()
                        .copied()
                        .filter(|e| e.1 == Dram::Main)
                        .collect();
                    assert_eq!(trace.len(), 4, "route{route} read{read}: {trace:?}");
                    for pair in trace.as_chunks::<2>().0 {
                        assert!(!pair[0].2 && pair[1].2);
                        assert_eq!(pair[1].0, pair[0].0 + 1);
                    }
                    let result = (
                        b.read32(destination).unwrap(),
                        trace,
                        b.sound.scsp.drain(1),
                        b.read32(SCU + 0xA4).unwrap(),
                    );
                    if let Some(old) = &previous {
                        assert_eq!(
                            &result, old,
                            "route{route} read{read} fast{fast} chunk{chunk}"
                        );
                    }
                    previous = Some(result);
                }
            }
        }
    }
}

#[test]
fn sound_memory_manual_reset_drains_admitted_words_and_power_reset_cancels_unpublished_lanes() {
    use crate::onchip::WDT;
    for p in [Processor::Master, Processor::Slave] {
        for manual in [false, true] {
            for assertion in [4, 5, 9, 10, 14, 15] {
                for chunk in [1, 3, 17, 10000] {
                    let mut b = Bus::default();
                    b.set_slave_enabled(true);
                    b.write16(SCSP + 0x400, 0x20F).unwrap();
                    assert_eq!(
                        b.replay_cpu(p).write32(SOUND_RAM | 0x20001000, 0x12345678),
                        Err(BusError::Pending)
                    );
                    advance(&mut b, assertion - 2, chunk);
                    let chip = &mut b.onchip[p as usize];
                    chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                        .unwrap();
                    chip.write16(WDT, 0xA578).unwrap();
                    chip.write16(WDT, 0x5AFF).unwrap();
                    advance(&mut b, 600 - assertion + 2, chunk);
                    let expected = if manual || assertion >= 15 {
                        0x12345678
                    } else if assertion >= 10 {
                        0x12340000
                    } else {
                        0
                    };
                    assert_eq!(
                        b.read32(SOUND_RAM + 0x1000),
                        Ok(expected),
                        "{p:?} manual{manual} t{assertion} chunk{chunk}"
                    );
                    assert!(b.take_cpu_reset(p).is_some());
                    assert!(b.transactions.active.is_none());
                }
            }
        }
    }
}

#[test]
fn sound_memory_reset_releases_canceled_request_before_admitting_waiting_peer() {
    use crate::onchip::{ResetRequest, WDT};
    use crate::sound::memory::Owner as Dram;
    for p in [Processor::Master, Processor::Slave] {
        let peer = if p == Processor::Master {
            Processor::Slave
        } else {
            Processor::Master
        };
        for manual in [false, true] {
            // Before a DRAM grant, during a grant, and at each word's
            // completion. Watchdog assertion follows arming by two clocks.
            for assertion in [4, 7, 10, 15] {
                for chunk in [1, 3, 17, 10000] {
                    let mut b = Bus::default();
                    b.set_slave_enabled(true);
                    let original = SOUND_RAM | 0x20001000;
                    let replacement = SOUND_RAM | 0x20002000;
                    assert_eq!(
                        b.replay_cpu(p).write32(original, 0x12345678),
                        Err(BusError::Pending)
                    );
                    advance(&mut b, assertion - 2, chunk);
                    assert_eq!(
                        b.replay_cpu(peer).write32(replacement, 0x9ABCDEF0),
                        Err(BusError::Pending)
                    );
                    let chip = &mut b.onchip[p as usize];
                    chip.write16(WDT + 2, if manual { 0x5A7F } else { 0x5A5F })
                        .unwrap();
                    chip.write16(WDT, 0xA578).unwrap();
                    chip.write16(WDT, 0x5AFF).unwrap();
                    advance(&mut b, 600 - assertion + 2, chunk);
                    let expected = if manual || assertion >= 15 {
                        0x12345678
                    } else if assertion >= 10 {
                        0x12340000
                    } else {
                        0
                    };
                    assert_eq!(
                        b.read32(original),
                        Ok(expected),
                        "{p:?} manual{manual} t{assertion} chunk{chunk}"
                    );
                    assert_eq!(b.read32(replacement), Ok(0x9ABCDEF0));
                    assert_eq!(
                        b.take_cpu_reset(p),
                        Some(if manual {
                            ResetRequest::Manual
                        } else {
                            ResetRequest::PowerOn
                        })
                    );
                    // Replaying the peer acknowledges its accepted write;
                    // neither it nor canceled work may publish again later.
                    assert_eq!(b.replay_cpu(peer).write32(replacement, 0x9ABCDEF0), Ok(()));
                    b.finish_cpu_transaction(peer);
                    assert!(b.transactions.active.is_none());
                    assert!(b.reset_drain_complete(p));
                    let main_trace = |b: &Bus| {
                        b.sound
                            .memory_trace()
                            .iter()
                            .copied()
                            .filter(|e| e.1 == Dram::Main)
                            .collect::<Vec<_>>()
                    };
                    let trace = main_trace(&b);
                    let completed = if manual || assertion >= 15 {
                        4
                    } else if assertion >= 10 {
                        3
                    } else {
                        2
                    };
                    assert_eq!(trace.iter().filter(|e| e.2).count(), completed);
                    b.write32(original, 0x11223344).unwrap();
                    b.write32(replacement, 0x55667788).unwrap();
                    advance(&mut b, 1000, chunk);
                    assert_eq!(b.read32(original), Ok(0x11223344));
                    assert_eq!(b.read32(replacement), Ok(0x55667788));
                    assert_eq!(main_trace(&b), trace);
                }
            }
        }
    }
}

#[test]
fn memory_completed_read_is_retained_until_the_later_cpu_acknowledgement() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = Bus::default();
        b.set_slave_enabled(true);
        b.write32(SOUND_RAM + 0x1000, 0x12345678).unwrap();
        assert_eq!(
            b.replay_cpu(p).read32(SOUND_RAM | 0x20001000),
            Err(BusError::Pending)
        );
        b.transactions.journals[p as usize]
            .borrow_mut()
            .operand_completion = Some(30);
        advance(&mut b, 10, 3); // first retained word
        assert_eq!(
            b.replay_cpu(p).read32(SOUND_RAM | 0x20001000),
            Err(BusError::Pending)
        );
        advance(&mut b, 10, 3); // second memory completion, CPU bus bound still30
        let trace = b.sound.memory_trace().to_vec();
        b.write32(SOUND_RAM + 0x1000, 0xABCD9876).unwrap();
        assert_eq!(b.read32(SOUND_RAM + 0x1000), Ok(0xABCD9876));
        assert_eq!(b.sound.memory_trace(), trace); // debugger never acquires RAM
        advance(&mut b, 9, 3);
        assert_eq!(
            b.replay_cpu(p).read32(SOUND_RAM | 0x20001000),
            Err(BusError::Pending)
        );
        advance(&mut b, 1, 1);
        assert_eq!(
            b.replay_cpu(p).read32(SOUND_RAM | 0x20001000),
            Ok(0x12345678)
        );
        assert_eq!(b.sound.memory_trace(), trace);
    }
}
