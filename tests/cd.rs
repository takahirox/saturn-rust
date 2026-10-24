//! ST-162-062094 §§3.1/3.4/5.3/5.4/7.2/8.2. Independent register/sector vectors.
use saturn_rust::{
    bus::{Bus, HIGH_WRAM, Memory},
    cd::{CdBlock, DATA, REGISTERS as CD},
    media::Media,
    sound::{SCSP, Sound},
};
fn issue(cd: &mut CdBlock, cmd: [u16; 4]) -> [u16; 4] {
    cd.write16(CD + 8, !1).unwrap();
    for (i, w) in cmd.into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
    }
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 0);
    cd.advance(1074);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 0);
    cd.advance(1);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 1);
    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}
// ST-162 §§3.2/6.2: acceptance (CMOK) precedes acquisition (EFLS).
// Preserve the malformed/extent corpus while observing the asynchronous result.
fn directory(cd: &mut CdBlock, cmd: [u16; 4]) -> bool {
    directory_with_chunk(cd, cmd, 400_000)
}
fn directory_with_chunk(cd: &mut CdBlock, cmd: [u16; 4], chunk: u32) -> bool {
    cd.write16(CD + 8, !512).unwrap();
    let response = issue(cd, cmd);
    if response[0] == 0xFF00 {
        return false;
    }
    assert_eq!(response[0] & 0x8000, 0);
    for _ in 0..1024 {
        if cd.read16(CD + 8).unwrap() & 512 != 0 {
            break;
        }
        let mut remaining = 400_000;
        while remaining != 0 {
            let n = remaining.min(chunk);
            cd.advance(n);
            remaining -= n;
        }
    }
    assert_ne!(cd.read16(CD + 8).unwrap() & 512, 0);
    issue(cd, [0x7200, 0, 0, 0])[0] != 0xFF00
}
fn read(cd: &CdBlock, n: usize) -> Vec<u8> {
    (0..n / 2)
        .flat_map(|_| cd.read16(DATA).unwrap().to_be_bytes())
        .collect()
}
fn mounted() -> (CdBlock, Vec<u8>) {
    let bytes: Vec<_> = (0..2048 * 8).map(|i| (i ^ (i >> 8)) as u8).collect();
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(&bytes).unwrap());
    (cd, bytes)
}
#[test]
fn cue_indices_are_used_by_track_index_seek_status_and_q_subcode() {
    let audio = vec![0; saturn_rust::media::RAW_SECTOR * 8];
    let cue = "FILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nINDEX 02 00:00:02\nINDEX 03 00:00:04\n";
    let mut cd = CdBlock::default();
    cd.mount(Media::cue(cue, &[("a.bin", &audio)]).unwrap());
    // ST-162 §§7.2.4/8.2.2 track/index addressing: 000103 is track 1, index 3.
    let response = issue(&mut cd, [0x1100, 0x0103, 0, 0]);
    assert_ne!(response[0], 0xFF00);
    cd.advance(26_875);
    let response = issue(&mut cd, [0, 0, 0, 0]);
    assert_eq!(response[1], 0x0101);
    assert_eq!(response[2], 0x0300);
    assert_eq!(response[3], 154);
    assert_eq!(issue(&mut cd, [0x2000, 0, 0, 0])[1], 5);
    assert_eq!(read(&cd, 10), [1, 1, 3, 0, 0, 4, 0, 0, 0, 154]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    // Missing indices reject rather than silently seeking to INDEX 01.
    assert_eq!(issue(&mut cd, [0x1100, 0x0104, 0, 0])[0], 0xFF00);
    issue(&mut cd, [0x1180, 152, 0, 0]);
    cd.advance(26_875);
    let response = issue(&mut cd, [0, 0, 0, 0]);
    assert_eq!(response[2], 0x0200);
    assert_eq!(response[3], 152);
}
fn sectors(cd: &mut CdBlock, n: u16) {
    assert_ne!(issue(cd, [0x3000, 0, 0, 0])[0] >> 8, 255);
    assert_ne!(issue(cd, [0x1080, 150, 0x0080, n])[0] >> 8, 255);
    cd.advance(268_742 * n as u32);
}
#[test]
fn command_latency_hirq_mask_write_zero_clear_and_periodic_response_latch() {
    let (mut cd, _) = mounted();
    cd.write16(CD + 8, 0).unwrap();
    cd.write16(CD + 12, 1).unwrap();
    assert!(!cd.interrupt());
    let r = issue(&mut cd, [0x0100, 0, 0, 0]);
    assert_eq!(r[1], 2);
    assert!(cd.interrupt());
    cd.write16(CD + 8, !1).unwrap();
    assert!(!cd.interrupt());
    for (i, w) in [0x0100, 0, 0, 0].into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, w).unwrap();
    }
    cd.advance(500_000);
    assert_eq!(cd.read16(CD + 0x1C).unwrap(), 2);
    cd.read16(CD + 0x24).unwrap();
    cd.advance(500_000);
    assert_ne!(cd.read16(CD + 0x18).unwrap() & 0x2000, 0);
    assert_eq!(issue(&mut cd, [0xFF00, 0, 0, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0])[0] & 255, 255);
}
#[test]
fn runtime_sector_get_delete_partial_abort_and_transfer_bounds() {
    let (mut cd, bytes) = mounted();
    sectors(&mut cd, 2);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 2);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(read(&cd, 20), bytes[..20]);
    let r = issue(&mut cd, [0x0600, 0, 0, 0]);
    assert_eq!(r[1], 10);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
    issue(&mut cd, [0x6100, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), bytes[2048..4096]);
    assert_eq!(cd.read16(DATA).unwrap(), 0xFFFF);
    assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0])[1], 1024);
    issue(&mut cd, [0x6200, 0, 0, 1]);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
    assert_ne!(issue(&mut cd, [0x6100, 0, 0, 1])[0] & 0x8000, 0); // WAIT, not dummy data
}
#[test]
fn selectors_fad_range_connections_copy_move_and_size() {
    let (mut cd, _) = mounted();
    issue(&mut cd, [0x4000, 151, 0, 1]);
    issue(&mut cd, [0x4440, 0, 0, 0]);
    issue(&mut cd, [0x4603, 0x0304, 0, 0]); // true partition 3, false filter 4
    sectors(&mut cd, 3);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0300, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0400, 0])[3], 2);
    let last = issue(&mut cd, [0x3200, 0, 0, 0])[2];
    issue(&mut cd, [0x6505, 0, 0x0300, 1]);
    cd.advance(2352);
    assert_eq!(issue(&mut cd, [0x3200, 0, 0, 0])[2], last);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0500, 0])[3], 1);
    issue(&mut cd, [0x6606, 0, 0x0500, 1]);
    cd.advance(2352);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0500, 0])[3], 0);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0600, 0])[3], 1);
    issue(&mut cd, [0x5200, 0, 0x0400, 0xFFFF]);
    assert_eq!(issue(&mut cd, [0x5300, 0, 0, 0])[1], 2048);
    assert_eq!(issue(&mut cd, [0x5400, 0, 0x0300, 0])[1], 151);
    issue(&mut cd, [0x5500, 0, 0x0400, 151]);
    assert_eq!(issue(&mut cd, [0x5600, 0, 0, 0])[1], 1);
}

fn empty_sectors(cd: &mut CdBlock, filter: u16, count: u16) {
    assert_ne!(issue(cd, [0x6400, 0, filter << 8, count])[0], 0xFF00);
    issue(cd, [0x0600, 0, 0, 0]);
}

