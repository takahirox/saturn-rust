//! Literal ST-162 §7.2.4/5 and later §7.2.5/4 vectors; see docs/cd-drive-reports.md.
use saturn_rust::{
    bus::{Bus, Memory},
    cd::{DATA, REGISTERS as CD},
    media::Media,
};

fn advance(cd: &mut Bus, mut clocks: u32, chunk: u32, preview: bool) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        cd.cd.advance(n);
        clocks -= n;
        if preview {
            // Debugger reads are passive; CR4's protocol read is intentional
            // only in response(), never in a preview.
            cd.debug_read16(CD + 0x24).unwrap();
            cd.debug_read32(DATA).unwrap();
        }
    }
}
fn response(cd: &Bus) -> [u16; 4] {
    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}
fn peek(cd: &Bus) -> [u16; 4] {
    std::array::from_fn(|i| cd.debug_read16(CD + 0x18 + i as u32 * 4).unwrap())
}
fn start(cd: &mut Bus, cmd: [u16; 4]) {
    cd.write16(CD + 8, !1).unwrap();
    for (i, word) in cmd.into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
    }
}
fn issue(cd: &mut Bus, cmd: [u16; 4], chunk: u32) -> [u16; 4] {
    start(cd, cmd);
    advance(cd, 1074, chunk, true);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 0);
    advance(cd, 1, chunk, true);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 1);
    response(cd)
}
fn q(cd: &mut Bus, expected_status: u16, expected: [u16; 5], chunk: u32) {
    assert_eq!(
        issue(cd, [0x2000, 0, 0, 0], chunk),
        [expected_status, 5, 0, 0]
    );
    for word in expected {
        assert_eq!(cd.debug_read16(DATA).unwrap(), word);
        assert_eq!(cd.read16(DATA).unwrap(), word);
    }
    assert_eq!(cd.read16(DATA).unwrap(), 0xFFFF);
    assert_eq!(issue(cd, [0x0600, 0, 0, 0], chunk)[1], 5);
}
fn mounted() -> Bus {
    let mut cd = Bus::default();
    cd.cd.mount(Media::iso(&vec![0x69; 2048 * 100]).unwrap());
    cd
}

#[test]
fn reports_and_q_follow_every_supported_state_and_both_standby_origins() {
    for chunk in [1, 3, 17, 10000] {
        let mut cd = Bus::default();
        assert_eq!(response(&cd), [0x0043, 0x4442, 0x4C4F, 0x434B]);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x07FF, 0xFFFF, 0xFFFF, 0xFFFF]
        );
        q(
            &mut cd,
            0x47FF,
            [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF],
            chunk,
        );
        cd.cd.mount(Media::iso(&vec![0; 2048 * 100]).unwrap());
        issue(&mut cd, [0x0400, 1, 0, 0], chunk);
        assert_eq!(
            issue(&mut cd, [0x1180, 181, 0, 0], chunk),
            [0x0400, 0x4101, 0x0100, 181]
        );
        q(&mut cd, 0x4400, [0x4101, 0x0100, 0x001F, 0, 181], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x4101, 0x0100, 181]
        );
        q(&mut cd, 0x4180, [0x4101, 0x0100, 0x001F, 0, 181], chunk);
        advance(&mut cd, 26_874_100, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0280, 0x4101, 0x0100, 181]
        );
        q(&mut cd, 0x4280, [0x4101, 0x0100, 0x001F, 0, 181], chunk);
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x4101, 0x0100, 181]
        );
        assert_eq!(
            issue(&mut cd, [0x1100, 0, 0, 0], chunk),
            [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]
        );
        q(
            &mut cd,
            0x42FF,
            [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF],
            chunk,
        );
        assert_eq!(
            issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk),
            [0x0400, 0x4101, 0x0100, 150]
        );
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x4101, 0x0100, 150]
        );
        assert_eq!(
            issue(&mut cd, [0x1200, 0, 0, 0], chunk),
            [0x0500, 0x4101, 0x0100, 150]
        );
        q(&mut cd, 0x4500, [0x4101, 0x0100, 0, 0, 150], chunk);
        assert_eq!(
            issue(&mut cd, [0x0500, 0, 0, 0], chunk),
            [0x06FF, 0xFFFF, 0xFFFF, 0xFFFF]
        );
        q(
            &mut cd,
            0x46FF,
            [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF],
            chunk,
        );
        assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0], chunk)[0], 0xFF00);
        assert_eq!(issue(&mut cd, [0x0100, 0, 0, 0], chunk), [0x06FF, 2, 0, 0]);
        issue(&mut cd, [0x0400, 1, 0, 0], chunk);
        issue(&mut cd, [0x1080, 181, 0x0280, 2], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0380, 0x4101, 0x0100, 181]
        );
        q(&mut cd, 0x4380, [0x4101, 0x0100, 0x001F, 0, 181], chunk);
    }
}

