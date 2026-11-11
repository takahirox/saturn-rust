//! Original reset-vector firmware; the private BIOS instruction is unavailable.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

fn rom(text: &str) -> Vec<u8> {
    let mut image = vec![0; 524288];
    for line in text.lines().filter(|s| !s.starts_with('#')) {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            image[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    image
}

#[test]
fn original_dstp_load_and_store_firmware_proceeds() {
    for (source, records) in [
        (
            include_str!("fixtures/scu-dstp-read.hex"),
            include_str!("fixtures/scu-dstp-read.tsv"),
        ),
        (
            include_str!("fixtures/scu-dstp-write.hex"),
            include_str!("fixtures/scu-dstp-write.tsv"),
        ),
        (
            include_str!("fixtures/scu-dstp-access.hex"),
            include_str!("fixtures/scu-dstp-access.tsv"),
        ),
    ] {
        let image = rom(source);
        let expected: Vec<_> = records
            .lines()
            .map(|v| u32::from_str_radix(v, 16).unwrap())
            .collect();
        let mut reference = None;
        for chunk in [1, 3, 127, 10000] {
            let mut m = Machine::new();
            for boot in 0..2 {
                if boot == 1 {
                    m.mount_media(
                        saturn_rust::media::Media::iso(include_bytes!("../web/star-collector.iso"))
                            .unwrap(),
                    );
                }
                m.reset(Boot::ExternalBios(&image)).unwrap();
                if boot == 1 {
                    m.set_dstp_trace(true);
                }
                for start in (0..10000).step_by(chunk) {
                    m.run_steps((10000 - start).min(chunk) as u32).unwrap();
                }
                let actual: Vec<_> = (0..expected.len())
                    .map(|i| m.bus().read32(0x0600d000 + i as u32 * 4).unwrap())
                    .collect();
                assert_eq!(actual, expected, "{:?}", m.cpu_state());
                assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
                assert_eq!(
                    m.cpu_state().exceptions,
                    if expected.len() > 1 { 3 } else { 0 }
                );
                let state = (m.clocks(), m.cpu_state().clone());
                assert_eq!(*reference.get_or_insert(state.clone()), state);
            }
        }
    }
}

#[test]
fn completed_cycle_trace_distinguishes_operand_alias_value_and_generated_refill() {
    for (source, direction, value) in [
        (
            include_str!("fixtures/scu-dstp-read.hex"),
            "direct read",
            "value=None",
        ),
        (
            include_str!("fixtures/scu-dstp-write.hex"),
            "buffered write write",
            "value=Some(A55A1234)",
        ),
    ] {
        let mut m = Machine::new();
        let image = rom(source);
        m.reset(Boot::ExternalBios(&image)).unwrap();
        assert_eq!(m.dstp_trace(), None);
        m.set_dstp_trace(true);
        m.run_steps(10000).unwrap();
        let trace = m.dstp_trace().unwrap();
        assert_eq!(
            trace.lines().count(),
            if direction == "direct read" { 1 } else { 64 }
        );
        assert!(trace.contains(direction), "{trace}");
        assert!(trace.contains(value), "{trace}");
        assert!(
            trace.contains("source=Master SH-2 PC=Some(200002"),
            "{trace}"
        );
        assert!(
            trace.contains("address=0x25FE0060 physical=0x05FE0060 width=4"),
            "{trace}"
        );
        for _ in 0..100 {
            assert_eq!(m.debug_read32(0x25fe0060), Ok(0));
            assert_eq!(m.dstp_trace(), Some(trace.clone()));
        }
        m.reset(Boot::ExternalBios(&image)).unwrap();
        assert_eq!(m.dstp_trace(), None);
    }
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom(include_str!(
        "fixtures/scu-dstp-refill.hex"
    ))))
    .unwrap();
    m.set_dstp_trace(true);
    let error = m.run_steps(10000).unwrap_err();
    assert!(error.to_string().contains("0x05FE0064"), "{error}");
    let trace = m.dstp_trace().unwrap();
    assert_eq!(trace.lines().count(), 1);
    assert!(
        trace.contains("operand=Some((05FE006C, 00000004)) cache line-fill read"),
        "{trace}"
    );
    assert!(trace.contains("address=0x05FE0060"), "{trace}");
    assert!(trace.contains("result=Ok(00000000)"), "{trace}");
    let clocks = m.clocks();
    assert_eq!(m.run_steps(1).unwrap_err(), error);
    assert_eq!(m.clocks(), clocks);
}

#[test]
fn dstp_is_not_a_general_mmio_or_alignment_bypass() {
    use saturn_rust::bus::{Bus, BusError, Processor};
    let mut b = Bus::default();
    for address in [0x05fe0060, 0x25fe0060] {
        assert_eq!(b.read32(address), Ok(0));
        for value in [0, 1, u32::MAX] {
            b.write32(address, value).unwrap();
            assert_eq!(b.read32(address), Ok(0));
        }
        assert!(matches!(
            b.read8(address),
            Err(BusError::UnsupportedDevice { width: 1, .. })
        ));
        assert!(matches!(
            b.read16(address),
            Err(BusError::UnsupportedDevice { width: 2, .. })
        ));
        assert!(b.write8(address, 1).is_err());
        assert!(b.write16(address, 1).is_err());
        assert!(matches!(
            b.read32(address + 1),
            Err(BusError::Unaligned { .. })
        ));
        assert!(matches!(
            b.write32(address + 2, 1),
            Err(BusError::Unaligned { .. })
        ));
        for p in [Processor::Master, Processor::Slave] {
            let mut view = b.for_cpu(p);
            view.write32(address, 1).unwrap();
            assert_eq!(view.read32(address), Ok(0));
            view.write8(saturn_rust::cache::CCR, 0x11).unwrap();
            view.write32(address, 1).unwrap(); // stores do not allocate cache lines
            // Final-spec item5: +6C operand generates +60, then fails at +64.
            if address < 0x20000000 {
                assert!(matches!(
                    view.read32(address + 12),
                    Err(BusError::UnsupportedDevice {
                        address: 0x05fe0064,
                        width: 4
                    })
                ));
            }
            view.write8(saturn_rust::cache::CCR, 0).unwrap();
        }
    }
    for address in [0x05fe0064, 0x05fe0070, 0x05fe0078, 0x05fe00ac] {
        assert!(b.read32(address).is_err());
        assert!(b.write32(address, 0).is_err());
    }
    assert_eq!(b.read32(0x05fe007c), Ok(0)); // Separate DSTA activity subset.
    assert!(b.write32(0x05fe007c, 0).is_err());
    assert!(b.read16(0x05fe007c).is_err());
    assert!(b.read8(0x05fe007c).is_err());
    assert!(b.read32(0x05fe0008).is_err()); // write-only DMA count
    assert!(b.read32(0x05fe00a0).is_err()); // write-only IMS
    assert!(b.write32(0x05fe00c8, 4).is_err()); // read-only version
    b.write32(0x05fe00a4, 0).unwrap(); // Separate zero-clear IST contract.
    assert_eq!(b.read32(0x05fe00a4), Ok(0));
    assert!(matches!(
        b.read32(0x08000000),
        Err(BusError::Unmapped { .. })
    ));
}
