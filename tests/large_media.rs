//! Wholly original MIT guest/layout: fixtures/build_large_media.py + LICENSE.
use saturn_rust::{
    BootMode, Machine,
    bus::Memory,
    disc::{self, DiscError, SECTOR, SectorReader},
    media::Media,
};
use std::cell::RefCell;
const SEED: &[u8] = include_bytes!("fixtures/large-media.seed");
const FIRST: usize = 9003;
const LATE: usize = 260000;
const SIZE: usize = 0x1400;
fn image(count: usize) -> Vec<u8> {
    let mut b = vec![0; count * SECTOR];
    b[..22 * SECTOR].copy_from_slice(&SEED[..22 * SECTOR]);
    dual(&mut b, 16 * SECTOR + 80, count as u32);
    for i in 0..SIZE.div_ceil(SECTOR) {
        let n = (SIZE - i * SECTOR).min(SECTOR);
        let at = (FIRST + i / 2 * 3 + i % 2) * SECTOR;
        b[at..at + n].copy_from_slice(&SEED[24 * SECTOR + i * SECTOR..][..n]);
    }
    if count > LATE {
        b[LATE * SECTOR..(LATE + 1) * SECTOR].copy_from_slice(&SEED[28 * SECTOR..29 * SECTOR]);
    }
    b
}
fn dual(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_le_bytes());
    b[at + 4..at + 8].copy_from_slice(&n.to_be_bytes());
}
fn run(m: &mut Machine) {
    for _ in 0..80 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(0x0601000C) == Ok(0x600D) {
            return;
        }
    }
    panic!(
        "late guest pc {:08X}, sum {:?}, dma {:?}, irq {:?}",
        m.cpu_state().pc,
        m.bus().read32(0x06010000),
        m.bus().read32(0x06010004),
        m.bus().read32(0x06010008)
    );
}
fn check(m: &Machine, sum: u32) {
    assert_eq!(m.mode(), Some(BootMode::HleDisc));
    assert_eq!(m.bus().read32(0x06010000), Ok(sum));
    assert_eq!(m.bus().read32(0x06010004), Ok(1));
    assert!(m.bus().read32(0x06010008).unwrap() > 0);
    assert_eq!(m.bus().read32(0x06011000), Ok(3));
    assert_eq!(m.bus().read32(0x060117FC), Ok(1536));
    assert!(m.cpu_state().exceptions >= 2); // actual DMA/CD interrupt entries
}
#[test]
fn standard_size_iso_and_mixed_cue_execute_late_read_and_guest_sysres() {
    // Keep large allocations sequential; ownership is transferred, never cloned.
    for mixed in [false, true] {
        let bytes = image(270000);
        let media = if mixed {
            let mut bytes = bytes;
            for _ in 0..300 * 588 {
                bytes.extend([0, 16, 0, 248]);
            }
            Media::cue_owned(
                include_str!("fixtures/large-media.cue"),
                &[
                    ("large-media.iso", 0..552960000),
                    ("large-media-audio.bin", 552960000..553665600),
                ],
                bytes,
            )
            .unwrap()
        } else {
            Media::iso_owned(bytes).unwrap()
        };
        let mut m = Machine::new();
        m.reset_media(media).unwrap();
        run(&mut m);
        check(&m, 393991); // 3*(1+...+512) + guest literal 7
        assert_eq!(m.backup_ram()[0], 1);
        if mixed {
            let media = m.bus().cd.media().unwrap();
            assert_eq!(media.tracks()[0].end, 270300);
            assert_eq!(media.tracks()[1].index0, 270300);
            assert_eq!(media.tracks()[1].start, 270450);
            assert_eq!(media.leadout(), 270750);
            assert_eq!(&media.toc()[4..8], &0x01042072u32.to_be_bytes());
            for _ in 0..3 {
                m.advance_frame().unwrap();
            }
            assert!(
                m.drain_audio(8192)
                    .as_chunks::<2>()
                    .0
                    .contains(&[4096, -2048])
            );
        }
        m.set_buttons(128);
        for _ in 0..80 {
            m.run_steps(10000).unwrap();
            if m.backup_ram()[0] == 2 {
                break;
            }
        }
        m.set_buttons(0);
        assert_eq!(m.backup_ram()[0], 2);
        run(&mut m);
        check(&m, 393991);
        assert_eq!(
            m.bus().cd.media().unwrap().tracks().len(),
            if mixed { 2 } else { 1 }
        );
        m.reset_loaded_disc().unwrap();
        run(&mut m);
        check(&m, 393991);
        assert_eq!(m.backup_ram()[0], 3);
    }
}

