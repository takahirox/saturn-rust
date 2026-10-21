//! Original dual-SH-2 refresh guest; H §§7.5.7/7.10. Native bus unit
//! vectors provide exact deadlines; these guest FRT observations test the
//! scheduling effect independently of a measured timing oracle.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};

fn rom() -> Vec<u8> {
    parse(include_str!("fixtures/refresh.hex"))
}
fn parse(source: &str) -> Vec<u8> {
    let mut image = vec![0; BIOS_SIZE];
    for line in source.lines().filter(|l| !l.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}
fn replace(image: &mut [u8], before: u32, after: u32) {
    let places: Vec<_> = image
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == before.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(places.len(), 1);
    image[places[0]..places[0] + 4].copy_from_slice(&after.to_be_bytes());
}
fn run(image: &[u8], expected_sum: u32, refresh: bool) -> [Vec<u32>; 2] {
    let mut m = Machine::new();
    let mut reference = None;
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(image)).unwrap();
        m.run_steps(6000).unwrap();
        assert_eq!(m.cpu_state().exceptions, 0);
        assert_eq!(m.slave_state().exceptions, 0);
        let trace = [(0x0600E400, 7), (0x0600E500, 4)].map(|(at, n)| {
            (0..n)
                .map(|i| m.bus().read32(at + i * 4).unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            [
                trace[0][1],
                trace[0][2],
                trace[0][4],
                trace[0][5],
                trace[0][6]
            ],
            [
                expected_sum,
                32,
                expected_sum,
                if refresh { 0x88 } else { 0 },
                0x600D
            ]
        );
        assert_eq!(&trace[1][1..], &[expected_sum, 0, 0x600D]);
        assert!(trace[0][0] > 0 && trace[0][0] < 128);
        assert!(trace[0][3] > 0 && trace[0][3] < 128);
        assert!(trace[1][0] > 0 && trace[1][0] < 128);
        if refresh {
            assert!(trace[0][3] > trace[0][0] + 4);
        }
        if let Some(previous) = &reference {
            assert_eq!(&trace, previous);
        }
        reference = Some(trace);
    }
    reference.unwrap()
}

#[test]
fn refresh_delays_both_real_cpus_and_cached_guest_can_exit_self_refresh() {
    let original = rom();
    let active = run(&original, 0x26AF37A0, true); // 32 × 013579BD
    let mut disabled = original.clone();
    replace(&mut disabled, 0xA55A0008, 0xA55A0000);
    let inactive = run(&disabled, 0x26AF37A0, false);
    assert_eq!(active[0][0], inactive[0][0]);
    assert!(active[0][3] > inactive[0][3] + 4);
    assert!(active[1][0] > inactive[1][0] + 4);
    let mut mutated = original;
    replace(&mut mutated, 0x013579BD, 0x02468ACE);
    let changed = run(&mutated, 0x48D159C0, true); // 32 × 02468ACE
    assert_eq!(changed[0][0], active[0][0]);
    assert_eq!(changed[0][3], active[0][3]);
    assert_eq!(changed[1][0], active[1][0]);
}

#[test]
fn omitting_the_guest_self_refresh_exit_cannot_manufacture_a_completion() {
    let mut image = rom();
    // MOV.L R9,@R10 at 4200 + entry word + 32 ADD words.
    assert_eq!(&image[0x4242..0x4244], &[0x2A, 0x92]);
    image[0x4242..0x4244].copy_from_slice(&[0, 9]); // NOP, leaves RMODE set
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&image)).unwrap();
    m.run_steps(6000).unwrap(); // bounded scheduler returns while bus is held
    assert_eq!(m.bus().read32(0x0600E408), Ok(0));
    assert_eq!(m.bus().read32(0x0600E418), Ok(0));
    assert_eq!(m.slave_state().instructions, 0);
}

#[test]
fn guest_external_monitor_delays_slave_peripheral_reads_but_preserves_sampled_data() {
    let original = parse(include_str!("fixtures/monitor.hex"));
    let mut intervals = Vec::new();
    for control in [0x3000, 0x1000] {
        for value in [90, 60] {
            let mut image = original.clone();
            replace(&mut image, 0x3000, control);
            let at: Vec<_> = image
                .windows(2)
                .enumerate()
                .filter_map(|(i, b)| (b == [0xE0, 90]).then_some(i))
                .collect();
            assert_eq!(at.len(), 1);
            image[at[0] + 1] = value;
            let mut m = Machine::new();
            let mut previous = None;
            for _ in 0..2 {
                m.reset(Boot::ExternalBios(&image)).unwrap();
                m.run_steps(6000).unwrap();
                assert_eq!(m.cpu_state().exceptions, 0);
                assert_eq!(m.slave_state().exceptions, 0);
                let ticks = m.bus().read32(0x0600E600).unwrap();
                assert!(ticks > 0 && ticks < 256);
                assert_eq!(m.bus().read32(0x0600E604), Ok(u32::from(value) * 32));
                assert_eq!(m.bus().read32(0x0600E608), Ok(0x600D));
                assert_eq!(m.bus().read32(0x0600E700), Ok(0x600D));
                if let Some(before) = previous {
                    assert_eq!(ticks, before);
                }
                previous = Some(ticks);
            }
            intervals.push(previous.unwrap());
        }
    }
    assert_eq!(intervals[0], intervals[1]);
    assert_eq!(intervals[2], intervals[3]);
    assert!(intervals[0] > intervals[2] + 4);
}
