//! Original query/CD-DA guests and exact owned-Sound differentials.
use super::*;

fn sound_cpu(m: &mut Machine) {
    m.bus.sound.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
    m.bus.sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
    m.bus.sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
    m.bus.sound.set_enabled(true).unwrap();
}

#[test]
fn cd_query_domain_keeps_sound_inputs_irqs_and_delta_recovery_exact() {
    check_delta_owner_controller(true, true, true);
}

#[test]
fn cd_query_domain_keeps_unknown_commands_writes_and_strict_fences() {
    let mut m = Machine::new();
    m.mount_media(mixed_media());
    for enabled in [false, true] {
        m.enable_sound_cd_queries(enabled);
        for strict in [false, true] {
            m.enable_sound_dependency_filter(!strict);
            for address in [
                crate::cd::DATA,
                crate::cd::DATA + 2,
                crate::cd::WORD_DATA,
                crate::cd::REGISTERS + 8,
                crate::cd::REGISTERS + 12,
                crate::cd::REGISTERS + 24,
                crate::cd::REGISTERS + 28,
                crate::cd::REGISTERS + 32,
                crate::cd::REGISTERS + 36,
            ] {
                assert_eq!(m.bus.sound_input_address(address, None), !enabled || strict);
            }
            for address in [
                crate::cd::REGISTERS + 8,
                crate::cd::REGISTERS + 12,
                crate::cd::REGISTERS + 24,
                crate::cd::REGISTERS + 28,
                crate::cd::REGISTERS + 32,
            ] {
                assert_eq!(
                    m.bus.sound_input_address(address, Some(0)),
                    !enabled || strict
                );
            }
            for command in [0x0000u16, 0x5100, 0x1080, 0x0200, 0xFFFF] {
                m.bus
                    .cd
                    .write16(crate::cd::REGISTERS + 24, command)
                    .unwrap();
                assert_eq!(
                    m.bus
                        .sound_input_address(crate::cd::REGISTERS + 36, Some(0)),
                    !enabled || strict || !matches!(command, 0x0000 | 0x5100)
                );
            }
            for address in [
                crate::cd::DATA,
                crate::cd::WORD_DATA,
                crate::cd::REGISTERS,
                crate::sound::SOUND_RAM,
                crate::sound::SCSP + 0x400,
            ] {
                assert!(m.bus.sound_input_address(address, Some(0)));
            }
        }
    }
}

#[test]
fn cd_query_forecast_excludes_only_neutral_completion_and_preserves_main_deadlines() {
    for command in [0x0000u16, 0x5100, 0x1080, 0x0200, 0xFFFF] {
        for remaining in [1, 511, 512, 513, 1075] {
            let (mut pending, mut ordinary) = (Machine::new(), Machine::new());
            for m in [&mut pending, &mut ordinary] {
                m.mount_media(mixed_media());
            }
            pending.bus.cd.set_quiet_clock_enabled(true);
            for (offset, value) in [(24, command), (28, 153), (32, 0x80), (36, 3)] {
                pending
                    .bus
                    .cd
                    .write16(crate::cd::REGISTERS + offset, value)
                    .unwrap();
            }
            pending.bus.cd.advance(1075 - remaining);
            ordinary.bus.cd.advance(1075 - remaining);
            assert_eq!(pending.bus.cd.clocks_to_event(), remaining);
            for strict in [false, true] {
                for enabled in [false, true] {
                    let expected = if enabled && !strict && matches!(command, 0 | 0x5100) {
                        ordinary.bus.cd.clocks_to_event()
                    } else {
                        remaining
                    };
                    assert_eq!(
                        pending.bus.cd.sound_source_deadline(strict, enabled),
                        expected
                    );
                    assert_eq!(
                        pending.bus.cd.clocks_to_event(),
                        remaining,
                        "Main still stops at the actual command/IRQ completion"
                    );
                }
            }
        }
    }
}

