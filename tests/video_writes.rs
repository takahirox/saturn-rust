//! Original MIT CPU/DMAC guest; identical bytes run in production browser Wasm.
use saturn_rust::{Boot, Machine, bus::Memory};
fn bios() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/video-writes.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let at = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
        for (i, w) in fields.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn image(payload: u32, color: u32, omit: bool) -> Vec<u8> {
    let mut rom = bios();
    rom[0x100..0x104].copy_from_slice(&payload.to_be_bytes());
    rom[0x108..0x10C].copy_from_slice(&color.to_be_bytes());
    if omit {
        rom[0x104..0x106].copy_from_slice(&0u16.to_be_bytes());
        let mut count = 0;
        for at in (0x200..0x1000).step_by(2) {
            if rom[at..at + 2] == [0x24, 0xC2] {
                rom[at..at + 2].copy_from_slice(&[0, 9]);
                count += 1;
            }
        }
        assert_eq!(count, 64);
    }
    rom
}
fn run(m: &mut Machine, rom: &[u8], chunk: u32) -> (Vec<u32>, Vec<u8>) {
    m.reset(Boot::ExternalBios(rom)).unwrap();
    for _ in 0..2_000_000 / chunk {
        m.run_steps(chunk).unwrap();
        // Passive previews cannot move the beam or publish pending stores.
        let _ = m.bus().read32(0x25E06000).unwrap();
        let _ = m.bus().read16(0x25F80004).unwrap();
        if m.bus().read32(0x0600E150) == Ok(0x600D) {
            break;
        }
    }
    assert_eq!(m.bus().read32(0x0600E150), Ok(0x600D));
    assert_eq!(m.bus().read32(0x0600E208), Ok(0x600D));
    let records = [0x0600E000, 0x0600E100]
        .into_iter()
        .flat_map(|base| (0..21).map(move |i| base + i * 4))
        .chain([0x0600E200, 0x0600E204])
        .map(|at| m.bus().read32(at).unwrap())
        .collect();
    assert_eq!(m.cpu_state().exceptions, 0);
    assert_eq!(m.slave_state().exceptions, 0);
    m.advance_frame().unwrap();
    (records, m.framebuffer().unwrap())
}
#[test]
fn guest_video_writes_both_cpus_dmac_frt_readback_and_two_palette_publications() {
    let mut m = Machine::new();
    let mut normal_times = Vec::new();
    for (payload, color, omit) in [
        (0x11223344u32, 0x000000FFu32, false),
        (0x55667788, 0x0000FF00, false),
        (0x11223344, 0x000000FF, true),
    ] {
        for _ in 0..2 {
            let (records, pixels) = run(&mut m, &image(payload, color, omit), 500);
            let mut times = Vec::new();
            for base in [0, 21] {
                for n in 0..4 {
                    let r = &records[base + n * 5..][..5];
                    times.push(r[0]);
                    assert!(
                        (if omit { 1..200 } else { 32..200 }).contains(&r[0]),
                        "{r:X?}"
                    );
                    assert_eq!(r[1], if omit { 0 } else { payload });
                    assert!((1..2000).contains(&r[2]));
                    assert_eq!(&r[3..], &[if omit { 0 } else { payload }; 2]);
                }
                assert_eq!(records[base + 20], 0x600D);
            }
            if !omit {
                if normal_times.is_empty() {
                    normal_times = times;
                } else {
                    assert_eq!(times, normal_times);
                }
            } else {
                for (short, long) in times.iter().zip(&normal_times) {
                    assert!(short + 24 < *long, "{short}/{long}");
                }
            }
            assert_eq!(records[42], if omit { 0x00FF0000 } else { color });
            assert!((300..600).contains(&records[43]));
            let row = &pixels[16 * 320 * 4..17 * 320 * 4];
            let black: Vec<_> = row
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter_map(|(x, p)| (*p == [0, 0, 0, 255]).then_some(x))
                .collect();
            if omit {
                assert!(black.is_empty());
                assert!(
                    row.as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| *p == [0, 0, 255, 255])
                );
            } else {
                assert_eq!(black.len(), 2);
                assert_eq!(black[1], black[0] + 1);
                assert!((50..300).contains(&black[0]));
                let new = if color == 0xFF {
                    [255, 0, 0, 255]
                } else {
                    [0, 255, 0, 255]
                };
                for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
                    assert_eq!(
                        p,
                        if x < black[0] {
                            &[0, 0, 255, 255]
                        } else if x <= black[1] {
                            &[0, 0, 0, 255]
                        } else {
                            &new
                        }
                    );
                }
            }
        }
    }
}
#[test]
fn guest_video_records_and_pixels_are_identical_across_chunks_and_passive_previews() {
    let rom = image(0x11223344, 0xFF, false);
    let mut expected = None;
    for chunk in [1, 3, 17, 10000] {
        let actual = run(&mut Machine::new(), &rom, chunk);
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected);
        } else {
            expected = Some(actual);
        }
    }
}
