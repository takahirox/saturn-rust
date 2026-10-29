//! Original MIT guest, identical bytes exercised in production browser Wasm.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut b = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/interrupt-entry.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut w = line.split_whitespace();
        let at = usize::from_str_radix(w.next().unwrap(), 16).unwrap();
        for (i, v) in w.enumerate() {
            b[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(v, 16).unwrap().to_be_bytes());
        }
    }
    b
}
fn patch(image: &mut [u8], before: u32, after: u32, width: usize) {
    let a = before.to_be_bytes();
    let b = after.to_be_bytes();
    let mut found = 0;
    for at in (0x200..0x7000).step_by(2) {
        if image[at..at + width] == a[4 - width..] {
            image[at..at + width].copy_from_slice(&b[4 - width..]);
            found += 1;
        }
    }
    assert_eq!(found, 1, "unique patch {before:X}");
}
#[test]
fn guest_measures_interrupt_entry_and_returns_on_both_cpus_then_acquires_scu_vector() {
    let original = rom();
    let mut m = Machine::new();
    for control in 0..5 {
        let mut image = original.clone();
        match control {
            1 => {
                patch(&mut image, 0xE020, 0xE030, 2);
                patch(&mut image, 0x0000E607, 0x0000E60B, 4);
                patch(&mut image, 0x600, 0xA00, 4);
            }
            2 => patch(&mut image, 0x13579BDF, 0x2468ACE0, 4),
            3 => patch(&mut image, 0xE0082100, 0xE0002100, 4),
            4 => patch(&mut image, 0x101, 0x100, 4),
            _ => (),
        }
        for boot in 0..2 {
            m.reset(Boot::ExternalBios(&image)).unwrap();
            m.run_steps(10000).unwrap();
            let results: Vec<_> = [0x0600D000, 0x0600D100]
                .into_iter()
                .map(|base| {
                    (0..7)
                        .map(|i| m.bus().read32(base + i * 4).unwrap())
                        .collect::<Vec<_>>()
                })
                .collect();
            let mut record = vec![
                if control == 1 { 51 } else { 35 },
                0x50,
                0xC000050C,
                if control == 1 { 0xA0 } else { 0x60 },
                if control == 1 { 11 } else { 7 },
                0x600D,
            ];
            let expected = if control == 3 {
                vec![vec![0; 7], vec![0; 7]]
            } else if control == 4 {
                record.push(0);
                vec![record, vec![0; 7]]
            } else {
                let mut master = record.clone();
                master.push(0x4B);
                record.push(0);
                vec![master, record]
            };
            assert_eq!(results, expected, "control={control} boot={boot}");
            assert_eq!(
                m.bus().read32(0x0600D204),
                Ok(if control >= 3 {
                    0
                } else if control == 2 {
                    0x2468ACE0
                } else {
                    0x13579BDF
                })
            );
            assert_eq!(
                (m.cpu_state().exceptions, m.slave_state().exceptions),
                if control == 3 {
                    (0, 0)
                } else if control == 4 {
                    (1, 0)
                } else {
                    (2, 1)
                }
            );
        }
    }
}