struct Observed<'a> {
    bytes: &'a [u8],
    reads: RefCell<Vec<u32>>,
    count: u32,
}
impl SectorReader for Observed<'_> {
    fn sector_count(&self) -> Result<u32, DiscError> {
        Ok(self.count)
    }
    fn read_sector(&self, lba: u32) -> Result<[u8; SECTOR], DiscError> {
        self.reads.borrow_mut().push(lba);
        self.bytes.read_sector(lba)
    }
}
#[test]
fn bootstrap_reads_only_metadata_and_interleaved_file_not_the_medium() {
    let bytes = image(9100);
    let reader = Observed {
        bytes: &bytes,
        reads: RefCell::default(),
        count: 270000,
    };
    let boot = disc::parse_sectors(&reader).unwrap();
    assert_eq!(boot.executable, SEED[24 * SECTOR..24 * SECTOR + SIZE]);
    assert_eq!(
        *reader.reads.borrow(),
        [0, 0, 1, 16, 17, 20, 21, 9003, 9004, 9006]
    );
    // Reader capacity can exceed actual available storage; required reads fail.
    let truncated = Observed {
        bytes: &bytes[..FIRST * SECTOR],
        reads: RefCell::default(),
        count: 270000,
    };
    assert!(disc::parse_sectors(&truncated).is_err());
    for count in [0, 17, disc::MAX_SECTORS + 1, u32::MAX] {
        let bad = Observed {
            bytes: &bytes,
            reads: RefCell::default(),
            count,
        };
        assert!(disc::parse_sectors(&bad).is_err());
        assert!(bad.reads.borrow().is_empty());
    }
}

#[test]
fn declared_volume_overflow_cross_sector_directory_and_first_read_load_area_vectors() {
    let original = image(9100);
    for (at, value) in [
        (16 * SECTOR + 80, 9006),
        (21 * SECTOR + 2, u32::MAX),
        (21 * SECTOR + 10, u32::MAX),
    ] {
        let mut bytes = original.clone();
        dual(&mut bytes, at, value);
        assert!(disc::parse(&bytes).is_err());
    }
    // The directory spans sectors; ISO directory records themselves may not.
    let mut bytes = original.clone();
    bytes[20 * SECTOR + 68] = 255;
    assert!(disc::parse(&bytes).is_err());
    // A valid first read plus many valid zero-length children, followed by a
    // record whose length crosses the logical-sector boundary.
    let mut crossing = original.clone();
    let mut record = crossing[20 * SECTOR..20 * SECTOR + 34].to_vec();
    record[25] = 0;
    record[33] = b'X';
    dual(&mut record, 2, FIRST as u32);
    dual(&mut record, 10, 0);
    let mut at = 21 * SECTOR + usize::from(crossing[21 * SECTOR]);
    while at % SECTOR + 34 <= SECTOR {
        crossing[at..at + 34].copy_from_slice(&record);
        at += 34;
    }
    crossing[at] = 34;
    assert!(
        disc::parse(&crossing)
            .err()
            .unwrap()
            .0
            .contains("directory record")
    );
    for (address, accepted) in [
        (0x060FDC00u32, true),
        (0x060FDC04, false),
        (0x060FF000, false),
    ] {
        let mut bytes = original.clone();
        bytes[0xF0..0xF4].copy_from_slice(&address.to_be_bytes());
        bytes[0xE8..0xEC].fill(0); // keep stack outside boundary vector
        // SIZE=0x1400, end at 0x060FF000 when address=0x060FDC00.
        assert_eq!(disc::parse(&bytes).is_ok(), accepted);
    }
}