#[test]
fn cd_queries_complete_while_owned_and_later_audio_play_still_matches() {
    let rom = guest();
    for strict in [false, true] {
        for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
            let (mut reference, mut remote) = (Machine::new(), Machine::new());
            for m in [&mut reference, &mut remote] {
                m.reset(Boot::ExternalBios(&rom)).unwrap();
                m.mount_media(mixed_media());
                m.set_pal(pal);
                m.bus.change_system_clock(fast);
                sound_cpu(m);
            }
            remote.enable_sound_cd_queries(true);
            remote.enable_sound_dependency_filter(!strict);
            remote.bus.cd.set_quiet_clock_enabled(true);
            let mut owner = crate::sound::delta::DeltaOwner::default();
            let mut active = false;
            let mut frontier = 0;
            let mut nonzero = false;
            for _ in 0..4 {
                reference.advance_video_field().unwrap();
                loop {
                    if !active {
                        let range = remote.remote_sound_projection_range(32768);
                        if !range.is_empty() {
                            let origin = u64::from(range[0]) | u64::from(range[1]) << 32;
                            let end = u64::from(range[2]) | u64::from(range[3]) << 32;
                            owner.begin(&remote.export_sound_delta().unwrap()).unwrap();
                            owner.extend(end).unwrap();
                            let prefix = owner.prefix().unwrap();
                            frontier = u64::from(prefix[4]) | u64::from(prefix[5]) << 32;
                            if frontier > origin {
                                remote
                                    .accept_remote_sound_projection(
                                        origin,
                                        end,
                                        frontier,
                                        prefix[6] != 0,
                                    )
                                    .unwrap();
                                active = true;
                            } else {
                                remote
                                    .restore_remote_sound_delta(&owner.finish(origin).unwrap())
                                    .unwrap();
                            }
                        }
                    }
                    let done = remote.advance_sound_projection_field().unwrap();
                    if active {
                        let at = remote.remote_sound_projection_time();
                        let prefix = owner.prefix().unwrap();
                        let stable = u64::from(prefix[4]) | u64::from(prefix[5]) << 32;
                        if !done
                            && remote.sound_rendezvous_boundary().is_empty()
                            && stable > frontier
                        {
                            assert_eq!(at, frontier);
                            remote.extend_remote_sound_projection(stable).unwrap();
                            frontier = stable;
                            continue;
                        }
                        remote
                            .restore_remote_sound_delta(&owner.finish(at).unwrap())
                            .unwrap();
                        active = false;
                    }
                    if done {
                        break;
                    }
                    remote.acknowledge_sound_rendezvous();
                }
                let pcm = reference.drain_audio(8192);
                nonzero |= pcm.iter().any(|&v| v != 0);
                assert!(
                    pcm.iter().all(|v| matches!(*v, 0 | 12000 | -6000)),
                    "original constant CD-DA samples keep their literal amplitudes"
                );
                assert_eq!(remote.drain_audio(8192), pcm);
                assert_eq!(remote.bus.cd.interrupt(), reference.bus.cd.interrupt());
                for offset in [8, 12, 24, 28, 32, 36] {
                    assert_eq!(
                        remote.bus.cd.peek16(crate::cd::REGISTERS + offset),
                        reference.bus.cd.peek16(crate::cd::REGISTERS + offset)
                    );
                }
                assert_eq!(
                    remote.debug_read32(0x0600_D008).unwrap(),
                    reference.debug_read32(0x0600_D008).unwrap()
                );
                same(&mut reference, &mut remote);
            }
            assert_eq!(reference.debug_read32(0x0600_D008).unwrap(), 0);
            assert!(nonzero, "CD Play still starts real original audio");
            assert!(remote.bus.cd.quiet_clock_steps > 0);
            let counts = remote.sound_cd_query_counts();
            if strict {
                assert_eq!(counts, [0, 0, 0]);
            } else {
                assert!(
                    counts.iter().all(|&n| n > 0),
                    "must read, write and complete actual CD queries while owned: {counts:?}"
                );
            }
        }
    }
}

