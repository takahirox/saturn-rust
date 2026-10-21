//! Independent original master/slave guest evidence for SH7604 §§5/7/14.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    load(include_str!("fixtures/bsc.hex"))
}
fn load(text: &str) -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in text
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let mut a = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for word in words {
            rom[a..a + 2].copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            a += 2;
        }
    }
    rom
}

fn waits_run(rom: &[u8]) -> [Vec<u32>; 2] {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(rom)).unwrap();
    m.run_steps(3000).unwrap();
    assert_eq!(m.cpu_state().exceptions, 0);
    assert_eq!(m.slave_state().exceptions, 0);
    assert_eq!(
        m.slave_state().status,
        saturn_rust::cpu::CpuStatus::Sleeping
    );
    [0x0600E000, 0x0600E100].map(|base| {
        (0..24)
            .map(|i| m.bus().read32(base + i * 4).unwrap())
            .collect()
    })
}

#[test]
fn both_guests_measure_programmed_external_waits_using_independent_free_running_timers() {
    let rom = load(include_str!("fixtures/bsc-waits.hex"));
    let expected: Vec<_> = include_str!("fixtures/bsc-waits.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let endian: Vec<_> = include_str!("fixtures/bsc-endian.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    for _ in 0..2 {
        for (cpu, trace) in waits_run(&rom).iter().enumerate() {
            assert_eq!(trace[0], if cpu == 0 { 0x3F0 } else { 0x83F0 });
            assert_eq!(trace[1], 0xAAFF);
            for (i, expected) in expected.iter().enumerate() {
                assert_eq!(trace[2 + i * 2], *expected, "CPU {cpu} wait setting {i}");
                assert_eq!(trace[3 + i * 2], 0x13579BDF);
            }
            assert_eq!(&trace[16..23], &endian);
            assert_eq!(trace[23], 0x600D);
        }
    }
}

#[test]
fn wait_and_data_operand_mutations_change_guest_timer_and_transfer_results() {
    let rom = load(include_str!("fixtures/bsc-waits.hex"));
    let original = waits_run(&rom);
    for (before, after) in [(0xA55AAA40_u32, 0xA55AAA80_u32), (0x13579BDF, 0x2468ACE0)] {
        let mut changed = rom.clone();
        let positions: Vec<_> = changed
            .windows(4)
            .enumerate()
            .filter_map(|(i, b)| (b == before.to_be_bytes()).then_some(i))
            .collect();
        assert_eq!(positions.len(), 1);
        changed[positions[0]..positions[0] + 4].copy_from_slice(&after.to_be_bytes());
        for (cpu, actual) in waits_run(&changed).iter().enumerate() {
            let mut expected = original[cpu].clone();
            if before == 0xA55AAA40 {
                expected[4] = 14; // One Tw becomes two, over eight actual reads.
            } else {
                for i in 0..7 {
                    expected[3 + i * 2] = after;
                }
            }
            assert_eq!(*actual, expected);
        }
    }
}

fn replace_literal(rom: &mut [u8], before: u32, after: u32) {
    let positions: Vec<_> = rom
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == before.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(positions.len(), 1);
    rom[positions[0]..positions[0] + 4].copy_from_slice(&after.to_be_bytes());
}

#[test]
fn reset_time_bus_width_and_endian_operands_change_actual_guest_transfers() {
    let rom = load(include_str!("fixtures/bsc-waits.hex"));
    let baseline = waits_run(&rom);
    for (bcr2, width, little) in [(0xFC, 4, false), (0xAC, 2, true), (0x5C, 1, false)] {
        let mut changed = rom.clone();
        replace_literal(&mut changed, 0xA55A00FC, 0xA55A0000 | bcr2);
        if !little {
            replace_literal(&mut changed, 0x1000, 0);
        }
        for (cpu, actual) in waits_run(&changed).iter().enumerate() {
            let mut expected = baseline[cpu].clone();
            for w in 0..7 {
                expected[2 + w * 2] = 12 + w as u32 * (4 / width);
            }
            if !little {
                expected[16..23].copy_from_slice(&[
                    0x11223344, 0x1122, 0x3344, 0x11, 0xA1B2C3D4, 0x1234C3D4, 0x5634C3D4,
                ]);
            }
            assert_eq!(*actual, expected);
        }
    }
}
#[test]
fn both_guest_cpus_use_hardware_identification_refresh_irq_rte_and_module_stops() {
    let mut m = Machine::new();
    let rom = rom();
    let mut previous = None;
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..3000 {
            m.step().unwrap();
        }
        let read = |a| m.bus().read32(a).unwrap();
        for (base, id) in [(0x06008000, 0x3F0), (0x06008040, 0x83F0)] {
            assert_eq!(read(base), id);
            assert_eq!(read(base + 4), id); // pin not writable
            assert_eq!(read(base + 8), 0xD0); // IRQ handler read CMF|CMIE|/16
            assert_eq!(read(base + 12), 1); // actual IRQ handler computation
            assert_eq!(read(base + 20), 0); // stopped FRT reset
            assert!(read(base + 24) > 0); // resumed FRT
            assert_eq!(read(base + 28), 0x600D);
        }
        assert_eq!(m.cpu_state().exceptions, 1);
        assert_eq!(m.slave_state().exceptions, 1);
        assert_eq!(m.cpu_state().r[15], 0x06100000);
        assert_eq!(m.slave_state().r[15], 0x060FE000);
        let trace = (m.cpu_state().clone(), m.slave_state().clone(), m.clocks());
        if let Some(old) = previous {
            assert_eq!(trace, old);
        }
        previous = Some(trace);
    }
}
