//! Host cover lifecycle through CD MMIO and an original reset-vector guest.
use saturn_rust::{
    Boot, Machine,
    cd::{CdBlock, DATA, REGISTERS as CD},
    media::Media,
};

const RECORDS: u32 = 0x06010000;
const RECOGNITION: u32 = 1_343_705;
const OPEN: [u16; 4] = [0x06ff, 0xffff, 0xffff, 0xffff];
const NO_DISC: [u16; 4] = [0x07ff, 0xffff, 0xffff, 0xffff];
const BUSY: [u16; 4] = [0x00ff, 0xffff, 0xffff, 0xffff];
const STANDBY: [u16; 4] = [0x02ff, 0xffff, 0xffff, 0xffff];

fn response(cd: &CdBlock) -> [u16; 4] {
    std::array::from_fn(|i| cd.read16(CD + 0x18 + i as u32 * 4).unwrap())
}

fn issue(cd: &mut CdBlock, packet: [u16; 4]) -> [u16; 4] {
    for (i, word) in packet.into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
    }
    cd.advance(1075);
    assert_eq!(cd.read16(CD + 8).unwrap() & 1, 1);
    response(cd)
}

fn wait(report: [u16; 4]) -> [u16; 4] {
    [report[0] | 0x8000, report[1], report[2], report[3]]
}

fn image(byte: u8, count: usize) -> Media {
    Media::iso_owned(vec![byte; count * 2048]).unwrap()
}

#[test]
fn open_invalidates_old_reads_and_close_recognizes_with_a_bounded_deadline() {
    let mut cd = CdBlock::default();
    assert_eq!(issue(&mut cd, [0; 4]), NO_DISC);
    assert!(!cd.replace_open_media(Some(image(0xa1, 18))));
    cd.write16(CD + 8, 0).unwrap();
    cd.write16(CD + 12, 0x20).unwrap();
    cd.set_cover_open(true);
    assert_eq!(response(&cd), OPEN);
    assert!(cd.interrupt());
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x220, 0x220);
    cd.write16(CD + 8, !0x20).unwrap();
    assert!(!cd.interrupt());
    assert!(cd.replace_open_media(Some(image(0xa1, 18))));
    for packet in [[0x0200, 0, 0, 0], [0x0301, 0, 0, 0], [0xe000, 0, 0, 0]] {
        assert_eq!(issue(&mut cd, packet), wait(OPEN));
    }
    assert_eq!(issue(&mut cd, [0x0401, 0, 0, 0]), OPEN);
    assert_eq!(cd.read16(DATA), Ok(0xffff));
    cd.set_cover_open(false);
    assert_eq!(response(&cd), BUSY);
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0]), wait(BUSY));
    assert_eq!(issue(&mut cd, [0x0300, 0, 0, 0]), wait(BUSY));
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x22, 0);
    cd.advance(RECOGNITION - 2 * 1075 - 1);
    assert_eq!(cd.peek16(CD + 0x18).unwrap() & 0x0f00, 0);
    cd.advance(1);
    assert_eq!(response(&cd), [0x0180, 0x4101, 0x0100, 150]);

    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0]), [0x4100, 204, 0, 0]);
    assert_eq!(cd.read16(DATA), Ok(0x4100));
    issue(&mut cd, [0x0600, 0, 0, 0]);
    assert_eq!(issue(&mut cd, [0x0300, 0, 0, 0]), [0x0100, 0, 0x0100, 168]);
    issue(&mut cd, [0x3000, 0, 0, 0]);
    issue(&mut cd, [0x6400, 0, 0x0100, 1]);
    cd.write16(DATA, 0x1234).unwrap();
    issue(&mut cd, [0x0600, 0, 0, 0]);
    issue(&mut cd, [0x1080, 150, 0x80, 1]);
    cd.advance(250_000);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(cd.read16(DATA), Ok(0xa1a1));

    // While a response is latched, DCHG and the FIFO change immediately
    // and the next status command observes OPEN with the normal handshake.
    for (i, word) in [0x0100, 0, 0, 0].into_iter().enumerate() {
        cd.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
    }
    cd.advance(1075);
    cd.set_cover_open(true);
    assert_eq!(response(&cd)[1], 2);
    assert_eq!(cd.read16(DATA), Ok(0xffff));
    assert_eq!(cd.read16(CD + 8).unwrap() & 0x2e, 0x20);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
    assert_eq!(issue(&mut cd, [0x3100, 0, 0, 0])[2], 0); // retains filter connection
    issue(&mut cd, [0x6300, 0, 0x0100, 1]); // retains host-written buffers
    assert_eq!(cd.read16(DATA), Ok(0x1234));
    issue(&mut cd, [0x0600, 0, 0, 0]);
    assert!(cd.replace_open_media(None));
    cd.set_cover_open(false);
    assert_eq!(issue(&mut cd, [0; 4]), NO_DISC);

    cd.set_cover_open(true);
    assert!(cd.replace_open_media(Some(image(0x52, 32))));
    cd.set_cover_open(false);
    cd.advance(RECOGNITION);
    assert_eq!(issue(&mut cd, [0x0300, 0, 0, 0])[3], 182);
    // The old whole-disc range survived an intervening empty drive.
    issue(&mut cd, [0x10ff, 0xffff, 0x7fff, 0xffff]);
    cd.advance(1_000_000);
    assert_eq!(issue(&mut cd, [0; 4])[3], 151);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(cd.read16(DATA), Ok(0x5252));
}

