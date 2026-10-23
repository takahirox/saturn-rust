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
                    0 => (2, 15, 23), // MRD owns ref10..15
                    1 => {
                        for (at, value) in
                            [(2, 0x1000), (6, 4), (12, 0x100), (22, 0xE000), (0, 0x1820)]
                        {
                            b.write16(SCSP + at, value).unwrap();
                        }
                        (7, 20, 28) // second PCM read owns ref15..20
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
                        (2, 24, 32) // DMA's final word releases at24
                    }
                    _ => (588, 600, 610), // refresh ref596..600 and605..610 (128*44.1kHz)
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
                // Setup2, source8, destination8: target would finish at18.
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
                advance(&mut b, 2, chunk);
                assert_eq!(
                    b.replay_cpu(p).write16(SOUND_RAM | 0x20001000, 0x5678),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
            }
            let target = if owner == Owner::Scu { 20 } else { 10 };
            // Use the actual granted beat's deadline; the SCU setup/source
            // phases are retained and may already be in progress.
            advance(&mut b, target - 2, chunk);
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1234));
            assert!(b.sound.ram_busy());
            b.write16(SOUND_RAM + 0x1000, 0xABCD).unwrap(); // independent debugger mutation
            advance(&mut b, if owner == Owner::Scu { 3 } else { 4 }, chunk);
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
            advance(&mut b, 8, chunk); // first SCPU read and main write at ref48
            assert_eq!(b.read16(SOUND_RAM), Ok(6));
            assert_eq!(b.sound.cpu.state.a[7], 0);
            b.write16(SOUND_RAM, 5).unwrap();
            advance(&mut b, 2, chunk); // delayed SCPU high samples5 at ref50
            b.write16(SOUND_RAM, 4).unwrap();
            advance(&mut b, 10, chunk); // low completes at ref60
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
            advance(&mut b, 1, chunk); // last DMA word, release and main write
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
    // The main-interface beat would finish at15 (CPU) or18 (SCU),
    // but cannot publish while that second PCM cycle owns sound memory.
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
                advance(&mut b, 7, chunk);
                assert_eq!(
                    b.replay_cpu(p).write16(SOUND_RAM | 0x20001000, 0x5678),
                    Err(BusError::Pending)
                );
                b.grant_bus_transaction();
            } else {
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
                advance(&mut b, 7, chunk);
            }
            advance(&mut b, 12, chunk); // ref19, pending access not published
            assert!(b.sound.ram_busy());
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x1234));
            advance(&mut b, 1, chunk); // PCM completes before main publication
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0x5678));
            advance(&mut b, 610, chunk);
            assert_eq!(&b.sound.scsp.drain(1), &[0x1234; 2]);
        }
    }
}
