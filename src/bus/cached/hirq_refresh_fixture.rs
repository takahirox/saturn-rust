use super::*;
use crate::Machine;

impl Machine {
    pub(crate) fn verify_hirq_refresh(enabled: bool) -> [u64; 3] {
        Self::verify_hirq_refresh_options(enabled, false, false).0
    }

    pub(crate) fn verify_hirq_refresh_options(
        enabled: bool,
        overlap: bool,
        wait: bool,
    ) -> ([u64; 3], [u64; 2]) {
        let (physical, arbitration, _) =
            Self::verify_hirq_refresh_grants(enabled, overlap, wait, false);
        (physical, arbitration)
    }

    pub(crate) fn verify_hirq_refresh_grants(
        enabled: bool,
        overlap: bool,
        wait: bool,
        two: bool,
    ) -> ([u64; 3], [u64; 2], u64) {
        let (physical, arbitration, counts) =
            Self::verify_hirq_refreshes_with_sound(enabled, overlap, wait, two, false, false);
        (physical, arbitration, counts[0])
    }

    pub(crate) fn verify_hirq_refreshes_with_sound(
        enabled: bool,
        overlap: bool,
        wait: bool,
        two: bool,
        active_sound: bool,
        allow_sound: bool,
    ) -> ([u64; 3], [u64; 2], [u64; 3]) {
        let (mut seconds, mut sound_grants, mut shared_seconds) = (0, 0, 0);
        let mut total = [0; 3];
        let mut delayed = 0;
        let mut waiting = 0;
        for fast in [false, true] {
            for irq in [false, true] {
                for count in 0..16 {
                    let make = || {
                        let mut machine = Self::new();
                        machine.bus.change_system_clock(fast);
                        let words = [
                            0x60F1, 0x7701, 0x7701, 0x7701, 0x7701, 0x7701, 0x7701, 0x7701, 0x7701,
                            0x7701, 0xAFF4, 9,
                        ];
                        for (i, word) in words.into_iter().enumerate() {
                            machine.bus.write16(HIGH_WRAM + i as u32 * 2, word).unwrap();
                        }
                        let mut memory = machine.bus.for_cpu(Processor::Master);
                        memory.write32(0xFFFFFFE8, 0xA55A0000).unwrap();
                        memory.write8(crate::cache::CCR, 0x11).unwrap();
                        memory.fetch16(HIGH_WRAM).unwrap();
                        memory.fetch16(HIGH_WRAM + 16).unwrap();
                        machine.bus.cpu_main_delay(Processor::Master, 0);
                        machine.cpu = Cpu::start(HIGH_WRAM, 0x25890008);
                        machine
                            .cpu
                            .prime_fetch(&machine.bus.for_cpu(Processor::Master));
                        machine.bus.claim_external_bus(Processor::Master);
                        let chip = &mut machine.bus.onchip[0];
                        chip.write32(0xFFFF_FFE0, 0xA55A_0004).unwrap();
                        chip.write32(0xFFFF_FFEC, 0xA55A_0048).unwrap();
                        chip.write32(0xFFFF_FFF8, 0xA55A_001F).unwrap();
                        chip.write32(0xFFFF_FFF4, 0xA55A_0000 | count).unwrap();
                        chip.write32(0xFFFF_FFF0, 0xA55A_0008 | if irq { 64 } else { 0 })
                            .unwrap();
                        machine.bus.scu.write32(SCU + 0xB4, 0x60000000).unwrap();
                        machine.bus.scu.write32(SCU + 0xB8, 0x1F).unwrap();
                        if active_sound {
                            let sound = &mut machine.bus.sound;
                            sound.ram[..4].copy_from_slice(&0x1000u32.to_be_bytes());
                            sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                            sound.ram[0x100..0x104].copy_from_slice(&[0x4e, 0x71, 0x60, 0xfc]);
                            sound.set_enabled(true).unwrap();
                            assert!(sound.cached_lookahead_allowed());
                        }
                        machine
                    };
                    let (mut actual, mut reference) = (make(), make());
                    actual.bus.hirq_refresh_plan = enabled;
                    actual.bus.hirq_refresh_overlap = overlap;
                    actual.bus.hirq_refresh_wait = wait;
                    actual.bus.hirq_two_refreshes = two;
                    actual.bus.hirq_sound_refresh = allow_sound;
                    assert!(
                        actual.bus.passive_read(&actual.cpu).is_some(),
                        "original starts with a live HIRQ plan"
                    );
                    assert_eq!(
                        actual.bus.onchip[0]
                            .plan_hirq_refresh(actual.bus.main_clocks)
                            .is_some(),
                        !irq,
                        "original generates a physical refresh without an armed IRQ"
                    );
                    while actual.cpu_state().instructions < 480 {
                        let limit = (480 - actual.cpu_state().instructions).min(128) as u32;
                        if actual.try_cached_batch(limit).unwrap() == 0 {
                            actual.step_inner_with_idle(true).unwrap();
                        }
                        while reference.cpu_state().instructions < actual.cpu_state().instructions {
                            reference.step().unwrap();
                        }
                        assert_eq!(actual.cpu, reference.cpu, "HIRQ/refresh CPU retirement");
                        assert_eq!(actual.clocks, reference.clocks, "physical refresh clock");
                        assert_eq!(actual.bus.main_clocks, reference.bus.main_clocks);
                        assert_eq!(actual.cpu_ready, reference.cpu_ready);
                        assert_eq!(actual.bus.external_owner, reference.bus.external_owner);
                        assert_eq!(
                            actual.bus.scu.plan_cached_abus(),
                            reference.bus.scu.plan_cached_abus()
                        );
                        #[cfg(test)]
                        {
                            actual.bus.onchip[0]
                                .assert_cached_read_bus_matches(&reference.bus.onchip[0]);
                            actual
                                .bus
                                .assert_cached_refresh_arbiter_matches(&reference.bus);
                        }
                    }
                    assert_eq!(actual.loop_state_hashes(), reference.loop_state_hashes());
                    assert!(
                        actual.sound_domain_snapshot().unwrap()
                            == reference.sound_domain_snapshot().unwrap(),
                        "the full canonical Sound state must match",
                    );
                    let sound = actual.bus.hirq_sound_refresh_grants;
                    let shared = actual.bus.hirq_second_grants_with_sound;
                    if !enabled || !active_sound || !allow_sound || irq {
                        assert_eq!(sound, 0);
                    }
                    assert!(shared <= sound && shared <= actual.bus.hirq_second_grants);
                    if active_sound {
                        assert!(
                            actual.sound_state().instructions > 0 && actual.audio_generated() > 0
                        );
                    }
                    sound_grants += sound;
                    shared_seconds += shared;
                    let count = actual.bus.hirq_second_grants;
                    if !enabled || !two || irq {
                        assert_eq!(count, 0);
                    }
                    seconds += count;
                    let counts = actual.bus.hirq_refresh_plan_counts;
                    let shifted = actual.bus.hirq_refresh_delayed_grants;
                    if !enabled || !overlap || irq {
                        assert_eq!(shifted, 0, "strict refresh controls do not admit overlaps");
                    }
                    delayed += shifted;
                    let waited = actual.bus.hirq_refresh_waits;
                    if !enabled || !wait || irq {
                        assert_eq!(waited, 0, "only an eligible request can wait across REF");
                    }
                    waiting += waited;
                    if irq || !enabled {
                        assert_eq!(counts, [0; 3], "refresh IRQ keeps the original HIRQ fence");
                    } else {
                        for (total, count) in total.iter_mut().zip(counts) {
                            *total += count;
                        }
                    }
                }
            }
        }
        (
            total,
            [delayed, waiting],
            [seconds, sound_grants, shared_seconds],
        )
    }
}