#[test]
fn home_open_error_and_queries_keep_repeat_range_and_recovery_information() {
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1080, 181, 0x0280, 2], chunk);
        advance(&mut cd, 358_322, chunk, true); // two reads, one repeat
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0181, 0x4101, 0x0100, 181]
        );
        issue(&mut cd, [0x0400, 1, 0, 0], chunk);
        advance(&mut cd, 26_874_100, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0281, 0x4101, 0x0100, 181]
        );
        q(&mut cd, 0x4281, [0x4101, 0x0100, 31, 0, 181], chunk);
        issue(&mut cd, [0x1100, 0, 0, 0], chunk);
        issue(&mut cd, [0x0500, 0, 0, 0], chunk);
        q(
            &mut cd,
            0x46FF,
            [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF],
            chunk,
        );
        issue(&mut cd, [0x0400, 1, 0, 0], chunk);
        assert_eq!(issue(&mut cd, [0, 0, 0, 0], chunk)[0], 0x0181);
        issue(&mut cd, [0x1180, 181, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk)[0],
            0x0381
        );
        advance(&mut cd, 358_322, chunk, true);
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(issue(&mut cd, [0, 0, 0, 0], chunk)[0], 0x0182);
        issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk);
        advance(&mut cd, 358_322, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0182, 0x4101, 0x0100, 183]
        );
        // Damage the next sector of a noninitial two-sector repeat range.
        let cooked = Media::iso(&vec![0x69; 2048 * 100]).unwrap();
        let mut raw: Vec<u8> = (150..250)
            .flat_map(|fad| cooked.raw_sector(fad).unwrap())
            .collect();
        raw[32 * 2352 + 16] ^= 0x5A;
        cd.cd.mount(
            Media::cue(
                "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
                &[("d", &raw)],
            )
            .unwrap(),
        );
        // Start a separate error-recovery scenario with an explicit software
        // reset: replacement media correctly preserves the previous count 2.
        issue(&mut cd, [0x0401, 0, 0, 0], chunk);
        issue(&mut cd, [0x0400, 1, 0, 0x8001], chunk);
        issue(&mut cd, [0x1080, 181, 0x0280, 2], chunk);
        advance(&mut cd, 358_322, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0880, 0x4101, 0x0100, 182]
        );
        q(&mut cd, 0x4880, [0x4101, 0x0100, 32, 0, 182], chunk);
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]
        );
        q(
            &mut cd,
            0x49FF,
            [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF],
            chunk,
        );
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x4101, 0x0100, 182]
        );
        q(&mut cd, 0x4180, [0x4101, 0x0100, 32, 0, 182], chunk);
        // Allow the damaged sector once to acquire a nonzero notification,
        // then reenable failure without changing the retained play range.
        issue(&mut cd, [0x0480, 1, 0, 0xFF40], chunk);
        issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk);
        advance(&mut cd, 179_161, chunk, true);
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(issue(&mut cd, [0, 0, 0, 0], chunk)[0], 0x0181);
        issue(&mut cd, [0x0480, 1, 0, 0xFF01], chunk);
        issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk);
        advance(&mut cd, 537_483, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]
        );
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0181, 0x4101, 0x0100, 182]
        );
    }
}

