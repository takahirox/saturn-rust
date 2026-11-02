//! MAME-produced original MIT media; independent source media are the oracle.
use saturn_rust::{Boot, Machine, bus::Memory, disc::SectorReader, media::Media};
const DATA: &[u8] = include_bytes!("fixtures/cd-stream.iso");
const ISO: &[u8] = include_bytes!("../web/hardware-lab.iso");
const AUDIO: &[u8] = include_bytes!("../web/hardware-lab-audio.bin");
fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/chd/{name}.chd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn mixed(gaps: bool) -> Media {
    Media::cue(
        if gaps {
            include_str!("fixtures/chd/gaps.cue")
        } else {
            include_str!("fixtures/chd/mixed.cue")
        },
        &[
            ("data.iso", ISO),
            ("audio.bin", AUDIO),
            ("mode2.bin", include_bytes!("fixtures/mode2.bin")),
        ],
    )
    .unwrap()
}
fn equivalent(a: &Media, b: &Media) {
    assert_eq!(a.toc(), b.toc());
    assert_eq!(a.leadout(), b.leadout());
    for (a, b) in a.tracks().iter().zip(b.tracks()) {
        assert_eq!(
            (a.number, a.mode, a.control, a.index0, a.start, a.end),
            (b.number, b.mode, b.control, b.index0, b.start, b.end)
        );
        for n in 0..=99 {
            assert_eq!(a.index(n), b.index(n));
        }
    }
    // Every logical byte, including stored/unstored pregap and postgap edges,
    // padding removal, raw headers/checksums, stereo order and leadout errors.
    for fad in a.tracks()[0].index0..=a.leadout() {
        assert_eq!(a.raw_sector(fad), b.raw_sector(fad), "raw FAD {fad}");
        assert_eq!(
            a.cooked_sector(fad),
            b.cooked_sector(fad),
            "cooked FAD {fad}"
        );
    }
    assert_eq!(a.sector_count(), b.sector_count());
    for lba in 0..a.sector_count().unwrap() {
        assert_eq!(a.read_sector(lba), b.read_sector(lba), "boot LBA {lba}");
    }
}
#[test]
fn mame_cd_codecs_match_every_sector_track_toc_and_boot_byte() {
    for codec in ["default", "cdlz", "cdzl", "cdfl", "none"] {
        equivalent(
            &Media::iso(DATA).unwrap(),
            &Media::chd(&fixture(&format!("data-{codec}"))).unwrap(),
        );
        equivalent(
            &mixed(false),
            &Media::chd(&fixture(&format!("mixed-{codec}"))).unwrap(),
        );
    }
    equivalent(&mixed(true), &Media::chd(&fixture("gaps-cdlz")).unwrap());
    let raw = Media::cue(
        include_str!("fixtures/chd/raw.cue"),
        &[
            (
                "mode1.bin",
                include_bytes!("fixtures/large-media-mode1.raw"),
            ),
            ("xa.bin", include_bytes!("fixtures/large-media-xa.raw")),
            ("mode2.bin", include_bytes!("fixtures/mode2.bin")),
            ("audio.bin", AUDIO),
        ],
    )
    .unwrap();
    equivalent(&raw, &Media::chd(&fixture("raw-cdlz")).unwrap());
    let mut changed = DATA.to_vec();
    changed[64 * 2048] ^= 0x1F;
    equivalent(
        &Media::iso(&changed).unwrap(),
        &Media::chd(&fixture("changed-cdlz")).unwrap(),
    );
}
#[test]
fn malformed_chd_errors_are_bounded_and_actionable() {
    assert!(
        Media::chd(&fixture("self-cycle"))
            .err()
            .unwrap()
            .0
            .contains("cyclic self")
    );
    let original = fixture("data-cdlz");
    for length in [
        0,
        7,
        16,
        63,
        123,
        124,
        original.len() / 2,
        original.len() - 1,
    ] {
        assert!(
            Media::chd(&original[..length]).is_err(),
            "truncation {length}"
        );
    }
    for (at, value, message) in [
        (12, 4u32, "v5"),
        (16, u32::from_be_bytes(*b"cdzs"), "codec"),
        (56, u32::MAX, "hunks"),
        (60, 2048, "2448"),
    ] {
        let mut bytes = original.clone();
        bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
        assert!(Media::chd(&bytes).err().unwrap().0.contains(message));
    }
    let mut parent = original.clone();
    parent[104] = 1;
    assert!(Media::chd(&parent).err().unwrap().0.contains("parent"));
    let meta = u64::from_be_bytes(original[48..56].try_into().unwrap()) as usize;
    let mut cycle = original.clone();
    cycle[meta + 8..meta + 16].copy_from_slice(&(meta as u64).to_be_bytes());
    assert!(Media::chd(&cycle).err().unwrap().0.contains("cyclic"));
    let map = u64::from_be_bytes(original[40..48].try_into().unwrap()) as usize;
    for (at, value) in [(map, 255), (map + 12, 255), (map + 16, 0x1F), (124, 0xFF)] {
        let mut bytes = original.clone();
        bytes[at] ^= value;
        assert!(Media::chd(&bytes).is_err(), "corrupt byte {at}");
    }
    // Corrupt uncompressed data too: CRC-less images can still have raw SHA-1.
    let mut bytes = fixture("data-none");
    bytes[64..84].copy_from_slice(&original[64..84]);
    let map = u64::from_be_bytes(bytes[40..48].try_into().unwrap()) as usize;
    let hunk = u32::from_be_bytes(bytes[56..60].try_into().unwrap()) as usize;
    let at = u32::from_be_bytes(bytes[map..map + 4].try_into().unwrap()) as usize * hunk;
    bytes[at] ^= 1;
    assert!(Media::chd(&bytes).err().unwrap().0.contains("SHA-1"));
}
#[test]
fn corrupted_codec_payloads_and_maps_never_panic() {
    for codec in ["cdlz", "cdzl", "cdfl", "none"] {
        let original = fixture(&format!("mixed-{codec}"));
        for i in 0..128 {
            let mut bytes = original.clone();
            let at = 124 + (i * 7919) % (bytes.len() - 124);
            bytes[at] ^= 1 << (i % 8);
            let _ = Media::chd(&bytes);
        }
    }
}

