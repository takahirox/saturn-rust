//! Source-authored media and malformed-input vectors. No downloaded discs.
use saturn_rust::media::{Media, Mode2Format, RAW_SECTOR, TrackMode};

#[test]
fn ecma_mode_two_data_preserves_all_2336_bytes_without_xa_parity_or_subheaders() {
    // ECMA-130 §14 fig12 / §14.2. The trailer is ordinary data, not
    // a corrupt XA EDC/ECC. Exercise every byte value in the payload.
    let cue = "FILE \"data.bin\" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00";
    let mut raw = [0; RAW_SECTOR];
    raw[1..11].fill(255);
    raw[12..16].copy_from_slice(&[0, 2, 0, 2]);
    for (i, byte) in raw[16..].iter_mut().enumerate() {
        *byte = ((i * 73) ^ (i >> 3)) as u8;
    }
    let media = Media::cue(cue, &[("data.bin", &raw)]).unwrap();
    assert_eq!(media.raw_sector(150).unwrap(), raw);
    assert_eq!(media.cooked_sector(150).unwrap(), raw[16..]);
    assert!(saturn_rust::disc::SectorReader::read_sector(&media, 0).is_err()); // not a 2048-byte ISO logical sector
    // An unformatted Mode2 payload may even look like a duplicated XA
    // subheader. Explicit source metadata resolves that ambiguity, and
    // rejected XA is never silently retried as unchecked Mode2 data.
    for value in 0..=255 {
        raw[16..].fill(value);
        let data =
            Media::cue_with_mode2_format(cue, &[("data.bin", &raw)], Mode2Format::Data).unwrap();
        assert_eq!(data.cooked_sector(150).unwrap(), vec![value; 2336]);
    }
    raw[13] = 0x60;
    let bad = Media::cue_with_mode2_format(cue, &[("data.bin", &raw)], Mode2Format::Data).unwrap();
    assert!(bad.raw_sector(150).is_err());
    raw[13] = 2;
    raw[16..24].copy_from_slice(&[1, 2, 8, 0, 1, 2, 8, 0]);
    let xa = Media::cue(cue, &[("data.bin", &raw)]).unwrap();
    assert!(xa.raw_sector(150).is_err());
}

#[test]
fn cue_subindices_translate_file_offsets_without_creating_new_tracks_or_sectors() {
    let bytes: Vec<_> = (0..8).flat_map(|n| vec![n; RAW_SECTOR]).collect();
    let cue = "FILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nINDEX 02 00:00:02\nINDEX 03 00:00:03\nTRACK 02 AUDIO\nPREGAP 00:00:02\nINDEX 00 00:00:04\nINDEX 01 00:00:05\nINDEX 02 00:00:06\nINDEX 99 00:00:07\n";
    let m = Media::cue(cue, &[("a.bin", &bytes)]).unwrap();
    assert_eq!(m.tracks().len(), 2);
    let indices: Vec<_> = (150..160)
        .map(|fad| m.track(fad).unwrap().index_at(fad))
        .collect();
    assert_eq!(indices, [1, 1, 2, 3, 0, 0, 0, 1, 2, 99]);
    assert_eq!(m.tracks()[1].index(0), Some(154));
    assert_eq!(m.tracks()[1].index(1), Some(157));
    assert_eq!(m.tracks()[1].index(2), Some(158));
    assert_eq!(m.tracks()[1].index(99), Some(159));
    assert_eq!(m.tracks()[1].index(3), None);
    assert_eq!(m.raw_sector(158).unwrap(), [6; RAW_SECTOR]);
    for invalid in [
        "INDEX 02 00:00:00",                    // coincides with index 1
        "INDEX 02 00:00:08",                    // outside track
        "INDEX 03 00:00:02\nINDEX 02 00:00:03", // reversed numbers
        "INDEX 02 00:00:03\nINDEX 03 00:00:02", // reversed times
        "INDEX 02 00:00:02\nINDEX 02 00:00:03", // duplicate number
        "INDEX 100 00:00:02",
    ] {
        let cue = format!("FILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\n{invalid}\n");
        assert!(Media::cue(&cue, &[("a.bin", &bytes)]).is_err(), "{invalid}");
    }
}