#[test]
fn cd_query_deadline_keeps_pending_audio_play_before_its_first_sample() {
    let rom = image("0000 0000 0200 0610 0000\n0200 001B AFFE 0009");
    for fast in [false, true] {
        let (mut reference, mut projected) = (Machine::new(), Machine::new());
        for m in [&mut reference, &mut projected] {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.mount_media(mixed_media());
            m.bus.change_system_clock(fast);
            sound_cpu(m);
            for (offset, value) in [(0x400, 0x020F), (0x216, 0xFF), (0x236, 0xEF)] {
                m.bus.write16(crate::sound::SCSP + offset, value).unwrap();
            }
            for (offset, value) in [(24, 0x1080), (28, 153), (32, 0x80), (36, 3)] {
                m.bus
                    .cd
                    .write16(crate::cd::REGISTERS + offset, value)
                    .unwrap();
            }
        }
        projected.enable_sound_cd_queries(true);
        assert!(
            projected.begin_sound_projection(32768),
            "must actually forecast before the pending Play completes"
        );
        let mut nonzero = false;
        let mut projected_before_play = false;
        for _ in 0..4 {
            reference.advance_video_field().unwrap();
            loop {
                projected_before_play |= projected.begin_sound_projection(32768);
                let done = projected.advance_sound_projection_field().unwrap();
                projected.finish_sound_projection();
                if done {
                    break;
                }
                projected.acknowledge_sound_rendezvous();
            }
            let pcm = reference.drain_audio(8192);
            nonzero |= pcm.iter().any(|&v| v != 0);
            assert_eq!(projected.drain_audio(8192), pcm);
            same(&mut reference, &mut projected);
        }
        assert!(
            projected_before_play && nonzero,
            "must forecast before pending Play and then route nonzero CD-DA"
        );
    }
}

fn mixed_media() -> crate::media::Media {
    let mut bytes = vec![0; 6 * 2352];
    for frame in bytes[3 * 2352..].as_chunks_mut::<4>().0 {
        frame[..2].copy_from_slice(&12000i16.to_le_bytes());
        frame[2..].copy_from_slice(&(-6000i16).to_le_bytes());
    }
    crate::media::Media::cue(
        "FILE \"original\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:00:03",
        &[("original", &bytes)],
    ).unwrap()
}

fn guest() -> Vec<u8> {
    // Small original SH-2 program. Literal loads are fixed after assembly.
    fn literal(words: &mut Vec<u16>, fixups: &mut Vec<(usize, u32)>, r: u16, value: u32) {
        fixups.push((words.len(), value));
        words.push(0xD000 | r << 8);
    }
    fn store(words: &mut Vec<u16>, fixups: &mut Vec<(usize, u32)>, address: u32, value: u32) {
        literal(words, fixups, 1, address);
        literal(words, fixups, 0, value);
        words.push(0x2101);
    }
    fn command(words: &mut Vec<u16>, fixups: &mut Vec<(usize, u32)>, values: [u32; 4]) {
        store(words, fixups, 0x2589_0008, 0xFFFE);
        for (i, value) in values.into_iter().enumerate() {
            store(words, fixups, 0x2589_0018 + 4 * i as u32, value);
        }
        literal(words, fixups, 1, 0x2589_0008);
        words.extend([0x6011, 0xC801, 0x89FC]); // Poll CMOK.
    }
    let (mut words, mut fixups) = (Vec::new(), Vec::new());
    for (address, value) in [
        (0x25B0_0400, 0x020F),
        (0x25B0_0216, 0xFF),
        (0x25B0_0236, 0xEF),
    ] {
        store(&mut words, &mut fixups, address, value);
    }
    words.push(0xE620); // 32 pairs of real Get Status / Get Sector Number commands.
    let start = words.len();
    command(&mut words, &mut fixups, [0, 0, 0, 0]);
    command(&mut words, &mut fixups, [0x5100, 0, 0, 0]);
    words.push(0x4610);
    let offset = start as isize - words.len() as isize - 2;
    assert!((-128..=127).contains(&offset));
    words.push(0x8B00 | (offset as u16 & 255));
    literal(&mut words, &mut fixups, 1, 0x2589_0024);
    words.extend([0x6011, 0x600D]);
    literal(&mut words, &mut fixups, 5, 0x0600_D008);
    words.push(0x2502); // Empty original partition reports zero sectors.
    command(&mut words, &mut fixups, [0x1080, 153, 0x0080, 3]);
    words.extend([0x001B, 0xAFFE, 0x0009]);
    if words.len() % 2 != 0 {
        words.push(0x0009);
    }
    for (at, value) in fixups {
        let offset = (words.len() * 2 - ((at * 2 + 4) & !3)) / 4;
        assert!(offset < 256);
        words[at] |= offset as u16;
        words.extend([(value >> 16) as u16, value as u16]);
    }
    let mut rom = vec![0; BIOS_SIZE];
    rom[..4].copy_from_slice(&0x10000u32.to_be_bytes());
    rom[4..8].copy_from_slice(&0x0610_0000u32.to_be_bytes());
    for (i, word) in words.into_iter().enumerate() {
        rom[0x10000 + 2 * i..0x10002 + 2 * i].copy_from_slice(&word.to_be_bytes());
    }
    rom
}