#[test]
fn selector_copy_move_deadlines_flags_and_raw_data_cover_every_source_and_target() {
    // ST-162 §§3.2/8.2.7: ECPY ends a separate asynchronous engine;
    // COPY_BUSY=FF, COPY_OK=0. 2352 clocks/sector is the documented model
    // policy, not a measured physical CD-block bandwidth.
    for source in 0..24u16 {
        for target in 0..24u16 {
            for moving in [false, true] {
                let mut cd = CdBlock::default();
                issue(&mut cd, [0x6400, 0, source << 8, 2]);
                for sector in 0..2 {
                    for word in 0..1024 {
                        cd.write16(DATA, (0x1234 + sector * 0x1000) ^ word).unwrap();
                    }
                }
                issue(&mut cd, [0x0600, 0, 0, 0]);
                cd.write16(CD + 8, !0x100).unwrap();
                cd.write16(CD + 12, 0x100).unwrap();
                assert_ne!(
                    issue(
                        &mut cd,
                        [
                            if moving { 0x6600 } else { 0x6500 } | target,
                            0,
                            source << 8,
                            2
                        ]
                    )[0],
                    0xFF00
                );
                assert!(cd.active());
                assert!(!cd.interrupt());
                assert_eq!(cd.clocks_to_event(), 2352);
                assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 255);
                cd.advance(1276);
                assert!(!cd.interrupt()); // one clock before the first sector
                cd.advance(1);
                assert!(!cd.interrupt()); // intermediate sector is not ECPY
                cd.advance(2351);
                assert!(!cd.interrupt());
                cd.advance(1);
                assert!(cd.interrupt());
                assert!(!cd.active());
                assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 0);
                let expected_source = if moving && source != target {
                    0
                } else if source == target && !moving {
                    4
                } else {
                    2
                };
                assert_eq!(
                    issue(&mut cd, [0x5100, 0, source << 8, 0])[3],
                    expected_source
                );
                let count = if source == target && !moving { 4 } else { 2 };
                assert_eq!(issue(&mut cd, [0x5100, 0, target << 8, 0])[3], count);
                // Copy is always raw2352 regardless of the host GET length.
                issue(&mut cd, [0x6003, 0x0300, 0, 0]);
                issue(
                    &mut cd,
                    [0x6100, if count == 4 { 2 } else { 0 }, target << 8, 2],
                );
                let actual = read(&cd, 2352 * 2);
                let mut expected = vec![0; 2352 * 2];
                for sector in 0..2 {
                    for word in 0..1024 {
                        let value = (0x1234u16 + sector as u16 * 0x1000) ^ word as u16;
                        let at = sector * 2352 + 24 + word * 2;
                        expected[at..at + 2].copy_from_slice(&value.to_be_bytes());
                    }
                }
                assert_eq!(
                    actual, expected,
                    "source{source} target{target} moving{moving}"
                );
            }
        }
    }
}

#[test]
fn copy_capacity_wait_and_mid_operation_failure_preserve_completed_and_unmoved_sectors() {
    // ST-162 §8.2.7 functions7.6–7.8: insufficient space at issue is
    // WAIT; another device consuming space after acceptance is COPY_NG.
    let mut cd = CdBlock::default();
    empty_sectors(&mut cd, 0, 200);
    cd.write16(CD + 8, !0x100).unwrap();
    assert_ne!(issue(&mut cd, [0x6501, 0, 0, 1])[0] & 0x8000, 0);
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0);
    assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 0);
    // Move requires no free sector and must work at 200/200 capacity.
    issue(&mut cd, [0x6601, 0xFFFF, 0, 0xFFFF]);
    cd.advance(2352);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 199);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 0);

    let mut cd = CdBlock::default();
    empty_sectors(&mut cd, 0, 3);
    empty_sectors(&mut cd, 2, 193);
    cd.write16(CD + 8, !0x100).unwrap();
    issue(&mut cd, [0x6501, 0, 0, 3]);
    cd.advance(2352); // one published copy; total197
    // EHST and ECPY are independent. A host PUT reserves all remaining
    // capacity; the next copy fails and must retain its completed prefix.
    assert_ne!(issue(&mut cd, [0x6400, 0, 0x0300, 3])[0], 0xFF00);
    cd.advance(1276);
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0);
    cd.advance(1);
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0x100);
    // Queries remain possible while the host transfer is open.
    assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 1);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 3);
    assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 0);
    assert_ne!(issue(&mut cd, [0x6501, 0, 0, 1])[0] & 0x8000, 0);
    assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 1); // WAIT retains error
    issue(&mut cd, [0x0600, 0, 0, 0]);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0300, 0])[3], 3);
    assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 0);
}

#[test]
fn drive_sector_wins_the_last_free_slot_at_a_simultaneous_copy_deadline() {
    // Reference-clock ordering policy: drive -> selector -> host command.
    // 26,874,100/150 gives the first drive boundary at clock179161.
    // Every setup command consumes1075 clocks; all media here are data tracks.
    for chunk in [1, 7, 127, 100_000] {
        let (mut cd, bytes) = mounted();
        empty_sectors(&mut cd, 0, 2);
        empty_sectors(&mut cd, 2, 197);
        issue(&mut cd, [0x3000, 0, 0x0200, 0]);
        issue(&mut cd, [0x1080, 150, 0x0080, 1]);
        let mut left = 179_161 - 6 * 1075 - 2352 - 1075;
        while left != 0 {
            let n = left.min(chunk);
            cd.advance(n);
            left -= n;
        }
        cd.write16(CD + 8, !0x100).unwrap();
        issue(&mut cd, [0x6501, 0, 0, 1]);
        cd.advance(2351);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x104, 0);
        cd.advance(1);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x104, 0x104); // CSCT and ECPY
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 1);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 0);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0200, 0])[3], 198);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 2);
        assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 0);
        issue(&mut cd, [0x6100, 197, 0x0200, 1]);
        assert_eq!(read(&cd, 2048), bytes[..2048]);
    }
}

#[test]
fn selector_detachment_is_a_partial_copy_move_error_and_a_new_job_clears_it() {
    for moving in [false, true] {
        let mut cd = CdBlock::default();
        empty_sectors(&mut cd, 0, 4);
        cd.write16(CD + 8, !0x100).unwrap();
        issue(&mut cd, [if moving { 0x6601 } else { 0x6501 }, 0, 0, 4]);
        cd.advance(2352);
        // Reconfigure the target through the independent ESEL engine.
        issue(&mut cd, [0x4601, 0xFF00, 0x0100, 0]);
        cd.advance(1277);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0x100);
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 1);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 1);
        assert_eq!(
            issue(&mut cd, [0x5100, 0, 0, 0])[3],
            if moving { 3 } else { 4 }
        );
        issue(&mut cd, [0x4601, 0x0100, 0x0100, 0]);
        cd.write16(CD + 8, !0x100).unwrap();
        issue(&mut cd, [0x6501, 0xFFFF, 0, 0xFFFF]);
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 255);
        cd.advance(1277);
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 0);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 2);
    }
}

#[test]
fn transfer_classes_run_independently_and_put_retains_its_accepted_sector_length() {
    for size in 0..4 {
        let mut cd = CdBlock::default();
        issue(&mut cd, [0x60FF, size << 8, 0, 0]);
        issue(&mut cd, [0x6400, 0, 0x0200, 1]);
        cd.write16(DATA, 0x5AA5).unwrap();
        // ESEL may change the length for the NEXT transfer, never this PUT.
        issue(&mut cd, [0x60FF, ((size + 1) % 4) << 8, 0, 0]);
        assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 199);
        assert_ne!(issue(&mut cd, [0x6400, 0, 0x0300, 1])[0] & 0x8000, 0);
        assert_ne!(issue(&mut cd, [0x6200, 0, 0x0200, 1])[0] & 0x8000, 0);
        assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0])[1], 1);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0200, 0])[3], 1);
        issue(&mut cd, [0x6000 | size, 0xFF00, 0, 0]);
        issue(&mut cd, [0x6100, 0, 0x0200, 1]);
        let length = [2048, 2336, 2340, 2352][size as usize];
        let bytes = read(&cd, length);
        assert_eq!(&bytes[..2], &[0x5A, 0xA5]);
        assert!(bytes[2..].iter().all(|b| *b == 0));
        issue(&mut cd, [0x0600, 0, 0, 0]);
    }
}

