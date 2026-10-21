//! ECMA-130 §14.2 and ST-162 §5.4/8.2: real guest reads of unformatted Mode2.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};

#[test]
fn guest_reads_every_non_xa_mode_two_byte_through_runtime_hirq_and_fifo() {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/mode2.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    for mutated in [false, true] {
        let mut raw = include_bytes!("fixtures/mode2.bin").to_vec();
        if mutated {
            raw[16] = 0xAA;
            raw[2351] = 0x55;
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.mount_media(
                Media::cue(
                    include_str!("fixtures/mode2.cue"),
                    &[
                        ("mode2.bin", &raw),
                        (
                            "mode2-audio.bin",
                            include_bytes!("fixtures/mode2-audio.bin"),
                        ),
                    ],
                )
                .unwrap(),
            );
            for _ in 0..10 {
                m.run_steps(10000).unwrap();
            }
            assert_eq!(m.bus().read32(0x0600D084), Ok(0x600D));
            assert_eq!(m.bus().read32(0x0600D064), Ok(1168));
            assert_eq!(m.bus().read32(0x0600D07C), Ok(0));
            let mut sum = 0;
            for (i, word) in raw[16..].as_chunks::<2>().0.iter().enumerate() {
                let word = u16::from_be_bytes(*word);
                sum += u32::from(word);
                assert_eq!(m.bus().read16(0x06010000 + i as u32 * 2), Ok(word));
            }
            assert_eq!(m.bus().read32(0x0600D080), Ok(sum));
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}
