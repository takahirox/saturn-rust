//! Original reset/MMIO guest, SCSP table4.32 / fig4.58 and CPU-board lanes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};

fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/memory-contracts.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let address = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
        for (i, value) in fields.enumerate() {
            bytes[address + i * 2..address + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(value, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}

#[test]
fn original_dual_cpu_guest_observes_backup_aliases_memory_configuration_dma_dsp_and_pcm() {
    let original = rom();
    for mutated in [false, true] {
        let mut image = original.clone();
        if mutated {
            let mut count = 0;
            for at in (0..image.len() - 4).step_by(2) {
                if image[at..at + 4] == [0x23, 0x45, 0xDC, 0xBB] {
                    image[at..at + 4].copy_from_slice(&[0x12, 0x34, 0xED, 0xCC]);
                    count += 1;
                }
            }
            assert_eq!(count, 2);
        }
        let word = if mutated { 0x1234 } else { 0x2345 };
        let mut m = Machine::new();
        for boot in 1..=2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            for _ in 0..10 {
                m.run_steps(10000).unwrap();
            }
            let actual: Vec<_> = (0..16)
                .map(|i| m.bus().read32(0x0600F000 + i * 4).unwrap())
                .collect();
            let mut expected = vec![0xFF00 + boot; 8];
            expected.extend([
                word,
                0xFFFF,
                word,
                word & 0xFFF8,
                0x600D,
                u32::MAX,
                0x200,
                0x600D,
            ]);
            assert_eq!(actual, expected);
            assert!(!m.bus().sound.cpu.state.halted);
            assert_eq!(m.bus().sound.cpu.state.exceptions, 0);
            // Executing DSP copied the high sample word into MEMS0.
            assert_eq!(m.bus().read16(0x05B00E02), Ok(word as u16));
            let audio = m.drain_audio(8192);
            assert!(
                audio
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .any(|v| v[0] > 0 && v[1] > 0)
            );
            assert!(audio.contains(&-(word as i16)));
            assert!(audio.contains(&(word as i16)));
            assert!(m.import_backup_ram(&[0; 7]).is_err());
            assert_eq!(m.backup_ram()[0], boot as u8);
        }
    }
}
