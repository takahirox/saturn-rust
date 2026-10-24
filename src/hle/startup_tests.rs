use super::*;
use crate::{bus::Processor, cpu::Cpu};

fn reset_defaults(bus: &mut Bus, processor: Processor) {
    let view = bus.for_cpu(processor);
    for (address, expected) in [
        (0xFFFF_FE60, 0),
        (0xFFFF_FE66, 0),
        (0xFFFF_FE68, 0),
        (0xFFFF_FEE2, 0),
        (0xFFFF_FEE0, 0x8000),
    ] {
        assert_eq!(view.read16(address), Ok(expected));
    }
    assert_eq!(view.read8(0xFFFF_FE10), Ok(1));
    assert_eq!(view.read8(0xFFFF_FE11), Ok(0));
}

#[test]
fn installation_preserves_architectural_reset_defaults_and_external_bios() {
    let mut bus = Bus::default();
    for p in [Processor::Master, Processor::Slave] {
        reset_defaults(&mut bus, p);
    }
    install(&mut bus, 0x06004000, 0x06001000).unwrap();
    for p in [Processor::Master, Processor::Slave] {
        reset_defaults(&mut bus, p);
    }
    let mut m = crate::Machine::new();
    let mut bios = vec![0; BIOS_SIZE];
    bios[..4].copy_from_slice(&0x100u32.to_be_bytes());
    bios[4..8].copy_from_slice(&0x060FF000u32.to_be_bytes());
    bios[0x100..0x102].copy_from_slice(&0x001Bu16.to_be_bytes());
    m.reset(crate::Boot::ExternalBios(&bios)).unwrap();
    m.run_steps(100).unwrap();
    for p in [Processor::Master, Processor::Slave] {
        reset_defaults(&mut m.bus, p);
    }
    assert_eq!(m.bus.read32(0x06000310), Ok(0));
}

