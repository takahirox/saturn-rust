//! Original same-clock grant and full-board differential witnesses.
use super::*;
use crate::{Boot, Machine};
fn rom(source: &str) -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in source.lines().filter(|s| !s.starts_with('#')) {
        let mut words = line.split_whitespace();
        let Some(at) = words.next() else { continue };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
#[test]
fn settled_grants_reuse_only_owned_display_and_already_quiescent_work() {
    for owned in [false, true] {
        let (mut a, mut b) = (Machine::new(), Machine::new());
        b.enable_settled_grant_reuse(false);
        if owned {
            assert!(a.enable_video_journal(true));
            assert!(b.enable_video_journal(true));
        }
        a.enable_settled_grant_reuse(true);
        for clocks in [1, 2, 7, 32, 127] {
            a.bus.advance_clocks(clocks).unwrap();
            b.bus.advance_clocks(clocks).unwrap();
            assert_eq!(a.bus.main_clocks, b.bus.main_clocks);
            assert_eq!(a.loop_state_hashes(), b.loop_state_hashes());
            assert_eq!(
                a.sound_domain_snapshot().unwrap(),
                b.sound_domain_snapshot().unwrap()
            );
        }
        assert_eq!(a.settled_grant_reuse_counts()[1] > 0, owned);
        assert_eq!(b.settled_grant_reuse_counts()[1], 0);
    }
    let mut m = Machine::new();
    assert!(m.enable_video_journal(true));
    assert_eq!(
        m.bus
            .replay_cpu(Processor::Master)
            .read32(HIGH_WRAM + 0x800),
        Err(BusError::Pending)
    );
    assert!(
        !m.bus.grant_bus_transaction_settled(),
        "allocation must not manufacture a fixed-point proof"
    );
    assert_eq!(m.bus.main_clocks, 0);
}
#[test]
fn settled_grant_proof_expires_after_the_first_hardware_interval() {
    let (mut a, mut b) = (Machine::new(), Machine::new());
    b.enable_settled_grant_reuse(false);
    for m in [&mut a, &mut b] {
        assert!(m.enable_video_journal(true));
        m.bus.onchip[0]
            .write8(crate::onchip::FRT + 2, 0xFF)
            .unwrap();
        m.bus.onchip[0]
            .write8(crate::onchip::FRT + 3, 0xFF)
            .unwrap();
        m.bus.onchip[0].write8(crate::onchip::FRT, 2).unwrap();
        m.bus.onchip[0]
            .write16(crate::onchip::IPRB, 0x0500)
            .unwrap();
        assert!(m.bus.grant_bus_transaction_settled());
    }
    a.enable_settled_grant_reuse(true);
    a.bus.advance_main_clocks_after_grant(32, 0, true).unwrap();
    b.bus.advance_main_clocks_after_grant(32, 0, true).unwrap();
    assert_eq!(a.settled_grant_reuse_counts()[1], 1);
    assert_eq!(a.bus.main_clocks, b.bus.main_clocks);
    assert_eq!(a.bus.irq_observed, b.bus.irq_observed);
    assert_eq!(a.loop_state_hashes(), b.loop_state_hashes());
    assert_eq!(
        a.bus.onchip[0].read8(crate::onchip::FRT + 1).unwrap(),
        b.bus.onchip[0].read8(crate::onchip::FRT + 1).unwrap()
    );
}
#[test]
fn settled_grants_preserve_original_cpu_sound_and_device_fields() {
    let sources = [
        include_str!("../../examples/runtime-loop.hex"),
        include_str!("../../examples/runtime-poll-sound-busy-scene.hex"),
        include_str!("../../tests/fixtures/dual-poll.hex"),
    ];
    let mut reused = 0;
    for source in sources {
        let bytes = rom(source);
        for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
            let (mut a, mut b) = (Machine::new(), Machine::new());
            for m in [&mut a, &mut b] {
                m.reset(Boot::ExternalBios(&bytes)).unwrap();
                m.set_pal(pal);
                m.bus.change_system_clock(fast);
                assert!(m.enable_video_journal(true));
            }
            b.enable_settled_grant_reuse(false);
            a.enable_settled_grant_reuse(true);
            for _ in 0..2 {
                a.advance_video_field().unwrap();
                b.advance_video_field().unwrap();
                assert_eq!(a.loop_state_hashes(), b.loop_state_hashes());
                assert_eq!(a.cpu, b.cpu);
                assert_eq!(a.slave, b.slave);
                assert_eq!(a.cpu_ready, b.cpu_ready);
                assert_eq!(a.clocks, b.clocks);
                assert_eq!(a.bus.main_clocks, b.bus.main_clocks);
                assert_eq!(a.bus.irq_observed, b.bus.irq_observed);
                assert_eq!(
                    a.sound_domain_snapshot().unwrap(),
                    b.sound_domain_snapshot().unwrap()
                );
                assert_eq!(a.drain_audio(8192), b.drain_audio(8192));
                for i in 0..2 {
                    a.bus.cache[i]
                        .borrow()
                        .assert_same_contents(&b.bus.cache[i].borrow());
                    a.bus.onchip[i].assert_cached_read_bus_matches(&b.bus.onchip[i]);
                }
            }
            reused += a.settled_grant_reuse_counts()[1];
            assert_eq!(b.settled_grant_reuse_counts()[1], 0);
        }
    }
    assert!(reused > 0, "must omit actual redundant arbitration calls");
}
