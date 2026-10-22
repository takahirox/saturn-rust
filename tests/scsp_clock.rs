//! Independent original guest: 68k modifies sound RAM between actual DSP reads.
//! ST-077 §2.2 table2.2, §3.2, §5 fig5.1; ST-228 Reference2.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/scsp-clock.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}
#[test]
fn sound_cpu_observes_early_dsp_result_then_changes_the_later_read_and_real_audio() {
    let original = rom();
    let operand =
        usize::from_str_radix(include_str!("fixtures/scsp-clock.tsv").trim(), 16).unwrap();
    for (value, late) in [(0x2345u16, 0x11A2), (0x3456, 0x1A2B), (0x1234, 0x91A)] {
        let mut image = original.clone();
        assert_eq!(&image[operand..operand + 2], &[0x23, 0x45]);
        image[operand..operand + 2].copy_from_slice(&value.to_be_bytes());
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..10 {
                m.run_steps(10000).unwrap();
            }
            assert_eq!(m.bus().read32(0x0600EA00), Ok(0x091A0000 | late));
            assert_eq!(m.bus().read32(0x0600EA04), Ok(0x600D));
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert!(m.sound_state().stopped && !m.sound_state().halted);
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.sound_state().exceptions, 0);
            assert_eq!(m.bus().read16(0x05A01000), Ok(value));
            let audio = m.drain_audio(8192);
            assert!(audio.as_chunks::<2>().0.contains(&[late as i16 * 2; 2]));
        }
    }
}