#[test]
fn get_delete_retains_sector_identity_when_an_independent_move_shifts_the_source() {
    let mut cd = CdBlock::default();
    issue(&mut cd, [0x6400, 0, 0, 3]);
    for sector in 1..=3 {
        for _ in 0..1024 {
            cd.write16(DATA, sector).unwrap();
        }
    }
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x6300, 1, 0, 1]); // retain sector2 for GET/DELETE
    assert_eq!(read(&cd, 2), [0, 2]);
    issue(&mut cd, [0x6601, 0, 0, 1]); // move sector1; sector2 moves to index0
    cd.advance(2352);
    assert_eq!(read(&cd, 2), [0, 2]); // unrelated command did not replace FIFO
    issue(&mut cd, [0x0600, 0, 0, 0]); // delete sector2, never its new neighbour
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
    issue(&mut cd, [0x6100, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), [0, 3].repeat(1024));
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x6100, 0, 0x0100, 1]);
    assert_eq!(read(&cd, 2048), [0, 1].repeat(1024));
}

#[test]
fn pending_selector_job_never_replaces_an_accepted_job_and_missing_sources_fail_boundedly() {
    for command in [0x6502, 0x6602] {
        let mut cd = CdBlock::default();
        empty_sectors(&mut cd, 0, 10);
        cd.write16(CD + 8, !0x100).unwrap();
        issue(&mut cd, [0x6501, 0, 0, 10]);
        assert_ne!(issue(&mut cd, [command, 0, 0, 1])[0] & 0x8000, 0);
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 255);
        cd.advance(10 * 2352);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 10);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0200, 0])[3], 0);
        issue(&mut cd, [0x6501, 0, 0, 10]);
        issue(&mut cd, [0x4800, 0, 0, 0]); // ESEL removes source, not unrelated RAM
        cd.advance(2352);
        assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 1);
        cd.advance(1_000_000);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 10);
    }
}

#[test]
fn copy_jobs_cancel_at_reset_boundaries_and_advance_is_chunk_invariant() {
    for chunk in [1, 3, 17, 1075, 2352, 100_000] {
        for elapsed in [0, 1, 2351, 2352, 2353, 7055, 7056] {
            let mut cd = CdBlock::default();
            empty_sectors(&mut cd, 0, 3);
            cd.write16(CD + 8, !0x100).unwrap();
            issue(&mut cd, [0x6501, 0, 0, 3]);
            let mut left = elapsed;
            while left != 0 {
                let n = left.min(chunk);
                cd.advance(n);
                left -= n;
            }
            assert_eq!(
                cd.read16(CD + 8).unwrap() & 0x100,
                if elapsed == 7056 { 0x100 } else { 0 }
            );
            // Reset interrupts even unfinished work and leaves no stale event
            // able to append a sector into newly initialized partitions.
            issue(&mut cd, [0x0401, 0, 0, 0]);
            cd.write16(CD + 8, !0x100).unwrap();
            cd.advance(100_000);
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0);
            assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 0);
            assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 0);
        }
    }
}

#[test]
fn power_and_media_replacement_cancel_an_in_flight_selector_job_without_late_irq() {
    for media_replacement in [false, true] {
        for elapsed in [0, 1, 2351, 2352, 2353, 7055] {
            let (mut cd, _) = mounted();
            empty_sectors(&mut cd, 0, 3);
            cd.write16(CD + 8, !0x100).unwrap();
            issue(&mut cd, [0x6501, 0, 0, 3]);
            cd.advance(elapsed);
            if media_replacement {
                cd.mount(Media::iso(&vec![0x5A; 2048]).unwrap());
            } else {
                cd.set_enabled(false);
                cd.advance(10_000);
                cd.set_enabled(true);
            }
            cd.write16(CD + 8, !0x100).unwrap();
            assert!(!cd.active());
            cd.advance(100_000);
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x100, 0);
            assert_eq!(issue(&mut cd, [0x6700, 0, 0, 0])[0] & 255, 0);
            assert_eq!(
                issue(&mut cd, [0x5100, 0, 0x0100, 0])[3],
                if media_replacement {
                    (elapsed / 2352) as u16
                } else {
                    0
                }
            );
            assert_eq!(
                issue(&mut cd, [0x5100, 0, 0, 0])[3],
                if media_replacement { 3 } else { 0 }
            );
        }
    }
}

#[test]
fn selector_connection_commands_validate_both_outputs_and_exclusive_inputs_atomically() {
    // ST-162 §§5.3/7.2.5 table5.1: 24 selectors, FF=disconnected,
    // one source per filter input, multiple true outputs per partition.
    let (mut cd, _) = mounted();
    assert_ne!(issue(&mut cd, [0x4603, 0x0304, 0, 0])[0], 0xFF00);
    for invalid in [24u16, 25, 127, 254] {
        for args in [(invalid << 8) | 5, 0x0600 | invalid] {
            cd.write16(CD + 8, !0x40).unwrap();
            assert_eq!(issue(&mut cd, [0x4603, args, 0, 0])[0], 0xFF00);
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x40, 0); // no ESEL
            assert_eq!(issue(&mut cd, [0x4700, 0, 0, 0])[1], 0x0304);
        }
        assert_eq!(issue(&mut cd, [0x3000, 0, invalid << 8, 0])[0], 0xFF00);
    }
    // Flags select bytes individually; ignored invalid values cannot reject.
    assert_ne!(issue(&mut cd, [0x4601, 0x07FE, 0, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x4700, 0, 0, 0])[1], 0x0704);
    assert_ne!(issue(&mut cd, [0x4602, 0xFE05, 0, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x4700, 0, 0, 0])[1], 0x0705);
    // Another true output can feed partition 7; another false output or
    // CD device cannot share the already connected filter input 5.
    assert_ne!(issue(&mut cd, [0x4601, 0x0700, 0x0100, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x4603, 0x0805, 0x0100, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x4700, 0, 0x0100, 0])[1], 0x07FF);
    assert_eq!(issue(&mut cd, [0x3000, 0, 0x0500, 0])[0], 0xFF00);
    assert_ne!(issue(&mut cd, [0x3000, 0, 0x0600, 0])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x4602, 0x0006, 0, 0])[0], 0xFF00);
    // Disconnect the old source, connect the new source, and observe actual
    // sector routing through filter 6 to partition 6 rather than just readback.
    assert_ne!(issue(&mut cd, [0x3000, 0, 0xFF00, 0])[0], 0xFF00);
    assert_ne!(issue(&mut cd, [0x4602, 0x0006, 0, 0])[0], 0xFF00);
    assert_ne!(issue(&mut cd, [0x3000, 0, 0, 0])[0], 0xFF00);
    issue(&mut cd, [0x4000, 151, 0, 1]);
    issue(&mut cd, [0x4440, 0, 0, 0]);
    sectors(&mut cd, 2);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0700, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0600, 0])[3], 1);
}
#[test]
fn toc_raw_transfer_lengths_put_reset_and_chunk_invariance() {
    let (mut cd, bytes) = mounted();
    let r = issue(&mut cd, [0x0200, 0, 0, 0]);
    assert_eq!(r[1], 204);
    assert_eq!(&read(&cd, 408)[..4], &[0x41, 0, 0, 150]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    for size in 0..4 {
        issue(&mut cd, [0x6000 | size, 0, 0, 0]);
        sectors(&mut cd, 1);
        issue(&mut cd, [0x6100, 0, 0, 1]);
        let n = [2048, 2336, 2340, 2352][size as usize];
        let data = read(&cd, n);
        let offset = [0, 0, 4, 16][size as usize];
        assert_eq!(&data[offset..offset + 2048], &bytes[..2048]);
        issue(&mut cd, [0x0600, 0, 0, 0]);
        issue(&mut cd, [0x4800, 0, 0, 0]);
    }
    issue(&mut cd, [0x6400, 0, 0x1700, 1]);
    for _ in 0..1024 {
        cd.write16(DATA, 0x1234).unwrap();
    }
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x6000, 0, 0, 0]);
    issue(&mut cd, [0x6100, 0, 0x1700, 1]);
    assert_eq!(read(&cd, 2048), [0x12, 0x34].repeat(1024));
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x0401, 0, 0, 0]);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x1700, 0])[3], 0);
    for chunk in [1, 7, 1000, 400_000] {
        let (mut c, _) = mounted();
        sectors(&mut c, 1);
        let mut left = 800_000;
        while left != 0 {
            let n = left.min(chunk);
            c.advance(n);
            left -= n;
        }
        assert_eq!(issue(&mut c, [0x5100, 0, 0, 0])[3], 1);
    }
}