#[test]
fn cooked_and_raw_data_have_identical_user_bytes_and_header_addresses() {
    let bytes: Vec<_> = (0..4096).map(|i| (i ^ (i >> 8)) as u8).collect();
    let iso = Media::iso(&bytes).unwrap();
    assert_eq!(iso.leadout(), 152);
    let raw = iso.raw_sector(151).unwrap();
    assert_eq!(
        &raw[..16],
        &[
            0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 2, 1, 1
        ]
    );
    assert_eq!(&raw[16..2064], &bytes[2048..]);
    assert!(raw[2064..].iter().any(|x| *x != 0));
    let mut bin = iso.raw_sector(150).unwrap().to_vec();
    bin.extend(raw);
    let cue = "FILE \"data.bin\" BINARY\n TRACK 01 MODE1/2352\n INDEX 01 00:00:00\n";
    let media = Media::cue(cue, &[("data.bin", &bin)]).unwrap();
    assert_eq!(
        saturn_rust::disc::SectorReader::read_sector(&media, 0).unwrap(),
        bytes[..2048]
    );
    assert_eq!(media.cooked_sector(151).unwrap(), bytes[2048..]);
    assert!(media.raw_sector(152).is_err());
    bin[1] = 3;
    assert!(
        Media::cue(cue, &[("data.bin", &bin)])
            .unwrap()
            .raw_sector(150)
            .is_err()
    );
}
#[test]
fn shared_bin_indices_pregaps_postgaps_and_toc_are_distinct() {
    let mut bytes = vec![0; 6 * RAW_SECTOR];
    for (i, s) in bytes.chunks_mut(RAW_SECTOR).enumerate() {
        s.fill(i as u8 + 1);
    }
    let cue = "FILE \"mixed.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nPREGAP 00:00:02\nINDEX 00 00:00:02\nINDEX 01 00:00:03\nPOSTGAP 00:00:01\n";
    let m = Media::cue(cue, &[("mixed.bin", &bytes)]).unwrap();
    assert_eq!(
        m.tracks()
            .iter()
            .map(|t| (t.index0, t.start, t.end))
            .collect::<Vec<_>>(),
        [(150, 150, 152), (152, 155, 159)]
    );
    assert_eq!(m.raw_sector(151).unwrap()[0], 2);
    assert_eq!(m.raw_sector(152).unwrap(), [0; RAW_SECTOR]);
    assert_eq!(m.raw_sector(154).unwrap()[0], 3);
    assert_eq!(m.raw_sector(155).unwrap()[0], 4);
    assert_eq!(m.raw_sector(158).unwrap(), [0; RAW_SECTOR]);
    let toc = m.toc();
    assert_eq!(&toc[0..8], &[1, 0, 0, 150, 1, 0, 0, 155]);
    assert_eq!(&toc[396..408], &[1, 1, 0, 0, 1, 2, 0, 0, 1, 0, 0, 159]);
}
#[test]
fn separate_files_mixed_audio_data_and_missing_or_ambiguous_references() {
    let iso = vec![0x35; 4096];
    let audio = vec![0x42; RAW_SECTOR * 2];
    let cue = "FILE \"data.iso\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"music.bin\" BINARY\nTRACK 02 AUDIO\nPREGAP 00:00:01\nINDEX 01 00:00:00\n";
    assert!(Media::cue(cue, &[("data.iso", &iso)]).is_err());
    assert!(
        Media::cue(
            cue,
            &[
                ("data.iso", &iso),
                ("data.iso", &iso),
                ("music.bin", &audio)
            ]
        )
        .is_err()
    );
    let m = Media::cue(cue, &[("data.iso", &iso), ("music.bin", &audio)]).unwrap();
    assert_eq!(m.tracks()[1].start, 153);
    assert_eq!(m.leadout(), 155);
    assert_eq!(m.tracks()[1].mode, TrackMode::Audio);
    assert_eq!(m.raw_sector(153).unwrap()[0], 0x42);
    assert!(m.cooked_sector(153).is_err());
}
#[test]
fn malformed_cue_and_sector_inputs_never_panic_or_allocate_unboundedly() {
    let bytes = vec![0; RAW_SECTOR * 4];
    let prefix = "FILE \"data.bin\" BINARY\n";
    for bad in [
        "TRACK 01 AUDIO\n",
        "TRACK 02 AUDIO\nINDEX 01 00:00:00",
        "TRACK 01 AUDIO\nINDEX 01 00:60:00",
        "TRACK 01 AUDIO\nINDEX 01 00:00:75",
        "TRACK 01 AUDIO\nINDEX 01 00:00:04",
        "TRACK 01 AUDIO\nINDEX 01 00:00:00\nINDEX 00 00:00:00",
        "TRACK 01 MODE1/2048\nINDEX 01 00:00:00",
        "FILE \"unterminated BINARY",
        "TRACK 01 AUDIO\nINDEX 01 00:00:00\nTRACK 02 AUDIO\nINDEX 01 00:00:00",
    ] {
        assert!(
            Media::cue(&(prefix.to_owned() + bad), &[("data.bin", &bytes)]).is_err(),
            "{bad}"
        );
    }
    for n in 0..RAW_SECTOR {
        let data = vec![0x61; n];
        assert!(
            Media::cue(
                &(prefix.to_owned() + "TRACK 01 AUDIO\nINDEX 01 00:00:00"),
                &[("data.bin", &data)]
            )
            .is_err()
        );
    }
    // Deterministic mutation corpus covers parser boundaries and non-ASCII text.
    let valid = prefix.to_owned() + "TRACK 01 AUDIO\nINDEX 01 00:00:00";
    for (i, _) in valid.char_indices() {
        for byte in [0u8, b'\n', b'"', b'9', 255] {
            let mut text = valid.as_bytes().to_vec();
            text[i] = byte;
            let _ = Media::cue(&String::from_utf8_lossy(&text), &[("data.bin", &bytes)]);
        }
    }
}