#[test]
fn cd_query_faults_restore_owned_sound_on_read_and_write_errors() {
    for (port, opcode, disabled) in [
        (0x2581_8000u32, 0x6010u16, false),
        (0x2581_8002, 0x6010, false),
        (0x2589_8000, 0x6010, false),
        (0x2589_0008, 0x6010, false),
        (0x2589_0008, 0x2100, false),
        (0x2589_0018, 0x2101, true),
    ] {
        for (pal, fast) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut rom = vec![0; BIOS_SIZE];
            rom[..4].copy_from_slice(&0x10000u32.to_be_bytes());
            rom[4..8].copy_from_slice(&0x0610_0000u32.to_be_bytes());
            let mut words = vec![0x0009; 1024];
            words.extend([
                0xD101,
                opcode,
                0x001B,
                0x0009,
                (port >> 16) as u16,
                port as u16,
            ]);
            for (i, word) in words.into_iter().enumerate() {
                rom[0x10000 + 2 * i..0x10002 + 2 * i].copy_from_slice(&word.to_be_bytes());
            }
            let (mut reference, mut remote) = (Machine::new(), Machine::new());
            for m in [&mut reference, &mut remote] {
                m.reset(Boot::ExternalBios(&rom)).unwrap();
                m.mount_media(mixed_media());
                if disabled {
                    m.bus.cd.set_enabled(false);
                }
                m.set_pal(pal);
                m.bus.change_system_clock(fast);
                m.bus.sound.ram[..4].copy_from_slice(&0x7000u32.to_be_bytes());
                m.bus.sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
                m.bus.sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
                m.bus.sound.set_enabled(true).unwrap();
            }
            remote.enable_sound_cd_queries(true);
            let expected = reference.advance_video_field().unwrap_err();
            let mut owner = crate::sound::delta::DeltaOwner::default();
            let mut active = false;
            let actual = loop {
                if !active {
                    let range = remote.remote_sound_projection_range(32768);
                    if !range.is_empty() {
                        let origin = u64::from(range[0]) | u64::from(range[1]) << 32;
                        let end = u64::from(range[2]) | u64::from(range[3]) << 32;
                        owner.begin(&remote.export_sound_delta().unwrap()).unwrap();
                        owner.extend(end).unwrap();
                        let prefix = owner.prefix().unwrap();
                        let stable = u64::from(prefix[4]) | u64::from(prefix[5]) << 32;
                        if stable > origin {
                            remote
                                .accept_remote_sound_projection(origin, end, stable, prefix[6] != 0)
                                .unwrap();
                            active = true;
                        } else {
                            remote
                                .restore_remote_sound_delta(&owner.finish(origin).unwrap())
                                .unwrap();
                        }
                    }
                }
                match remote.advance_sound_projection_field() {
                    Err(error) => {
                        assert!(
                            active && remote.bus.sound.projected(),
                            "must fault with actual remote ownership"
                        );
                        remote.abort_sound_transport().unwrap();
                        break error;
                    }
                    Ok(done) => assert!(!done, "byte reads must fail before completing the field"),
                }
                if active {
                    let at = remote.remote_sound_projection_time();
                    remote
                        .restore_remote_sound_delta(&owner.finish(at).unwrap())
                        .unwrap();
                    active = false;
                }
                remote.acknowledge_sound_rendezvous();
            };
            assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
            assert_eq!(
                remote.sound_domain_snapshot().unwrap(),
                reference.sound_domain_snapshot().unwrap()
            );
            same(&mut reference, &mut remote);
        }
    }
}
