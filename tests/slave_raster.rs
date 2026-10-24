//! Original MIT diagnostic also executed unchanged by production browser Wasm.
use saturn_rust::{Boot, Machine, bus::Memory};
fn image(h: u32, v: u32, masked: bool, omit: bool) -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/slave-raster.hex")
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
    rom[0x100..0x104].copy_from_slice(&h.to_be_bytes());
    rom[0x104..0x108].copy_from_slice(&v.to_be_bytes());
    if masked {
        rom[0x108..0x10C].copy_from_slice(&0xF0u32.to_be_bytes());
    }
    if omit {
        // Remove the master's actual COMREG store, without substituting inputs.
        let mut count = 0;
        for at in (0x200..0x300).step_by(2) {
            if rom[at..at + 2] == [0x21, 0x00] {
                count += 1;
                if count == 2 {
                    rom[at..at + 2].copy_from_slice(&[0, 9]);
                }
            }
        }
        assert_eq!(count, 3); // SF, COMREG, then slave CCR; omit COMREG only
    }
    rom
}
fn run(m: &mut Machine, rom: &[u8], chunk: u32, inspect: bool) -> Vec<u32> {
    m.reset(Boot::ExternalBios(rom)).unwrap();
    for _ in 0..300_000u32.div_ceil(chunk) {
        m.run_steps(chunk).unwrap();
        if inspect {
            let _ = m.bus().debug_read16(0x25F80004).unwrap();
            let _ = m.slave_state();
        }
        if m.bus().read32(0x0600E044) == Ok(0x600D) {
            break;
        }
    }
    (0..18)
        .map(|i| m.bus().read32(0x0600E000 + i * 4).unwrap())
        .collect()
}
fn check(r: &[u32], h: u32, v: u32) {
    assert_eq!(r[17], 0x600D, "{r:X?}");
    assert_eq!(
        (r[0], r[1], r[8], r[9], r[16]),
        (2, h + 14, 2, v + 22, h + v + 36)
    );
    assert_eq!(r[2], r[4], "HBlank must re-enter within one line: {r:X?}");
    assert_eq!(r[3] & 4, 4);
    assert_eq!(r[5] & 4, 4);
    assert_eq!(r[11] & 8, 8);
    assert_eq!(r[13] & 8, 8);
    assert_eq!(r[10], r[12]);
    assert!((224..263).contains(&r[10]));
}
#[test]
fn guest_slave_raster_reentry_arithmetic_controls_and_cold_boots() {
    let mut m = Machine::new();
    for (h, v, masked, omit) in [
        (3, 5, false, false),
        (23, 41, false, false),
        (3, 5, true, false),
        (3, 5, false, true),
    ] {
        for _ in 0..2 {
            let r = run(&mut m, &image(h, v, masked, omit), 500, true);
            if masked || omit {
                assert_eq!((r[0], r[8], r[17]), (0, 0, 0));
                assert_eq!(m.slave_state().exceptions, 0);
            } else {
                check(&r, h, v);
                assert_eq!(m.slave_state().exceptions, 4);
                assert_eq!(m.slave_state().r[15], 0x060FE000);
            }
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}
#[test]
fn guest_slave_raster_records_agree_across_chunks_and_passive_inspection() {
    let mut expected = None;
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let r = run(
                &mut Machine::new(),
                &image(3, 5, false, false),
                chunk,
                inspect,
            );
            check(&r, 3, 5);
            if let Some(ref e) = expected {
                assert_eq!(&r, e);
            } else {
                expected = Some(r);
            }
        }
    }
}