#[test]
fn publication_edges_cr4_scdq_masks_and_reset_cancel_invalid_responses() {
    for chunk in [1, 3, 17, 10000] {
        for edge in [-1i32, 0, 1] {
            let mut cd = mounted();
            cd.write16(CD + 8, 0).unwrap();
            cd.write16(CD + 12, 1024).unwrap();
            start(&mut cd, [0x1100, 0, 0, 0]);
            advance(&mut cd, 1074, chunk, true);
            assert_eq!(peek(&cd), [0x0180, 0x4101, 0x0100, 150]);
            cd.cd.advance(1);
            let home = [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF];
            assert_eq!(peek(&cd), home);
            assert!(!cd.cd.interrupt());
            // First stopped periodic deadline = ceil(26874100 / 60).
            advance(&mut cd, (447_902 - 1075) - 1, chunk, true);
            assert_eq!(cd.read16(CD + 8).unwrap() & 1024, 0);
            if edge == -1 {
                cd.read16(CD + 0x24).unwrap();
            }
            cd.cd.advance(1);
            assert!(cd.cd.interrupt());
            assert_eq!(
                peek(&cd),
                if edge == -1 {
                    [0x22FF, 0xFFFF, 0xFFFF, 0xFFFF]
                } else {
                    home
                }
            );
            if edge == 0 {
                cd.read16(CD + 0x24).unwrap();
            }
            cd.cd.advance(1);
            if edge == 1 {
                cd.read16(CD + 0x24).unwrap();
            }
            cd.write16(CD + 8, !1024).unwrap();
            assert!(!cd.cd.interrupt());
            advance(&mut cd, 447_901, chunk, true);
            assert!(cd.cd.interrupt());
            assert_eq!(peek(&cd), [0x22FF, 0xFFFF, 0xFFFF, 0xFFFF]);
            // A query pending across the periodic edge protects registers;
            // completion publishes its own payload, not a generic report.
            start(&mut cd, [0x0100, 0, 0, 0]);
            advance(&mut cd, 1075, chunk, true);
            assert_eq!(peek(&cd), [0x02FF, 2, 0, 0]);
            advance(&mut cd, 447_902, chunk, true);
            assert_eq!(peek(&cd), [0x02FF, 2, 0, 0]);
            response(&cd);
            // Software reset leaves no home marker or pending Q transfer.
            issue(&mut cd, [0x2000, 0, 0, 0], chunk);
            assert_eq!(
                issue(&mut cd, [0x0401, 0xFFFF, 0, 0xFFFF], chunk),
                [0x0180, 0x4101, 0x0100, 150]
            );
            assert_eq!(cd.read16(DATA).unwrap(), 0xFFFF);
            start(&mut cd, [0x1100, 0, 0, 0]);
            advance(&mut cd, 1074, chunk, true);
            cd.cd.set_enabled(false);
            advance(&mut cd, 1_000_000, chunk, true);
            assert_eq!(peek(&cd), [0x0043, 0x4442, 0x4C4F, 0x434B]);
            cd.cd.set_enabled(true);
            assert_eq!(response(&cd), [0x0180, 0x4101, 0x0100, 150]);
            advance(&mut cd, 447_902, chunk, true);
            assert_eq!(response(&cd), [0x2180, 0x4101, 0x0100, 150]);
        }
    }
}