#[test]
fn early_put_end_publishes_all_reserved_sectors_through_the_selected_filter() {
    // ST-162 §8.2.7 function7.5: early DataEnd writes every designated
    // sector; bytes the host did not transfer are unspecified (zero here).
    // §5.3.3: a host input enters a filter, not its same-numbered partition.
    for (size_code, size) in [2048, 2336, 2340, 2352].into_iter().enumerate() {
        for words in [
            0,
            1,
            size / 2 - 1,
            size / 2,
            size / 2 + 1,
            size,
            size * 3 / 2 - 1,
            size * 3 / 2,
        ] {
            let (mut cd, _) = mounted();
            issue(
                &mut cd,
                [0x6000 | size_code as u16, (size_code as u16) << 8, 0, 0],
            );
            issue(&mut cd, [0x4601, 0x0700, 0x0300, 0]); // filter3 -> partition7
            cd.write16(CD + 8, !0x80).unwrap();
            assert_ne!(issue(&mut cd, [0x6400, 0, 0x0300, 3])[0] & 0x4000, 0);
            let mut expected = vec![0; size * 3];
            for n in 0..words {
                let value = (n as u16).wrapping_mul(0x123).wrapping_add(0x2468);
                cd.write16(DATA + (n as u32 & 1) * 2, value).unwrap();
                expected[n * 2..n * 2 + 2].copy_from_slice(&value.to_be_bytes());
            }
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x80, 0);
            assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0])[1], words as u16);
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x82, 0x80);
            assert_eq!(issue(&mut cd, [0x5100, 0, 0x0300, 0])[3], 0);
            assert_eq!(issue(&mut cd, [0x5100, 0, 0x0700, 0])[3], 3);
            assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 197);
            issue(&mut cd, [0x6300, 0, 0x0700, 3]);
            assert_eq!(read(&cd, size * 3), expected, "size={size}, words={words}");
            issue(&mut cd, [0x0600, 0, 0, 0]);
            assert_eq!(issue(&mut cd, [0x5000, 0, 0, 0])[1], 200);
        }
    }
}

#[test]
fn only_sector_buffer_transfers_raise_host_io_completion() {
    // ST-162 §3.2 table3.2: TOC, Q and file info use DATATRNS but have no
    // EHST command-end flag. The payload size does not determine IRQ type.
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(include_bytes!("../web/star-collector.iso")).unwrap());
    assert!(directory(&mut cd, [0x7000, 0, 0x00FF, 0xFFFF]));
    for cmd in [
        [0x0200, 0, 0, 0],
        [0x2000, 0, 0, 0],
        [0x7300, 0, 0, 2],
        [0x7300, 0, 0xFF, 0xFFFF],
    ] {
        for words in [0, 1] {
            cd.write16(CD + 8, !0x80).unwrap();
            assert_ne!(issue(&mut cd, cmd)[0] & 0x4000, 0);
            read(&cd, words * 2);
            assert_eq!(issue(&mut cd, [0x0600, 0, 0, 0])[1], words as u16);
            assert_eq!(cd.read16(CD + 8).unwrap() & 0x82, 0);
        }
    }
    issue(&mut cd, [0x483F, 0, 0, 0]);
    sectors(&mut cd, 1);
    for cmd in [0x6100, 0x6300] {
        cd.write16(CD + 8, !0x80).unwrap();
        issue(&mut cd, [cmd, 0, 0, 1]);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x82, 2);
        issue(&mut cd, [0x0600, 0, 0, 0]);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x82, 0x80);
    }
}
#[test]
fn cdda_stereo_external_scsp_route_has_exact_samples_and_muting() {
    let iso = vec![0; 2048];
    let mut audio = vec![0; 2352 * 2];
    for (i, frame) in audio.chunks_mut(4).enumerate() {
        frame[..2].copy_from_slice(&((i as i16 % 32) * 128).to_le_bytes());
        frame[2..].copy_from_slice(&(-1024i16).to_le_bytes());
    }
    let media=Media::cue("FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"a\" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00",&[("d",&iso),("a",&audio)]).unwrap();
    let mut cd = CdBlock::default();
    cd.mount(media);
    issue(&mut cd, [0x1080, 151, 0x0080, 2]);
    cd.advance(26875);
    let mut sound = Sound::default();
    sound.scsp.write16(0x400, 15).unwrap();
    sound.scsp.write16(16 * 32 + 22, 0xFF).unwrap(); // EXTS0 left only
    sound.scsp.write16(17 * 32 + 22, 0xEF).unwrap(); // EXTS1 right only
    sound.advance_with_cd(2438, &mut cd).unwrap();
    assert_eq!(
        sound.scsp.drain(4),
        [0, -1024, 128, -1024, 256, -1024, 384, -1024]
    );
    sound.scsp.write16(16 * 32 + 22, 0).unwrap();
    sound.scsp.write16(17 * 32 + 22, 0).unwrap();
    sound.advance_with_cd(610, &mut cd).unwrap();
    assert_eq!(sound.scsp.drain(1), [0, 0]);
    cd.set_enabled(false);
    assert_eq!(cd.audio_sample(), [0, 0]);
}
#[test]
fn cd_data_port_supports_scu_fixed_source_dma_and_real_bus_widths() {
    let mut b = Bus::default();
    let data = vec![0x69; 4096];
    b.cd.mount(Media::iso(&data).unwrap());
    sectors(&mut b.cd, 1);
    issue(&mut b.cd, [0x6100, 0, 0, 1]);
    assert_eq!(b.read32(DATA), Ok(0x69696969));
    assert_eq!(b.read16(DATA + 2), Ok(0x6969));
    assert!(b.read8(DATA).is_err());
    assert!(b.read32(CD + 0x18).is_err());
    b.write32(HIGH_WRAM, 0xDEAD_BEEF).unwrap();
    assert_eq!(b.read16(SCSP + 0x420).unwrap(), 0);
}

