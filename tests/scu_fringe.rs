//! ST-097 §2.1 fig.2.1: UBC observes byte heads/tails from real SCU DMA.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};

fn rom() -> Vec<u8> {
    let mut image = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/scu-fringe.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
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
fn direct_and_indirect_guest_dma_exposes_byte_cycles_to_ubc_and_both_irq_handlers() {
    let goldens: Vec<u32> = include_str!("fixtures/scu-fringe.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for mutate in [false, true] {
        let mut image = rom();
        if mutate {
            let matches: Vec<_> = image
                .windows(4)
                .enumerate()
                .filter(|(_, v)| *v == 0x12345678u32.to_be_bytes())
                .map(|(i, _)| i)
                .collect();
            assert_eq!(matches.len(), 4);
            for at in matches {
                image[at + 3] = 0x5a;
            }
        }
        let mut expected = goldens.clone();
        if mutate {
            for scene in expected[..1200].chunks_mut(5) {
                if scene[3] & 255 == 0x78 {
                    scene[3] = scene[3] & !255 | 0x5a;
                }
            }
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            let run = (0..8).try_for_each(|_| m.run_steps(10000));
            assert!(
                run.is_ok(),
                "{run:?}; {:?}; first={:?}",
                m.cpu_state(),
                (0..10)
                    .map(|i| m.bus().read32(0x0600d000 + i * 4))
                    .collect::<Vec<_>>()
            );
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0600d000 + i as u32 * 4).unwrap())
                .collect();
            for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(
                    a,
                    e,
                    "record {i}, scene {}, field {}, mutate {mutate}",
                    i / 5,
                    i % 5
                );
            }
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(m.cpu_state().exceptions, 480);
            assert_eq!(m.cpu_state().r[15], 0x060ff000);
        }
    }
}