#[test]
fn executing_guest_transfers_q_computes_sums_and_recovers_from_home_and_error() {
    use saturn_rust::{
        Boot, Machine,
        bus::{BIOS_SIZE, Memory},
    };
    let mut original = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-reports.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let Some(at) = fields.next() else {
            continue;
        };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in fields.enumerate() {
            original[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let mut machine = Machine::new();
    for (position, changed, omit) in [
        (181u16, false, false),
        (187, true, false),
        (181, false, true),
    ] {
        let mut image = original.clone();
        image[0x100..0x102].copy_from_slice(&position.to_be_bytes());
        if omit {
            image[0x104..0x106].fill(0);
        }
        let raw = if changed {
            include_bytes!("fixtures/cd-reports-changed.bin").as_slice()
        } else {
            include_bytes!("fixtures/cd-reports.bin").as_slice()
        };
        let mut previous = None;
        for preview in [false, false, true, true] {
            machine.reset(Boot::ExternalBios(&image)).unwrap();
            machine.mount_media(
                Media::cue(
                    include_str!("fixtures/cd-reports.cue"),
                    &[("cd-reports.bin", raw)],
                )
                .unwrap(),
            );
            let steps = if preview { 17 } else { 10000 };
            let mut q_previews = 0;
            for _ in 0..(1_500_000 / steps) {
                if preview {
                    machine.debug_read16(CD + 0x24).unwrap();
                    let value = machine.debug_read32(DATA).unwrap();
                    assert_eq!(machine.debug_read32(DATA).unwrap(), value);
                    if value == 0x4101_0100 {
                        q_previews += 1;
                    }
                }
                machine.run_steps(steps).unwrap();
                if machine.bus().read32(0x0600EFFC) == Ok(0x600D) {
                    break;
                }
            }
            if preview {
                assert!(q_previews > 0, "must inspect a pending guest Q transfer");
            }
            let read = |at| machine.debug_read32(0x0600E000 + at).unwrap();
            assert_eq!(read(0xFFC), 0x600D);
            let report = |slot: u32| {
                std::array::from_fn::<_, 4, _>(|word| read(slot * 16 + word as u32 * 4))
            };
            assert_eq!(report(2), [0x0180, 0x4101, 0x0100, u32::from(position)]);
            assert_eq!(
                report(6),
                if omit {
                    [0x0180, 0x4101, 0x0100, u32::from(position)]
                } else {
                    [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]
                }
            );
            let paused = if omit { u32::from(position) } else { 150 };
            assert_eq!(report(10), [0x0180, 0x4101, 0x0100, paused]);
            assert_eq!(report(13), [0x06FF, 0xFFFF, 0xFFFF, 0xFFFF]);
            assert_eq!(report(18), [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]);
            assert_eq!(report(22), [0x0180, 0x4101, 0x0100, 173]);
            let initial = [
                0x4101,
                0x0100,
                u32::from(position) - 150,
                0,
                u32::from(position),
            ];
            let invalid = [0xFFFF, 0xFFFF, 0xFFFF, 0x00FF, 0xFFFF];
            for (q, expected) in [
                initial,
                if omit { initial } else { invalid },
                [0x4101, 0x0100, paused - 150, 0, paused],
                invalid,
                invalid,
                [0x4101, 0x0100, 23, 0, 173],
            ]
            .into_iter()
            .enumerate()
            {
                for (word, value) in expected.into_iter().enumerate() {
                    assert_eq!(read(0x800 + q as u32 * 32 + word as u32 * 4), value);
                }
                assert_eq!(
                    read(0x814 + q as u32 * 32),
                    expected.into_iter().sum::<u32>()
                );
            }
            assert_eq!(read(0xFE0), if changed { 0xA6A6 } else { 0x6969 });
            assert_eq!(report(28)[1], 1);
            let records: Vec<_> = (0..1024).map(|i| read(i * 4)).collect();
            if let Some(old) = previous {
                assert_eq!(records, old);
            }
            previous = Some(records);
        }
    }
}

#[test]
fn home_seek_recovery_reports_target_before_at_and_after_completion() {
    for chunk in [1, 3, 17, 10000] {
        for preview in [false, true] {
            for edge in [-1i32, 0, 1] {
                let mut cd = mounted();
                issue(&mut cd, [0x1180, 181, 0, 0], chunk);
                advance(&mut cd, 26_875, chunk, preview);
                assert_eq!(
                    issue(&mut cd, [0x1100, 0, 0, 0], chunk),
                    [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]
                );
                assert_eq!(
                    issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk),
                    [0x0400, 0x4101, 0x0100, 150]
                );
                // Complete a status query exactly around the seek deadline.
                advance(&mut cd, (26_875i32 - 1075 + edge) as u32, chunk, preview);
                let status = issue(&mut cd, [0, 0, 0, 0], chunk);
                assert_eq!(
                    status,
                    [if edge < 0 { 0x0400 } else { 0x0180 }, 0x4101, 0x0100, 150]
                );
            }
        }
    }
}

#[test]
fn command_completion_wins_periodic_coincidence_and_reset_cancels_at_each_edge() {
    for chunk in [1, 3, 17, 10000] {
        for edge in [-1i32, 0, 1] {
            let mut cd = mounted();
            cd.write16(CD + 8, 0).unwrap();
            // Command CMOK at first report deadline−1/deadline/deadline+1.
            advance(&mut cd, (447_902i32 - 1075 + edge) as u32, chunk, true);
            start(&mut cd, [0x1100, 0, 0, 0]);
            advance(&mut cd, 1074, chunk, true);
            assert_eq!(peek(&cd), [0x0180, 0x4101, 0x0100, 150]);
            cd.cd.advance(1);
            assert_eq!(peek(&cd), [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]);
            cd.cd.advance(1);
            assert_eq!(cd.read16(CD + 8).unwrap() & 1025, 1025);
            assert_eq!(peek(&cd), [0x02FF, 0xFFFF, 0xFFFF, 0xFFFF]);
            // An invalid Q transfer must disappear at software reset, before
            // or after its final command publication, and after power cycling.
            for power in [false, true] {
                let mut cd = mounted();
                issue(&mut cd, [0x1100, 0, 0, 0], chunk);
                start(&mut cd, [0x2000, 0, 0, 0]);
                advance(&mut cd, (1075i32 + edge) as u32, chunk, true);
                if power {
                    cd.cd.set_enabled(false);
                    cd.cd.advance(1_000_000);
                    cd.cd.set_enabled(true);
                } else {
                    // A pending command must finish before another command
                    // can be submitted; software reset then cancels DATATRNS.
                    if edge < 0 {
                        cd.cd.advance(1);
                    }
                    issue(&mut cd, [0x0401, 0, 0, 0], chunk);
                }
                assert_eq!(cd.debug_read16(DATA).unwrap(), 0xFFFF);
                assert_eq!(
                    issue(&mut cd, [0, 0, 0, 0], chunk),
                    [0x0180, 0x4101, 0x0100, 150]
                );
                advance(&mut cd, 900_000, chunk, true);
                assert_eq!(response(&cd), [0x2180, 0x4101, 0x0100, 150]);
            }
        }
    }
}

#[test]
fn audio_position_restarts_at_disc_start_after_seek_home_pause() {
    let mut audio = vec![0; 2352 * 100];
    for (i, frame) in audio.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let sample = (100 + i % 30000) as i16;
        frame[..2].copy_from_slice(&sample.to_le_bytes());
        frame[2..].copy_from_slice(&(-sample).to_le_bytes());
    }
    let mut cd = Bus::default();
    cd.cd.mount(
        Media::cue(
            "FILE \"a\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
            &[("a", &audio)],
        )
        .unwrap(),
    );
    // Retain a range containing 150 so canceling PAUSE after home is valid.
    issue(&mut cd, [0x1080, 150, 0, 0], 17);
    cd.cd.advance(26_875);
    issue(&mut cd, [0x1180, 181, 0, 0], 17);
    cd.cd.advance(26_875);
    issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], 17);
    for _ in 0..13 {
        cd.cd.audio_sample();
    }
    issue(&mut cd, [0x1100, 0, 0, 0], 17);
    issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], 17);
    cd.cd.advance(26_875);
    assert_eq!(
        issue(&mut cd, [0, 0, 0, 0], 17),
        [0x0100, 0x0101, 0x0100, 150]
    );
    issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], 17);
    assert_eq!(cd.cd.audio_sample(), [100, -100]);
}

