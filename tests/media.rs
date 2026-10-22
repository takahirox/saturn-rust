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
    assert!(media.boot_iso().is_err()); // not a 2048-byte ISO logical sector
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
    assert_eq!(media.boot_iso().unwrap(), bytes);
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
