//! Original MIT guest, with a cycle-budget lower bound independent of the
//! interpreter. 512 word cycles require at least2438 reference clocks; FRT/8
//! must count >=304. See docs/scsp-cpu-memory.md and the fixture builder.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; 524288];
    for line in include_str!("fixtures/scsp-cpu-memory.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}
#[test]
fn guest_cpu_memory_budget_and_computed_pcm_dsp_stereo() {
    let original = rom();
    let offsets: Vec<_> = include_str!("fixtures/scsp-cpu-memory.tsv")
        .lines()
        .map(|s| usize::from_str_radix(s, 16).unwrap())
        .collect();
    for (a, b) in [(0x2000u16, 0x4000u16), (0x1000, 0x3000), (0x800, 0x2800)] {
        let mut image = original.clone();
        image[offsets[0]..offsets[0] + 2].copy_from_slice(&a.to_be_bytes());
        image[offsets[1]..offsets[1] + 2].copy_from_slice(&b.to_be_bytes());
        let mut previous = None;
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..20 {
                m.run_steps(10000).unwrap_or_else(|e| {
                    panic!("{e:?}, CPU={:?}, SCPU={:?}", m.cpu_state(), m.sound_state())
                });
            }
            let records: Vec<_> = (0..3)
                .map(|i| m.bus().read32(0x0600EF00 + i * 4).unwrap())
                .collect();
            assert_eq!(
                records[1..],
                [0x20002000, 0x600D],
                "{records:X?} state={:?}",
                m.sound_state()
            );
            assert!(
                (304..520).contains(&records[0]),
                "DRAM cycle budget {records:?}"
            );
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert!(m.sound_state().stopped && !m.sound_state().halted);
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.sound_state().exceptions, 0);
            let audio = m.drain_audio(8192);
            let stereo = audio.as_chunks::<2>().0;
            assert!(stereo.contains(&[a as i16, (a / 2) as i16]));
            assert!(stereo.contains(&[b as i16, (b / 2) as i16]));
            let midpoint = ((u32::from(a) + u32::from(b)) / 2) as i16;
            assert!(stereo.iter().all(
                |s| [0, 0x2000, a as i16, b as i16, midpoint].contains(&s[0])
                    && [0, 0x1000, (a / 2) as i16, (b / 2) as i16].contains(&s[1])
            ));
            if let Some(old) = &previous {
                assert_eq!(&(records.clone(), audio.clone()), old);
            }
            previous = Some((records, audio));
        }
    }
}
#[test]
fn omitted_guest_dma_start_cannot_synthesize_records_or_audio() {
    let mut image = rom();
    let at = usize::from_str_radix(
        include_str!("fixtures/scsp-cpu-memory.tsv")
            .lines()
            .nth(2)
            .unwrap(),
        16,
    )
    .unwrap();
    image[at..at + 2].copy_from_slice(&[0, 9]);
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&image)).unwrap();
    for _ in 0..2 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x0600EF08), Ok(0));
    assert_eq!(m.bus().read32(0x05A01000), Ok(0));
    assert!(m.drain_audio(8192).iter().all(|v| *v == 0));
    assert_eq!(m.cpu_state().exceptions, 0);
    assert_eq!(m.sound_state().exceptions, 0);
}

#[test]
fn contending_guest_records_and_exact_stereo_do_not_depend_on_execution_chunks() {
    let image = rom();
    let mut previous = None;
    for chunk in [1, 3, 17, 10000] {
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&image)).unwrap();
        let mut remaining = 200000;
        while remaining != 0 {
            let n = remaining.min(chunk);
            m.run_steps(n).unwrap();
            remaining -= n;
        }
        let records: Vec<_> = (0..3)
            .map(|i| m.bus().read32(0x0600EF00 + i * 4).unwrap())
            .collect();
        assert_eq!(records[1..], [0x20002000, 0x600D]);
        let result = (records, m.drain_audio(8192), m.clocks());
        if let Some(old) = &previous {
            assert_eq!(&result, old);
        }
        previous = Some(result);
    }
}