#[test]
fn disc_end_pause_and_q_agree_on_the_leadout_position() {
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1080, 249, 0x80, 1], chunk);
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x41AA, 0x0100, 250]
        );
        q(&mut cd, 0x4180, [0x41AA, 0x0100, 0, 0, 250], chunk);
        assert_eq!(
            issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk),
            [0x0400, 0x41AA, 0x0100, 250]
        );
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x41AA, 0x0100, 250]
        );
        assert_eq!(issue(&mut cd, [0x1180, 250, 0, 0], chunk)[0], 0xFF00);
    }
}

#[test]
fn scdq_acknowledgement_and_mask_changes_before_at_and_after_the_deadline() {
    for chunk in [1, 3, 17, 10000] {
        for edge in [-1i32, 0, 1] {
            let mut cd = mounted();
            cd.write16(CD + 8, 0).unwrap();
            cd.write16(CD + 12, 0).unwrap();
            advance(&mut cd, (447_902i32 + edge) as u32, chunk, true);
            assert!(!cd.cd.interrupt()); // A mask never suppresses flag generation.
            assert_eq!(
                cd.read16(CD + 8).unwrap() & 1024,
                if edge < 0 { 0 } else { 1024 }
            );
            cd.write16(CD + 12, 1024).unwrap();
            assert_eq!(cd.cd.interrupt(), edge >= 0);
            cd.write16(CD + 8, !1024).unwrap();
            assert!(!cd.cd.interrupt());
            // Finish at deadline+2. An acknowledgement before the edge cannot
            // erase that future event; at/after acknowledgements clear it.
            advance(&mut cd, (2 - edge) as u32, chunk, true);
            assert_eq!(
                cd.read16(CD + 8).unwrap() & 1024,
                if edge < 0 { 1024 } else { 0 }
            );
            assert_eq!(cd.cd.interrupt(), edge < 0);
            cd.write16(CD + 12, 0).unwrap();
            assert!(!cd.cd.interrupt());
            cd.write16(CD + 8, !1024).unwrap();
            advance(&mut cd, 895_804 - 447_904 - 1, chunk, true);
            assert_eq!(cd.read16(CD + 8).unwrap() & 1024, 0);
            cd.cd.advance(1);
            assert_eq!(cd.read16(CD + 8).unwrap() & 1024, 1024);
            assert!(!cd.cd.interrupt());
            cd.write16(CD + 12, 1024).unwrap();
            assert!(cd.cd.interrupt());
        }
    }
}

