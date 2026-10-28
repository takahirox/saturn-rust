//! Independent command/media/timing oracles for Sega CD I/F §4.3.
use saturn_rust::{
    bus::{Bus, Memory},
    cd::{DATA, REGISTERS as CD},
    media::Media,
};
const HZ: u32 = 26_874_100;
const CLEAN: [u8; 24] = [
    56, 0, 46, 22, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 39, 59, 11, 23,
];
fn companion(first: u32, count: u32) -> Vec<u8> {
    let mut b = b"SRW1".to_vec();
    b.extend(first.to_be_bytes());
    b.extend(count.to_be_bytes());
    for _ in 0..count * 4 {
        b.extend(CLEAN);
    }
    b
}
fn media() -> Media {
    Media::cue(
        "FILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
        &[("a.bin", &vec![0; 2352 * 100])],
    )
    .unwrap()
}
fn mounted() -> Bus {
    let mut b = Bus::default();
    b.cd.mount(media().with_subchannel(&companion(150, 100)).unwrap());
    b
}
fn advance(b: &mut Bus, mut n: u32, chunk: u32, preview: bool) {
    while n > 0 {
        let d = n.min(chunk);
        b.cd.advance(d);
        n -= d;
        if preview {
            b.debug_read32(DATA).unwrap();
            b.debug_read16(CD + 0x24).unwrap();
        }
    }
}
fn response(b: &Bus) -> [u16; 4] {
    std::array::from_fn(|i| b.read16(CD + 0x18 + 4 * i as u32).unwrap())
}
fn issue(b: &mut Bus, c: [u16; 4]) -> [u16; 4] {
    for (i, w) in c.into_iter().enumerate() {
        b.write16(CD + 0x18 + 4 * i as u32, w).unwrap();
    }
    b.cd.advance(1075);
    response(b)
}
fn play(b: &mut Bus) {
    issue(b, [0x0402, 0, 0, 0]);
    issue(b, [0x1080, 150, 0, 0]);
}
fn words(b: &Bus) -> Vec<u16> {
    (0..12).map(|_| b.read16(DATA).unwrap()).collect()
}
fn expected() -> Vec<u16> {
    CLEAN
        .chunks_exact(2)
        .map(|w| u16::from_be_bytes([w[0], w[1]]))
        .collect()
}
#[test]
fn bounded_atomic_companion_loading_and_explicit_fad_mapping() {
    let m = media();
    assert!(m.clone().with_subchannel(&companion(150, 100)).is_ok());
    for (first, count) in [(149, 2), (249, 2), (u32::MAX, 1), (150, 0)] {
        assert!(m.clone().with_subchannel(&companion(first, count)).is_err());
    }
    let valid = companion(150, 1);
    for n in [0, 4, 11, 12, 13, 107] {
        assert!(m.clone().with_subchannel(&valid[..n]).is_err());
    }
    let mut b = valid.clone();
    b.push(0);
    assert!(m.clone().with_subchannel(&b).is_err());
    b = valid.clone();
    b[12] = 64;
    assert!(m.clone().with_subchannel(&b).is_err());
    b = valid.clone();
    b[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(m.clone().with_subchannel(&b).is_err());
    let data = Media::iso(&[0; 2048]).unwrap();
    assert!(data.with_subchannel(&valid).is_err());
    let loaded = m.clone().with_subchannel(&valid).unwrap();
    assert!(loaded.with_subchannel(&valid).is_err());
    let cue = "REM SATURN_RW \"a.srw\"\nFILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n";
    let audio = vec![0; 2352 * 2];
    assert!(Media::cue(cue, &[("a.bin", &audio)]).is_err());
    assert!(
        Media::cue(
            cue,
            &[("a.bin", &audio), ("a.srw", &valid), ("a.srw", &valid)]
        )
        .is_err()
    );
    assert!(
        Media::cue(
            &format!("REM SATURN_RW \"a.srw\"\n{cue}"),
            &[("a.bin", &audio), ("a.srw", &valid)]
        )
        .is_err()
    );
    let names = [
        ("a.bin", 0..audio.len()),
        ("a.srw", audio.len()..audio.len() + valid.len()),
    ];
    assert!(Media::cue_owned(cue, &names, [audio.as_slice(), &valid].concat()).is_ok());
    assert!(Media::cue_owned(cue, &names, audio.clone()).is_err());
    assert_eq!(m.raw_sector(150).unwrap(), [0; 2352]);
}
#[test]
fn scheduled_startup_deadlines_and_passive_inspection_are_chunk_invariant() {
    let mut traces = Vec::new();
    for chunk in [1, 3, 17, 10000] {
        for preview in [false, true] {
            let mut b = mounted();
            play(&mut b);
            // Existing seek convention (1 ms) plus two real 75 Hz pre-roll frames.
            let startup = 26_875 + (u64::from(HZ) * 2).div_ceil(75) as u32;
            let decode = (u64::from(HZ) * 32).div_ceil(10000) as u32;
            advance(&mut b, startup + decode - 1076, chunk, preview);
            // The retrieval command itself costs 1075 clocks: completes one clock early.
            assert_ne!(issue(&mut b, [0x2001, 0, 0, 0])[0] & 0x8000, 0);
            advance(&mut b, 1, chunk, preview);
            let r = issue(&mut b, [0x2001, 0, 0, 0]);
            assert_eq!(r, [0x4300, 12, 0, 0]);
            let before = b.debug_read32(DATA).unwrap();
            assert_eq!(before, 0x38002e16);
            assert_eq!(b.debug_read32(DATA).unwrap(), before);
            let w = words(&b);
            assert_eq!(w, expected());
            assert_eq!(b.read16(DATA).unwrap(), 0xffff);
            assert_eq!(issue(&mut b, [0x0600, 0, 0, 0])[1], 12);
            traces.push((r, w));
        }
    }
    assert!(traces.iter().all(|t| t == &traces[0]));
}
#[test]
fn fifo_arbitration_partial_excess_dataend_and_independent_interrupt_ack() {
    let mut b = mounted();
    play(&mut b);
    b.cd.advance(1_200_000);
    b.write16(CD + 8, 0).unwrap();
    b.write16(CD + 12, 0x482).unwrap();
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    assert!(b.cd.interrupt());
    assert_ne!(issue(&mut b, [0x0200, 0, 0, 0])[0] & 0x8000, 0);
    assert_ne!(issue(&mut b, [0x2000, 0, 0, 0])[0] & 0x8000, 0);
    assert_eq!(b.read16(DATA).unwrap(), 0x3800);
    assert_eq!(issue(&mut b, [0x0600, 0, 0, 0])[1], 2);
    assert_eq!(b.read16(CD + 8).unwrap() & 0x82, 0);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    for _ in 0..15 {
        b.read16(DATA).unwrap();
    }
    assert_eq!(issue(&mut b, [0x0600, 0, 0, 0])[1], 12);
    b.cd.advance(400_000);
    let h = b.read16(CD + 8).unwrap();
    assert_ne!(h & 0x400, 0);
    b.write16(CD + 8, !1).unwrap();
    assert_ne!(b.read16(CD + 8).unwrap() & 0x400, 0);
    b.write16(CD + 8, !0x400).unwrap();
    assert_eq!(b.read16(CD + 8).unwrap() & 0x400, 0);
}
#[test]
fn ring_capacity_overrun_outstanding_slot_overwrite_and_23_pack_recovery() {
    let mut b = mounted();
    let mut sub = companion(150, 100);
    sub[12..36].fill(0); // A clean ZERO pack occupies physical slot0 first.
    b.cd.mount(media().with_subchannel(&sub).unwrap());
    play(&mut b);
    let startup = 26_875 + (u64::from(HZ) * 2).div_ceil(75) as u32;
    let first = startup + (u64::from(HZ) * 32).div_ceil(10000) as u32;
    // 24 publications: ring remains valid at physical capacity.
    b.cd.advance(first + (23 * u64::from(HZ)).div_ceil(300) as u32);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[3], 0);
    assert_eq!(b.read16(DATA).unwrap(), 0);
    // Pin acquisition pointer while physical slots continue to be overwritten.
    b.cd.advance((2 * u64::from(HZ)).div_ceil(300) as u32);
    // The next 11 words observe the new pack in the overwritten physical slot,
    // while the already transferred first word remains the old zero.
    assert_eq!(
        (0..11).map(|_| b.read16(DATA).unwrap()).collect::<Vec<_>>(),
        expected()[1..]
    );
    issue(&mut b, [0x0600, 0, 0, 0]);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[3], 0);
    issue(&mut b, [0x0600, 0, 0, 0]);
    b.cd.advance(HZ / 2);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[3], 2);
    issue(&mut b, [0x0600, 0, 0, 0]);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[3], 0);
}
#[test]
fn pause_seek_scan_decode_controls_and_reset_domains() {
    for boundary in [0, 780_000, 1_200_000] {
        for action in [0, 1, 2, 3, 4, 5] {
            let mut b = mounted();
            play(&mut b);
            b.cd.advance(boundary);
            if boundary == 1_200_000 {
                issue(&mut b, [0x2001, 0, 0, 0]);
                b.read16(DATA).unwrap();
            }
            match action {
                0 => {
                    issue(&mut b, [0x0401, 0, 0, 0]);
                }
                1 => {
                    b.cd.set_enabled(false);
                    b.cd.set_enabled(true);
                }
                2 => {
                    issue(&mut b, [0x0500, 0, 0, 0]);
                }
                3 => {
                    b.cd.mount(media());
                }
                4 => {
                    issue(&mut b, [0x11ff, 0xffff, 0, 0]);
                }
                _ => {
                    issue(&mut b, [0x1200, 0, 0, 0]);
                }
            }
            if action >= 4 && boundary == 1_200_000 {
                assert_eq!(issue(&mut b, [0x0600, 0, 0, 0])[1], 2);
            }
            if action < 4 {
                assert_eq!(b.read16(CD + 8).unwrap() & 2, 0);
            }
            b.cd.advance(2_000_000);
            let r = issue(&mut b, [0x2001, 0, 0, 0]);
            if action < 4 || boundary < 1_200_000 {
                assert_ne!(r[0] & 0x8000, 0, "{action}/{boundary}: {r:x?}");
            } else {
                assert_eq!(r[1], 12);
                issue(&mut b, [0x0600, 0, 0, 0]);
            }
        }
    }
    let mut b = mounted();
    play(&mut b);
    b.cd.advance(1_200_000);
    issue(&mut b, [0x0400, 0, 0, 0]); // disable retains queued data
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    issue(&mut b, [0x0600, 0, 0, 0]);
    issue(&mut b, [0x0402, 0, 0, 0]); // enable alone retains old data
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    issue(&mut b, [0x0600, 0, 0, 0]);
    issue(&mut b, [0x0480, 0xffff, 0, 0xffff]); // no-change retains enabled
    issue(&mut b, [0x1080, 150, 0, 0]);
    b.cd.advance(1_200_000);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
}