#[test]
fn cue_control_flags_reach_toc_and_reject_invalid_or_misplaced_flags() {
    // ST-040 §2.2 TOC CONTROL: data/copy/pre-emphasis/channel bits belong
    // to each track; they must not be discarded while accepting a CUE file.
    let audio = vec![0; 2352 * 2];
    let cue = "FILE \"a\" BINARY\nTRACK 01 AUDIO\nFLAGS PRE DCP 4CH\nINDEX 01 00:00:00";
    let media = Media::cue(cue, &[("a", &audio)]).unwrap();
    assert_eq!(media.tracks()[0].control, 11);
    assert_eq!(media.toc()[0], 0xB1);
    assert_eq!(media.toc()[396], 0xB1);
    assert_eq!(media.toc()[400], 0xB1);
    for flags in [
        "",
        "DCP DCP",
        "PRE PRE",
        "4CH 4CH",
        "SCMS",
        "DCP\nFLAGS PRE",
    ] {
        let malformed = cue.replace("PRE DCP 4CH", flags);
        assert!(Media::cue(&malformed, &[("a", &audio)]).is_err(), "{flags}");
    }
    assert!(Media::cue(&format!("FLAGS DCP\n{cue}"), &[("a", &audio)]).is_err());
    let data = vec![0; 2048 * 2];
    for flag in ["PRE", "4CH"] {
        let cue =
            format!("FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nFLAGS {flag}\nINDEX 01 00:00:00");
        assert!(Media::cue(&cue, &[("d", &data)]).is_err());
    }
}

#[test]
fn mode_one_product_code_corrects_every_symbol_position_and_rejects_unrecoverable_data() {
    let image = Media::iso(&(0..2048).map(|i| (i ^ (i >> 4)) as u8).collect::<Vec<_>>()).unwrap();
    let raw = image.raw_sector(150).unwrap();
    let cue = "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00";
    // All header/data/EDC/reserved/P/Q positions; all nonzero byte deltas at
    // selected locations, plus a burst crossing multiple P and Q codewords.
    for index in 12..2352 {
        let mut damaged = raw;
        damaged[index] ^= ((index * 31) % 255 + 1) as u8;
        let media = Media::cue(cue, &[("d", &damaged)]).unwrap();
        assert_eq!(media.raw_sector(150).unwrap(), raw, "offset {index}");
    }
    for error in 1..=255 {
        for index in [12, 1024, 2064, 2076, 2248, 2351] {
            let mut damaged = raw;
            damaged[index] ^= error;
            assert_eq!(
                Media::cue(cue, &[("d", &damaged)])
                    .unwrap()
                    .raw_sector(150)
                    .unwrap(),
                raw
            );
        }
    }
    let mut damaged = raw;
    for byte in &mut damaged[512..544] {
        *byte ^= 0xB7;
    }
    assert_eq!(
        Media::cue(cue, &[("d", &damaged)])
            .unwrap()
            .raw_sector(150)
            .unwrap(),
        raw
    );
    damaged[16..2064].fill(0);
    assert!(
        Media::cue(cue, &[("d", &damaged)])
            .unwrap()
            .cooked_sector(150)
            .is_err()
    );
    // An adversary can recompute EDC but cannot make mismatched parity valid.
    damaged = raw;
    damaged[2076..].fill(0);
    assert!(
        Media::cue(cue, &[("d", &damaged)])
            .unwrap()
            .raw_sector(150)
            .is_err()
    );
}