fn run_lab(media: Media) -> (Vec<u32>, Vec<u8>, Vec<i16>) {
    let mut m = Machine::new();
    m.reset_media(media).unwrap();
    for _ in 0..40 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(0x06010020) == Ok(0x600D) {
            break;
        }
    }
    let fields = (0..11)
        .map(|i| m.bus().read32(0x06010000 + i * 4).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(fields[..6], [1, 1, 84, 35, 42, 393984]);
    assert!(fields[6] >= 1);
    assert_eq!(fields[7..9], [1, 0x600D]);
    assert_eq!(fields[10], 0x900);
    let dma = (0..2048)
        .map(|i| m.bus().read8(0x06011000 + i).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(dma, ISO[60 * 2048..61 * 2048]);
    let pcm = m.drain_audio(8192);
    assert!(pcm.iter().any(|v| *v != 0));
    (fields, dma, pcm)
}
#[test]
fn hle_guest_fifo_scu_dma_irq_and_cdda_are_identical_without_sidecars() {
    let baseline = run_lab(mixed(false));
    for codec in ["default", "cdlz", "cdzl", "cdfl", "none"] {
        assert_eq!(
            run_lab(Media::chd(&fixture(&format!("mixed-{codec}"))).unwrap()),
            baseline
        );
    }
}
#[test]
fn retained_chd_reset_reuses_the_same_boot_and_media_contract() {
    let mut m = Machine::new();
    m.reset_media(Media::chd(&fixture("mixed-cdlz")).unwrap())
        .unwrap();
    for _ in 0..30 {
        m.run_steps(10000).unwrap();
    }
    m.reset_loaded_disc().unwrap();
    for _ in 0..30 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x06010020), Ok(0x600D));
    assert_eq!(m.mode(), Some(saturn_rust::BootMode::HleDisc));
    let mut rom = vec![0; 524288];
    rom[..8].copy_from_slice(&[0, 0, 2, 0, 6, 16, 0, 0]);
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    let before = m.cpu_state().pc;
    m.mount_media(Media::chd(&fixture("data-cdlz")).unwrap());
    assert_eq!(m.cpu_state().pc, before);
    assert_eq!(m.mode(), Some(saturn_rust::BootMode::ExternalBios));
}