const CODEWORDS: [[u8; 24]; 4] = [
    [
        56, 0, 46, 22, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 39, 59, 11, 23,
    ],
    [
        56, 1, 45, 20, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 44, 32, 12, 16,
    ],
    [
        56, 2, 40, 18, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 39, 34, 61,
        56,
    ],
    [
        56, 3, 43, 16, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 28, 11, 39,
        0,
    ],
];
const CHANGED: [[u8; 24]; 4] = [
    [
        56, 0, 46, 22, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 41, 20, 58, 23,
    ],
    [
        56, 1, 45, 20, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 13, 54, 62, 5,
    ],
    [
        56, 2, 40, 18, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 26, 46, 45,
        9,
    ],
    [
        56, 3, 43, 16, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 53, 43, 58,
        36,
    ],
];

#[test]
fn extending_active_play_preserves_prefetched_packs_and_publication_times() {
    fn command(b: &mut Bus, c: [u16; 4]) -> [u16; 4] {
        for (i, word) in c.into_iter().enumerate() {
            b.write16(CD + 0x18 + 4 * i as u32, word).unwrap();
        }
        b.advance_clocks(1075).unwrap();
        response(b)
    }
    let audio = vec![0; 2352 * 30];
    let mut sub = companion(150, 30);
    for (i, pack) in sub[12..].chunks_exact_mut(24).enumerate() {
        pack.copy_from_slice(&CODEWORDS[(i / 4 + i % 4) % 4]);
    }
    let media = Media::cue(
        "FILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00",
        &[("a.bin", &audio)],
    )
    .unwrap()
    .with_subchannel(&sub)
    .unwrap();
    let trace = |initial_length, end, horizon| {
        let mut b = Bus::default();
        b.cd.mount(media.clone());
        command(&mut b, [0x0402, 0, 0, 0]);
        command(&mut b, [0x1080, 150, 0x0080, initial_length]);
        b.advance_clocks(830_000).unwrap();
        command(&mut b, [0x10ff, 0xffff, 0x0080, end]);
        b.advance_clocks(horizon).unwrap();
        command(&mut b, [0x11ff, 0xffff, 0, 0]);
        let mut packs = Vec::new();
        loop {
            let r = command(&mut b, [0x2001, 0, 0, 0]);
            if r[0] & 0x8000 != 0 {
                break;
            }
            packs.push((r, words(&b)));
            assert_eq!(command(&mut b, [0x0600, 0, 0, 0])[1], 12);
            assert!(packs.len() <= 24);
        }
        packs
    };
    // Pause completes 1075 clocks after each horizon. These three horizons
    // bracket the second publication after extending (PLAY + 919097 clocks).
    for (horizon, count) in [(0, 1), (86_946, 1), (86_947, 2), (86_948, 2), (900_000, 11)] {
        let original = trace(30, 30, horizon);
        assert_eq!(trace(1, 30, horizon), original, "horizon {horizon}");
        assert_eq!(original.len(), count, "horizon {horizon}");
        if horizon == 900_000 {
            for (i, (r, payload)) in original.iter().enumerate() {
                assert_eq!(r[1..], [12, 0, 0]);
                let expected: Vec<_> = CODEWORDS[(i / 4 + i % 4) % 4]
                    .chunks_exact(2)
                    .map(|w| u16::from_be_bytes([w[0], w[1]]))
                    .collect();
                assert_eq!(*payload, expected, "pack {i}");
            }
        }
    }
    // Lookahead past the range must never publish unless PLAY is extended.
    assert_eq!(trace(1, 1, 900_000).len(), 4);
}