#[test]
fn pause_cancellation_requires_both_retained_bounds_after_home() {
    // ST-162 §4.2 pp.37–38: home retains [181,183), but PAUSE moves to 150.
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1080, 181, 0x0280, 2], chunk);
        advance(&mut cd, 358_322, chunk, true);
        issue(&mut cd, [0x1100, 0, 0, 0], chunk);
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0181, 0x4101, 0x0100, 150]
        );
        for position in [150, 180, 181, 182, 183, 184] {
            // The first cancellation is directly from Home -> current PAUSE.
            if position != 150 {
                issue(&mut cd, [0x1180, position, 0, 0], chunk);
                advance(&mut cd, 26_875, chunk, true);
            }
            cd.write16(CD + 8, !16).unwrap();
            let inside = (181..183).contains(&position);
            assert_eq!(
                issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
                [
                    if inside { 0x0381 } else { 0x0181 },
                    0x4101,
                    0x0100,
                    position
                ]
            );
            assert_eq!(cd.read16(CD + 8).unwrap() & 16, if inside { 0 } else { 16 });
            if !inside {
                advance(&mut cd, 537_483, chunk, true);
                assert_eq!(
                    issue(&mut cd, [0, 0, 0, 0], chunk),
                    [0x0181, 0x4101, 0x0100, position]
                );
                q(
                    &mut cd,
                    0x4181,
                    [0x4101, 0x0100, position - 150, 0, position],
                    chunk,
                );
            }
        }
        // A valid cancellation still uses the original lower bound and finite
        // repeat maximum: one remaining repeat, then stop at the exclusive end.
        issue(&mut cd, [0x1180, 182, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk);
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0382, 0x4101, 0x0100, 181]
        );
        advance(&mut cd, 358_322, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0182, 0x4101, 0x0100, 183]
        );
    }
}

#[test]
fn replacement_media_retains_host_play_information_until_software_reset() {
    // ST-162 §5.5 p.49: attaching media is not a software reset. Use different
    // contents AND lead-out so resetting either retained range bound is visible.
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1080, 181, 0x0280, 2], chunk);
        advance(&mut cd, 358_322, chunk, true);
        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0181, 0x4101, 0x0100, 181]
        );
        issue(&mut cd, [0x0500, 0, 0, 0], chunk);
        cd.cd.mount(Media::iso(&vec![0xA6; 2048 * 120]).unwrap());
        assert_eq!(response(&cd), [0x0181, 0x4101, 0x0100, 150]);
        // Remain outside the old lower bound, despite the new disc's start.
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0181, 0x4101, 0x0100, 150]
        );
        issue(&mut cd, [0x1180, 182, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0381, 0x4101, 0x0100, 182]
        );
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0382, 0x4101, 0x0100, 181]
        );
        advance(&mut cd, 358_322, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0182, 0x4101, 0x0100, 183]
        );
        // Software reset deliberately clears notifications and installs the new
        // disc end, unlike media replacement. Play now crosses the old end.
        issue(&mut cd, [0x0401, 0, 0, 0], chunk);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0380, 0x4101, 0x0100, 183]
        );
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0380, 0x4101, 0x0100, 184]
        );
    }
}

#[test]
fn software_reset_restores_whole_disc_range_at_a_noninitial_pickup() {
    // ST-162 §5.5 p.49 and §7.2.6 pp.65–69: reset initializes the play
    // range to the whole disc independently of the retained pickup position.
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1180, 181, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x0401, 0, 0, 0], chunk),
            [0x0180, 0x4101, 0x0100, 181]
        );
        issue(&mut cd, [0x1180, 150, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0380, 0x4101, 0x0100, 150]
        );
        advance(&mut cd, 17_916_067, chunk, true); // 100 sectors
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x41AA, 0x0100, 250]
        );
    }
}