#[test]
fn runtime_iso_directory_information_and_file_read_are_device_commands() {
    let iso = include_bytes!("../web/star-collector.iso");
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(iso).unwrap());
    assert!(directory(&mut cd, [0x7000, 0, 0x00FF, 0xFFFF]));
    let scope = issue(&mut cd, [0x7200, 0, 0, 0]);
    assert_eq!(scope[1..], [2, 0x100, 2]);
    issue(&mut cd, [0x7300, 0, 0, 2]);
    let file = read(&cd, 12);
    assert_eq!(u32::from_be_bytes(file[..4].try_into().unwrap()), 171);
    let size = u32::from_be_bytes(file[4..8].try_into().unwrap());
    assert!(size > 0);
    assert_eq!(file[8..], [0; 4]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x7400, 0, 0x0300, 2]);
    cd.advance(1_000_000);
    let sectors = issue(&mut cd, [0x5100, 0, 0x0300, 0])[3];
    assert_eq!(sectors as u32, size.div_ceil(2048));
    issue(&mut cd, [0x6100, 0, 0x0300, sectors]);
    let transferred = read(&cd, sectors as usize * 2048);
    assert_eq!(
        &transferred[..size as usize],
        &iso[21 * 2048..21 * 2048 + size as usize]
    );
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x7500, 0, 0, 0]);
    assert_ne!(cd.read16(CD + 8).unwrap() & 512, 0);
}
#[test]
fn mode2_subheader_filters_reverse_only_subheader_and_preserve_fad_gate() {
    let mut raw = vec![0; 2352 * 3];
    for (i, s) in raw.chunks_mut(2352).enumerate() {
        s[1..11].fill(255);
        s[15] = 2;
        s[16..20].copy_from_slice(&[5, i as u8, 0x28, 0x21]);
        s[20..24].copy_from_slice(&[5, i as u8, 0x28, 0x21]);
        s[24..2348].fill(i as u8 + 1);
        // Independent MSB-first polynomial remainder, reversed at the boundary.
        let mut crc = 0u32;
        for byte in &s[16..2348] {
            for bit in 0..8 {
                let feedback = (crc >> 31) ^ u32::from((byte >> bit) & 1);
                crc <<= 1;
                if feedback != 0 {
                    crc ^= 0x8001_801B;
                }
            }
        }
        s[2348..].copy_from_slice(&crc.reverse_bits().to_le_bytes());
    }
    let mut cd = CdBlock::default();
    cd.mount(
        Media::cue(
            "FILE \"x\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00",
            &[("x", &raw)],
        )
        .unwrap(),
    );
    issue(&mut cd, [0x4201, 0xFF0F, 5, 0x2801]);
    let sub = issue(&mut cd, [0x4300, 0, 0, 0]);
    assert_eq!(sub[0] & 255, 1);
    assert_eq!(sub[1..], [0xFF0F, 5, 0x2801]);
    issue(&mut cd, [0x4000, 150, 0, 2]);
    issue(&mut cd, [0x445F, 0, 0, 0]);
    issue(&mut cd, [0x4603, 0x0304, 0, 0]);
    sectors(&mut cd, 3);
    // Invert the subheader match, then AND the address range: FAD150 matches
    // reversed conditions; FAD151 subheader matches normally, 152 is out of range.
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0300, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0400, 0])[3], 2);
    issue(&mut cd, [0x5200, 0, 0x0300, 1]);
    assert_eq!(issue(&mut cd, [0x5300, 0, 0, 0])[1], 1162);
    issue(&mut cd, [0x6100, 0, 0x0300, 1]);
    assert!(read(&cd, 2324).iter().all(|b| *b == 1));
}
#[test]
fn power_controls_cancel_pending_transfers_retain_media_and_use_forty_microseconds() {
    use saturn_rust::{Boot, Machine, bus::BIOS_SIZE};
    let mut rom = vec![0; BIOS_SIZE];
    rom[..8].copy_from_slice(&[0, 0, 1, 0, 6, 16, 0, 0]);
    // CD OFF, wait SF, CD ON, wait SF, then SLEEP. All are SH-2 instructions.
    let words: [u16; 24] = [
        0xD10B, 0xD20C, 0xE001, 0x2100, 0xE009, 0x2200, 0x6010, 0xC801, 0x8BFC, 0xE001, 0x2100,
        0xE008, 0x2200, 0x6010, 0xC801, 0x8BFC, 0x001B, 0xAFFD, 0x0009, 0x0009, 0x0009, 0x0009,
        0x0009, 0x0009,
    ];
    for (i, w) in words.into_iter().enumerate() {
        rom[0x100 + i * 2..0x102 + i * 2].copy_from_slice(&w.to_be_bytes());
    }
    // MOV.L literals at aligned 130/134.
    rom[0x130..0x134].copy_from_slice(&0x20100063u32.to_be_bytes());
    rom[0x134..0x138].copy_from_slice(&0x2010001Fu32.to_be_bytes());
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(Media::iso(&vec![0; 4096]).unwrap());
    m.run_steps(400).unwrap();
    assert!(m.bus().cd.media().is_some());
    assert_eq!(m.bus().read16(CD + 8).unwrap(), 0xBE1);
    assert_eq!(m.bus().read16(CD + 12).unwrap(), 0);
    assert_eq!(m.cpu_state().status, saturn_rust::cpu::CpuStatus::Sleeping);
}

#[test]
fn pause_reports_run_at_sixty_hz_and_standby_preserves_pickup_and_media() {
    // ST-162 §§3.4/4.2/8.2.1: 16.7 ms pause report, programmable
    // PAUSE->STANDBY seconds; seek-current returns from standby without ejecting.
    let (mut cd, _) = mounted();
    issue(&mut cd, [0x0400, 1, 0, 0]);
    cd.write16(CD + 8, 0).unwrap();
    cd.advance(447_901 - 1075);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1024, 0);
    cd.advance(1);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1024, 1024);
    assert_eq!(cd.read16(CD + 0x18).unwrap() >> 8, 0x21);
    cd.advance(26_874_100 - (447_902 - 1075) - 1);
    assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0] >> 8, 2);
    assert_eq!(issue(&mut cd, [0x11FF, 0xFFFF, 0, 0])[3], 150);
    cd.advance(26_875);
    assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0] >> 8, 1);
    assert!(cd.media().is_some());
}

#[test]
fn tray_open_stops_audio_and_scan_retains_inserted_media_and_close_reuses_it() {
    let (mut cd, _) = mounted();
    sectors(&mut cd, 1);
    assert_eq!(issue(&mut cd, [0x0500, 0, 0, 0])[0] >> 8, 6);
    assert_eq!(cd.audio_sample(), [0, 0]);
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0])[0], 0xFF00);
    // Tray opening is distinct from removing the medium; this virtual drive
    // closes deterministically on the documented initialization command.
    assert!(cd.media().is_some());
    assert_eq!(issue(&mut cd, [0x0400, 0xFFFF, 0, 0])[0] >> 8, 1);
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0])[1], 204);
    assert_eq!(&read(&cd, 4), &[0x41, 0, 0, 150]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
}

#[test]
fn scan_is_directional_never_reads_data_and_obeys_range_and_pause_mute() {
    // ST-162 §8.2.2: scan does not decode CD-ROM sectors; reversing a scan
    // changes pickup direction, exceeding the play range pauses the drive.
    for chunk in [1, 7, 1000, 400_000] {
        let mut cd = CdBlock::default();
        cd.mount(Media::iso(&vec![0; 2048 * 100]).unwrap());
        issue(&mut cd, [0x3000, 0, 0, 0]);
        assert_eq!(issue(&mut cd, [0x1202, 0, 0, 0])[0], 0xFF00);
        assert_eq!(issue(&mut cd, [0x1200, 0, 0, 0])[0] >> 8, 5);
        let mut remaining = 800_000;
        while remaining != 0 {
            let n = remaining.min(chunk);
            cd.advance(n);
            remaining -= n;
        }
        let before = issue(&mut cd, [0, 0, 0, 0]);
        assert_eq!(before[0] >> 8, 5);
        assert!(before[3] > 150 && before[3] < 250);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
        issue(&mut cd, [0x1201, 0, 0, 0]);
        cd.advance(800_000);
        let after = issue(&mut cd, [0, 0, 0, 0]);
        assert!(after[3] < before[3]);
        cd.advance(2_000_000);
        assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0] >> 8, 1);
        assert_eq!(cd.audio_sample(), [0; 2]);
        // The R-W FIFO is empty in BIN/CUE (no stored subcode packs).
        assert_ne!(issue(&mut cd, [0x2001, 0, 0, 0])[0] & 0x8000, 0);
        assert_eq!(cd.read16(DATA).unwrap(), 0xFFFF);
    }
}

#[test]
fn scan_from_play_attenuates_audio_but_scan_from_pause_is_muted() {
    let mut audio = vec![0; 2352 * 100];
    for frame in audio.as_chunks_mut::<4>().0 {
        frame[..2].copy_from_slice(&16384i16.to_le_bytes());
        frame[2..].copy_from_slice(&(-8192i16).to_le_bytes());
    }
    let mut cd = CdBlock::default();
    cd.mount(
        Media::cue(
            "FILE \"a\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
            &[("a", &audio)],
        )
        .unwrap(),
    );
    issue(&mut cd, [0x1080, 150, 0x0080, 100]);
    cd.advance(26_875);
    assert_eq!(cd.audio_sample(), [16384, -8192]);
    issue(&mut cd, [0x1200, 0, 0, 0]);
    assert_eq!(cd.audio_sample(), [4096, -2048]);
    issue(&mut cd, [0x11FF, 0xFFFF, 0, 0]);
    cd.advance(26_875);
    issue(&mut cd, [0x1200, 0, 0, 0]);
    assert_eq!(cd.audio_sample(), [0; 2]);
}

