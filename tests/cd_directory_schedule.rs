//! ERROR report bytes follow ST-162 §7.2.4(4), later §7.2.5(4); retained
//! acquisition/retry/sector/workspace assertions are unchanged (Issue #45).
//! Original scheduled directory vectors: ST-162 §§3.2/6.2/8.2.8.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cd::{CdBlock, DATA, REGISTERS as CD},
    media::Media,
};
const ISO: &[u8] = include_bytes!("fixtures/cd-directory-schedule.iso");
fn issue(cd: &mut CdBlock, words: [u16; 4]) -> [u16; 4] {
    for (i, w) in words.into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
    }
    cd.advance(1075);
    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}
fn mount() -> CdBlock {
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(ISO).unwrap());
    cd.write16(CD + 8, 0).unwrap();
    cd
}
fn advance(cd: &mut CdBlock, mut n: u32, chunk: u32) {
    while n > 0 {
        let step = n.min(chunk);
        cd.advance(step);
        n -= step;
    }
}
fn free(cd: &mut CdBlock) -> u16 {
    issue(cd, [0x5000, 0, 0, 0])[1]
}
fn table(cd: &mut CdBlock) -> bool {
    issue(cd, [0x7200, 0, 0, 0])[0] != 0xFF00
}
fn flags(cd: &CdBlock) -> u16 {
    cd.read16(CD + 8).unwrap() & 512
}
fn start(cd: &mut CdBlock) {
    cd.write16(CD + 8, !512).unwrap();
    assert_eq!(issue(cd, [0x7000, 0, 0xFF, 0xFFFF])[0], 0x0180);
    assert_eq!(flags(cd), 0);
}
#[test]
fn literal_sector_trace_and_table_publication_are_chunk_and_speed_invariant() {
    for (speed, deadlines) in [
        (2, [179161, 358322, 537482, 716643, 895804]),
        (1, [358322, 716643, 1074964, 1433286, 1791607]),
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut cd = mount();
            issue(
                &mut cd,
                [if speed == 1 { 0x0410 } else { 0x0400 }, 0xFFFF, 0, 0],
            );
            start(&mut cd);
            let mut elapsed = 0;
            let mut trace = Vec::new();
            for (i, deadline) in deadlines.into_iter().enumerate() {
                advance(&mut cd, deadline - elapsed - 1, chunk);
                assert_eq!(flags(&cd), 0);
                cd.advance(1);
                let f = flags(&cd);
                let r = issue(&mut cd, [0, 0, 0, 0]);
                let scope = issue(&mut cd, [0x7200, 0, 0, 0]);
                trace.push((r[0], r[3], f, scope[0] != 0xFF00));
                if i == 4 {
                    assert_eq!(scope[1..], [3, 0x100, 2]);
                }
                elapsed = deadline + 2150;
            }
            assert_eq!(
                trace,
                [
                    (0x0380, 167, 0, false),
                    (0x0400, 170, 0, false),
                    (0x0380, 171, 0, false),
                    (0x0380, 172, 0, false),
                    (0x0180, 173, 512, true)
                ]
            );
            assert_eq!(free(&mut cd), 200);
            assert_ne!(issue(&mut cd, [0x7000, 0, 0, 2])[0], 0xFF00);
            assert!(!table(&mut cd));
            cd.advance(400_000);
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1], 1);
            assert_ne!(issue(&mut cd, [0x7000, 0, 0, 1])[0], 0xFF00);
            cd.advance(1_100_000);
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1], 3);
        }
    }
}
#[test]
fn acquired_raw_sectors_use_correction_retry_deadlines_and_keep_accepted_response() {
    let media = Media::iso(ISO).unwrap();
    for sector in [16, 21] {
        for (damage, ecc, retry, success) in [
            (0, 0, 0, true),
            (1, 0, 0, true),
            (1, 0x80, 2, false),
            (1, 0x80, 0x40, true),
            (2, 0, 2, false),
        ] {
            let mut raw = Vec::new();
            for fad in 150..182 {
                raw.extend(media.raw_sector(fad).unwrap());
            }
            if damage == 1 {
                raw[sector * 2352 + 16 + 100] ^= 0x5A;
            }
            if damage == 2 {
                raw[sector * 2352 + 16..sector * 2352 + 2064].fill(0xB7);
            }
            let mut cd = CdBlock::default();
            cd.mount(
                Media::cue(
                    "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
                    &[("d", &raw)],
                )
                .unwrap(),
            );
            issue(&mut cd, [0x0400, 0xFFFF, 0, ecc << 8 | retry]);
            cd.write16(CD + 8, !512).unwrap();
            // Leave CR4 unread, retaining command response throughout completion.
            for (i, w) in [0x7000, 0, 0xFF, 0xFFFF].into_iter().enumerate() {
                cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
            }
            cd.advance(1075);
            assert_eq!(cd.read16(CD + 0x18).unwrap(), 0x0180);
            assert_eq!(flags(&cd), 0);
            let attempt = if sector == 16 { 1 } else { 4 };
            cd.advance((26_874_100u64 * attempt).div_ceil(150) as u32);
            if !success {
                assert_eq!(flags(&cd), 0); // two retries have yet to expire
                // Query RETRY without releasing the response latch at CR4.
                for (i, w) in [0, 0, 0, 0].into_iter().enumerate() {
                    cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
                }
                cd.advance(1075);
                assert_eq!(cd.read16(CD + 0x18).unwrap(), 0x0880);
                let first = (26_874_100u64 * attempt).div_ceil(150) as u32;
                let terminal = (26_874_100u64 * (attempt + 2)).div_ceil(150) as u32;
                cd.advance(terminal - first - 1075 - 1);
                assert_eq!(flags(&cd), 0); // second failed read still owns workspace
                cd.advance(1);
                assert_eq!(flags(&cd), 512); // third failed read exhausts exactly two retries
            }
            cd.advance(1_000_000);
            assert_ne!(flags(&cd), 0);
            assert_eq!(
                cd.read16(CD + 0x18).unwrap(),
                if success { 0x0180 } else { 0x0880 }
            );
            assert_eq!(
                issue(&mut cd, [0, 0, 0, 0])[0],
                if success { 0x0180 } else { 0x09FF }
            );
            assert_eq!(table(&mut cd), success);
            assert_eq!(free(&mut cd), 200);
        }
    }
}
#[test]
fn unreadable_root_acquisition_finishes_at_first_deadline_and_releases_workspace() {
    let audio = vec![0; 32 * 2352];
    let media = [
        // PVD FAD166 is at leadout; no data-track flag is reported there.
        (Media::iso(&[0; 16 * 2048]).unwrap(), 0x0180, 179_161),
        (
            Media::cue(
                "FILE \"a\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
                &[("a", &audio)],
            )
            .unwrap(),
            0x0100,
            358_322, // Audio tracks always run at 1×, including with CdInit 2×.
        ),
    ];
    for (media, response, double_speed_deadline) in media {
        for (speed, deadline) in [(1, 358_322), (2, double_speed_deadline)] {
            for chunk in [1, 3, 17, 10000] {
                let mut cd = CdBlock::default();
                cd.mount(media.clone());
                issue(
                    &mut cd,
                    [if speed == 1 { 0x0410 } else { 0x0400 }, 0xFFFF, 0, 0],
                );
                cd.write16(CD + 8, !512).unwrap();
                for (i, w) in [0x7000, 0, 0xFF, 0xFFFF].into_iter().enumerate() {
                    cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
                }
                cd.advance(1075);
                // Keep CR4 unread so asynchronous completion must preserve the
                // accepted response, independently of the current drive status.
                let retained = |cd: &CdBlock| -> [u16; 3] {
                    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
                };
                let accepted = retained(&cd);
                assert_eq!(accepted[0], response);
                assert_eq!(flags(&cd), 0);
                advance(&mut cd, deadline - 1, chunk);
                assert_eq!(flags(&cd), 0);
                assert_eq!(retained(&cd), accepted);
                cd.advance(1);
                assert_eq!(flags(&cd), 512);
                assert_eq!(retained(&cd), accepted);
                advance(&mut cd, 1_000_000, chunk);
                assert_eq!(retained(&cd), accepted);
                cd.read16(CD + 0x24).unwrap();
                assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0], 0x09FF);
                assert!(!table(&mut cd));
                assert_eq!(free(&mut cd), 200);
                cd.write16(CD + 8, !512).unwrap();
                cd.advance(1_000_000);
                assert_eq!(flags(&cd), 0); // no stale completion
                assert!(!table(&mut cd));
                assert_eq!(free(&mut cd), 200);
                // A new command is accepted rather than stranded in WAIT.
                assert_eq!(issue(&mut cd, [0x7000, 0, 0xFF, 0xFFFF])[0], 0x09FF);
                assert_eq!(free(&mut cd), 199);
            }
        }
    }
}
#[test]
fn directory_after_audio_playback_fails_at_its_scheduled_read_and_releases_workspace() {
    let mut audio = vec![0; 32 * 2352];
    for frame in audio.as_chunks_mut::<4>().0 {
        frame[..2].copy_from_slice(&1234i16.to_le_bytes());
        frame[2..].copy_from_slice(&(-1234i16).to_le_bytes());
    }
    let media = Media::cue(
        "FILE \"a\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
        &[("a", &audio)],
    )
    .unwrap();
    for speed in [1, 2] {
        for chunk in [1, 3, 17, 10000] {
            let mut cd = CdBlock::default();
            cd.mount(media.clone());
            issue(
                &mut cd,
                [if speed == 1 { 0x0410 } else { 0x0400 }, 0xFFFF, 0, 0],
            );
            assert_eq!(issue(&mut cd, [0x1080, 150, 0, 0])[0], 0x0400);
            advance(&mut cd, 26_875, chunk);
            for _ in 0..570 {
                assert_eq!(cd.audio_sample(), [1234, -1234]);
                advance(&mut cd, 610, chunk);
            }
            cd.write16(CD + 8, !512).unwrap();
            for (i, w) in [0x7000, 0, 0xFF, 0xFFFF].into_iter().enumerate() {
                cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
            }
            cd.advance(1075);
            // Retain the accepted response while SCSP continues sampling. The
            // previous sector's 570-frame cursor must not skip required FAD166.
            let accepted: [u16; 3] =
                std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap());
            assert_eq!(accepted[0], 0x0300);
            assert_eq!(flags(&cd), 0);
            let mut elapsed = 0;
            // Audio still uses a 1× deadline even when CdInit selects 2×.
            while elapsed < 358_321 {
                cd.audio_sample();
                let step = 610.min(358_321 - elapsed);
                advance(&mut cd, step, chunk);
                elapsed += step;
                assert_eq!(flags(&cd), 0);
            }
            cd.advance(1);
            assert_eq!(flags(&cd), 512);
            for (i, word) in accepted.into_iter().enumerate() {
                assert_eq!(cd.read16(CD + 0x18 + i as u32 * 4).unwrap(), word);
            }
            cd.read16(CD + 0x24).unwrap();
            let status = issue(&mut cd, [0, 0, 0, 0]);
            assert_eq!((status[0], status[3]), (0x09FF, 0xFFFF));
            assert_eq!(cd.audio_sample(), [0, 0]);
            assert!(!table(&mut cd));
            assert_eq!(free(&mut cd), 200);
            cd.write16(CD + 8, !512).unwrap();
            for _ in 0..10_000 {
                assert_eq!(cd.audio_sample(), [0, 0]);
                advance(&mut cd, 610, chunk);
            }
            assert_eq!(flags(&cd), 0);
            assert!(!table(&mut cd));
            assert_eq!(free(&mut cd), 200);
            assert_eq!(issue(&mut cd, [0x7000, 0, 0xFF, 0xFFFF])[0], 0x09FF);
            assert_eq!(free(&mut cd), 199); // accepted again, rather than WAIT
        }
    }
}
#[test]
fn concurrent_audio_play_preserves_pending_directory_until_its_required_sector_resumes() {
    let mut audio = vec![0; 4 * 2352];
    for frame in audio.as_chunks_mut::<4>().0 {
        frame[..2].copy_from_slice(&1234i16.to_le_bytes());
        frame[2..].copy_from_slice(&(-1234i16).to_le_bytes());
    }
    let media = Media::cue(
        "FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\n\
         FILE \"a\" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00",
        &[("d", ISO), ("a", &audio)],
    )
    .unwrap();
    // Interrupt before the first descriptor or between directory sectors.
    for (elapsed, required_fad) in [(0, 166), (537_600, 171)] {
        for chunk in [1, 3, 17, 10000] {
            for directory in [false, true] {
                let mut cd = CdBlock::default();
                cd.mount(media.clone());
                cd.write16(CD + 8, 0).unwrap();
                if directory {
                    start(&mut cd);
                    advance(&mut cd, elapsed, chunk);
                }
                assert_eq!(issue(&mut cd, [0x1080, 182, 0, 0])[0], 0x0400);
                advance(&mut cd, 400_000, chunk);
                assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0], 0x0300);
                assert_eq!(cd.read16(CD + 8).unwrap() & 0x210, 0); // EFLS/PEND
                assert!(!table(&mut cd));
                assert_eq!(free(&mut cd), if directory { 199 } else { 200 });
                for sector in 0..4 {
                    for _ in 0..588 {
                        assert_eq!(cd.audio_sample(), [1234, -1234]);
                        advance(&mut cd, 610, chunk);
                    }
                    assert_eq!(flags(&cd), 0);
                    assert_eq!(
                        issue(&mut cd, [0, 0, 0, 0])[0],
                        if sector == 3 { 0x0100 } else { 0x0300 }
                    );
                }
                assert_eq!(cd.audio_sample(), [0, 0]);
                assert_eq!(cd.read16(CD + 8).unwrap() & 0x210, 0x10);
                assert!(!table(&mut cd));
                assert_eq!(free(&mut cd), if directory { 199 } else { 200 });
                if directory {
                    cd.write16(CD + 8, !0x210).unwrap();
                    issue(&mut cd, [0x1080, required_fad, 0, 0]);
                    advance(&mut cd, 1_000_000, chunk);
                    assert_eq!(flags(&cd), 512);
                    assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1..], [3, 0x100, 2]);
                    assert_eq!(free(&mut cd), 200);
                }
            }
        }
    }
}
#[test]
fn bounded_play_pauses_at_each_directory_sector_and_keeps_unfinished_work_resumable() {
    for deadlines in [
        [179_161, 358_322, 537_482, 716_643, 895_804],
        [358_322, 716_643, 1_074_964, 1_433_286, 1_791_607],
    ] {
        for chunk in [1, 3, 17, 10000] {
            for (i, (required, next)) in
                [(166, 167), (167, 170), (170, 171), (171, 172), (172, 173)]
                    .into_iter()
                    .enumerate()
            {
                let mut cd = mount();
                issue(
                    &mut cd,
                    [
                        if deadlines[0] == 179_161 {
                            0x0400
                        } else {
                            0x0410
                        },
                        0xFFFF,
                        0,
                        0,
                    ],
                );
                start(&mut cd);
                let before = if i == 0 { 0 } else { deadlines[i - 1] };
                advance(&mut cd, before, chunk);
                // Retain Play's response across its range end and later ticks.
                for (j, word) in [0x1080, required, 0x0080, 1].into_iter().enumerate() {
                    cd.write16(CD + 0x18 + j as u32 * 4, word).unwrap();
                }
                cd.advance(1075);
                assert_eq!(cd.read16(CD + 0x18).unwrap(), 0x0400);
                advance(&mut cd, deadlines[i] - before - 1075 - 1, chunk);
                assert_eq!(cd.read16(CD + 8).unwrap() & 0x210, 0);
                cd.advance(1);
                let completed = i == 4;
                assert_eq!(
                    cd.read16(CD + 8).unwrap() & 0x210,
                    if completed { 0x210 } else { 0x10 }
                );
                advance(&mut cd, 1_000_000, chunk);
                assert_eq!(cd.read16(CD + 0x18).unwrap(), 0x0400);
                cd.read16(CD + 0x24).unwrap();
                let status = issue(&mut cd, [0, 0, 0, 0]);
                assert_eq!((status[0], status[3]), (0x0180, next));
                assert_eq!(table(&mut cd), completed);
                assert_eq!(free(&mut cd), if completed { 200 } else { 199 });
                cd.write16(CD + 8, !0x210).unwrap();
                if !completed {
                    issue(&mut cd, [0x1080, next, 0, 0]);
                    advance(&mut cd, 2_000_000, chunk);
                    assert_eq!(flags(&cd), 512);
                    assert_eq!(free(&mut cd), 200);
                }
                assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1..], [3, 0x100, 2]);
            }
        }
    }
}
#[test]
fn bounded_play_repeats_normally_while_directory_waits_for_the_next_sector() {
    for repeat in [1, 15] {
        for chunk in [1, 3, 17, 10000] {
            let mut cd = mount();
            start(&mut cd);
            issue(&mut cd, [0x1080, 166, (repeat << 8) | 0x80, 1]);
            advance(&mut cd, 179_161 - 1075, chunk);
            let status = issue(&mut cd, [0, 0, 0, 0]);
            assert_eq!((status[0], status[3]), (0x0381, 166));
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x210, 0);
            assert!(!table(&mut cd));
            assert_eq!(free(&mut cd), 199);
            advance(&mut cd, 179_161 - 3 * 1075, chunk);
            let status = issue(&mut cd, [0, 0, 0, 0]);
            assert_eq!(
                (status[0], status[3], cd.read16(CD + 8).unwrap() & 0x210),
                if repeat == 1 {
                    (0x0181, 167, 0x10)
                } else {
                    (0x0382, 166, 0)
                }
            );
            assert!(!table(&mut cd));
            assert_eq!(free(&mut cd), 198); // workspace and normally routed repeat
            issue(&mut cd, [0x1080, 167, 0, 0]);
            advance(&mut cd, 1_000_000, chunk);
            assert_eq!(flags(&cd), 512);
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1..], [3, 0x100, 2]);
            assert_eq!(free(&mut cd), 199); // repeated payload remains stored
        }
    }
}
#[test]
fn workspace_shares_pool_with_host_reservations_and_never_overallocates() {
    for count in [199, 200] {
        let mut cd = mount();
        issue(&mut cd, [0x6400, 0, 0x0400, count]);
        cd.write16(DATA, 0x1357).unwrap();
        let r = issue(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]);
        assert_eq!(r[0] & 0x8000, if count == 200 { 0x8000 } else { 0 });
        assert_eq!(free(&mut cd), 0);
        issue(&mut cd, [0x0600, 0, 0, 0]);
        assert_eq!(free(&mut cd), 0);
        if count == 200 {
            assert_ne!(issue(&mut cd, [0x7000, 0, 0xFF, 0xFFFF])[0] & 0x8000, 0);
        }
        assert_ne!(issue(&mut cd, [0x6400, 0, 0x0500, 1])[0] & 0x8000, 0);
        cd.advance(1_000_000);
        assert_eq!(table(&mut cd), count == 199);
        assert_eq!(free(&mut cd), 200 - count);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0400, 0])[3], count);
        issue(&mut cd, [0x6100, 0, 0x0400, 1]);
        assert_eq!(cd.read16(DATA).unwrap(), 0x1357);
    }
}
#[test]
fn every_cancellation_boundary_releases_workspace_and_prevents_stale_completion() {
    for boundary in [0u32, 537_600, 895_803] {
        for cancel in 0..5 {
            let mut cd = mount();
            issue(&mut cd, [0x6400, 0, 0x0400, 1]);
            cd.write16(DATA, 0x2468).unwrap();
            issue(&mut cd, [0x0600, 0, 0, 0]);
            start(&mut cd);
            cd.advance(if boundary == 895_803 && cancel < 3 {
                boundary - 1075
            } else {
                boundary
            });
            assert_eq!(flags(&cd), 0);
            match cancel {
                0 => {
                    issue(&mut cd, [0x7500, 0, 0, 0]);
                }
                1 => {
                    issue(&mut cd, [0x0500, 0, 0, 0]);
                }
                2 => {
                    issue(&mut cd, [0x0401, 0, 0, 0]);
                }
                3 => {
                    cd.set_enabled(false);
                    cd.set_enabled(true);
                }
                _ => cd.mount(Media::iso(ISO).unwrap()),
            }
            cd.write16(CD + 8, !512).unwrap();
            cd.advance(2_000_000);
            assert_eq!(flags(&cd), 0);
            assert!(!table(&mut cd));
            assert_eq!(
                free(&mut cd),
                if matches!(cancel, 2 | 3) { 200 } else { 199 }
            );
            if !matches!(cancel, 2 | 3) {
                issue(&mut cd, [0x6100, 0, 0x0400, 1]);
                assert_eq!(cd.read16(DATA).unwrap(), 0x2468);
                issue(&mut cd, [0x0600, 0, 0, 0]);
            }
            if cancel == 0 {
                assert_eq!(issue(&mut cd, [0x4500, 0, 0, 0])[0] & 0x7F, 0x40);
                assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0], 0x0180);
            }
        }
    }
}
#[test]
fn drive_pause_obstructs_directory_while_independent_host_copy_and_status_run() {
    let mut cd = mount();
    issue(&mut cd, [0x6400, 0, 0x0400, 1]);
    cd.write16(DATA, 0xCAFE).unwrap();
    issue(&mut cd, [0x0600, 0, 0, 0]);
    start(&mut cd);
    assert_ne!(issue(&mut cd, [0x7100, 0, 0, 2])[0] & 0x8000, 0);
    assert_eq!(free(&mut cd), 198); // stored canary plus one workspace
    cd.write16(CD + 8, !0x380).unwrap();
    let event = |cd: &CdBlock| cd.read16(CD + 8).unwrap() & 0x380;
    let mut trace = vec![event(&cd)];
    issue(&mut cd, [0x65_05, 0, 0x0400, 1]);
    trace.push(event(&cd));
    issue(&mut cd, [0x6100, 0, 0x0400, 1]);
    trace.push(event(&cd));
    cd.advance(1277); // copy deadline2352 after its accepted response plus GET1075
    trace.push(event(&cd));
    assert_eq!(cd.read16(DATA).unwrap(), 0xCAFE);
    issue(&mut cd, [0x11FF, 0xFFFF, 0, 0]);
    trace.push(event(&cd));
    cd.advance(1_000_000);
    assert_eq!(flags(&cd), 0);
    assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0], 0x0180);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0500, 0])[3], 1);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    trace.push(event(&cd));
    assert_eq!(trace, [0, 0, 0, 0x100, 0x100, 0x180]);
    issue(&mut cd, [0x61_00, 0, 0x0500, 1]);
    assert_eq!(cd.read16(DATA).unwrap(), 0xCAFE);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x10FF, 0xFFFF, 0, 0]);
    cd.advance(1_000_000);
    assert!(table(&mut cd));
    // Invalid parameters reject without destroying an existing table.
    assert_eq!(issue(&mut cd, [0x7000, 0, 0x1800, 0])[0], 0xFF00);
    assert!(table(&mut cd));
    assert_eq!(issue(&mut cd, [0x7000, 0, 0xFF, 0xFFFE])[0], 0xFF00);
    assert!(table(&mut cd));
}
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-directory-schedule.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
#[test]
fn guest_measures_directory_progress_receives_scu_irq_and_controls_navigation_and_abort() {
    for (fid, command, abort) in [
        (2u16, 0x7000u16, 0u16),
        (0, 0x7000, 0),
        (0, 0x7000, 1),
        (2, 0, 0),
    ] {
        let mut rom = rom();
        for (at, value) in [(0x100, fid), (0x102, command), (0x104, abort)] {
            rom[at..at + 2].copy_from_slice(&value.to_be_bytes());
        }
        for _ in 0..2 {
            let mut m = Machine::new();
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.mount_media(Media::iso(ISO).unwrap());
            for _ in 0..100 {
                m.run_steps(10000).unwrap();
                if m.bus().read32(0x0600C014) == Ok(0x600D) {
                    break;
                }
            }
            let read = |at: u32| m.bus().read32(0x0600C000 + at).unwrap();
            assert_eq!(m.cpu_state().exceptions, if command == 0 { 0 } else { 1 });
            if command == 0 {
                assert_eq!(read(0x14), 0);
                continue;
            }
            assert_eq!(read(0), 0); // inherited synchronous implementation fails here
            assert!((6900..7200).contains(&read(4)), "FRT elapsed={}", read(4));
            assert_eq!(read(8), 0x10000); // masked SCU completion was latched
            assert_eq!(read(0x1C), 0); // CD-masked EFLS did not reach SCU
            assert_eq!(read(0x18), 0); // guest handler acknowledged HIRQ
            assert_eq!(read(12), 1); // handler executed after unmask and acknowledged CD
            assert_eq!(read(0x24), 3);
            assert_eq!(read(0x14), 0x600D);
            if abort != 0 {
                assert_eq!(read(0x40), 0xFF00);
                assert_eq!(read(0x10), u32::MAX);
            } else {
                assert_eq!(read(0x44), if fid == 2 { 1 } else { 3 });
                assert_eq!(read(0x10), if fid == 2 { 178 } else { 174 });
            }
        }
    }
}

