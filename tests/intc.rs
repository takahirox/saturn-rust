//! Original guest on the SH7604/SCU/SMPC/VDP2 bus, also run by browser-smoke.
//! H §5.3.8/table5.4; ST-169 table1.5; ST-058 §2.5/tables2.3–2.4.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
};
fn rom() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/intc.hex")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let Some(at) = words.next() else { continue };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn guest_changes_external_vector_mode_acknowledges_dma_and_handles_both_latch_ports() {
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom())).unwrap();
        m.run_steps(10000).unwrap();
        for (i, value) in [0xFFFF8000, 0x42, 0x4B, 0x600D].into_iter().enumerate() {
            assert_eq!(m.bus().read32(0x06009000 + i as u32 * 4), Ok(value));
        }
        assert_eq!(m.bus().read32(0x06005000), Ok(0x12345678));
        assert_eq!(m.bus().read32(0x06005004), Ok(0x12345678));
        for (i, line) in include_str!("fixtures/signed-divisions.tsv")
            .lines()
            .enumerate()
        {
            let fields: Vec<_> = line.split('\t').collect();
            for (half, field) in fields[1..].iter().enumerate() {
                assert_eq!(
                    m.bus().read32(0x06009100 + (i * 8 + half * 4) as u32),
                    Ok(u32::from_str_radix(field, 16).unwrap()),
                    "{} half {half}",
                    fields[0]
                );
            }
        }
        // One autovector, acquisition of its retained request, then the second DMA's
        // external-vector entry. Guest handlers record status and count.
        assert_eq!(m.cpu_state().exceptions, 3);
        for (at, expected) in [(0x06009020, 0x800), (0x06009024, 0), (0x06009028, 2)] {
            assert_eq!(m.bus().read32(at), Ok(expected));
        }
        assert_eq!(m.bus().read32(0x25FE00A4).unwrap() & 0x800, 0);
        for (i, port) in [0, 1, 0].into_iter().enumerate() {
            m.set_peripheral_th(port, true);
            let time = m.clocks();
            // Reference NTSC320 active-line counter encoding, independent of IRQ latency.
            let h = (time % 1708 / 4 * 2) as u32;
            let v = (time / 1708 % 263) as u32;
            m.set_peripheral_th(port, false);
            m.run_steps(500).unwrap();
            assert_eq!(m.bus().read32(0x06009010), Ok(i as u32 + 1));
            assert_eq!(m.bus().read32(0x06009014), Ok(h << 16 | v));
            assert_eq!(m.bus().read32(0x06009018).unwrap() & 0x202, 0x202);
            assert_eq!(m.bus().read32(0x0600901C).unwrap() & 0x202, 2);
            assert_eq!(m.cpu_state().last_exception.as_ref().unwrap().vector, 0x48);
            m.set_peripheral_th(port, false);
            m.run_steps(500).unwrap();
            assert_eq!(m.bus().read32(0x06009010), Ok(i as u32 + 1));
        }
        assert_eq!(m.cpu_state().exceptions, 6);
    }
}

#[test]
fn disabling_external_vectors_changes_guest_handler_and_prevents_the_next_stage() {
    let mut rom = rom();
    let pattern = [0xE0, 1, 0x21, 1]; // MOV #1,R0; MOV.W R0,@R1 (ICR)
    let matches: Vec<_> = rom
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == pattern).then_some(i))
        .collect();
    assert_eq!(matches.len(), 1);
    rom[matches[0] + 1] = 0;
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.run_steps(5000).unwrap();
    assert_eq!(m.bus().read32(0x06009004), Ok(0x42));
    assert_eq!(m.bus().read32(0x06009008), Ok(0));
    assert_eq!(m.bus().read32(0x0600900C), Ok(0));
    assert_eq!(m.cpu_state().exceptions, 2);
    // With VECMD=0 both entries use the internal vector, leaving the original
    // DMA0 request pending and preventing the subsequent DMA from starting.
    assert_eq!(m.bus().read32(0x25FE00A4).unwrap() & 0x800, 0x800);
    for (at, expected) in [(0x06009020, 0x800), (0x06009024, 0), (0x06009028, 0)] {
        assert_eq!(m.bus().read32(at), Ok(expected));
    }
}
