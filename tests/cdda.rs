//! IEC 60908 50/15 µs audio, ST-162 §8.2.2 and ST-077 table 4.31.
//! Original SH-2 code owns CD commands and SCSP routing. No host MMIO writes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cdda.hex")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let address = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[address + i * 2..address + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
#[test]
fn guest_cdda_preemphasis_has_reference_stereo_samples_and_continuous_sector_history() {
    let mut audio = vec![0; 3 * 2352];
    for (i, frame) in audio.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let left = if [0, 587, 588, 1176].contains(&i) {
            10000i16
        } else {
            0
        };
        frame[..2].copy_from_slice(&left.to_le_bytes());
        frame[2..].copy_from_slice(&(-20000i16).to_le_bytes());
    }
    for pre in [false, true] {
        let cue = format!(
            "FILE \"a\" BINARY\nTRACK 01 AUDIO\n{}INDEX 01 00:00:00",
            if pre { "FLAGS PRE\n" } else { "" }
        );
        let mut machine = Machine::new();
        machine.reset(Boot::ExternalBios(&rom())).unwrap();
        machine.mount_media(Media::cue(&cue, &[("a", &audio)]).unwrap());
        for _ in 0..12 {
            machine.run_steps(10000).unwrap();
        }
        assert_eq!(machine.bus().read32(0x06009010), Ok(0x600D));
        let samples = machine.drain_audio(8192);
        let first = samples
            .as_chunks::<2>()
            .0
            .iter()
            .position(|frame| frame[1] != 0)
            .unwrap();
        assert!(samples.len() / 2 - first > 1177);
        let (mut input_previous, mut output_previous) = ([0.; 2], [0.; 2]);
        for (frame, input) in samples[first * 2..]
            .as_chunks::<2>()
            .0
            .iter()
            .take(1178)
            .zip(audio.as_chunks::<4>().0.iter())
        {
            for channel in 0..2 {
                let input = f64::from(i16::from_le_bytes(
                    input[channel * 2..channel * 2 + 2].try_into().unwrap(),
                ));
                let expected = if pre {
                    ((1. + 88200. * 15e-6) * input
                        + (1. - 88200. * 15e-6) * input_previous[channel]
                        - (1. - 88200. * 50e-6) * output_previous[channel])
                        / (1. + 88200. * 50e-6)
                } else {
                    input
                };
                assert!((f64::from(frame[channel]) - expected).abs() < 0.51);
                input_previous[channel] = input;
                output_previous[channel] = expected;
            }
        }
    }
}
