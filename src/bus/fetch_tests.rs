//! H §§7.3.1/7.11.2, P §7.5: independently authored instruction-bus vectors.
use super::*;

#[test]
fn uncached_fetch_is_one_aligned_longword_split_on_physical_widths_and_retains_the_second_word() {
    for p in [Processor::Master, Processor::Slave] {
        for (select, width) in [(1, 1), (2, 2), (3, 4)] {
            for (ccr, alias) in [(0, 0), (1, 0x20000000), (3, 0)] {
                for offset in [0, 2] {
                    let mut b = Bus::default();
                    b.set_slave_enabled(true);
                    b.for_cpu(p)
                        .write32(0xFFFFFFE4, 0xA55A003C | select << 6)
                        .unwrap();
                    b.for_cpu(p).write8(CCR, ccr).unwrap();
                    b.write32(HIGH_WRAM, 0xE107E22A).unwrap();
                    let pc = (HIGH_WRAM | alias) + offset;
                    let mut trace = Vec::new();
                    let mut clocks = 0;
                    loop {
                        match b.replay_cpu(p).fetch16(pc) {
                            Ok(word) => {
                                assert_eq!(word, if offset == 0 { 0xE107 } else { 0xE22A });
                                break;
                            }
                            Err(BusError::Pending) => (),
                            e => panic!("{e:?}"),
                        }
                        b.grant_bus_transaction();
                        let active = b.transactions.active.as_ref().unwrap();
                        trace.push((
                            active.transfer.address,
                            active.transfer.width,
                            active.transfer.first,
                            active.transfer.last,
                        ));
                        let n = active.remaining;
                        clocks += n;
                        b.advance_clocks(n).unwrap();
                    }
                    let expected: Vec<_> = (0..4 / width)
                        .map(|i| {
                            (
                                (HIGH_WRAM | alias) + (i * width) as u32,
                                width,
                                i == 0,
                                (i + 1) * width == 4,
                            )
                        })
                        .collect();
                    assert_eq!(trace, expected);
                    // Default WCR=3, ordinary access=2+6 wait states per beat.
                    assert_eq!(clocks, 8 * 4 / width as u32);
                    b.finish_cpu_transaction(p);
                    b.write16(HIGH_WRAM + 2, 0xE263).unwrap();
                    if offset == 0 {
                        assert_eq!(b.replay_cpu(p).fetch16(pc + 2), Ok(0xE22A));
                        assert!(b.transactions.active.is_none());
                        b.finish_cpu_transaction(p);
                    }
                    // Entry at +2 must refetch the entire longword. Retention
                    // is single-use, never an implicit two-word data cache.
                    assert_eq!(
                        b.replay_cpu(p).fetch16((HIGH_WRAM | alias) + 2),
                        Err(BusError::Pending)
                    );
                    b.cancel_cpu_transaction(p);
                    b.for_cpu(p).fetch16(HIGH_WRAM | alias).unwrap();
                    b.cancel_cpu_transaction(p);
                    assert_eq!(
                        b.replay_cpu(p).fetch16((HIGH_WRAM | alias) + 2),
                        Err(BusError::Pending)
                    );
                }
            }
        }
    }
}

#[test]
fn taken_branch_discards_the_word_fetched_beside_its_self_modifying_delay_slot() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = Bus::default();
        // P1 avoids cache coherence: MOV.W R2,@R3; MOV #7,R6.
        b.write32(HIGH_WRAM, 0x2321E607).unwrap();
        let image = [0xD202, 0xD303, 0x2321, 0xE607, 0x001B, 0x0009];
        for (i, word) in image.into_iter().enumerate() {
            b.write16(HIGH_WRAM + i as u32 * 2, word).unwrap();
        }
        b.write32(HIGH_WRAM + 12, 0xE62A).unwrap();
        b.write32(HIGH_WRAM + 16, (HIGH_WRAM | 0x20000000) + 6)
            .unwrap();
        let mut cpu = crate::cpu::Cpu::start(HIGH_WRAM | 0x20000000, HIGH_WRAM + 0x1000);
        for _ in 0..5 {
            cpu.step(&mut b.for_cpu(p)).unwrap();
        }
        assert_eq!(cpu.state().r[6], 7); // retained old immediate, real store changed RAM
        assert_eq!(b.read16(HIGH_WRAM + 6), Ok(0xE62A));

        // BRA at +6 targets +10. Its slot at +8 overwrites the
        // instruction fetched beside it. A taken branch discards that word.
        b.for_cpu(p).sh2_discard_prefetch();
        b.write16(HIGH_WRAM, 0xD202).unwrap();
        b.write16(HIGH_WRAM + 2, 0xD303).unwrap();
        b.write16(HIGH_WRAM + 4, 0x0009).unwrap();
        b.write16(HIGH_WRAM + 6, 0xA000).unwrap();
        b.write16(HIGH_WRAM + 8, 0x2321).unwrap();
        b.write16(HIGH_WRAM + 10, 0xE607).unwrap();
        b.write32(HIGH_WRAM + 16, (HIGH_WRAM | 0x20000000) + 10)
            .unwrap();
        let mut cpu = crate::cpu::Cpu::start(HIGH_WRAM | 0x20000000, HIGH_WRAM + 0x1000);
        for _ in 0..6 {
            cpu.step(&mut b.for_cpu(p)).unwrap();
        }
        assert_eq!(cpu.state().r[6], 42);
    }
}

#[test]
fn taken_conditional_to_the_sequential_address_still_refetches_its_target() {
    for p in [Processor::Master, Processor::Slave] {
        let mut b = Bus::default();
        b.write32(HIGH_WRAM, 0x00180009).unwrap(); // SETT; NOP
        b.write32(HIGH_WRAM + 4, 0x89FFE607).unwrap(); // BT -1 -> +6; MOV #7,R6
        let mut cpu = crate::cpu::Cpu::start(HIGH_WRAM | 0x20000000, HIGH_WRAM + 0x1000);
        for _ in 0..3 {
            cpu.step(&mut b.for_cpu(p)).unwrap();
        }
        assert_eq!(cpu.state().pc, (HIGH_WRAM | 0x20000000) + 6);
        b.write16(HIGH_WRAM + 6, 0xE62A).unwrap();
        cpu.step(&mut b.for_cpu(p)).unwrap();
        assert_eq!(cpu.state().r[6], 42);
    }
}
