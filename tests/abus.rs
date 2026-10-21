//! ST-097 §3.2 pp.62–72: authored wait-state and physical select vectors.
use saturn_rust::{
    Boot, Machine,
    bus::{Bus, Memory, Processor},
    scu::{SCU, Scu},
};
fn image() -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in include_str!("fixtures/abus.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    image
}
fn run(rom: &[u8]) -> Vec<u32> {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(rom)).unwrap();
    m.run_steps(10000).unwrap();
    assert_eq!(m.cpu_state().exceptions, 0);
    (0..13)
        .map(|i| m.bus().read32(0x0600D700 + i * 4).unwrap())
        .collect()
}
#[test]
fn guest_measures_normal_narrow_burst_boundary_and_precharge_cycles() {
    let expected: Vec<_> = include_str!("fixtures/abus.tsv")
        .lines()
        .map(|v| u32::from_str_radix(v, 16).unwrap())
        .collect();
    let rom = image();
    for _ in 0..2 {
        assert_eq!(run(&rom), expected);
    }
    let mut changed = rom;
    let at: Vec<_> = changed
        .windows(4)
        .enumerate()
        .filter_map(|(i, b)| (b == 0x00300000u32.to_be_bytes()).then_some(i))
        .collect();
    assert_eq!(at.len(), 1);
    changed[at[0]..at[0] + 4].copy_from_slice(&0x00400000u32.to_be_bytes());
    let mut new = expected;
    new[2] = 12;
    assert_eq!(run(&changed), new);
}
#[test]
fn write_only_asr_registers_accept_all_documented_fields_on_both_cpu_views() {
    for processor in [Processor::Master, Processor::Slave] {
        let mut b = Bus::default();
        let mut memory = b.for_cpu(processor);
        for (offset, value) in [(0xB0, 0xFFFDFFFD), (0xB4, 0xF00DFFFD), (0xB8, 31)] {
            memory.write32(SCU + offset, value).unwrap();
            assert!(memory.read32(SCU + offset).is_err());
        }
    }
    let mut s = Scu::default();
    assert!(s.write32(SCU + 0xBC, 1).is_err()); // reserved, not AREF
}