#[test]
fn full_sector_pool_pauses_and_freed_partition_resumes_at_same_pickup() {
    // ST-162 §4.2: buffer-full pause is reversible, without rereading or
    // dropping the blocked sector, and BFUL remains software-acknowledged.
    let bytes: Vec<u8> = (0..2048 * 210).map(|i| (i / 2048) as u8).collect();
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(&bytes).unwrap());
    issue(&mut cd, [0x3000, 0, 0, 0]);
    issue(&mut cd, [0x1080, 150, 0x0080, 210]);
    cd.advance(40_000_000);
    let full = issue(&mut cd, [0, 0, 0, 0]);
    assert_eq!((full[0] >> 8, full[3]), (1, 350));
    assert_ne!(cd.read16(CD + 8).unwrap() & 8, 0);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 200);
    issue(&mut cd, [0x6200, 0, 0, 1]);
    cd.advance(400_000);
    let resumed = issue(&mut cd, [0, 0, 0, 0]);
    assert_eq!(resumed[3], 351);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 200);
    issue(&mut cd, [0x6100, 199, 0, 1]);
    assert_eq!(read(&cd, 2048), [200; 2048]);
}

#[test]
fn cue_copy_preemphasis_and_channel_control_survive_drive_and_q_reports() {
    let audio = vec![0; 2352 * 2];
    let mut cd = CdBlock::default();
    cd.mount(
        Media::cue(
            "FILE \"a\" BINARY\nTRACK 01 AUDIO\nFLAGS DCP PRE 4CH\nINDEX 01 00:00:00",
            &[("a", &audio)],
        )
        .unwrap(),
    );
    assert_eq!(issue(&mut cd, [0, 0, 0, 0])[1], 0xB101);
    issue(&mut cd, [0x2000, 0, 0, 0]);
    assert_eq!(read(&cd, 10), [0xB1, 1, 1, 0, 0, 0, 0, 0, 0, 150]);
}

// ISO9660 §§8.4/9.1 and ST-162 §6.2. The runtime CD filesystem must reject
// malformed media at scheduled completion, never a native panic/Wasm trap.
#[test]
fn runtime_directory_rejects_overflow_truncation_and_invalid_root_and_child_records() {
    let original = include_bytes!("../web/star-collector.iso");
    let root = 16 * 2048 + 156;
    let child = 20 * 2048 + 68;
    let dual32 = |image: &mut [u8], at: usize, value: u32| {
        image[at..at + 4].copy_from_slice(&value.to_le_bytes());
        image[at + 4..at + 8].copy_from_slice(&value.to_be_bytes());
    };
    for record in [root, child] {
        for (field, value) in [
            (2, u32::MAX),
            (2, u32::MAX - 149),
            (2, 64),
            (2, 63),
            (10, u32::MAX),
            (10, 1_048_577),
        ] {
            let mut iso = original.to_vec();
            dual32(&mut iso, record + field, value);
            if field == 2 && value == 63 {
                dual32(&mut iso, record + 10, 2049);
            }
            let mut cd = CdBlock::default();
            cd.mount(Media::iso(&iso).unwrap());
            assert!(
                !directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]),
                "record={record}, field={field}, value={value}"
            );
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[0], 0xFF00);
        }
        for (field, value) in [
            (0, 33),
            (6, 255),
            (14, 255),
            (25, 0x80),
            (27, 1),
            (28, 2),
            (31, 2),
            (32, 0),
            (32, 255),
        ] {
            let mut iso = original.to_vec();
            iso[record + field] = value;
            let mut cd = CdBlock::default();
            cd.mount(Media::iso(&iso).unwrap());
            assert!(
                !directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]),
                "record={record}, field={field}, value={value}"
            );
        }
    }
    // The record must fit the declared directory size, not only its last sector.
    let mut iso = original.to_vec();
    dual32(&mut iso, root + 10, 69);
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(&iso).unwrap());
    assert!(!directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
    // Exact reviewer reproducer: minimally signed 17-sector image.
    let mut iso = vec![0; 17 * 2048];
    iso[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
    dual32(&mut iso, root + 2, u32::MAX);
    dual32(&mut iso, root + 10, 2048);
    cd.mount(Media::iso(&iso).unwrap());
    assert!(!directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
}

#[test]
fn runtime_directory_mutation_corpus_is_bounded_and_new_media_discards_old_files() {
    let original = include_bytes!("../web/star-collector.iso");
    let mut seed = 0x9817u32;
    for _ in 0..512 {
        let mut iso = original.to_vec();
        for _ in 0..8 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let base = if seed & 1 == 0 { 16 * 2048 } else { 20 * 2048 };
            iso[base + ((seed >> 8) as usize % 2048)] = (seed >> 24) as u8;
        }
        let mut cd = CdBlock::default();
        cd.mount(Media::iso(&iso).unwrap());
        directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]);
    }
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(original).unwrap());
    assert!(directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
    cd.mount(Media::iso(&vec![0; 17 * 2048]).unwrap());
    assert_eq!(issue(&mut cd, [0x7300, 0, 0, 2])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x7400, 0, 0, 2])[0], 0xFF00);
}

#[test]
fn runtime_directory_checks_volume_extent_and_invalidates_failed_acquisition() {
    let original = include_bytes!("../web/star-collector.iso");
    for blocks in [0u32, 16, 20, 21, 65, u32::MAX] {
        let mut iso = original.to_vec();
        iso[16 * 2048 + 80..16 * 2048 + 84].copy_from_slice(&blocks.to_le_bytes());
        iso[16 * 2048 + 84..16 * 2048 + 88].copy_from_slice(&blocks.to_be_bytes());
        let mut cd = CdBlock::default();
        cd.mount(Media::iso(&iso).unwrap());
        assert!(
            !directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]),
            "volume {blocks}"
        );
    }
    let mut iso = original.to_vec();
    iso[20 * 2048 + 68 + 25] = 2; // A directory record whose extent contains guest instructions.
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(&iso).unwrap());
    assert!(directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
    assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1], 2);
    assert!(!directory(&mut cd, [0x7000, 0, 0, 2]));
    assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[0], 0xFF00);
    // Accepted acquisition failed on media; the interrupted table is invalid.
    assert_eq!(issue(&mut cd, [0x7300, 0, 0, 3])[0], 0xFF00);
}

#[test]
fn cdda_pause_resume_preserves_sample_and_filter_history_and_explicit_seek_restarts() {
    let mut audio = vec![0; 2352 * 2];
    for (index, frame) in audio.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        frame[..2].copy_from_slice(&((index as i16 + 1) * 20).to_le_bytes());
        frame[2..].copy_from_slice(&(-10000i16).to_le_bytes());
    }
    let media = Media::cue(
        "FILE \"a\" BINARY\nTRACK 01 AUDIO\nFLAGS PRE\nINDEX 01 00:00:00",
        &[("a", &audio)],
    )
    .unwrap();
    let mut cd = CdBlock::default();
    let mut uninterrupted = CdBlock::default();
    cd.mount(media.clone());
    uninterrupted.mount(media);
    for device in [&mut cd, &mut uninterrupted] {
        issue(device, [0x1080, 150, 0x80, 2]);
        device.advance(26_875);
    }
    let initial = cd.audio_sample();
    assert_eq!(initial, uninterrupted.audio_sample());
    for _ in 0..17 {
        assert_eq!(cd.audio_sample(), uninterrupted.audio_sample());
    }
    issue(&mut cd, [0x11FF, 0xFFFF, 0, 0]);
    cd.advance(26_875);
    for _ in 0..64 {
        assert_eq!(cd.audio_sample(), [0, 0]);
    }
    issue(&mut cd, [0x10FF, 0xFFFF, 0x00FF, 0xFFFF]);
    for _ in 0..600 {
        assert_eq!(cd.audio_sample(), uninterrupted.audio_sample());
    }
    issue(&mut cd, [0x1080, 150, 0x80, 2]);
    cd.advance(26_875);
    assert_eq!(cd.audio_sample(), initial);
}

