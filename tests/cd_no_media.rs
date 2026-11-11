//! ST-162 §§3.2/3.3, 8.2.1 functions1.5–1.7: bounded no-media protocol.
use saturn_rust::{
    bus::{Bus, Memory},
    cd::{CdBlock, DATA, REGISTERS as CD},
    media::Media,
};

fn advance(cd: &mut CdBlock, mut clocks: u32, chunk: u32, preview: bool) {
    while clocks != 0 {
        let n = clocks.min(chunk);
        cd.advance(n);
        clocks -= n;
        if preview {
            for at in [8, 12, 0x18, 0x1c, 0x20, 0x24] {
                cd.peek16(CD + at).unwrap();
            }
        }
    }
}
fn response(cd: &CdBlock) -> [u16; 4] {
    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}
fn issue(cd: &mut CdBlock, request: [u16; 4], chunk: u32, preview: bool) -> [u16; 4] {
    let old = std::array::from_fn::<_, 4, _>(|i| cd.peek16(CD + 0x18 + i as u32 * 4).unwrap());
    for (i, word) in request.into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
    }
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 0);
    advance(cd, 1074, chunk, preview);
    for (i, word) in old.into_iter().enumerate() {
        assert_eq!(cd.peek16(CD + 0x18 + i as u32 * 4), Ok(word));
    }
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 0);
    advance(cd, 1, chunk, preview);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 1);
    response(cd)
}

#[test]
fn reset_wait_queries_and_released_periodic_reports_have_bounded_stable_no_disc_state() {
    for chunk in [1, 127, 10000] {
        for preview in [false, true] {
            let mut cd = CdBlock::default();
            for _ in 0..2 {
                cd.write16(CD + 8, 0).unwrap();
                cd.write16(CD + 12, 0x401).unwrap();
                assert!(!cd.interrupt());
                assert_eq!(
                    issue(&mut cd, [0x0401, 0, 0, 0], chunk, preview),
                    [0x07ff, 0xffff, 0xffff, 0xffff]
                );
                assert_eq!(cd.read16(CD + 8).unwrap() & 0x43, 0x41);
                assert!(cd.interrupt());
                for command in [0x0200, 0x0300, 0x0301] {
                    assert_eq!(
                        issue(&mut cd, [command, 0, 0, 0], chunk, preview),
                        [0x87ff, 0xffff, 0xffff, 0xffff]
                    );
                    assert_eq!(cd.read16(CD + 8).unwrap() & 2, 0); // no fabricated DATATRNS
                    assert_eq!(cd.read16(DATA), Ok(0xffff));
                }
                assert_eq!(
                    issue(&mut cd, [0, 0, 0, 0], chunk, preview),
                    [0x07ff, 0xffff, 0xffff, 0xffff]
                );
                // Functional 60 Hz reporting; <=447902 clocks to next stopped report.
                for _ in 0..2 {
                    cd.write16(CD + 8, !0x400).unwrap();
                    advance(&mut cd, 447902, chunk, preview);
                    assert_ne!(cd.read16(CD + 8).unwrap() & 0x400, 0);
                    assert_eq!(response(&cd), [0x27ff, 0xffff, 0xffff, 0xffff]);
                }
                assert!(!cd.active());
                assert!(cd.media().is_none());
                // OPEN has WAIT semantics too, while its valid drive state differs.
                issue(&mut cd, [0x0500, 0, 0, 0], chunk, preview);
                for command in [0x0200, 0x0300] {
                    assert_eq!(
                        issue(&mut cd, [command, 0, 0, 0], chunk, preview),
                        [0x86ff, 0xffff, 0xffff, 0xffff]
                    );
                }
            }
        }
    }
}

#[test]
fn pending_no_media_query_is_cancelled_by_power_reset_and_observation_cannot_release_response() {
    for edge in [0, 1074, 1075] {
        let mut b = Bus::default();
        for (i, word) in [0x0100, 0, 0, 0].into_iter().enumerate() {
            b.write16(0x25890018 + i as u32 * 4, word).unwrap();
        }
        b.advance_clocks(edge).unwrap();
        b.cd.set_enabled(false);
        b.cd.set_enabled(true);
        b.advance_clocks(1075).unwrap();
        assert_eq!(b.read16(CD + 0x1c), Ok(0xffff));
        // CPU alias uses the protocol read; debugger alias only previews.
        for (i, word) in [0x0100, 0, 0, 0].into_iter().enumerate() {
            b.write16(0x25890018 + i as u32 * 4, word).unwrap();
        }
        b.advance_clocks(1075).unwrap();
        for _ in 0..10 {
            assert_eq!(b.debug_read16(0x25890024), Ok(0));
        }
        b.advance_clocks(500000).unwrap();
        assert_eq!(b.read16(CD + 0x1c), Ok(2));
        b.read16(0x25890024).unwrap();
        b.advance_clocks(500000).unwrap();
        assert_eq!(b.read16(CD + 0x1c), Ok(0xffff));
        assert!(b.read32(CD + 0x18).is_err());
        assert!(b.write16(CD + 0x19, 0).is_err());
        assert!(b.read16(CD + 0x26).is_err());
    }
    // Replacement media supplies actual TOC bytes after a no-media WAIT.
    let mut cd = CdBlock::default();
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0], 127, true)[0], 0x87ff);
    cd.mount(Media::iso(&vec![0; 2048 * 8]).unwrap());
    assert_eq!(
        issue(&mut cd, [0x0200, 0, 0, 0], 127, true),
        [0x4100, 204, 0, 0]
    );
    assert_eq!(cd.read16(DATA), Ok(0x4100));
}
