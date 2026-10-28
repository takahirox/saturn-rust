//! ST-169 §§2.3.9–10 and ST-162-R1 System Program §2.2 services 4.1/4.2.
//! Original, independently authored fixtures run on native and browser Wasm.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};
fn bios() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/clock.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut fields = line.split_whitespace();
        let mut address = usize::from_str_radix(fields.next().unwrap(), 16).unwrap();
        for word in fields {
            rom[address..address + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
            address += 2;
        }
    }
    rom
}
fn finish_clock_commands(m: &mut Machine) {
    // Two hardware 100 ms transitions, WDT-selected wakes and guest setup.
    // Bound emulated time,
    // independently of instruction mix or the number of competing CPUs.
    while m.clocks() < 9_000_000 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x0600E050), Ok(0x600D));
    // The completed field is historical. Let both the partial and following
    // full field scan the guest's final green back screen before inspecting it.
    m.advance_frame().unwrap();
    m.advance_frame().unwrap();
}
#[test]
fn synthetic_bios_and_hle_clock_calls_return_through_retained_code_and_memory() {
    let expected: Vec<_> = include_str!("fixtures/clock.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    let rom = bios();
    for hle in [false, true] {
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(if hle {
                Boot::HleDisc(include_bytes!("fixtures/clock.iso"))
            } else {
                Boot::ExternalBios(&rom)
            })
            .unwrap();
            finish_clock_commands(&mut m);
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0600E000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected, "HLE={hle}, PC={:08X}", m.cpu_state().pc);
            let lifecycle: Vec<_> = include_str!("fixtures/clock-lifecycle.tsv")
                .lines()
                .map(|l| u32::from_str_radix(l, 16).unwrap())
                .collect();
            let actual: Vec<_> = (0..lifecycle.len())
                .map(|i| m.bus().read32(0x0600E100 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, lifecycle, "HLE={hle}");
            m.drain_audio(8192);
            m.advance_frame().unwrap();
            let audio = m.drain_audio(8192);
            assert!(!audio.is_empty());
            assert!(audio.iter().all(|v| *v == 4096), "audio must recover");
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(m.cpu_state().exceptions, 2);
            assert_eq!(m.slave_state().status, CpuStatus::Reset);
            assert_eq!(m.slave_state().instructions, 0);
            let slave_result = m.bus().read32(0x0600D100).unwrap();
            assert!(slave_result > 0); // independently executed before CKCHG
            m.run_steps(1000).unwrap();
            assert_eq!(m.bus().read32(0x0600D100), Ok(slave_result));
            assert_eq!(m.video_dimensions(), (352, 224));
            let pixels = m.framebuffer().unwrap();
            assert_eq!(pixels.len(), 352 * 224 * 4);
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [0, 255, 0, 255])
            );
            assert_eq!(m.cpu_state().r[8], 0x0600E054); // callee-saved across both calls
            assert_eq!(m.bus().read32(0x0600E240), Ok(0));
            assert_eq!(m.bus().read32(0x0600E244), Ok(0));
            assert_eq!(m.cpu_state().r[9], 0x0600E160);
            for reg in 10..15 {
                assert_eq!(m.cpu_state().r[reg], 0x1000 + reg as u32);
            }
            if hle {
                for i in 0..2 {
                    assert_eq!(
                        m.bus().read32(0x0600E280 + i * 4),
                        m.bus().read32(0x0600E288 + i * 4)
                    );
                }
                assert_eq!(m.cpu_state().r[15], 0x060FF000);
                assert_eq!(m.bus().read32(0x06000324), Ok(1));
            } else {
                for i in 0..4 {
                    assert_eq!(m.bus().read32(0x0600E220 + i * 4), Ok(1));
                }
            }
            m.set_reset_button(true);
            m.run_steps(1000).unwrap();
            assert_eq!(m.cpu_state().exceptions, 3);
        }
    }
}

#[test]
fn changed_hle_clock_argument_changes_dotsel_and_the_public_clock_value() {
    let mut image = include_bytes!("fixtures/clock.iso").to_vec();
    let positions: Vec<_> = image[21 * 2048..]
        .windows(2)
        .enumerate()
        .filter_map(|(i, b)| (b == [0xE4, 0]).then_some(21 * 2048 + i))
        .collect();
    assert_eq!(positions.len(), 1); // original MOV #0,R4 call operand
    image[positions[0] + 1] = 1;
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    finish_clock_commands(&mut m);
    let mut expected: Vec<_> = include_str!("fixtures/clock.tsv")
        .lines()
        .map(|l| u32::from_str_radix(l, 16).unwrap())
        .collect();
    expected[0] = 1;
    expected[9] = 0x75;
    let actual: Vec<_> = (0..expected.len())
        .map(|i| m.bus().read32(0x0600E000 + i as u32 * 4).unwrap())
        .collect();
    assert_eq!(actual, expected);
}

fn lifecycle(m: &Machine) -> Vec<u32> {
    (0..24)
        .map(|i| m.bus().read32(0x0600E100 + i * 4).unwrap())
        .collect()
}

#[test]
fn clock_records_are_chunk_and_inspection_invariant() {
    let expected: Vec<_> = include_str!("fixtures/clock-lifecycle.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for chunk in [1, 3, 17, 10000] {
        for inspect in [false, true] {
            let mut m = Machine::new();
            m.reset(Boot::HleDisc(include_bytes!("fixtures/clock.iso")))
                .unwrap();
            while m.clocks() < 9_000_000 {
                m.run_steps(chunk).unwrap();
                if inspect {
                    m.debug_read16(0x25F80004).unwrap();
                    let _ = m.cpu_state();
                }
            }
            assert_eq!(lifecycle(&m), expected);
            assert_eq!(m.cpu_state().exceptions, 2);
        }
    }
}

fn replace(bytes: &mut [u8], before: &[u8], after: &[u8]) -> usize {
    let positions: Vec<_> = bytes
        .windows(before.len())
        .enumerate()
        .filter_map(|(i, b)| (b == before).then_some(i))
        .collect();
    for &i in &positions {
        bytes[i..i + before.len()].copy_from_slice(after);
    }
    positions.len()
}

#[test]
fn clock_guest_operand_and_omitted_preparation_or_restart_controls() {
    let original = bios();
    for control in [0, 1, 2] {
        let mut rom = original.clone();
        match control {
            0 => assert_eq!(
                replace(
                    &mut rom,
                    &0x13579BDFu32.to_be_bytes(),
                    &0x2468ACE0u32.to_be_bytes()
                ),
                2
            ),
            1 => assert_eq!(
                replace(&mut rom, &0xE0u32.to_be_bytes(), &0x60u32.to_be_bytes()),
                2
            ),
            _ => assert_eq!(
                replace(
                    &mut rom,
                    &[0x60, 0x12, 0xE0, 1, 0x21, 2],
                    &[0x60, 0x12, 0xE0, 3, 0x21, 2]
                ),
                2
            ),
        }
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            while m.clocks() < 9_000_000 {
                m.run_steps(10000).unwrap();
            }
            match control {
                0 => {
                    assert_eq!(lifecycle(&m)[9], 0x2468ACE0);
                    assert_eq!(lifecycle(&m)[21], 0x2468ACE0);
                }
                1 => {
                    assert_eq!(&lifecycle(&m)[..3], &[0x12, 0x34, 0x17]);
                }
                _ => {
                    assert_eq!(m.bus().read32(0x0600D204), Ok(0));
                    assert_eq!(m.bus().read32(0x0600E050), Ok(0));
                }
            }
        }
    }
}
