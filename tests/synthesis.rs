//! Original independent SH-2/68k slot-synthesis program, also run in browser Wasm.
//! ST-077 §4.2 figs.4.18/4.31, tables4.16–4.18/4.21–4.24.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    sound::SOUND_RAM,
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/synthesis.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            bytes[at..at + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    bytes
}
fn change(rom: &mut [u8], address: u32, old: u16, value: u16) {
    // Replace only an original MOVE.W immediate operand; actual 68k code must
    // execute the new register write. No host writes to SCSP or result mailboxes.
    let mut instruction = vec![0x33, 0xFC];
    instruction.extend(old.to_be_bytes());
    instruction.extend(address.to_be_bytes());
    let offsets: Vec<_> = rom
        .windows(8)
        .enumerate()
        .filter_map(|(i, w)| (w == instruction).then_some(i))
        .collect();
    assert_eq!(offsets.len(), 1);
    rom[offsets[0] + 2..offsets[0] + 4].copy_from_slice(&value.to_be_bytes());
}
fn execute(m: &mut Machine, rom: &[u8]) -> Vec<[i16; 2]> {
    m.reset(Boot::ExternalBios(rom)).unwrap();
    for _ in 0..20 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x06007004).unwrap(), 0x600D);
    assert!(m.sound_state().stopped && !m.sound_state().halted);
    assert_eq!(m.sound_state().last_vector, Some(28));
    assert_eq!(m.bus().read16(SOUND_RAM + 0x4300).unwrap(), 8192);
    m.drain_audio(8192)
        .chunks_exact(2)
        .map(|s| [s[0], s[1]])
        .collect()
}
fn audible(audio: &[[i16; 2]]) -> &[[i16; 2]] {
    &audio[audio.iter().position(|s| s[1] != 0).unwrap()..]
}
#[test]
fn executing_guest_fm_and_alfo_have_independent_stereo_vectors_and_repeat_on_reset() {
    let rom = rom();
    let mut m = Machine::new();
    let audio = execute(&mut m, &rom);
    let first = audible(&audio);
    assert_eq!(
        &first[..8],
        &[
            [1024, 1024],
            [1152, 1024],
            [1280, 1024],
            [1408, 1024],
            [1536, 1024],
            [1664, 1024],
            [1792, 1024],
            [1920, 1024]
        ]
    );
    assert!(first.len() > 1000);
    assert!(first.iter().all(|s| s[1] == 1024 || s[1] == 64));
    assert!(first.iter().any(|s| s[1] == 64));
    assert_eq!(execute(&mut m, &rom), audio);
}
#[test]
fn guest_modulation_and_pitch_operands_change_output_without_touching_waveform() {
    let rom = rom();
    let mut m = Machine::new();
    let mut quieter = rom.clone();
    change(&mut quieter, 0x1000AC, 16, 32);
    let quiet = execute(&mut m, &quieter);
    assert_eq!(
        &audible(&quiet)[..4],
        &[[512, 1024], [576, 1024], [640, 1024], [704, 1024]]
    );
    let mut no_fm = rom.clone();
    change(&mut no_fm, 0x1000AE, 0x5EFB, 0);
    let dry = execute(&mut m, &no_fm);
    assert_eq!(
        &audible(&dry)[..4],
        &[[0, 1024], [128, 1024], [256, 1024], [384, 1024]]
    );
    let mut vibrato = rom.clone();
    change(&mut vibrato, 0x1000B2, 0x8000, 0x81E0); // held square +490.14 cents
    let pitched = execute(&mut m, &vibrato);
    let first = audible(&pitched);
    // Mathematical 2^(494*127/128/1200) phase increment; independent double
    // oracle permits one PCM unit for the explicitly documented integer rounding.
    let ratio = 2f64.powf(494.0 * 127.0 / 128.0 / 1200.0);
    for (i, s) in first[..16].iter().enumerate() {
        let expected = ((8.0 + i as f64 * ratio) * 128.0).floor() as i16;
        assert!(
            (i32::from(s[0]) - i32::from(expected)).abs() <= 1,
            "i={i} {s:?} vs {expected}"
        );
        assert_eq!(s[1], 1024);
    }
    let mut no_alfo = rom.clone();
    change(&mut no_alfo, 0x1000D2, 0x7C0F, 0x800F);
    let unmodulated = execute(&mut m, &no_alfo);
    assert!(audible(&unmodulated).iter().all(|s| s[1] == 1024));
}

#[test]
fn guest_noise_and_zero_sources_execute_and_noise_clock_is_independent_of_lfo_reset() {
    let mut noise = rom();
    change(&mut noise, 0x1000C0, 0x0820, 0x08A0); // slot6 internal noise
    change(&mut noise, 0x1000CC, 32, 0x100); // direct: bypass TL/AM
    let mut m = Machine::new();
    let audio = execute(&mut m, &noise);
    let samples = audible(&audio);
    assert!(samples.iter().any(|s| s[1] > 16000));
    assert!(samples.iter().any(|s| s[1] < -16000));
    assert_eq!(execute(&mut m, &noise), audio);
    change(&mut noise, 0x1000D2, 0x7C0F, 0xFC0F); // LFORE must not reset noise
    assert_eq!(execute(&mut m, &noise), audio);
    let mut zero = rom();
    change(&mut zero, 0x1000C0, 0x0820, 0x0920); // internal all-zero source
    let silent = execute(&mut m, &zero);
    assert!(silent.iter().all(|s| s[1] == 0));
    assert!(silent.iter().any(|s| s[0] > 0)); // independently executing FM carrier
}
