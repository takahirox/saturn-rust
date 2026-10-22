//! Original 68k guest: ST-077 fig.4.3 / §4.2 p.89. No host-written running results.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus, sound::SOUND_RAM};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/scsp-monitor.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn run(machine: &mut Machine, rom: &[u8]) -> (Vec<u32>, Vec<i16>) {
    machine.reset(Boot::ExternalBios(rom)).unwrap();
    for _ in 0..20 {
        machine.run_steps(10000).unwrap();
    }
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 0);
    assert_eq!(machine.bus().read32(0x06007004), Ok(0x600D));
    assert!(machine.sound_state().stopped && !machine.sound_state().halted);
    assert_eq!(machine.sound_state().exceptions, 0);
    assert!(machine.sound_state().instructions > 3000);
    assert_eq!(machine.bus().read32(SOUND_RAM + 0x800), Ok(0x600D));
    let records = (0..64)
        .map(|i| machine.bus().read32(SOUND_RAM + 0x5000 + i * 4).unwrap())
        .collect();
    let audio = machine.drain_audio(8192);
    (records, audio)
}
fn expected() -> Vec<u32> {
    include_str!("fixtures/scsp-monitor.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect()
}
fn replace(rom: &mut [u8], before: &[u8], after: &[u8]) {
    let matches: Vec<_> = rom
        .windows(before.len())
        .enumerate()
        .filter_map(|(at, bytes)| (bytes == before).then_some(at))
        .collect();
    assert_eq!(matches.len(), 1);
    rom[matches[0]..matches[0] + before.len()].copy_from_slice(after);
}

#[test]
fn sound_guest_monitors_every_pcm_slot_with_word_and_byte_reads_after_cold_reset() {
    let mut machine = Machine::new();
    let rom = rom();
    let first = run(&mut machine, &rom);
    assert_eq!(first.0, expected());
    assert!(first.1.len() >= 128);
    assert!(
        first.1[first.1.len() - 128..]
            .as_chunks::<2>()
            .0
            .iter()
            .all(|p| *p == [0x1100, 0x1200])
    );
    assert_eq!(run(&mut machine, &rom), first);
}

#[test]
fn guest_loop_and_waveform_mutations_change_only_the_computed_monitor_or_audio_results() {
    let mut machine = Machine::new();
    let original = rom();
    // Slot31 loop block2 -> block3, actual 68k writes both LSA and LEA.
    let mut changed = original.clone();
    replace(
        &mut changed,
        &[0x33, 0xfc, 0x20, 0, 0, 0x10, 3, 0xe4],
        &[0x33, 0xfc, 0x30, 0, 0, 0x10, 3, 0xe4],
    );
    replace(
        &mut changed,
        &[0x33, 0xfc, 0x20, 1, 0, 0x10, 3, 0xe6],
        &[0x33, 0xfc, 0x30, 1, 0, 0x10, 3, 0xe6],
    );
    let (records, audio) = run(&mut machine, &changed);
    let mut vectors = expected();
    vectors[62] = 0x01800180;
    vectors[63] = 0x01800000;
    assert_eq!(records, vectors);
    assert!(
        audio[audio.len() - 128..]
            .as_chunks::<2>()
            .0
            .iter()
            .all(|p| *p == [0x1100, 0x1300])
    );
    // PCM sample at the first loop changes, without changing any slot monitor.
    let mut changed = original;
    replace(
        &mut changed,
        &[0x33, 0xfc, 0x11, 0, 0, 1, 0x20, 0],
        &[0x33, 0xfc, 0x09, 0, 0, 1, 0x20, 0],
    );
    let (records, audio) = run(&mut machine, &changed);
    assert_eq!(records, expected());
    assert!(
        audio[audio.len() - 128..]
            .as_chunks::<2>()
            .0
            .iter()
            .all(|p| *p == [0x0900, 0x1200])
    );
}

#[test]
fn omitting_guest_key_on_executes_the_same_reads_without_pcm_or_nonzero_monitors() {
    let mut changed = rom();
    replace(
        &mut changed,
        &[0x33, 0xFC, 0x18, 0x21, 0, 0x10, 0, 0],
        &[0x33, 0xFC, 0, 0x21, 0, 0x10, 0, 0],
    );
    let (records, audio) = run(&mut Machine::new(), &changed);
    assert_eq!(records, vec![0; 64]);
    assert!(!audio.is_empty());
    assert!(audio.iter().all(|sample| *sample == 0));
}