#[test]
fn shorter_replacement_bounds_play_without_overwriting_retained_settings() {
    // ST-162 §5.5 p.49 retains host settings across tray changes; §7.2.6
    // pp.65–69 bounds the effective end to the disc end. A longer third disc
    // distinguishes bounding playback from overwriting the retained end.
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x0500, 0, 0, 0], chunk);
        cd.cd.mount(Media::iso(&vec![0xA6; 2048 * 50]).unwrap());
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0380, 0x4101, 0x0100, 150]
        );
        advance(&mut cd, 8_958_034, chunk, true); // 50 sectors
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0180, 0x41AA, 0x0100, 200]
        );
        assert_ne!(cd.read16(CD + 8).unwrap() & 16, 0);
        q(&mut cd, 0x4180, [0x41AA, 0x0100, 0, 0, 200], chunk);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0180, 0x41AA, 0x0100, 200]
        );
        // Establish a nondefault range and finite repeat on the original disc.
        cd.cd.mount(Media::iso(&vec![0x69; 2048 * 100]).unwrap());
        issue(&mut cd, [0x1080, 198, 0x0280, 52], chunk);
        issue(&mut cd, [0x0500, 0, 0, 0], chunk);
        cd.cd.mount(Media::iso(&vec![0xA6; 2048 * 50]).unwrap());
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0180, 0x4101, 0x0100, 150]
        );
        issue(&mut cd, [0x1180, 198, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0380, 0x4101, 0x0100, 198]
        );
        advance(&mut cd, 358_322, chunk, true); // repeat at the shorter disc end
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0381, 0x4101, 0x0100, 198]
        );
        issue(&mut cd, [0x0500, 0, 0, 0], chunk);
        cd.cd.mount(Media::iso(&vec![0x69; 2048 * 120]).unwrap());
        issue(&mut cd, [0x1180, 249, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        assert_eq!(
            issue(&mut cd, [0x10FF, 0xFFFF, 0x7FFF, 0xFFFF], chunk),
            [0x0381, 0x4101, 0x0100, 249]
        );
        advance(&mut cd, 179_161, chunk, true);
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0382, 0x4101, 0x0100, 198]
        );
        advance(&mut cd, 9_316_355, chunk, true); // retained 52-sector range
        assert_eq!(
            issue(&mut cd, [0, 0, 0, 0], chunk),
            [0x0182, 0x4101, 0x0100, 250]
        );
    }
}

#[test]
fn debugger_fifo_longwords_preserve_each_word_and_cr4_publication_protection() {
    for chunk in [1, 3, 17, 10000] {
        let mut cd = mounted();
        issue(&mut cd, [0x1180, 181, 0, 0], chunk);
        advance(&mut cd, 26_875, chunk, true);
        start(&mut cd, [0x2000, 0, 0, 0]);
        advance(&mut cd, 1075, chunk, true);
        for _ in 0..3 {
            assert_eq!(cd.debug_read16(CD + 0x24).unwrap(), 0);
            assert_eq!(cd.debug_read32(DATA).unwrap(), 0x4101_0100);
            assert_eq!(cd.debug_read32(DATA + 0x2000_0000).unwrap(), 0x4101_0100);
        }
        // Previewing CR4 must not release this command-specific payload at the
        // next periodic edge, though SCDQ still rises.
        advance(&mut cd, 447_902, chunk, true);
        assert_eq!(peek(&cd), [0x4180, 5, 0, 0]);
        assert_ne!(cd.read16(CD + 8).unwrap() & 1024, 0);
        assert_eq!(cd.read32(DATA).unwrap(), 0x4101_0100);
        assert_eq!(cd.debug_read32(DATA).unwrap(), 0x001F_0000);
        assert_eq!(cd.read32(DATA).unwrap(), 0x001F_0000);
        assert_eq!(cd.debug_read32(DATA).unwrap(), 0x00B5_FFFF);
        assert_eq!(cd.read16(DATA).unwrap(), 181);
        assert_eq!(cd.debug_read32(DATA).unwrap(), 0xFFFF_FFFF);
        assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0], chunk)[1], 5);
        advance(&mut cd, 447_902, chunk, true);
        assert_eq!(peek(&cd), [0x2180, 0x4101, 0x0100, 181]);
    }
}
