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
    assert_eq!(issue(&mut cd, [0x3200, 0, 0, 0])[2], last);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0500, 0])[3], 1);
    issue(&mut cd, [0x6606, 0, 0x0500, 1]);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0500, 0])[3], 0);
    assert_eq!(issue(&mut cd, [0x5100, 0, 0x0600, 0])[3], 1);
    issue(&mut cd, [0x5200, 0, 0x0400, 0xFFFF]);
    assert_eq!(issue(&mut cd, [0x5300, 0, 0, 0])[1], 2048);
    assert_eq!(issue(&mut cd, [0x5400, 0, 0x0300, 0])[1], 151);
    issue(&mut cd, [0x5500, 0, 0x0400, 151]);
    assert_eq!(issue(&mut cd, [0x5600, 0, 0, 0])[1], 1);
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
    issue(&mut cd, [0x7000, 0, 0x00FF, 0xFFFF]);
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