#[test]
fn uncorrectable_runtime_sector_stops_drive_without_publishing_corrupt_data() {
    let original = vec![0x69; 4096];
    let cooked = Media::iso(&original).unwrap();
    let mut raw = cooked.raw_sector(150).unwrap().to_vec();
    raw.extend(cooked.raw_sector(151).unwrap());
    raw[16..2064].fill(0); // Deliberately beyond P/Q recovery, keeping old EDC.
    let mut cd = CdBlock::default();
    cd.mount(
        Media::cue(
            "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
            &[("d", &raw)],
        )
        .unwrap(),
    );
    cd.write16(CD + 8, 0).unwrap();
    sectors(&mut cd, 2);
    let response = issue(&mut cd, [0, 0, 0, 0]);
    assert_eq!(response[0] >> 8, 9); // ST-162 table 4.3: ERROR until a drive command.
    assert_eq!(response, [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]); // ST-162 §7.2.4(4).
    assert_eq!(cd.read16(CD + 8).unwrap() & 4, 0); // No sector-stored interrupt.
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
    assert_ne!(issue(&mut cd, [0x6100, 0, 0, 1])[0] & 0x8000, 0);
    issue(&mut cd, [0x1080, 151, 0x80, 1]);
    cd.advance(300_000);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
    issue(&mut cd, [0x6100, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), original[2048..]);
}

fn damaged_disc() -> (Media, Vec<u8>) {
    let original = Media::iso(&vec![0x69; 4096]).unwrap();
    let mut raw = original.raw_sector(150).unwrap().to_vec();
    raw[16] ^= 0x5A;
    raw.extend(original.raw_sector(151).unwrap());
    let media = Media::cue(
        "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
        &[("d", &raw)],
    )
    .unwrap();
    (media, raw)
}

#[test]
fn initialize_ecc_disable_no_change_reset_and_invalid_arguments_have_runtime_effects() {
    // ST-162 §8.2.1 CDC_CdInit: 80=no ECC, FF=no change, 40=error output.
    let (media, damaged) = damaged_disc();
    let mut cd = CdBlock::default();
    cd.mount(media);
    issue(&mut cd, [0x0400, 1, 0, 0x8040]);
    sectors(&mut cd, 1);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), damaged[16..2064]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    // No-change fields retain correction and output policy independently.
    issue(&mut cd, [0x04FF, 0xFFFF, 0, 0xFFFF]);
    // Invalid settings must reject before changing any of those fields.
    assert_eq!(issue(&mut cd, [0x0400, 3, 0, 0x0640])[0], 0xFF00);
    assert_eq!(issue(&mut cd, [0x0400, 3, 0, 0x0020])[0], 0xFF00);
    sectors(&mut cd, 1);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), damaged[16..2064]);
    issue(&mut cd, [0x0600, 0, 0, 0]);
    // Reset ignores all other parameters, restores one P/Q pass and drops old
    // selectors/transfer state. It must correct, not output the received byte.
    assert_ne!(issue(&mut cd, [0x0401, 7, 0, 0x0620])[0], 0xFF00);
    sectors(&mut cd, 1);
    issue(&mut cd, [0x6100, 0, 0, 1]);
    assert_eq!(read(&cd, 2048), vec![0x69; 2048]);
}

#[test]
fn finite_retries_hold_fad_and_publish_only_after_the_configured_last_attempt() {
    for retries in [0, 1, 2, 15] {
        for output in [false, true] {
            let (media, damaged) = damaged_disc();
            let mut cd = CdBlock::default();
            cd.mount(media);
            issue(
                &mut cd,
                [
                    0x0400,
                    0,
                    0,
                    0x8000 | retries | if output { 0x40 } else { 0 },
                ],
            );
            issue(&mut cd, [0x3000, 0, 0, 0]);
            cd.write16(CD + 8, 0).unwrap();
            issue(&mut cd, [0x1080, 150, 0x80, 1]);
            for attempt in 0..=retries {
                cd.advance(179_161); // One 150 Hz sector deadline.
                let status = issue(&mut cd, [0, 0, 0, 0]);
                if attempt < retries {
                    assert_eq!(status[0] >> 8, 8);
                    assert_eq!(status[3], 150);
                    assert_eq!(cd.read16(CD + 8).unwrap() & 4, 0);
                } else {
                    assert_eq!(status[0] >> 8, if output { 1 } else { 9 });
                    // ST-162 §7.2.4(4): ERROR serializes invalid fields;
                    // the failed pickup is retained for recovery, not exposed.
                    assert_eq!(status[3], if output { 151 } else { 0xFFFF });
                    if !output {
                        assert_eq!(status, [0x09FF, 0xFFFF, 0xFFFF, 0xFFFF]);
                        issue(&mut cd, [0x11FF, 0xFFFF, 0, 0]);
                        cd.advance(26_875);
                        assert_eq!(issue(&mut cd, [0, 0, 0, 0]), [0x0180, 0x4101, 0x0100, 150]);
                    }
                }
            }
            assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], u16::from(output));
            if output {
                issue(&mut cd, [0x6100, 0, 0, 1]);
                assert_eq!(read(&cd, 2048), damaged[16..2064]);
            }
        }
    }
}

#[test]
fn infinite_retry_is_chunk_invariant_and_drive_commands_cancel_it() {
    for cancel in [0x0400, 0x0500, 0x11FF] {
        let mut previous = None;
        for chunk in [7, 1075, 200_000] {
            let (media, _) = damaged_disc();
            let mut cd = CdBlock::default();
            cd.mount(media);
            issue(&mut cd, [0x0400, 0, 0, 0x8080]);
            issue(&mut cd, [0x3000, 0, 0, 0]);
            issue(&mut cd, [0x1080, 150, 0x80, 1]);
            let mut left = 2_000_000u32;
            while left != 0 {
                let clocks = left.min(chunk);
                cd.advance(clocks);
                left -= clocks;
            }
            let trace = issue(&mut cd, [0, 0, 0, 0]);
            assert_eq!(trace[0] >> 8, 8);
            assert_eq!(trace[3], 150);
            assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
            if let Some(old) = previous {
                assert_eq!(trace, old);
            }
            previous = Some(trace);
            issue(&mut cd, [cancel, 0xFFFF, 0, 0]);
            cd.advance(500_000);
            assert_ne!(issue(&mut cd, [0, 0, 0, 0])[0] >> 8, 8);
            // Reinitializing and seeking a good sector recovers without ejecting
            // or replacing the original medium.
            issue(&mut cd, [0x0400, 0, 0, 0]);
            issue(&mut cd, [0x1080, 151, 0x80, 1]);
            cd.advance(300_000);
            assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
        }
    }
}

