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
                expected[4] = 11; // One Tw becomes two, over eight actual reads.
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
fn guest_partial_sharing_disables_external_monitoring_but_preserves_local_breaks() {
    // H §§7.2.1/7.8/7.10.3: identical cold-boot code on both processors,
    // including valid BSTROM initialization on the board's 32-bit CS0 bus.
    let rom = load(include_str!("fixtures/bsc-share.hex"));
    let golden: Vec<_> = include_str!("fixtures/bsc-share.tsv")
        .lines()
        .map(|v| u32::from_str_radix(v, 16).unwrap())
        .collect();
    for sharing in [0xFF0, 0xBF0] {
        let mut rom = rom.clone();
        replace_literal(&mut rom, 0xA55A0FF0, 0xA55A0000 | sharing);
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            m.run_steps(3000).unwrap();
            assert_eq!(m.cpu_state().exceptions, 0);
            assert_eq!(m.slave_state().exceptions, 0);
            for (cpu, base) in [0x0600E600, 0x0600E700].into_iter().enumerate() {
                assert_eq!(
                    m.bus().read32(base).unwrap(),
                    sharing | if cpu == 1 { 0x8000 } else { 0 }
                );
                let mut expected = golden.clone();
                if sharing & 0x400 == 0 {
                    expected[0] = 0x7000;
                }
                for (i, value) in expected.into_iter().enumerate() {
                    assert_eq!(
                        m.bus().read32(base + 4 + i as u32 * 4).unwrap(),
                        value,
                        "CPU {cpu} record {i}"
                    );
                }
            }
        }
    }
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
                // Each extra narrow-bus data beat occupies two clocks and Tw.
                // The reset-phase FRT fixture removes tick-rounding ambiguity.
                expected[2 + w * 2] =
                    (9 + 2 * (4 / width - 1)) as u32 + w as u32 * (4 / width) as u32;
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

fn idle_run(rom: &[u8], machine: &mut Machine) -> [Vec<u32>; 2] {
    machine.reset(Boot::ExternalBios(rom)).unwrap();
    machine.run_steps(3000).unwrap();
    assert_eq!(machine.cpu_state().exceptions, 0);
    assert_eq!(machine.slave_state().exceptions, 0);
    for cpu in [machine.cpu_state(), machine.slave_state()] {
        assert_eq!(cpu.status, saturn_rust::cpu::CpuStatus::Sleeping);
    }
    [0x0600E200, 0x0600E300].map(|base| {
        (0..6)
            .map(|i| machine.bus().read32(base + i * 4).unwrap())
            .collect()
    })
}

#[test]
fn both_cached_guests_measure_tas_turnaround_and_reset_clears_bus_history() {
    let rom = load(include_str!("fixtures/bsc-idle.hex"));
    let expected: Vec<_> = include_str!("fixtures/bsc-idle.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut machine = Machine::new();
    for _ in 0..2 {
        for trace in idle_run(&rom, &mut machine) {
            assert_eq!(trace, expected);
        }
    }
}

#[test]
fn changed_idle_and_tas_data_operands_change_actual_guest_timing_and_memory() {
    let rom = load(include_str!("fixtures/bsc-idle.hex"));
    let mut machine = Machine::new();
    let baseline = idle_run(&rom, &mut machine);
    for (before, after, index, value) in [(0xA55A8000, 0xA55A4000, 3, 10), (0x25, 0x3C, 4, 0xBC)] {
        let mut changed = rom.clone();
        replace_literal(&mut changed, before, after);
        for (cpu, trace) in idle_run(&changed, &mut machine).iter().enumerate() {
            let mut expected = baseline[cpu].clone();
            expected[index] = value;
            assert_eq!(*trace, expected);
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
