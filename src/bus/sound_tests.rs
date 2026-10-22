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
            assert_eq!(b.read16(SOUND_RAM + 0x1000), Ok(0xABCD));
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