#[test]
fn play_waits_for_manual_closure_then_reads_the_replacement_disc() {
    let mut cd = CdBlock::default();
    cd.mount(image(0xa1, 18));
    issue(&mut cd, [0x3000, 0, 0, 0]);
    cd.set_cover_open(true);
    assert_eq!(issue(&mut cd, [0x1080, 151, 0x80, 1]), BUSY);
    assert!(cd.cover_open());
    assert!(cd.replace_open_media(Some(image(0x52, 32))));
    cd.advance(RECOGNITION);
    assert_eq!(issue(&mut cd, [0; 4]), BUSY);
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0]), wait(BUSY));
    assert_eq!(issue(&mut cd, [0x0300, 0, 0, 0]), wait(BUSY));
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
    cd.set_cover_open(false);
    assert_eq!(issue(&mut cd, [0x0200, 0, 0, 0]), wait(BUSY));
    assert_eq!(issue(&mut cd, [0x0300, 0, 0, 0]), wait(BUSY));
    cd.advance(RECOGNITION - 2 * 1075 - 1);
    assert_eq!(cd.peek16(CD + 0x18).unwrap() & 0x0f00, 0);
    cd.advance(1);
    assert_eq!(response(&cd), [0x0400, 0x4101, 0x0100, 151]);
    cd.advance(250_000);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 1);
    assert_eq!(issue(&mut cd, [0x5400, 0, 0, 0])[1], 151);
    issue(&mut cd, [0x6300, 0, 0, 1]);
    assert_eq!(cd.read16(DATA), Ok(0x5252));
}

#[test]
fn stop_supersedes_deferred_play_during_recognition() {
    for clocks_left in [RECOGNITION, 1] {
        let mut cd = CdBlock::default();
        issue(&mut cd, [0x3000, 0, 0, 0]);
        cd.set_cover_open(true);
        assert_eq!(issue(&mut cd, [0x1080, 150, 0x80, 1]), BUSY);
        assert!(cd.replace_open_media(Some(image(0x52, 32))));
        cd.set_cover_open(false);
        cd.advance(RECOGNITION - clocks_left);
        assert_eq!(issue(&mut cd, [0x1100, 0, 0, 0]), BUSY);
        cd.advance(clocks_left + 250_000);
        assert_eq!(issue(&mut cd, [0; 4]), STANDBY);
        assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
        assert_eq!(cd.read16(CD + 8).unwrap() & 0x0e, 0);
    }
}

#[test]
fn initialize_seek_and_scan_wait_for_manual_closure() {
    for (packet, ready) in [
        ([0x0400, 0, 0, 0], [0x0180, 0x4101, 0x0100, 150]),
        ([0x1180, 157, 0, 0], [0x0400, 0x4101, 0x0100, 157]),
        ([0x1200, 0, 0, 0], [0x0500, 0x4101, 0x0100, 150]),
    ] {
        let mut cd = CdBlock::default();
        cd.set_cover_open(true);
        assert_eq!(issue(&mut cd, [0x1080, 151, 0x80, 1]), BUSY);
        assert_eq!(issue(&mut cd, packet), BUSY);
        assert!(cd.cover_open());
        assert!(cd.replace_open_media(Some(image(0x52, 32))));
        cd.set_cover_open(false);
        cd.advance(RECOGNITION);
        assert_eq!(response(&cd), ready);
    }
}

#[test]
fn soft_reset_keeps_the_cover_open_and_cancels_deferred_play() {
    let mut cd = CdBlock::default();
    cd.set_cover_open(true);
    assert_eq!(issue(&mut cd, [0x1080, 151, 0x80, 1]), BUSY);
    assert_eq!(issue(&mut cd, [0x0401, 0, 0, 0]), OPEN);
    assert!(cd.cover_open());
    assert!(cd.replace_open_media(Some(image(0x52, 32))));
    cd.set_cover_open(false);
    cd.advance(RECOGNITION);
    assert_eq!(response(&cd), [0x0180, 0x4101, 0x0100, 150]);
}

#[test]
fn empty_closure_discards_deferred_play() {
    let mut cd = CdBlock::default();
    issue(&mut cd, [0x3000, 0, 0, 0]);
    cd.set_cover_open(true);
    assert_eq!(issue(&mut cd, [0x1080, 150, 0x80, 1]), BUSY);
    cd.set_cover_open(false);
    assert_eq!(issue(&mut cd, [0; 4]), NO_DISC);

    cd.set_cover_open(true);
    assert!(cd.replace_open_media(Some(image(0x52, 32))));
    cd.set_cover_open(false);
    cd.advance(RECOGNITION + 250_000);
    assert_eq!(issue(&mut cd, [0; 4]), [0x0180, 0x4101, 0x0100, 150]);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0, 0])[3], 0);
}