#[cfg(test)]
#[test]
fn hirq_read_plan_reacquires_the_bus_after_a_real_refresh() {
    assert_eq!(Machine::verify_hirq_refresh(false), [0; 3]);
    let [physical, resumed, clocks] = Machine::verify_hirq_refresh(true);
    assert!(
        physical > 0 && resumed > 0 && clocks > physical,
        "the HIRQ poll must read again after an actual planned refresh ({physical}, {resumed}, {clocks})"
    );
}

#[cfg(test)]
#[test]
fn hirq_read_already_granted_delays_the_physical_refresh() {
    assert_eq!(
        Machine::verify_hirq_refresh_options(true, false, false).1,
        [0, 0]
    );
    let (counts, [delayed, waiting]) = Machine::verify_hirq_refresh_options(true, true, false);
    assert_eq!(waiting, 0);
    assert!(
        counts[0] >= delayed && delayed > 0,
        "publish a physical refresh actually delayed by HIRQ"
    );
}

#[cfg(test)]
#[test]
fn complete_hirq_and_refresh_beats_match_the_literal_board() {
    let (_, [delayed, waiting]) = Machine::verify_hirq_refresh_options(true, false, true);
    assert_eq!(delayed, 0);
    assert!(
        waiting > 0,
        "first run must publish an HIRQ delayed by a real REF"
    );
    let (counts, [delayed, waiting]) = Machine::verify_hirq_refresh_options(true, true, true);
    assert!(counts[0] >= delayed && counts[1] >= waiting && delayed > 0 && waiting > 0);
}

#[cfg(test)]
#[test]
fn hirq_plan_publishes_two_physical_refreshes_without_moving_cpu_retirement() {
    assert_eq!(
        Machine::verify_hirq_refresh_grants(true, true, true, false).2,
        0
    );
    let (first, _, second) = Machine::verify_hirq_refresh_grants(true, true, true, true);
    assert!(
        first[0] >= second && second > 0,
        "two actual grants must be published"
    );
}

#[cfg(test)]
#[test]
fn two_hirq_refreshes_and_local_sound_share_the_same_machine_prefix() {
    assert_eq!(
        Machine::verify_hirq_refreshes_with_sound(true, true, true, false, true, false).2,
        [0, 0, 0]
    );
    let [seconds, sound, shared] =
        Machine::verify_hirq_refreshes_with_sound(true, true, true, true, true, true).2;
    assert!(shared > 0 && shared <= sound && shared <= seconds);
}