#[test]
fn infinite_directory_retry_is_bounded_per_tick_and_abort_releases_workspace() {
    for chunk in [1, 3, 17, 10000] {
        let media = Media::iso(ISO).unwrap();
        let mut raw = Vec::new();
        for fad in 150..182 {
            raw.extend(media.raw_sector(fad).unwrap());
        }
        raw[16 * 2352 + 100] ^= 0x5A;
        let mut cd = CdBlock::default();
        cd.mount(
            Media::cue(
                "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
                &[("d", &raw)],
            )
            .unwrap(),
        );
        issue(&mut cd, [0x0400, 0xFFFF, 0, 0x8080]);
        start(&mut cd);
        advance(&mut cd, 2_000_000, chunk);
        let retry = issue(&mut cd, [0, 0, 0, 0]);
        assert_eq!((retry[0], retry[3]), (0x0880, 166));
        assert_eq!(flags(&cd), 0);
        assert_eq!(free(&mut cd), 199);
        assert!(!table(&mut cd));
        issue(&mut cd, [0x7500, 0, 0, 0]);
        assert_eq!(flags(&cd), 512);
        assert_eq!(free(&mut cd), 200);
        cd.write16(CD + 8, !512).unwrap();
        advance(&mut cd, 2_000_000, chunk);
        assert_eq!(flags(&cd), 0);
        assert!(!table(&mut cd));
        assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0], 0x0180);
    }
}
