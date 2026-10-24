//! Original MIT HLE disc; the same bytes/records execute in production Wasm.
use saturn_rust::{Boot, Machine, bus::Memory};

const ISO: &[u8] = include_bytes!("fixtures/hle-dual.iso");
const INPUT: usize = 21 * 2048 + 0x7000;

fn golden() -> Vec<(u32, u32, &'static str)> {
    include_str!("fixtures/hle-dual.tsv")
        .lines()
        .map(|line| {
            let f: Vec<_> = line.split('\t').collect();
            (
                u32::from_str_radix(f[0], 16).unwrap(),
                u32::from_str_radix(f[1], 16).unwrap(),
                f[2],
            )
        })
        .collect()
}

fn run(m: &mut Machine, image: &[u8], chunk: u32, inspect: bool) {
    m.reset(Boot::HleDisc(image)).unwrap();
    while m.clocks() < 8_000_000 && m.bus().read32(0x0600E904) != Ok(0x600D) {
        m.run_steps(chunk).unwrap();
        if inspect {
            let _ = m.debug_read16(0x25F80004).unwrap();
            let _ = m.slave_state();
        }
    }
}

fn check(m: &Machine) {
    for (address, expected, name) in golden() {
        assert_eq!(
            m.bus().read32(address),
            Ok(expected),
            "{name} at {address:X}; master {:X} slave {:X}",
            m.cpu_state().pc,
            m.slave_state().pc
        );
    }
    check_raster(m, 3, 5);
    assert_eq!(m.bus().read32(0x0600E900), Ok(0));
    assert_eq!(m.bus().read32(0x0600E904), Ok(0x600D));
}

fn check_raster(m: &Machine, h: u32, v: u32) {
    let r: Vec<_> = (0..18)
        .map(|i| m.bus().read32(0x0600E800 + i * 4).unwrap())
        .collect();
    assert_eq!(
        (r[0], r[1], r[8], r[9], r[16], r[17]),
        (2, h + 14, 2, v + 22, h + v + 36, 0x600D),
        "{r:X?}"
    );
    assert_eq!(r[2], r[4], "HBlank re-entry: {r:X?}");
    assert_eq!((r[3] & 4, r[5] & 4), (4, 4));
    assert_eq!(r[10], r[12], "VBlank re-entry: {r:X?}");
    assert!((224..263).contains(&r[10]), "{r:X?}");
    assert_eq!((r[11] & 8, r[13] & 8), (8, 8));
}

#[test]
fn boot_provided_capture_communication_and_raster() {
    let mut m = Machine::new();
    run(&mut m, ISO, 10000, false);
    check(&m);
}

fn input(image: &mut [u8], offset: usize, value: u32) {
    image[INPUT + offset..INPUT + offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn raster_image(mask: u32) -> Vec<u8> {
    let mut image = ISO.to_vec();
    image[0xE28..0xE2C].copy_from_slice(&0x06006000u32.to_be_bytes());
    input(&mut image, 0x3C, mask);
    image
}

#[test]
fn identical_guest_records_across_chunks_inspection_and_repeated_cold_boots() {
    let mut reference = None;
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let mut m = Machine::new();
            for _ in 0..2 {
                run(&mut m, ISO, chunk, inspect);
                check(&m);
                let records: Vec<_> = golden()
                    .iter()
                    .map(|(a, _, _)| m.bus().read32(*a).unwrap())
                    .chain((0..18).map(|i| m.bus().read32(0x0600E800 + i * 4).unwrap()))
                    .collect();
                assert_eq!(reference.get_or_insert(records.clone()), &records);
                assert_eq!(m.cpu_state().exceptions, 5);
            }
        }
    }
}

#[test]
fn changed_operands_and_omitted_strobe_have_independent_results() {
    let mut image = ISO.to_vec();
    for (off, value) in [(0, 9), (4, 17), (8, 23), (0x34, 23), (0x38, 41), (0x40, 0)] {
        input(&mut image, off, value);
    }
    let mut m = Machine::new();
    for _ in 0..2 {
        run(&mut m, &image, 10000, true);
        for (address, expected) in [
            (0x0600E000, 69),
            (0x0600E004, 89),
            (0x0600E008, 86),
            (0x0600E010, 86),
            (0x0600E018, 86),
            (0x0600D028, 9),
            (0x0600D128, 17),
            (0x0600D328, 34),
            (0x0600D15C, 54),
            (0x0600D35C, 64),
        ] {
            assert_eq!(m.bus().read32(address), Ok(expected));
        }
        check_raster(&m, 23, 41);
        assert_eq!(m.bus().read32(0x0600E848), Ok(5));
    }
    let mut image = ISO.to_vec();
    input(&mut image, 12, 0);
    for _ in 0..2 {
        m.reset(Boot::HleDisc(&image)).unwrap();
        m.run_steps(10000).unwrap();
        for address in [
            0x0600D040, 0x0600D140, 0x0600D064, 0x0600E000, 0x0600E004, 0x0600E904,
        ] {
            assert_eq!(m.bus().read32(address), Ok(0));
        }
        assert_eq!(m.cpu_state().exceptions, 0);
        assert_eq!(m.slave_state().exceptions, 0);
        assert_eq!(m.cpu_state().status, saturn_rust::cpu::CpuStatus::Sleeping);
        assert_eq!(
            m.slave_state().status,
            saturn_rust::cpu::CpuStatus::Sleeping
        );
    }
}

