//! Original licensed SH-2 instructions upload/execute DSP programs via SCU MMIO.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/dsp-arithmetic.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn expected() -> Vec<u32> {
    include_str!("fixtures/dsp-arithmetic.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect()
}
fn records(m: &Machine) -> Vec<u32> {
    (0..expected().len())
        .map(|i| m.bus().read32(0x0600ec00 + i as u32 * 4).unwrap())
        .collect()
}
fn run(m: &mut Machine, image: &[u8], chunk: u32) -> Vec<u32> {
    m.reset(Boot::ExternalBios(image)).unwrap();
    assert_eq!(m.bus().read32(0x05fe0080).unwrap(), 0);
    assert_eq!(m.bus().read32(0x05fe00a4).unwrap() & 0x20, 0);
    let mut remaining = 10000;
    while remaining > 0 {
        let n = chunk.min(remaining);
        m.run_steps(n).unwrap();
        // Paused debugger inspection must not advance or change machine state.
        let clocks = m.clocks();
        let state = m.cpu_state().clone();
        assert_eq!(records(m), records(m));
        assert_eq!(m.clocks(), clocks);
        assert_eq!(m.cpu_state(), &state);
        remaining -= n;
    }
    assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(m.cpu_state().exceptions, 2); // exactly two DSP-end handlers
    assert_eq!(
        m.cpu_state().last_exception.as_ref().map(|e| e.vector),
        Some(0x45)
    );
    let result = records(m);
    m.run_steps(1000).unwrap();
    assert_eq!(records(m), result);
    assert_eq!(m.cpu_state().exceptions, 2);
    result
}

#[test]
fn guest_arithmetic_results_status_and_end_irqs_survive_chunks_pause_and_cold_boots() {
    let mut m = Machine::new();
    for chunk in [1, 3, 17, 10000] {
        for _ in 0..2 {
            assert_eq!(run(&mut m, &rom(), chunk), expected());
        }
    }
}

#[test]
fn guest_operand_mutations_and_omitted_arithmetic_change_the_authored_records() {
    let mut m = Machine::new();
    for variant in [1, 2, 3] {
        let mut image = rom();
        let mut golden = expected();
        match variant {
            1 => {
                // First ADD no longer overflows; second ADD now overflows.
                image[0x1f000..0x1f004].copy_from_slice(&0x7ffffffeu32.to_be_bytes());
                golden[2..7].copy_from_slice(&[0, 0, 0, 0x7fffffff, 0x7fff]);
                golden[7..12].copy_from_slice(&[0x480000, 0x400000, 0x400000, 0x80000000, 0x8000]);
            }
            2 => {
                image[0x1f000..0x1f004].copy_from_slice(&0x80000000u32.to_be_bytes());
                golden[2..7]
                    .copy_from_slice(&[0x400000, 0x400000, 0x400000, 0x80000001, 0xffff8000]);
                golden[7..12]
                    .copy_from_slice(&[0x400000, 0x400000, 0x400000, 0x80000002, 0xffff8000]);
            }
            _ => {
                // Omit only the first kernel's ADD; retain its parallel moves.
                let needle = 0x10043209u32.to_be_bytes();
                let positions: Vec<_> = image
                    .windows(4)
                    .enumerate()
                    .filter(|(_, b)| *b == needle)
                    .map(|(i, _)| i)
                    .collect();
                assert_eq!(positions.len(), 4);
                let at = positions[0];
                image[at..at + 4].copy_from_slice(&0x00043209u32.to_be_bytes());
                golden[2..7].copy_from_slice(&[0, 0, 0, 0x7fffffff, 0x7fff]);
            }
        }
        for _ in 0..2 {
            assert_eq!(run(&mut m, &image, 17), golden);
        }
    }
}

#[test]
fn cold_reset_cancels_overflow_and_an_unhandled_end_event() {
    let image = rom();
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&image)).unwrap();
    let mut found = false;
    for _ in 0..10000 {
        m.run_steps(1).unwrap();
        if m.bus().read32(0x05fe00a4).unwrap() & 0x20 != 0 {
            found = true;
            break;
        }
    }
    assert!(found, "guest ENDI must request an actual SCU interrupt");
    // The first ENDI kernel overflows. Leave its status unread so reset,
    // rather than a destructive control-port read, must cancel V and E.
    assert_eq!(run(&mut m, &image, 3), expected());
}