#[test]
fn resource_policy_retains_broader_runtime_layout_and_checked_owned_ranges() {
    use saturn_rust::disc::SectorReader;
    let cue = "FILE \"d\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nPOSTGAP 99:59:74\n";
    let media = Media::cue(cue, &[("d", &[0; 2048])]).unwrap();
    assert_eq!(media.leadout(), 450150);
    assert_eq!(media.sector_count(), Ok(1)); // synthesized gap is not bootstrap storage
    assert!(media.read_sector(1).is_err());
    assert!(media.read_sector(u32::MAX).is_err());
    assert!(Media::cue(cue, &[("d", &[0; 4096])]).is_err()); // one frame beyond policy
    assert!(Media::cue_owned(cue, &[("d", 0..2049)], vec![0; 2048]).is_err());
    assert!(
        Media::cue_owned(
            cue,
            &[("d", std::ops::Range { start: 8, end: 1 })],
            vec![0; 2048]
        )
        .is_err()
    );
}

// Exercise the same names and bytes through the borrowed native and owned Wasm
// storage boundaries, including nonzero offsets and reversed file-table order.
fn cue_apis(
    cue: &str,
    files: &[(&str, &[u8])],
) -> [Result<Media, saturn_rust::disc::DiscError>; 2] {
    let mut bytes = vec![0xEE; 13];
    let names: Vec<_> = files
        .iter()
        .map(|(name, data)| {
            let start = bytes.len();
            bytes.extend_from_slice(data);
            (*name, start..bytes.len())
        })
        .collect();
    [Media::cue(cue, files), Media::cue_owned(cue, &names, bytes)]
}

#[test]
fn cue_ascii_case_single_raw_bin_preserves_complete_sectors() {
    let cooked: Vec<_> = (0..4096).map(|i| (i ^ (i >> 5)) as u8).collect();
    let iso = Media::iso(&cooked).unwrap();
    let raw: Vec<_> = (150..152)
        .flat_map(|fad| iso.raw_sector(fad).unwrap())
        .collect();
    let cue = "FILE \"GAME.BIN\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00";
    for result in cue_apis(cue, &[("game.bin", &raw)]) {
        let media = result.unwrap();
        assert_eq!(media.leadout(), 152);
        assert_eq!(media.tracks()[0].mode, TrackMode::Mode1);
        for fad in 150..152 {
            let offset = (fad - 150) as usize;
            assert_eq!(
                media.raw_sector(fad).unwrap(),
                raw[offset * RAW_SECTOR..(offset + 1) * RAW_SECTOR]
            );
            assert_eq!(
                media.cooked_sector(fad).unwrap(),
                cooked[offset * 2048..(offset + 1) * 2048]
            );
        }
    }
}

#[test]
fn cue_ascii_case_multifile_preserves_order_indices_gaps_toc_and_audio() {
    let data = vec![0x35; 4096];
    let audio: Vec<_> = (0..4).flat_map(|n| vec![0x40 + n; RAW_SECTOR]).collect();
    let cue = "FILE \"DaTa.IsO\" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE \"MuSiC.BiN\" BINARY\nTRACK 02 AUDIO\nFLAGS PRE DCP\nPREGAP 00:00:01\nINDEX 00 00:00:00\nINDEX 01 00:00:01\nINDEX 02 00:00:02\nPOSTGAP 00:00:01";
    for result in cue_apis(cue, &[("music.bin", &audio), ("DATA.iso", &data)]) {
        let media = result.unwrap();
        assert_eq!(
            media
                .tracks()
                .iter()
                .map(|t| (t.number, t.mode, t.index0, t.start, t.end))
                .collect::<Vec<_>>(),
            [
                (1, TrackMode::Mode1, 150, 150, 152),
                (2, TrackMode::Audio, 152, 154, 158)
            ]
        );
        assert_eq!(media.tracks()[1].index(2), Some(155));
        assert_eq!(media.cooked_sector(151).unwrap(), data[2048..]);
        assert_eq!(media.raw_sector(152).unwrap(), [0; RAW_SECTOR]);
        for (fad, n) in [(153, 0), (154, 1), (155, 2), (156, 3)] {
            assert_eq!(media.raw_sector(fad).unwrap(), [0x40 + n; RAW_SECTOR]);
        }
        assert_eq!(media.raw_sector(157).unwrap(), [0; RAW_SECTOR]);
        assert!(media.cooked_sector(154).is_err());
        assert_eq!(media.leadout(), 158);
        let mut toc = [255; 408];
        toc[..8].copy_from_slice(&[0x41, 0, 0, 150, 0x31, 0, 0, 154]);
        toc[396..].copy_from_slice(&[0x41, 1, 0, 0, 0x31, 2, 0, 0, 1, 0, 0, 158]);
        assert_eq!(media.toc(), toc);
    }
}