fn guest_media(iso: &[u8], sub: &[u8]) -> Media {
    Media::cue(
        include_str!("fixtures/cd-rw.cue"),
        &[
            ("cd-rw.iso", iso),
            (
                "cd-rw-audio.bin",
                include_bytes!("fixtures/cd-rw-audio.bin"),
            ),
            ("cd-rw.srw", sub),
        ],
    )
    .unwrap()
}
#[test]
fn guest_cpu_dma_scdq_irqs_integrity_controls_and_repeated_machine_resets() {
    use saturn_rust::Machine;
    let original = include_bytes!("fixtures/cd-rw.iso");
    for (sub, omit, error) in [
        (
            include_bytes!("fixtures/cd-rw.srw").as_slice(),
            false,
            false,
        ),
        (
            include_bytes!("fixtures/cd-rw-corrected.srw").as_slice(),
            false,
            false,
        ),
        (
            include_bytes!("fixtures/cd-rw-bad.srw").as_slice(),
            false,
            true,
        ),
        (
            include_bytes!("fixtures/cd-rw-header-bad.srw").as_slice(),
            false,
            true,
        ),
        (include_bytes!("fixtures/cd-rw.srw").as_slice(), true, false),
        (
            include_bytes!("fixtures/cd-rw-changed.srw").as_slice(),
            false,
            false,
        ),
    ] {
        let mut machine = Machine::new();
        for _ in 0..2 {
            let mut iso = original.to_vec();
            if omit {
                iso[21 * 2048 + 0x1f00..21 * 2048 + 0x1f04]
                    .copy_from_slice(&0x0400u32.to_be_bytes());
            }
            machine.reset_media(guest_media(&iso, sub)).unwrap();
            for _ in 0..400 {
                machine.run_steps(10000).unwrap();
                if machine.bus().read32(0x060103fc) == Ok(0x600d) {
                    break;
                }
            }
            let read = |at| machine.bus().read32(0x06010000 + at).unwrap();
            assert_eq!(
                read(0x3fc),
                0x600d,
                "pc={:08x} irq={} dma={} count={}",
                machine.cpu_state().pc,
                read(0x300),
                read(0x304),
                read(0x308)
            );
            assert!(read(0x300) >= 4);
            assert_eq!(read(0x308), if omit { 0 } else { 4 });
            assert_eq!(read(0x304), if omit { 0 } else { 2 });
            if !omit {
                let codes = if sub == include_bytes!("fixtures/cd-rw-changed.srw") {
                    &CHANGED
                } else {
                    &CODEWORDS
                };
                for (k, code) in codes.iter().enumerate() {
                    let mut code = *code;
                    if k == 0 && error {
                        code.copy_from_slice(&sub[12..36]);
                    }
                    let expected: Vec<u32> = code
                        .chunks_exact(2)
                        .map(|w| u32::from(u16::from_be_bytes([w[0], w[1]])))
                        .collect();
                    let actual: Vec<_> = (0..12).map(|i| read(k as u32 * 0x80 + i * 4)).collect();
                    assert_eq!(actual, expected, "pack{k}");
                    assert_eq!(read(k as u32 * 0x80 + 0x30), expected.iter().sum::<u32>());
                    assert_eq!(read(k as u32 * 0x80 + 0x34), u32::from(error && k == 0));
                    assert_eq!(read(k as u32 * 0x80 + 0x38), u32::from(code[1]));
                    assert_eq!(read(k as u32 * 0x80 + 0x3c), 12);
                }
            }
            machine.run_steps(10000).unwrap();
            let samples = machine.drain_audio(8192);
            assert!(samples.len() > 512);
            assert!(
                samples[samples.len() - 512..]
                    .chunks_exact(2)
                    .all(|p| p == [1000, -2000])
            );
            // Machine reset cancels all queued, timed and outstanding transfer state.
            machine.reset(saturn_rust::Boot::HlePrototype).unwrap();
            assert_eq!(machine.bus().read16(DATA).unwrap(), 0xffff);
        }
    }
}