#[test]
fn manual_open_mutes_play_and_scan_and_survives_cd_power_cycle() {
    let audio = |sample| {
        Media::cue(
            "FILE a BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n",
            &[("a", &[sample; 2352 * 6])],
        )
        .unwrap()
    };
    let mut cd = CdBlock::default();
    cd.mount(audio(0x12));
    issue(&mut cd, [0x1080, 150, 0, 0]);
    cd.advance(26_875);
    assert_eq!(cd.audio_sample(), [0x1212; 2]);
    issue(&mut cd, [0x1200, 0, 0, 0]);
    assert_ne!(cd.audio_sample(), [0; 2]);
    cd.set_cover_open(true);
    cd.set_enabled(false);
    cd.set_enabled(true);
    assert!(cd.cover_open());
    assert_eq!(issue(&mut cd, [0; 4]), OPEN);
    assert!(cd.replace_open_media(Some(audio(0x34))));
    assert_eq!(cd.audio_sample(), [0; 2]);
    cd.set_cover_open(false);
    assert_eq!(cd.audio_sample(), [0; 2]);
    cd.advance(RECOGNITION);
    assert_eq!(cd.audio_sample(), [0; 2]);
    issue(&mut cd, [0x1080, 150, 0, 0]);
    cd.advance(26_875);
    assert_eq!(cd.audio_sample(), [0x3434; 2]);
}

fn firmware() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/cd-cover.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            let word = u16::from_str_radix(word, 16).unwrap();
            rom[at + i * 2..at + i * 2 + 2].copy_from_slice(&word.to_be_bytes());
        }
    }
    rom
}

fn word(machine: &Machine, index: u32) -> u32 {
    machine.debug_read32(RECORDS + index * 4).unwrap()
}

fn until(machine: &mut Machine, predicate: impl Fn(&Machine) -> bool) {
    for _ in 0..12 {
        if predicate(machine) {
            return;
        }
        machine.advance_frame().unwrap();
    }
    panic!(
        "guest did not reach expected milestone: {:08x?}",
        (0..43).map(|i| word(machine, i)).collect::<Vec<_>>()
    );
}

#[test]
fn reset_vector_guest_keeps_running_through_insert_remove_and_exchange() {
    for fast in [false, true] {
        let mut machine = Machine::new();
        machine.reset(Boot::ExternalBios(&firmware())).unwrap();
        machine.set_fast_mode(fast);
        until(&mut machine, |m| word(m, 0) > 1);
        assert_eq!(word(&machine, 1), 0x434f5645);
        assert_eq!(word(&machine, 2), 0x07ff);
        let backup = machine.backup_ram().to_vec();
        assert_eq!(backup[0], 0x5a);

        for (round, medium, leadout, marker) in [
            (0, Some(image(0xa1, 18)), 168, 0xa1a1a1a1),
            (1, None, 0, 0),
            (2, Some(image(0x52, 32)), 182, 0x52525252),
            (3, Some(image(0x73, 24)), 174, 0x73737373),
        ] {
            let before = (
                machine.clocks(),
                machine.cpu_state().pc,
                machine.frames(),
                machine.export_smpc_battery(),
            );
            let heartbeat = word(&machine, 0);
            machine.set_cd_cover_open(true);
            assert!(machine.replace_open_media(medium));
            assert_eq!(
                before,
                (
                    machine.clocks(),
                    machine.cpu_state().pc,
                    machine.frames(),
                    machine.export_smpc_battery()
                )
            );
            let base = 4 + 13 * round;
            until(&mut machine, |m| word(m, base + 5) == 0x86ff);
            assert_eq!(
                (0..6).map(|i| word(&machine, base + i)).collect::<Vec<_>>(),
                [0x26ff, 0x20, 0, 0x06ff, 0x86ff, 0x86ff]
            );
            machine.set_cd_cover_open(false);
            until(&mut machine, |m| word(m, 3) == round + 1);
            let remaining: Vec<_> = (6..13).map(|i| word(&machine, base + i)).collect();
            if leadout == 0 {
                assert_eq!(remaining, [0x27ff, 0, 0, 0, 0, 0, 0]);
            } else {
                assert_eq!(
                    remaining,
                    [0x20ff, 0x80ff, 0x80ff, 0x4100, 0x41000096, leadout, marker]
                );
            }
            assert!(word(&machine, 0) > heartbeat);
            assert_eq!(word(&machine, 1), 0x434f5645);
            assert_eq!(machine.backup_ram(), backup);
            assert_eq!(machine.cpu_state().exceptions, 0);
        }
    }
}