#[test]
fn cue_ascii_case_exact_winner_and_ambiguous_or_missing_diagnostics() {
    let cue = "FILE \"GAME.BIN\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00";
    let winner = vec![0x31; RAW_SECTOR];
    let other = vec![0x42; RAW_SECTOR];
    for files in [
        vec![
            ("game.bin", other.as_slice()),
            ("GAME.BIN", winner.as_slice()),
        ],
        vec![
            ("GAME.BIN", winner.as_slice()),
            ("game.bin", other.as_slice()),
        ],
    ] {
        for result in cue_apis(cue, &files) {
            assert_eq!(result.unwrap().raw_sector(150).unwrap(), [0x31; RAW_SECTOR]);
        }
    }
    for (names, error) in [
        (vec!["other.bin"], "Missing CUE file reference"),
        (vec!["game.bin", "Game.Bin"], "Ambiguous CUE file reference"),
        (vec!["Game.Bin", "game.bin"], "Ambiguous CUE file reference"),
        (vec!["GAME.BIN", "GAME.BIN"], "Ambiguous CUE file reference"),
        (
            vec!["game.bin", "GAME.BIN", "GAME.BIN"],
            "Ambiguous CUE file reference",
        ),
    ] {
        let files: Vec<_> = names.iter().map(|n| (*n, winner.as_slice())).collect();
        for result in cue_apis(cue, &files) {
            assert_eq!(result.err().unwrap().0, error);
        }
    }
}

#[test]
fn cue_ascii_case_preserves_paths_special_characters_and_unicode() {
    let bytes = [0x45; RAW_SECTOR];
    for (reference, selected, succeeds) in [
        ("Dir/Track #1 [é].BIN", "dIR/tRACK #1 [é].bin", true),
        ("Dir/Track.BIN", "track.bin", false),
        ("Dir/Track.BIN", "Dir\\track.bin", false),
        ("É.BIN", "é.bin", false),
        ("é.BIN", "e\u{301}.bin", false),
    ] {
        let cue = format!("FILE \"{reference}\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00");
        for result in cue_apis(&cue, &[(selected, &bytes)]) {
            if succeeds {
                assert_eq!(result.unwrap().raw_sector(150).unwrap(), bytes);
            } else {
                assert_eq!(result.err().unwrap().0, "Missing CUE file reference");
            }
        }
    }
}

#[test]
fn cue_ascii_case_optional_companion_uses_the_same_unique_matching_contract() {
    let audio = [0x42; RAW_SECTOR];
    let mut valid = b"SRW1".to_vec();
    valid.extend_from_slice(&150u32.to_be_bytes());
    valid.extend_from_slice(&1u32.to_be_bytes());
    valid.extend_from_slice(&[0; 96]);
    let cue =
        "REM SATURN_RW \"PACKS.SRW\"\nFILE \"a.bin\" BINARY\nTRACK 01 AUDIO\nINDEX 01 00:00:00";
    for files in [
        vec![("a.bin", audio.as_slice()), ("packs.srw", valid.as_slice())],
        // The exact valid companion wins over an invalid case-fold candidate.
        vec![
            ("a.bin", audio.as_slice()),
            ("packs.srw", b"bad".as_slice()),
            ("PACKS.SRW", valid.as_slice()),
        ],
    ] {
        for result in cue_apis(cue, &files) {
            assert_eq!(result.unwrap().raw_sector(150).unwrap(), audio);
        }
    }
    for (names, error) in [
        (vec![], "Missing SRW1 companion"),
        (vec!["packs.srw", "Packs.Srw"], "Ambiguous SRW1 companion"),
        (vec!["PACKS.SRW", "PACKS.SRW"], "Ambiguous SRW1 companion"),
    ] {
        let mut files = vec![("a.bin", audio.as_slice())];
        files.extend(names.iter().map(|n| (*n, valid.as_slice())));
        for result in cue_apis(cue, &files) {
            assert_eq!(result.err().unwrap().0, error);
        }
    }
    // A companion cannot alias a track, even through the case fallback.
    for result in cue_apis(&cue.replace("PACKS.SRW", "A.BIN"), &[("a.bin", &audio)]) {
        assert_eq!(result.err().unwrap().0, "Ambiguous SRW1 companion");
    }
    // No REM directive: ordinary CUE/BIN still needs no companion.
    for result in cue_apis(cue.split_once('\n').unwrap().1, &[("a.bin", &audio)]) {
        assert_eq!(result.unwrap().raw_sector(150).unwrap(), audio);
    }
}