#[test]
fn data_audio_boundaries_repeat_and_no_change_play_keep_the_documented_domains() {
    let raw = vec![0; 2352 * 4];
    let data = vec![0; 2048 * 2];
    let mixed=Media::cue("FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"a\" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\nFILE \"e\" BINARY\nTRACK 03 MODE1/2048\nINDEX 01 00:00:00",&[("d",&data),("a",&raw),("e",&data)]).unwrap().with_subchannel(&companion(152,4)).unwrap();
    let mut b = Bus::default();
    b.cd.mount(mixed);
    issue(&mut b, [0x0402, 0, 0, 0]);
    issue(&mut b, [0x1080, 150, 0, 0]);
    b.cd.advance(26_875);
    assert_ne!(issue(&mut b, [0x2001, 0, 0, 0])[0] & 0x8000, 0);
    // Data sectors advance with the 150 Hz drive. CD-DA starts at FAD152.
    b.cd.advance(400_000);
    assert_eq!(issue(&mut b, [0, 0, 0, 0])[3], 152);
    b.cd.advance(100_000);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    issue(&mut b, [0x0600, 0, 0, 0]);
    b.cd.advance(400_000);
    // An unchanged PLAY start while already decoding must not erase the ring.
    issue(&mut b, [0x10ff, 0xffff, 0xffff, 0xffff]);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    issue(&mut b, [0x0600, 0, 0, 0]);
    for _ in 0..4 * 588 {
        b.cd.audio_sample();
    }
    b.cd.advance(400_000); // audio→data cancels new decoding, retains old packs
    let mut got = 0;
    loop {
        let r = issue(&mut b, [0x2001, 0, 0, 0]);
        if r[0] & 0x8000 != 0 {
            break;
        }
        got += 1;
        issue(&mut b, [0x0600, 0, 0, 0]);
        assert!(got <= 24);
    }
    assert!(got > 0);
    b.cd.advance(1_000_000);
    assert_ne!(issue(&mut b, [0x2001, 0, 0, 0])[0] & 0x8000, 0);
    // Repeat restarts decoding at the loop origin, clearing a retained old pack.
    issue(&mut b, [0x1080, 152, 0x0180, 1]);
    b.cd.advance(900_000);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
    for _ in 0..588 {
        b.cd.audio_sample();
    }
    b.cd.advance(1);
    assert_eq!(b.read16(DATA).unwrap(), 0xffff);
    b.cd.advance(90_000);
    assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
}

