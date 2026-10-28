//! Independent original master/slave guest evidence for SH7604 §§5/7/14.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/ubc.hex")
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
#[test]
fn guest_instruction_slot_data_and_dmac_breaks_save_architectural_state() {
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom())).unwrap();
        m.run_steps(2000).unwrap();
        // Instruction addresses follow the authored sections; CPU/data breaks
        // save the next instruction, per H §6.3.5. BRCR identifies the source.
        let expected = [
            0x1000, 0, 0x9000, 0, 0x1102, 22, 0x9400, 0, 0x1206, 33, 0x9000, 0, 0x1288, 33, 0x1088,
            0, 0x133e, 33, 0x1048, 0, 0x600d,
        ];
        for (i, value) in expected.into_iter().enumerate() {
            assert_eq!(
                m.bus().read32(0x0600a000 + i as u32 * 4).unwrap(),
                value,
                "mailbox {i}"
            );
        }
        assert_eq!(m.cpu_state().r[15], 0x06100000);
        assert_eq!(m.cpu_state().exceptions, 5);
        assert_eq!(m.bus().read32(0x0600a050).unwrap(), 0x600d);
    }
}
