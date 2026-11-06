//! DSTP must not alter an accepted DMA transaction, its samples or deadlines.
use super::*;

fn setup(channel: u32, indirect: bool, phase: u32) -> Bus {
    let mut b = Bus::default();
    b.write32(HIGH_WRAM + 0x100, 0x12345678).unwrap();
    b.write32(HIGH_WRAM + 0x104, 0x9abcdef0).unwrap();
    for (i, v) in [8, HIGH_WRAM + 0x200, 0x86000100].into_iter().enumerate() {
        b.write32(HIGH_WRAM + i as u32 * 4, v).unwrap();
    }
    for (o, v) in [
        (0, HIGH_WRAM + 0x100),
        (
            4,
            if indirect {
                HIGH_WRAM
            } else {
                HIGH_WRAM + 0x200
            },
        ),
        (8, 8),
        (12, 0x102),
        (20, 2 | if indirect { 1 << 24 } else { 0 }),
        (16, if phase == 0 { 0 } else { 0x100 }),
    ] {
        b.write32(SCU + channel * 32 + o, v).unwrap();
    }
    if phase >= 2 {
        b.scu.signal(2);
    }
    b.write32(SCU + 0xa0, 0xbfff & !0xe00).unwrap();
    b
}

#[test]
fn dstp_leaves_dma_samples_deadlines_priority_retrigger_and_reset_cancellation_intact() {
    for channel in 0..3 {
        for indirect in [false, true] {
            for phase in 0..4 {
                // idle, armed, pending, active before observation
                let mut reference = None;
                for chunk in [1, 3, 127] {
                    for touch in [false, true] {
                        let mut b = setup(channel, indirect, phase);
                        if phase == 3 {
                            b.advance_clocks(10).unwrap();
                        }
                        let mut snapshots = Vec::new();
                        let mut elapsed = 0;
                        for target in [0, 1, 2, 9, 10, 17, 18, 19, 20, 35, 36, 100, 200, 400] {
                            while elapsed < target {
                                let n = chunk.min(target - elapsed);
                                b.advance_clocks(n).unwrap();
                                elapsed += n;
                            }
                            if touch {
                                for address in [SCU + 0x60, SCU + 0x20000060] {
                                    for value in [0, 1, u32::MAX] {
                                        b.write32(address, value).unwrap();
                                        assert_eq!(b.read32(address), Ok(0));
                                    }
                                }
                            }
                            // Inspect the retained granted beat as well as visible
                            // data and IRQ at exactly the same machine clock.
                            let active = b.transactions.active.as_ref().map(|a| {
                                (
                                    a.remaining,
                                    a.transfer.address,
                                    a.transfer.width,
                                    a.transfer.write,
                                )
                            });
                            let pending = b
                                .transactions
                                .scu
                                .map(|c| (c.channel, c.address, c.width, c.value));
                            snapshots.push((
                                b.main_clocks,
                                active,
                                pending,
                                b.read32(HIGH_WRAM + 0x200).unwrap(),
                                b.read32(HIGH_WRAM + 0x204).unwrap(),
                                b.read32(HIGH_WRAM + 0x300).unwrap(),
                                b.read32(SCU + 0xa4).unwrap(),
                                b.scu.interrupt(),
                            ));
                            if target == 0 && phase < 2 {
                                b.write32(SCU + channel * 32 + 16, 0x100).unwrap();
                                b.scu.signal(2);
                            }
                            if target == 1 {
                                b.scu.signal(2); // retained only once, including with DSTP
                            }
                            if target == 10 {
                                // Another level enters while this level owns a
                                // sampled or pending beat. Preserve priority/order.
                                let other = if channel == 2 { 0 } else { 2 };
                                for (o, v) in [
                                    (0, HIGH_WRAM + 0x100),
                                    (4, HIGH_WRAM + 0x300),
                                    (8, 4),
                                    (12, 0x102),
                                    (20, 7),
                                    (16, 0x101),
                                ] {
                                    b.write32(SCU + other * 32 + o, v).unwrap();
                                }
                            }
                        }
                        assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0x12345678));
                        assert_eq!(b.read32(HIGH_WRAM + 0x204), Ok(0x9abcdef0));
                        assert_eq!(b.read32(HIGH_WRAM + 0x300), Ok(0x12345678));
                        let other = if channel == 2 { 0 } else { 2 };
                        assert_eq!(
                            b.read32(SCU + 0xa4).unwrap() & 0xe00,
                            (1 << (11 - channel)) | (1 << (11 - other))
                        );
                        assert_eq!(*reference.get_or_insert(snapshots.clone()), snapshots);
                    }
                }
            }
            for reset_at in [1, 9, 10, 17, 18] {
                let mut b = setup(channel, indirect, 2);
                b.advance_clocks(reset_at).unwrap();
                b.write32(SCU + 0x60, 1).unwrap();
                b.system_reset();
                b.write32(HIGH_WRAM + 0x200, 0xa55a1234).unwrap();
                b.write32(SCU + 0x60, u32::MAX).unwrap();
                b.advance_clocks(400).unwrap();
                assert_eq!(b.read32(HIGH_WRAM + 0x200), Ok(0xa55a1234));
                assert_eq!(b.read32(SCU + 0xa4), Ok(0));
            }
        }
    }
}

#[test]
fn dstp_diagnostics_identify_device_initiated_cycles_without_a_fabricated_cpu_pc() {
    use crate::onchip::DMAC;
    for p in [Processor::Master, Processor::Slave] {
        for write in [false, true] {
            let mut b = Bus::default();
            b.set_slave_enabled(true);
            b.dstp_trace = Some(Vec::new());
            b.write32(HIGH_WRAM, 0xa55a1234).unwrap();
            b.write32(HIGH_WRAM + 4, u32::MAX).unwrap();
            let (source, destination) = if write {
                (HIGH_WRAM, SCU + 0x60)
            } else {
                (SCU + 0x60, HIGH_WRAM + 4)
            };
            for (o, v) in [
                (0, source),
                (4, destination),
                (8, 1),
                (12, 0xa01),
                (0x30, 1),
            ] {
                b.onchip[p as usize].write32(DMAC + o, v).unwrap();
            }
            b.advance_clocks(100).unwrap();
            assert_eq!(b.onchip[p as usize].read32(DMAC + 8), Ok(0));
            assert_ne!(b.onchip[p as usize].read32(DMAC + 12).unwrap() & 2, 0);
            assert_eq!(b.take_dma_errors(), [false; 2]);
            assert_eq!(b.read32(SCU + 0x60), Ok(0));
            if !write {
                assert_eq!(b.read32(HIGH_WRAM + 4), Ok(0));
            }
            let trace = b.dstp_trace.as_ref().unwrap();
            assert_eq!(trace.len(), 1);
            assert!(
                trace[0].contains(&format!("source={p:?} SH-2 DMAC PC=None operand=None DMA")),
                "{trace:?}"
            );
        }
    }
}