#[test]
fn original_raw_mode1_and_xa_ignore_unread_damage_but_reject_required_damage() {
    for (mode, vectors) in [
        (
            "MODE1/2352",
            include_bytes!("fixtures/large-media-mode1.raw").as_slice(),
        ),
        (
            "MODE2/2352",
            include_bytes!("fixtures/large-media-xa.raw").as_slice(),
        ),
    ] {
        let mut raw = vec![0; 270000 * 2352];
        // Sparse input intentionally has invalid raw sectors everywhere else.
        for (i, lba) in [0, 1, 16, 17, 20, 21, 9003, 9004, 9006]
            .into_iter()
            .enumerate()
        {
            raw[lba * 2352..(lba + 1) * 2352].copy_from_slice(&vectors[i * 2352..(i + 1) * 2352]);
        }
        let cue = format!("FILE \"r\" BINARY\nTRACK 01 {mode}\nINDEX 01 00:00:00");
        let media = Media::cue_owned(&cue, &[("r", 0..raw.len())], raw).unwrap();
        assert!(media.raw_sector(150 + 8000).is_err());
        let boot = disc::parse_sectors(&media).unwrap();
        assert_eq!(boot.executable, SEED[24 * SECTOR..24 * SECTOR + SIZE]);
        let mut m = Machine::new();
        m.reset_media(media).unwrap();
        m.run_steps(32).unwrap();
        assert_eq!(m.cpu_state().status, saturn_rust::cpu::CpuStatus::Running);
        drop(m);
        // Keep the declared volume fully present so failure must come from the
        // damaged required sector, rather than an earlier capacity check.
        let mut raw = vec![0; 270000 * 2352];
        for (i, lba) in [0, 1, 16, 17, 20, 21, 9003, 9004, 9006]
            .into_iter()
            .enumerate()
        {
            raw[lba * 2352..(lba + 1) * 2352].copy_from_slice(&vectors[i * 2352..(i + 1) * 2352]);
        }
        raw[9003 * 2352 + 32..9003 * 2352 + 2048].fill(0xB7);
        let media = Media::cue_owned(&cue, &[("r", 0..raw.len())], raw).unwrap();
        let mut m = Machine::new();
        assert!(m.reset_media(media).is_err());
        assert_eq!(m.cpu_state().status, saturn_rust::cpu::CpuStatus::Reset);
        assert_eq!(m.mode(), Some(BootMode::HleDisc));
        assert_eq!(m.bus().read32(0x06010000), Ok(0));
    }
}

#[test]
fn changed_guest_literal_late_payload_and_omitted_play_command_change_the_result() {
    for mutation in 0..3 {
        let mut bytes = image(270000);
        match mutation {
            0 => {
                let at = FIRST * SECTOR + 0x264;
                assert_eq!(&bytes[at..at + 4], &7u32.to_be_bytes());
                bytes[at + 3] = 11;
            }
            1 => {
                bytes[LATE * SECTOR + 3] = 9;
            }
            _ => {
                let at = FIRST * SECTOR + 0x100;
                assert_eq!(bytes[at] >> 4, 0xB); // omit BSR command for late play
                bytes[at..at + 2].copy_from_slice(&9u16.to_be_bytes());
            }
        }
        let mut m = Machine::new();
        m.reset_media(Media::iso_owned(bytes).unwrap()).unwrap();
        if mutation == 2 {
            for _ in 0..20 {
                m.run_steps(10000).unwrap();
            }
            assert_ne!(m.bus().read32(0x0601000C), Ok(0x600D));
            assert_eq!(m.bus().read32(0x06010000), Ok(0));
        } else {
            run(&mut m);
            assert_eq!(
                m.bus().read32(0x06010000),
                Ok(if mutation == 0 { 393995 } else { 393997 })
            );
        }
    }
}
