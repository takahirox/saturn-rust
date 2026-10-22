//! Original 68k/SH-2 guest; ST-077 §3.2 and §2.2. Sound RAM is
//! modified by real 68k instructions, not by the test host during execution.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn read_rom(text: &str) -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in text.lines().filter(|l| !l.starts_with('#')) {
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
fn real_68k_waveform_writes_between_pcm_operands_produce_the_arithmetic_midpoint() {
    let original = read_rom(include_str!("fixtures/scsp-pcm.hex"));
    for (a, b) in [(0x2000u16, 0x6000u16), (0x1000, 0x5000), (0x800, 0x2800)] {
        let mut image = original.clone();
        // Only original immediate waveform writes are changed. Code, timing,
        // device registers and all test expectations remain independently authored.
        for at in (0x4100..0x4400).step_by(2) {
            if image[at..at + 2] == [0x33, 0xFC] && image[at + 4..at + 8] == [0, 0, 0x10, 0] {
                let old = u16::from_be_bytes([image[at + 2], image[at + 3]]);
                image[at + 2..at + 4]
                    .copy_from_slice(&(if old == 0x2000 { a } else { b }).to_be_bytes());
            }
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..20 {
                m.run_steps(10000).unwrap();
            }
            assert_eq!(m.bus().read32(0x0600ED00), Ok(0x600D));
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert!(m.sound_state().stopped && !m.sound_state().halted);
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.sound_state().exceptions, 0);
            let audio = m.drain_audio(8192);
            let midpoint = ((u32::from(a) + u32::from(b)) / 2) as i16;
            assert!(
                audio.as_chunks::<2>().0.contains(&[midpoint; 2]),
                "missing midpoint {midpoint:04X}"
            );
            assert!(audio.as_chunks::<2>().0.contains(&[a as i16; 2]));
            assert!(audio.as_chunks::<2>().0.contains(&[b as i16; 2]));
            assert!(
                audio
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|p| p[0] == p[1] && [0, a as i16, b as i16, midpoint].contains(&p[0]))
            );
        }
    }
    let mut image = original;
    let keyon = usize::from_str_radix(include_str!("fixtures/scsp-pcm.tsv").trim(), 16).unwrap();
    image[keyon..keyon + 2].copy_from_slice(&0x1020u16.to_be_bytes());
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&image)).unwrap();
    for _ in 0..20 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x0600ED00), Ok(0x600D));
    assert!(m.drain_audio(8192).iter().all(|&v| v == 0));
}

#[test]
fn guest_frt_measures_all_pcm_fetches_and_suppression_in_real_dma_completion_times() {
    let original = read_rom(include_str!("fixtures/scsp-pcm-arbitration.hex"));
    let mut m = Machine::new();
    let mut previous = None;
    for _ in 0..3 {
        m.reset(Boot::ExternalBios(&original)).unwrap();
        m.run_steps(10000).unwrap();
        let records: Vec<_> = (0..9)
            .map(|i| m.bus().read32(0x0600EE00 + i * 4).unwrap())
            .collect();
        assert_eq!(&records[6..], &[0x1230, 0x1428, 0x600D]);
        // 64 DMA words use about64 DRAM cycles without PCM, about128 with
        // 32 slots claiming alternate cycles. FRT counts main clocks/8;
        // allow poll/phase/refresh overhead, independently of guest results.
        for n in 0..2 {
            assert!((30..70).contains(&records[n]), "quiet {records:?}");
            assert!((65..130).contains(&records[2 + n]), "PCM {records:?}");
            assert!(records[2 + n] >= records[n] + 20, "no PCM wait {records:?}");
            assert!(
                records[4 + n].abs_diff(records[n]) <= 8,
                "noise fetched {records:?}"
            );
        }
        if let Some(previous) = previous {
            assert_eq!(records, previous);
        }
        previous = Some(records);
        assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(m.cpu_state().exceptions, 0);
    }
}
