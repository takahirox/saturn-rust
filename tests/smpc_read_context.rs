//! SMPC diagnostics retain complete DMA and cache-refill read units.
use saturn_rust::bus::{Bus, BusError, Memory, Processor};
use saturn_rust::smpc::diagnostics::{ReadUnit, Rejection};

#[test]
fn dma_rejects_the_complete_longword_before_bsc_decomposition() {
    for processor in [Processor::Master, Processor::Slave] {
        for base in [0x00100000, 0x00100100, 0x20100000, 0x2017ff80] {
            let mut bus = Bus::default();
            bus.write8(base + 0x63, 1).unwrap();
            let before = bus.smpc_command_state();
            for (first, last) in [(true, true), (true, false), (false, true), (false, false)] {
                let address = base + 0x60;
                let Err(BusError::Smpc(fault)) =
                    bus.for_cpu(processor).dma_read32(address, first, last)
                else {
                    panic!("DMA must reject the complete SMPC longword");
                };
                assert_eq!(
                    (fault.address, fault.width, fault.write, fault.reason),
                    (address, 4, None, Rejection::ByteOnly)
                );
                assert_eq!(fault.state, before);
                assert_eq!(fault.refill, None);
                assert_eq!(bus.smpc_command_state(), before);
            }
            // Ordinary RAM DMA and individually readable SR/SF bytes still work.
            bus.write32(0x26010000, 0x12345678).unwrap();
            let memory = bus.for_cpu(processor);
            assert_eq!(memory.dma_read32(0x26010000, true, true), Ok(0x12345678));
            assert_eq!(memory.read8(base + 0x63), Ok(1));
        }
    }
}

#[test]
fn cache_refill_fault_retains_the_issuing_byte_operand() {
    for processor in [Processor::Master, Processor::Slave] {
        for base in [0x00100000, 0x00100100, 0x0017ff80] {
            let mut bus = Bus::default();
            bus.write8(base + 0x63, 1).unwrap();
            let before = bus.smpc_command_state();
            for offset in (0x20..0x30).chain(0x60..0x64) {
                let address = base + offset;
                let expected = bus.read8(address).unwrap();
                let mut memory = bus.for_cpu(processor);
                memory.write8(0xfffffe92, 0x11).unwrap(); // purge and enable cache
                // A rejected refill must not populate the line on retry.
                for _ in 0..2 {
                    let Err(BusError::Smpc(fault)) = memory.read8(address) else {
                        panic!("cache refill must retain explicit SMPC width rejection");
                    };
                    assert_eq!(
                        (fault.address, fault.width, fault.write, fault.reason),
                        (address, 1, None, Rejection::ByteOnly)
                    );
                    assert_eq!(fault.state, before);
                    let refill_address = (address & !15) | (((address & 12) + 4) & 15);
                    assert_eq!(
                        fault.refill,
                        Some(ReadUnit {
                            address: refill_address,
                            width: 4,
                        })
                    );
                    let message = fault.to_string();
                    assert!(message.contains(&format!("1-byte read at 0x{address:08X}")));
                    assert!(message.contains(&format!(
                        "cache refill: 4-byte read at 0x{refill_address:08X}"
                    )));
                }
                // Cache-through and refill-disabled reads need no wide unit.
                assert_eq!(memory.read8(address | 0x20000000), Ok(expected));
                memory.write8(0xfffffe92, 5).unwrap(); // CE + data refill disable
                assert_eq!(memory.read8(address), Ok(expected));
                memory.write8(0xfffffe92, 0).unwrap();
                assert_eq!(memory.read8(address), Ok(expected));
                assert_eq!(bus.smpc_command_state(), before);
            }
        }
    }
}
