//! ST-077 §4.2 figs.4.56/4.59–61. Original real 68k DMA and MIDI programs.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    sound::SOUND_RAM,
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/sound-io.hex")
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let mut at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for w in words {
            bytes[at..at + 2].copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
            at += 2;
        }
    }
    bytes
}
#[test]
fn sound_guest_polls_dma_and_paced_midi_then_acknowledges_three_interrupt_levels() {
    let rom = rom();
    let mut m = Machine::new();
    let mut previous = None;
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x06007004).unwrap(), 0x600D);
        assert_eq!(m.bus().read32(SOUND_RAM + 0x810).unwrap(), 0xA55A1234);
        assert_eq!(m.bus().read16(SOUND_RAM + 0x814).unwrap() & 0x210, 0x210);
        assert_eq!(m.bus().read32(SOUND_RAM + 0x818).unwrap(), 1);
        assert_eq!(m.bus().read32(SOUND_RAM + 0x81C).unwrap(), 1);
        assert_eq!(m.sound_state().exceptions, 5);
        for (offset, value) in [
            (0x830, 0x99),
            (0x834, 1),
            (0x838, 0x99),
            (0x83C, 0x12345678),
            (0x840, 0x11223344),
            (0x844, 0x55667788),
            (0x848, 0x4500),
            (0x850, 0xCAFEBABE),
            (0x854, 0xDEADBEEF),
            (0x858, 0x80000),
            (0x864, 0x80000),
            (0x884, 1),
            (0x88C, 0xFFFFFFFE),
            (0x890, 0xFFFFFFFD),
            (0x894, 0),
            (0x898, 0xFF),
            (0x89C, 0x9D2),
        ] {
            assert_eq!(
                m.bus().read32(SOUND_RAM + offset).unwrap(),
                value,
                "mailbox {offset:X}"
            );
        }
        assert_eq!(m.bus().read16(SOUND_RAM + 0x860).unwrap(), 0x15);
        assert_eq!(m.bus().read16(SOUND_RAM + 0x868).unwrap(), 0x4CD8);
        assert_eq!(m.bus().read16(SOUND_RAM + 0x8A0).unwrap(), 0x2718);
        assert_eq!(
            (0..4)
                .filter_map(|_| m.bus().sound.scsp.take_midi_output())
                .collect::<Vec<_>>(),
            [0x91, 0x40, 0x7F]
        );
        assert!(m.sound_state().stopped && !m.sound_state().halted);
        assert_eq!(m.sound_state().a[7], 0x7FF00);
        let samples = m.drain_audio(8192);
        assert!(samples.iter().any(|v| *v != 0));
        if let Some(old) = previous {
            assert_eq!(samples, old);
        }
        previous = Some(samples);
    }
}

#[test]
fn changed_guest_dma_source_changes_transferred_registers() {
    let mut rom = rom();
    let instruction = [0x33, 0xFC, 0xA5, 0x5A, 0, 0, 0x43, 0];
    let at: Vec<_> = rom
        .windows(8)
        .enumerate()
        .filter_map(|(i, w)| (w == instruction).then_some(i))
        .collect();
    assert_eq!(at.len(), 1);
    rom[at[0] + 2..at[0] + 4].copy_from_slice(&0x5AA5u16.to_be_bytes());
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    for _ in 0..10 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(SOUND_RAM + 0x810).unwrap(), 0x5AA51234);
}
