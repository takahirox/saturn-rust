//! ST-162 §8.2.1 initialization/retry and actual SH-2 runtime sector reads.
//! The same original MIT guest runs in browser Wasm. No host MMIO/result writes.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/cd-decode.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn guest_observes_retry_error_received_and_corrected_data_and_cancels_infinite_retry() {
    let fixture = rom();
    for (delta, offsets) in [
        (0x5A, vec![16]),
        (0xA6, vec![16]),
        (0x5A, vec![16, 102, 190, 18]),
    ] {
        let cooked = Media::iso(&vec![0x69; 4096]).unwrap();
        let mut raw = cooked.raw_sector(150).unwrap().to_vec();
        for &offset in &offsets {
            raw[offset] ^= delta;
        }
        let received = u32::from_be_bytes(raw[16..20].try_into().unwrap());
        raw.extend(cooked.raw_sector(151).unwrap());
        let mut m = Machine::new();
        let mut previous = None;
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&fixture)).unwrap();
            m.mount_media(
                Media::cue(
                    "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
                    &[("d", &raw)],
                )
                .unwrap(),
            );
            for _ in 0..100 {
                m.run_steps(10000).unwrap();
            }
            let read = |offset: u32| m.bus().read32(0x0600_A000 + offset).unwrap();
            assert_eq!(read(0x190), 0x600D);
            assert_eq!(read(0x30), 0x0980);
            assert_eq!(read(0x3C), 150);
            assert_eq!(read(0x4C), 0);
            for offset in [0x7C, 0xCC, 0x13C] {
                assert_eq!(read(offset), 1);
            }
            assert_eq!(read(0x180), received);
            assert_eq!(read(0x184), 0x6969_6969);
            assert_eq!(read(0x188), 0x6969_6969);
            assert!(read(0x18C) > 0); // The guest actually observed RETRY.
            assert_eq!(read(0x110), 0x0880);
            assert_eq!(read(0x11C), 150);
            let trace: Vec<_> = (0..=0x190 / 4).map(|i| read(i * 4)).collect();
            if let Some(old) = previous {
                assert_eq!(trace, old);
            }
            previous = Some(trace);
        }
    }
}
