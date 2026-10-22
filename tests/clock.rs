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
    // Two hardware 100 ms transitions plus guest setup. Bound emulated time,
    // independently of instruction mix or the number of competing CPUs.
    while m.clocks() < 6_000_000 {
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
            assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
            assert_eq!(m.cpu_state().exceptions, 0);
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
            if hle {
                assert_eq!(m.bus().read32(0x06000324), Ok(1));
            }
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