#[test]
fn machine_reset_cancels_acquisition_decode_and_partial_fifo_without_late_publication() {
    use saturn_rust::{Boot, Machine};
    let media = guest_media(
        include_bytes!("fixtures/cd-rw.iso"),
        include_bytes!("fixtures/cd-rw.srw"),
    );
    for marker in [0u32, 1, 2] {
        let mut m = Machine::new();
        m.reset_media(media.clone()).unwrap();
        let mut found = false;
        for _ in 0..1_000_000 {
            m.run_steps(3).unwrap();
            let status = m.bus().debug_read16(CD + 0x18).unwrap() >> 8;
            let data = m.bus().debug_read16(DATA).unwrap();
            if (marker == 0 && status & 15 == 4)
                || (marker == 1 && status & 15 == 3)
                || (marker == 2 && data != 0xffff)
            {
                found = true;
                break;
            }
        }
        assert!(found, "reset boundary {marker}");
        if marker == 2 {
            assert_ne!(m.bus().read16(DATA).unwrap(), 0xffff);
        }
        m.reset(Boot::HlePrototype).unwrap();
        m.run_steps(10000).unwrap();
        assert_eq!(m.bus().read16(DATA).unwrap(), 0xffff);
        assert_eq!(m.bus().read16(CD + 8).unwrap() & 2, 0);
    }
}

#[test]
fn dataend_prefetch_obeys_the_primary_partial_full_and_excess_count_relations() {
    let mut b = mounted();
    play(&mut b);
    b.cd.advance(2_000_000);
    issue(&mut b, [0x11ff, 0xffff, 0, 0]);
    for host in 0..=13 {
        assert_eq!(issue(&mut b, [0x2001, 0, 0, 0])[1], 12);
        for i in 0..host {
            assert_eq!(
                b.read16(DATA).unwrap(),
                if i < 12 { expected()[i] } else { 0xffff }
            );
        }
        let count = usize::from(issue(&mut b, [0x0600, 0, 0, 0])[1]);
        if host < 12 {
            assert!(count > host && count <= 12);
        } else {
            assert_eq!(count, 12);
        }
    }
}