#[test]
fn independent_raster_entry_uses_boot_mode_and_guest_sr_mask_only() {
    let mut m = Machine::new();
    for mask in [0x10, 0xF0] {
        for _ in 0..2 {
            let image = raster_image(mask);
            m.reset(Boot::HleDisc(&image)).unwrap();
            for _ in 0..30 {
                m.run_steps(10000).unwrap();
            }
            if mask == 0x10 {
                check_raster(&m, 3, 5);
            } else {
                assert_eq!(m.bus().read32(0x0600E800), Ok(0));
                assert_eq!(m.bus().read32(0x0600E820), Ok(0));
                assert_eq!(m.bus().read32(0x0600E844), Ok(0));
                assert_eq!(m.slave_state().exceptions, 0);
            }
            assert_eq!(m.cpu_state().exceptions, 0);
        }
    }
}

#[test]
fn retained_vector_bytes_and_default_or_custom_stacks_at_every_slave_entry() {
    for stack in [0u32, 0x060FD000] {
        let mut image = ISO.to_vec();
        image[0xEC..0xF0].copy_from_slice(&stack.to_be_bytes());
        let mut m = Machine::new();
        m.reset(Boot::HleDisc(&image)).unwrap();
        let mut retained = None;
        for epoch in 0..4 {
            let entry = if epoch == 0 { 0x06008000 } else { 0x06008040 };
            let start = m.clocks();
            while m.slave_state().pc != entry && m.clocks() - start < 4_000_000 {
                m.step().unwrap();
            }
            assert_eq!(m.slave_state().pc, entry, "epoch {epoch}");
            assert_eq!(m.slave_state().vbr, 0x06000400);
            assert_eq!(
                m.slave_state().r[15],
                if stack == 0 { 0x06001000 } else { stack }
            );
            let bytes: Vec<_> = (0..512)
                .map(|i| m.bus().read8(0x06000400 + i).unwrap())
                .collect();
            assert_eq!(
                retained.get_or_insert(bytes.clone()),
                &bytes,
                "epoch {epoch}"
            );
            assert_eq!(m.bus().read32(0x06000590), Ok(0x06009400));
            assert_eq!(m.bus().read32(0x060005C0), Ok(0x0600AC00));
            m.step().unwrap();
        }
        while m.bus().read32(0x0600E904) != Ok(0x600D) && m.clocks() < 8_000_000 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x0600E904), Ok(0x600D));
        check_raster(&m, 3, 5);
        for epoch in 0..4 {
            let base = 0x0600D100 + epoch * 0x200;
            let sp = if stack == 0 { 0x06001000 } else { stack };
            assert_eq!(m.bus().read32(base + 0x20), Ok(sp));
            assert_eq!(m.bus().read32(base + 0x50), Ok(sp - 8));
            assert_eq!(m.bus().read32(base + 0xC), Ok(0xF00));
            assert_eq!(m.bus().read32(base), Ok(0x81));
        }
    }
}

#[test]
fn sysres_reexecutes_boot_and_communication_retaining_power_clear_and_backup() {
    let mut image = ISO.to_vec();
    input(&mut image, 16, 1);
    let mut m = Machine::new();
    let backup = vec![0xA5; 32768];
    m.import_backup_ram(&backup).unwrap();
    for _ in 0..2 {
        m.reset(Boot::HleDisc(&image)).unwrap();
        assert_eq!(m.bus().read32(0x06000C00), Ok(0));
        while m.clocks() < 16_000_000 && m.bus().read32(0x0600E904) != Ok(0x600D) {
            m.run_steps(10000).unwrap();
        }
        for (address, expected, name) in golden() {
            assert_eq!(m.bus().read32(address), Ok(expected), "{name}");
        }
        check_raster(&m, 3, 5);
        assert_eq!(m.bus().read32(0x0600E900), Ok(0x12345678));
        assert_eq!(m.bus().read32(0x0600E904), Ok(0x600D));
        assert_eq!(m.backup_ram(), backup);
    }
}

#[test]
fn independent_capture_handlers_compute_both_sleep_wake_rounds() {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    m.run_steps(10000).unwrap();
    assert_eq!(m.bus().read32(0x0600E000), Ok(39));
    assert_eq!(m.bus().read32(0x0600E004), Ok(59));
    assert_eq!(m.bus().read32(0x0600D040), Ok(2));
    assert_eq!(m.bus().read32(0x0600D140), Ok(2));
}