fn probe_rom(count: u16, audio: u16) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/chd/probe.hex")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom[0x100..0x102].copy_from_slice(&count.to_be_bytes());
    rom[0x102..0x104].copy_from_slice(&audio.to_be_bytes());
    rom
}
fn probe(media: Media, data: &[u8], audio: u16) -> (Vec<u8>, Vec<i16>) {
    let toc = media.toc();
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&probe_rom(
        (data.len() / 2048) as u16,
        audio,
    )))
    .unwrap();
    m.mount_media(media);
    for _ in 0..800 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(0x0601fffc) == Ok(0x600d) {
            break;
        }
    }
    assert_eq!(
        m.bus().read32(0x0601fffc),
        Ok(0x600d),
        "pc {:08x}",
        m.cpu_state().pc
    );
    let output = (0..408)
        .map(|i| m.bus().read8(0x06020000 + i).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(output, toc);
    let output = (0..data.len())
        .map(|i| m.bus().read8(0x06021000 + i as u32).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(output, data);
    for _ in 0..20 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.mode(), Some(saturn_rust::BootMode::ExternalBios));
    assert_eq!(m.bus().read32(0x06000320), Ok(0));
    assert_eq!(m.bus().read32(0x25818000), Ok(u32::MAX));
    let pcm = m.drain_audio(8192);
    assert_eq!(pcm.iter().any(|v| *v != 0), audio != 0);
    (output, pcm)
}
#[test]
fn external_firmware_reads_complete_toc_and_all_data_and_preserves_stereo() {
    let data = probe(Media::iso(DATA).unwrap(), DATA, 0);
    let mixed = probe(mixed(false), ISO, 214);
    for codec in ["default", "cdlz", "cdzl", "cdfl", "none"] {
        assert_eq!(
            probe(
                Media::chd(&fixture(&format!("data-{codec}"))).unwrap(),
                DATA,
                0
            ),
            data
        );
        assert_eq!(
            probe(
                Media::chd(&fixture(&format!("mixed-{codec}"))).unwrap(),
                ISO,
                214
            ),
            mixed
        );
    }
    let gaps = probe(self::mixed(true), ISO, 214);
    assert_eq!(
        probe(Media::chd(&fixture("gaps-cdlz")).unwrap(), ISO, 214),
        gaps
    );
}

#[test]
fn standard_chd_subcode_preserves_data_audio_without_inventing_reconstructed_packs() {
    use saturn_rust::{
        bus::Bus,
        cd::{DATA as FIFO, REGISTERS as CD},
    };
    let original = Media::cue("FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"a\" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00", &[
        ("d",include_bytes!("fixtures/cd-rw.iso")),
        ("a",include_bytes!("fixtures/cd-rw-audio.bin")),
    ]).unwrap();
    let medium = Media::chd(&fixture("subcode-cdlz")).unwrap();
    equivalent(&original, &medium);
    let mut bus = Bus::default();
    bus.cd.mount(medium);
    let command = |bus: &mut Bus, words: [u16; 4]| {
        for (i, word) in words.into_iter().enumerate() {
            bus.write16(CD + 0x18 + i as u32 * 4, word).unwrap();
        }
        bus.cd.advance(1075);
        std::array::from_fn::<_, 4, _>(|i| bus.read16(CD + 0x18 + i as u32 * 4).unwrap())
    };
    command(&mut bus, [0x0402, 0, 0, 0]);
    command(&mut bus, [0x1080, 214, 0x80, 20]);
    bus.cd.advance(3_000_000);
    assert_ne!(
        command(&mut bus, [0x2001, 0, 0, 0])[0] & 0x8000,
        0,
        "raw optical symbols must not masquerade as deinterleaved packs"
    );
    assert_eq!(bus.read32(FIFO), Ok(u32::MAX));
}

#[test]
fn cd_flac_rejects_non_stereo_or_non_16_bit_frames_including_later_frames() {
    use chd::map::{CompressionTypeV5, MapEntry};
    use std::io::Cursor;
    let original = fixture("mixed-cdfl");
    let image = chd::Chd::open(Cursor::new(&original), None).unwrap();
    let entry = image
        .map()
        .iter()
        .find_map(|entry| match entry {
            MapEntry::V5Compressed(e)
                if matches!(e.hunk_type(), Ok(CompressionTypeV5::CompressionType0)) =>
            {
                Some((e.block_offset() as usize, e.block_size() as usize))
            }
            _ => None,
        })
        .unwrap();
    let input = &original[entry.0..entry.0 + entry.1];
    let mut reader = claxon::frame::FrameReader::new(Cursor::new(input));
    let block = reader.read_next_or_eof(Vec::new()).unwrap().unwrap();
    assert_eq!(block.channels(), 2);
    let next = reader.into_inner().position() as usize;
    assert!(
        next + 3 < entry.1,
        "fixture must contain a second FLAC frame"
    );
    for position in [entry.0 + 3, entry.0 + next + 3] {
        for flags in [0x08, 0x1C] {
            // mono 16-bit; stereo 24-bit
            let mut bytes = original.clone();
            bytes[position] = flags;
            assert!(
                Media::chd(&bytes)
                    .err()
                    .unwrap()
                    .0
                    .contains("stereo 16-bit")
            );
        }
    }
}
