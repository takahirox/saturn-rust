//! Original MIT guest/media (fixtures/LICENSE). Sega sound FAQ Overview;
//! ST-077 §4.2 pp.80–88: common final-stage MVOL, clipping remains upstream.
use saturn_rust::{Boot, Machine, bus::Memory, media::Media};

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in include_str!("fixtures/scsp-mvol.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
fn operand(name: &str) -> usize {
    let line = include_str!("fixtures/scsp-mvol.tsv")
        .lines()
        .find(|l| l.starts_with(&format!("{name}\t")))
        .unwrap();
    usize::from_str_radix(line.split_whitespace().nth(1).unwrap(), 16).unwrap()
}
fn word(image: &mut [u8], name: &str, value: u16) {
    let at = operand(name);
    image[at..at + 2].copy_from_slice(&value.to_be_bytes());
}
fn variant(mode: usize, gain: u16, wave: u8, coef: u16, omit: bool, no_mute: bool) -> Vec<u8> {
    let mut image = rom();
    image[operand("wave")] = wave;
    word(&mut image, "coef", coef);
    word(
        &mut image,
        "pcm",
        match mode {
            0 => 0xe800,
            1 => 0xff,
            2 => 0,
            _ => 0xe8ff,
        },
    );
    if mode < 2 {
        word(&mut image, "ext_left", 0);
        word(&mut image, "ext_right", 0);
    }
    word(&mut image, "sound_volume", gain);
    assert_eq!(image[operand("restore")], 0xe0);
    image[operand("restore") + 1] = gain as u8;
    if no_mute {
        image[operand("mute") + 1] = gain as u8;
    }
    if omit {
        // Replace the 68k byte store with equally sized NOPs and omit the
        // SH-2 restoration store. SCSP remains at the prior mute value.
        let at = operand("sound_volume") - 2;
        image[at..at + 8].copy_from_slice(&[0x4e, 0x71, 0x4e, 0x71, 0x4e, 0x71, 0x4e, 0x71]);
        let at = operand("restore") + 2;
        image[at..at + 2].copy_from_slice(&[0, 9]);
    }
    image
}
fn boot(m: &mut Machine, image: &[u8], changed_audio: bool) {
    m.reset(Boot::ExternalBios(image)).unwrap();
    assert_eq!(m.audio_generated(), 0);
    assert_eq!(m.audio_dropped(), 0);
    assert!(m.drain_audio(8192).is_empty());
    let mut audio = include_bytes!("fixtures/mvol-audio.bin").to_vec();
    if changed_audio {
        for frame in audio.as_chunks_mut::<4>().0 {
            frame[..2].copy_from_slice(&2048i16.to_le_bytes());
        }
    }
    m.mount_media(
        Media::cue(
            include_str!("fixtures/mvol.cue"),
            &[
                ("mvol-data.bin", include_bytes!("fixtures/mvol-data.bin")),
                ("mvol-audio.bin", &audio),
            ],
        )
        .unwrap(),
    );
}
fn finish(m: &mut Machine, chunk: usize) -> Vec<i16> {
    let mut samples = Vec::new();
    let mut steps = 0;
    while steps < 500000 {
        let count = chunk.min(500000 - steps);
        m.run_steps(count as u32).unwrap();
        steps += count;
        samples.extend(m.drain_audio(8192));
        if m.bus().read32(0x06009100) == Ok(4) {
            return samples;
        }
    }
    panic!(
        "MVOL guest did not finish: pc={:x}, sound={:x}, stage={:?}",
        m.cpu_state().pc,
        m.sound_state().pc,
        m.bus().read32(0x06009100)
    );
}
// Independent numerical oracle: binary half-power convention, nearest Q15
// coefficient; signed truncation. This is test arithmetic, never a core helper.
fn expected(value: i32, gain: u16) -> i16 {
    if gain == 0 {
        return 0;
    }
    let coefficient = (32768.0 * 2.0f64.powf((f64::from(gain) - 15.0) / 2.0)).round() as i32;
    (value.clamp(-32768, 32767) * coefficient / 32768) as i16
}
fn assert_authored(samples: &[i16], mode: usize, gain: u16, wave: u8, coef: u16, cd_left: i32) {
    assert!(samples.len() > 128);
    if gain == 0 {
        assert!(samples.iter().all(|v| *v == 0));
        return;
    }
    let wave = i32::from(wave) * 256;
    let pcm = [wave, -wave, -wave * 2, wave * 2];
    // Search only the independent starting phase and CD frame index. All
    // following samples must advance from that one phase/index continuously.
    // CD-DA starts with [1024,-1000], then its right input decreases by one.
    let found = (0..4).any(|phase| {
        (0..if mode < 2 { 1 } else { 18000 }).any(|cd| {
            samples
                .as_chunks::<2>()
                .0
                .iter()
                .enumerate()
                .all(|(i, frame)| {
                    let p = pcm[(phase + i) % 4];
                    let dry = if mode == 0 || mode == 3 {
                        [p / 16, p]
                    } else {
                        [0, 0]
                    };
                    let wet = if mode == 1 || mode == 3 {
                        p * i32::from(coef) / 32768
                    } else {
                        0
                    };
                    let ext = if mode >= 2 {
                        [cd_left, -1000 - cd - i as i32]
                    } else {
                        [0, 0]
                    };
                    *frame
                        == [
                            expected(dry[0] + wet + ext[0], gain),
                            expected(dry[1] + ext[1], gain),
                        ]
                })
        })
    });
    assert!(found, "mode={mode}, gain={gain}, head={:?}", &samples[..16]);
}

#[test]
fn guest_mvol_amplitudes_distinguish_pcm_dsp_runtime_cdda_and_combined_sources() {
    for mode in 0..4 {
        for gain in [0, 1, 13, 14, 15] {
            let mut m = Machine::new();
            for repeat in 0..2 {
                let image = variant(mode, gain, 16, 0x4000, false, false);
                boot(&mut m, &image, false);
                finish(&mut m, 10000);
                m.drain_audio(8192);
                m.run_steps(10000).unwrap();
                assert_authored(&m.drain_audio(8192), mode, gain, 16, 0x4000, 1024);
                assert_eq!(m.bus().read32(0x06009104), Ok(45000));
                let counts: Vec<_> = (1..=3)
                    .map(|i| m.bus().read32(0x06009110 + i * 4).unwrap())
                    .collect();
                assert!(
                    counts[0] > 0 && counts[0] < counts[1] && counts[1] < counts[2],
                    "68k progress {counts:?}, repeat={repeat}"
                );
                assert_eq!(m.bus().read16(0x05b00400).unwrap() & 0x20f, 0);
                assert_eq!(m.bus().read32(0x05a00808), Ok(0x5a5a0000));
                assert_eq!(m.bus().read32(0x06009120), Ok(0x5a5a));
                // Both CPU views retain MEM4MB after low-byte MVOL writes.
                assert!(!m.sound_state().halted);
                assert_eq!(m.sound_state().exceptions, 0);
                assert_eq!(m.audio_dropped(), 0);
            }
        }
    }
}

#[test]
fn guest_operand_mutations_and_omitted_volume_write_change_samples() {
    for (wave, coef, changed_audio) in [(8, 0x2000, false), (24, 0x4000, true), (16, 0, false)] {
        let mut m = Machine::new();
        boot(
            &mut m,
            &variant(3, 13, wave, coef, false, false),
            changed_audio,
        );
        finish(&mut m, 10000);
        m.run_steps(10000).unwrap();
        assert_authored(
            &m.drain_audio(8192),
            3,
            13,
            wave,
            coef,
            if changed_audio { 2048 } else { 1024 },
        );
    }
    let mut m = Machine::new();
    boot(&mut m, &variant(3, 13, 16, 0x4000, true, false), false);
    finish(&mut m, 10000);
    m.run_steps(10000).unwrap();
    assert!(m.drain_audio(8192).iter().all(|v| *v == 0));
}

#[test]
fn mute_preserves_source_progress_and_guest_trace_across_advancement_chunks() {
    let mut baseline = None;
    for chunk in [1, 3, 17, 10000] {
        let mut m = Machine::new();
        boot(&mut m, &variant(3, 15, 16, 0x4000, false, false), false);
        // A fixed instruction horizon includes all three phases and avoids
        // chunk-dependent overshoot at a polled marker.
        let mut trace = Vec::new();
        let mut left = 400000;
        while left > 0 {
            let n = left.min(chunk);
            m.run_steps(n).unwrap();
            left -= n;
            trace.extend(m.drain_audio(8192));
        }
        let records: Vec<_> = (0..7)
            .map(|i| m.bus().read32(0x06009100 + i * 4).unwrap())
            .collect();
        let state = (
            trace,
            records,
            m.audio_generated(),
            m.sound_state().instructions,
        );
        if let Some(ref previous) = baseline {
            assert_eq!(&state, previous);
        } else {
            baseline = Some(state);
        }
    }
    let (muted, _, _, instructions) = baseline.unwrap();
    let mut m = Machine::new();
    boot(&mut m, &variant(3, 15, 16, 0x4000, false, true), false);
    let mut continuous = Vec::new();
    for _ in 0..40 {
        m.run_steps(10000).unwrap();
        continuous.extend(m.drain_audio(8192));
    }
    assert_eq!(m.sound_state().instructions, instructions);
    assert_eq!(muted.len(), continuous.len());
    let differences: Vec<_> = muted
        .as_chunks::<2>()
        .0
        .iter()
        .zip(continuous.as_chunks::<2>().0.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, (a, _))| {
            assert_eq!(*a, [0, 0]);
            i
        })
        .collect();
    assert!(differences.len() > 100);
    assert!(differences.windows(2).all(|w| w[1] == w[0] + 1));
    let restored = (differences.last().unwrap() + 1) * 2;
    assert!(muted.len() - restored > 128);
    assert_eq!(&muted[restored..], &continuous[restored..]);
    // The right CD source is nonrepeating; exact continuation rejects restart/replay.
    assert_ne!(&continuous[restored..restored + 32], &continuous[..32]);
}
