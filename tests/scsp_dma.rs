//! Independent guest FRT/SCSP/68k contract: ST-077 §3.2 pp.20–21,
//! fig.4.66/tables4.39–40. Original fixture; adjacent MIT license.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in include_str!("fixtures/scsp-dma.hex")
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
#[test]
fn guest_times_real_dma_then_sound_cpu_computes_stereo_from_a_transferred_coefficient() {
    for value in [0x1230u16, 0x2340, 0x3450] {
        let mut image = rom();
        image[0x6000..0x6002].copy_from_slice(&value.to_be_bytes());
        let mut m = Machine::new();
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..10 {
                m.run_steps(10000).unwrap();
            }
            let records: Vec<_> = (0..6)
                .map(|i| m.bus().read32(0x0600EC00 + i * 4).unwrap())
                .collect();
            assert!(
                (32..100).contains(&records[0]),
                "FRT interval: {records:X?}"
            );
            assert_eq!(records[1], u32::from(value));
            assert_eq!(records[2], 0x1428);
            // Both processors may observe/acknowledge DMA completion on
            // either side of this SH-2 read; bit10 is the sample interrupt.
            assert_eq!(records[3] & !0x410, 0);
            assert_eq!(records[4..], [u32::from(value), 0x600D]);
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert!(m.sound_state().stopped && !m.sound_state().halted);
            assert_eq!(m.sound_state().exceptions, 0);
            assert_eq!(m.cpu_state().exceptions, 0);
            let samples = m.drain_audio(8192);
            let stereo = samples.as_chunks::<2>().0;
            assert!(stereo.contains(&[value as i16; 2]));
            assert!(stereo.contains(&[-(value as i16); 2]));
            assert!(
                stereo
                    .iter()
                    .all(|&s| s == [0; 2] || s == [value as i16; 2] || s == [-(value as i16); 2])
            );
            if let Some(old) = previous {
                assert_eq!((records.clone(), samples.clone()), old);
            }
            previous = Some((records, samples));
        }
    }
}

#[test]
fn omitting_the_guest_dma_start_never_produces_a_completion_result_or_sound() {
    let mut image = rom();
    let starts: Vec<_> = image
        .windows(2)
        .enumerate()
        .filter_map(|(at, word)| (word == [0x26, 0x71]).then_some(at))
        .collect();
    assert_eq!(starts.len(), 1); // MOV.W R7,@R6, the actual DEXE store
    image[starts[0]..starts[0] + 2].copy_from_slice(&[0, 9]); // SH-2 NOP
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&image)).unwrap();
    for _ in 0..10 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x0600EC14), Ok(0));
    assert_eq!(m.bus().read16(0x05B00700), Ok(0));
    assert_eq!(m.bus().read16(0x05A00808), Ok(0));
    assert!(m.drain_audio(8192).iter().all(|&sample| sample == 0));
    assert!(!m.sound_state().stopped && !m.sound_state().halted);
    assert_eq!(m.sound_state().exceptions, 0);
    assert_eq!(m.cpu_state().exceptions, 0);
}