#[test]
fn xa_form_two_error_output_retry_flag_and_subheader_confirmation_are_independent_of_routing() {
    let mut raw = [0x69; 2352];
    raw[..12].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0]);
    raw[12..16].copy_from_slice(&[0, 2, 0, 2]);
    raw[16..24].copy_from_slice(&[1, 2, 0x28, 0, 1, 2, 0x28, 0]);
    // Source-authored EDC golden vector for the 2324-byte constant payload.
    raw[2348..].copy_from_slice(&0xBD76_CE9Cu32.to_le_bytes());
    raw[24] ^= 0x5A; // An EDC failure in Form 2 (no product code exists).
    for (flags, retry, expected_status) in [(0, 0, 1), (8, 0, 9), (8, 0x40, 1), (8, 2, 8)] {
        let mut cd = CdBlock::default();
        cd.mount(
            Media::cue(
                "FILE \"d\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00",
                &[("d", &raw)],
            )
            .unwrap(),
        );
        issue(&mut cd, [0x0400 | flags, 0, 0, retry]);
        issue(&mut cd, [0x3000, 0, 0, 0]);
        issue(&mut cd, [0x1080, 150, 0x80, 1]);
        cd.advance(180_000);
        assert_eq!(issue(&mut cd, [0, 0, 0, 0])[0] >> 8, expected_status);
        assert_eq!(
            issue(&mut cd, [0x5100, 0, 0, 0])[3],
            u16::from(expected_status == 1)
        );
        if expected_status == 1 {
            issue(&mut cd, [0x6100, 0, 0, 1]);
            assert_eq!(read(&cd, 2324), raw[24..2348]);
        }
    }
    // Duplicated subheaders may be ignored for decode only; selector conditions
    // still read the first copy. A valid EDC isolates confirmation from CRC.
    raw[24] ^= 0x5A;
    raw[20] = 2;
    raw[2348..].copy_from_slice(&0x8B46_E16Bu32.to_le_bytes());
    for flags in [8, 12] {
        let mut cd = CdBlock::default();
        cd.mount(
            Media::cue(
                "FILE \"d\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00",
                &[("d", &raw)],
            )
            .unwrap(),
        );
        issue(&mut cd, [0x0400 | flags, 0, 0, 0x8000]);
        issue(&mut cd, [0x4202, 0, 1, 0]); // file1, channel2
        issue(&mut cd, [0x4403, 0, 0, 0]); // filter checks file + channel
        issue(&mut cd, [0x3000, 0, 0, 0]);
        issue(&mut cd, [0x1080, 150, 0x80, 1]);
        cd.advance(180_000);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], u16::from(flags == 12));
    }
}

#[test]
fn software_reset_aborts_an_incomplete_transfer_and_discards_cached_directory_information() {
    let image = include_bytes!("../web/star-collector.iso");
    let mut cd = CdBlock::default();
    cd.mount(Media::iso(image).unwrap());
    assert!(directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
    assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1], 2);
    // Directory access programs a FAD filter. Restore an unrestricted filter
    // before explicitly playing sector150 rather than the directory at170.
    issue(&mut cd, [0x4400, 0, 0, 0]);
    sectors(&mut cd, 1);
    issue(&mut cd, [0x6100, 0, 0, 1]);
    assert_ne!(cd.read16(CD + 8).unwrap() & 2, 0);
    issue(&mut cd, [0x0401, 0xFFFF, 0, 0xFFFF]);
    assert_eq!(cd.read16(CD + 8).unwrap() & 14, 0);
    assert_eq!(cd.read16(DATA).unwrap(), 0xFFFF);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
    assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[0], 0xFF00);
}

fn set_iso_number(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    bytes[at + 4..at + 8].copy_from_slice(&value.to_be_bytes());
}

#[test]
fn original_xa_fixture_license_is_an_accessible_noninterleaved_file() {
    let mut cd = CdBlock::default();
    cd.mount(
        Media::cue(
            include_str!("fixtures/cd-filesystem.cue"),
            &[(
                "cd-filesystem.bin",
                include_bytes!("fixtures/cd-filesystem.bin"),
            )],
        )
        .unwrap(),
    );
    assert!(directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]));
    assert_ne!(issue(&mut cd, [0x7400, 0, 0x0100, 3])[0], 0xFF00);
    cd.advance(1_000_000);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0100, 0])[3], 1);
    issue(&mut cd, [0x6100, 0, 0x0100, 1]);
    let bytes = read(&cd, 2048);
    let license = include_bytes!("fixtures/LICENSE");
    assert_eq!(&bytes[..license.len()], license);
}

#[test]
fn interleave_extent_bounds_include_gaps_attributes_and_empty_file_boundaries() {
    // Authored XA Form1 payload offsets, independent of the media decoder.
    let base: Vec<u8> = include_bytes!("fixtures/cd-filesystem.bin")
        .chunks_exact(2352)
        .flat_map(|s| s[24..2072].iter().copied())
        .collect();
    let child = 20 * 2048 + 68;
    for unit in [0u8, 1, 2, 7, 255] {
        for gap in [0u8, 1, 3, 255] {
            for sectors in [0u32, 1, 2, 3, 42, 257] {
                let mut iso = base.clone();
                iso[child + 25] = 1; // ISO hidden must not set reserved CdcFile bits.
                iso[child + 26] = unit;
                iso[child + 27] = gap;
                set_iso_number(&mut iso, child + 10, sectors * 2048);
                let last_offset = if sectors == 0 {
                    0
                } else if unit == 0 {
                    sectors - 1
                } else {
                    (sectors - 1) / u32::from(unit) * (u32::from(unit) + u32::from(gap))
                        + (sectors - 1) % u32::from(unit)
                };
                let valid = (unit != 0 || gap == 0) && (sectors == 0 || 22 + last_offset < 64);
                let mut cd = CdBlock::default();
                cd.mount(Media::iso(&iso).unwrap());
                assert_eq!(
                    directory(&mut cd, [0x7000, 0, 0xFF, 0xFFFF]),
                    valid,
                    "unit={unit} gap={gap} sectors={sectors}"
                );
                if valid {
                    issue(&mut cd, [0x7300, 0, 0, 2]);
                    let info = read(&cd, 12);
                    assert_eq!(&info[..4], &172u32.to_be_bytes());
                    assert_eq!(&info[8..], &[unit, gap, 7, 0x28]);
                }
            }
        }
    }
}

#[test]
fn held_information_window_preserves_self_parent_ids_and_has_exact_254_child_limit() {
    for speed in [1, 2] {
        for chunk in [1, 3, 17, 10000] {
            // ST-162 §§6.2.1/8.2.8: ReadDir selects a window of the current
            // directory, rather than treating its first FID argument as a directory.
            let mut iso = include_bytes!("../web/star-collector.iso").to_vec();
            let directory_size = 6 * 2048;
            set_iso_number(&mut iso, 16 * 2048 + 156 + 10, directory_size);
            let mut own = iso[20 * 2048..20 * 2048 + 34].to_vec();
            set_iso_number(&mut own, 10, directory_size);
            iso[20 * 2048..26 * 2048].fill(0);
            iso[20 * 2048..20 * 2048 + 34].copy_from_slice(&own);
            own[33] = 1;
            iso[20 * 2048 + 34..20 * 2048 + 68].copy_from_slice(&own);
            let mut template = own;
            template[25] = 0;
            template[33] = b'A';
            set_iso_number(&mut template, 2, 31);
            set_iso_number(&mut template, 10, 0);
            let mut at = 20 * 2048 + 68;
            for _ in 0..300 {
                if at % 2048 + 34 > 2048 {
                    at = (at / 2048 + 1) * 2048;
                }
                iso[at..at + 34].copy_from_slice(&template);
                at += 34;
            }
            let mut cd = CdBlock::default();
            cd.mount(Media::iso(&iso).unwrap());
            issue(
                &mut cd,
                [if speed == 1 { 0x0410 } else { 0x0400 }, 0xFFFF, 0, 0],
            );
            assert!(directory_with_chunk(
                &mut cd,
                [0x7000, 0, 0xFF, 0xFFFF],
                chunk
            ));
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1..], [254, 0x100, 2]);
            assert_eq!(issue(&mut cd, [0x7300, 0, 0, 256])[0], 0xFF00);
            assert!(directory_with_chunk(&mut cd, [0x7100, 0, 0, 256], chunk));
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[1..], [46, 0x100, 256]);
            for id in [0, 1, 256, 301] {
                assert_ne!(issue(&mut cd, [0x7300, 0, 0, id])[0], 0xFF00);
                assert_eq!(read(&cd, 12).len(), 12);
                issue(&mut cd, [0x0600, 0, 0, 0]);
            }
            for id in [2, 255, 302] {
                assert_eq!(issue(&mut cd, [0x7300, 0, 0, id])[0], 0xFF00);
            }
            assert!(!directory_with_chunk(&mut cd, [0x7100, 0, 0, 302], chunk));
            assert_eq!(issue(&mut cd, [0x7200, 0, 0, 0])[0], 0xFF00);
        }
    }
}