#[test]
fn guest_boot_capture_assertion_mask_pending_unmask_frame_and_acknowledgement() {
    for processor in [Processor::Master, Processor::Slave] {
        let mut bus = Bus::default();
        install(&mut bus, 0x06004000, 0x06001000).unwrap();
        bus.write32(0x06000250, 0x06004000).unwrap();
        bus.set_slave_enabled(true);
        let mut cpu = Cpu::start(0x100, 0x060FF000);
        for _ in 0..200 {
            if cpu.state().pc == 0x06004000 {
                break;
            }
            cpu.step(&mut bus.for_cpu(processor)).unwrap();
        }
        assert_eq!(cpu.state().pc, 0x06004000);
        let vbr = if processor == Processor::Master {
            0x06000000
        } else {
            0x06000400
        };
        let sp = if processor == Processor::Master {
            0x060FF000
        } else {
            0x06001000
        };
        assert_eq!((cpu.state().vbr, cpu.state().r[15]), (vbr, sp));
        let view = bus.for_cpu(processor);
        assert_eq!(view.read8(0xFFFF_FE10), Ok(0x81));
        assert_eq!(view.read8(0xFFFF_FE11), Ok(0));
        assert_eq!(view.read16(0xFFFF_FE60), Ok(0x0F00));
        assert_eq!(view.read16(0xFFFF_FEE2), Ok(0));
        assert_eq!(view.read16(0xFFFF_FE66), Ok(0x6465));
        assert_eq!(view.read16(0xFFFF_FE68), Ok(0x6600));
        assert_eq!(
            view.read16(0xFFFF_FEE0),
            Ok(if processor == Processor::Master {
                0x8001
            } else {
                0x8000
            })
        );
        assert_eq!(bus.interrupt(processor), None); // Initialization is not a strobe.
        for (i, word) in [0xE0E0, 0x400E, 0x0009, 0x001B].into_iter().enumerate() {
            bus.write16(0x06004000 + i as u32 * 2, word).unwrap();
        }
        // Independently encoded handler: guest FTCSR read, zero write, RTE.
        for (i, word) in [
            0xD101, 0xA003, 0x0009, 0x0009, 0xFFFF, 0xFE11, 0x6010, 0xE000, 0x2100, 0x002B, 0x0009,
        ]
        .into_iter()
        .enumerate()
        {
            bus.write16(0x06004200 + i as u32 * 2, word).unwrap();
        }
        bus.write32(vbr + 0x190, 0x06004200).unwrap();
        let strobe = if processor == Processor::Master {
            0x21800000
        } else {
            0x21000000
        };
        bus.write16(strobe, 0x1234).unwrap();
        assert_eq!(bus.interrupt(processor), Some((15, 0x64)));
        assert!(
            !cpu.accept_interrupt(&mut bus.for_cpu(processor), 15, 0x64)
                .unwrap()
        );
        // A zero write without first observing ICF cannot acknowledge it.
        bus.for_cpu(processor).write8(0xFFFF_FE11, 0).unwrap();
        assert_eq!(bus.interrupt(processor), Some((15, 0x64)));
        for _ in 0..3 {
            cpu.step(&mut bus.for_cpu(processor)).unwrap();
        }
        assert_eq!(cpu.state().sr & 0xF0, 0xE0);
        assert!(
            cpu.accept_interrupt(&mut bus.for_cpu(processor), 15, 0x64)
                .unwrap()
        );
        assert_eq!(cpu.state().pc, 0x06004200);
        assert_eq!(cpu.state().r[15], sp - 8);
        assert_eq!(bus.read32(sp - 8), Ok(0x06004006));
        assert_eq!(bus.read32(sp - 4).unwrap() & 0xF0, 0xE0);
        assert_eq!(bus.interrupt(processor), Some((15, 0x64))); // Entry is not acknowledgement.
        for _ in 0..5 {
            cpu.step(&mut bus.for_cpu(processor)).unwrap();
        }
        assert_eq!(cpu.state().pc, 0x06004210);
        assert_eq!(bus.interrupt(processor), Some((15, 0x64))); // Guest read alone is insufficient.
        cpu.step(&mut bus.for_cpu(processor)).unwrap();
        assert_eq!(bus.interrupt(processor), None);
        cpu.step(&mut bus.for_cpu(processor)).unwrap();
        cpu.step(&mut bus.for_cpu(processor)).unwrap();
        assert_eq!((cpu.state().pc, cpu.state().r[15]), (0x06004006, sp));
        bus.write16(strobe, 0x5678).unwrap();
        assert_eq!(bus.interrupt(processor), Some((15, 0x64))); // Acknowledged then reasserted.
        if processor == Processor::Slave {
            let retained: Vec<_> = (0..512)
                .map(|i| bus.read8(0x06000400 + i).unwrap())
                .collect();
            bus.set_slave_enabled(false);
            reset_defaults(&mut bus, Processor::Slave);
            assert_eq!(bus.interrupt(Processor::Slave), None);
            bus.set_slave_enabled(true);
            let mut restarted = Cpu::start(0x100, 0x060FF000);
            for _ in 0..200 {
                if restarted.state().pc == 0x06004000 {
                    break;
                }
                restarted.step(&mut bus.for_cpu(Processor::Slave)).unwrap();
            }
            assert_eq!(restarted.state().pc, 0x06004000);
            assert_eq!(bus.for_cpu(Processor::Slave).read8(0xFFFF_FE10), Ok(0x81));
            assert_eq!(
                bus.for_cpu(Processor::Slave).read16(0xFFFF_FE60),
                Ok(0x0F00)
            );
            assert_eq!(
                bus.for_cpu(Processor::Slave).read16(0xFFFF_FE66),
                Ok(0x6465)
            );
            assert_eq!(bus.interrupt(Processor::Slave), None);
            assert_eq!(
                retained,
                (0..512)
                    .map(|i| bus.read8(0x06000400 + i).unwrap())
                    .collect::<Vec<_>>()
            );
        }
    }
}
