//! ST-013-R3 §2.1 pp.19–20: system-controller accesses interrupt drawing
//! on the selected local memory; independent memories remain usable.
use super::*;
use crate::vdp1::{FRAMEBUFFER, REGISTERS, VRAM};

fn fixture() -> Bus {
    let mut bus = Bus::default();
    for (offset, value) in [
        (0, 9),
        (20, 511),
        (22, 255),
        (32, 6),
        (36, 0xC0),
        (38, 0x801F),
        (48, 31),
        (64, 0x8000),
    ] {
        bus.write16(VRAM + offset, value).unwrap();
    }
    bus.write16(REGISTERS + 2, 2).unwrap();
    bus.write16(REGISTERS + 4, 1).unwrap();
    bus
}
fn advance(bus: &mut Bus, clocks: u32, chunk: u32) {
    let mut left = clocks;
    while left != 0 {
        let n = left.min(chunk);
        bus.advance_clocks(n).unwrap();
        left -= n;
    }
}
fn request(bus: &mut Bus, owner: Owner, address: u32) -> u32 {
    match owner {
        Owner::Cpu(p) => {
            if p == Processor::Slave {
                bus.set_slave_enabled(true);
            }
            assert_eq!(
                bus.replay_cpu(p).read16(address | 0x20000000),
                Err(BusError::Pending)
            );
        }
        Owner::Dmac(p) => {
            if p == Processor::Slave {
                bus.set_slave_enabled(true);
            }
            let chip = &mut bus.onchip[p as usize];
            for (offset, value) in [
                (0, address),
                (4, HIGH_WRAM + 0x800),
                (8, 1),
                (12, 0x5601),
                (0x30, 1),
            ] {
                chip.write32(0xFFFFFF80 + offset, value).unwrap();
            }
        }
        Owner::Scu => {
            for (offset, value) in [
                (0, address),
                (4, HIGH_WRAM + 0x800),
                (8, 2),
                (12, 0x102),
                (20, 7),
                (16, 0x101),
            ] {
                bus.scu.write32(SCU + offset, value).unwrap();
            }
        }
        Owner::Refresh | Owner::WriteAccept(_, _) | Owner::WriteBuffer(_, _) => unreachable!(),
    }
    bus.grant_bus_transaction();
    let startup = if owner == Owner::Scu {
        // SCU channel activation precedes its first data request. It owns
        // no local VDP1 memory during these two internal clocks.
        assert_eq!(bus.transactions.active.as_ref().unwrap().transfer.width, 0);
        bus.advance_clocks(2).unwrap();
        bus.grant_bus_transaction();
        2
    } else {
        0
    };
    assert!(
        bus.transactions
            .active
            .as_ref()
            .is_some_and(|active| active.owner == owner)
    );
    // Reset BCR1 selects six Tw states for CS2; ordinary T1/T2 add two.
    assert_eq!(bus.transactions.active.as_ref().unwrap().remaining, 8);
    startup
}

#[test]
fn cpu_dmac_and_scu_vram_reads_hold_the_in_flight_command_word() {
    for owner in [
        Owner::Cpu(Processor::Master),
        Owner::Cpu(Processor::Slave),
        Owner::Dmac(Processor::Master),
        Owner::Dmac(Processor::Slave),
        Owner::Scu,
    ] {
        for chunk in [1, 3, 64] {
            let mut bus = fixture();
            advance(&mut bus, 19, chunk); // system clip XC is read at22
            let startup = request(&mut bus, owner, VRAM + 0x100);
            advance(&mut bus, 8, chunk);
            // Replacing XC now must affect the unfinished read. Without
            // ownership XC was read at22 and all32 dots would still draw.
            bus.write16(VRAM + 20, 0).unwrap();
            advance(&mut bus, 136 - 19 - startup - 8 - 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x801F));
            assert_eq!(bus.read16(FRAMEBUFFER + 14), Ok(0));
        }
    }
}

#[test]
fn framebuffer_access_holds_draw_dots_and_cancel_releases_them_without_delayed_cpu_write() {
    for processor in [Processor::Master, Processor::Slave] {
        let mut bus = fixture();
        bus.set_slave_enabled(processor == Processor::Slave);
        advance(&mut bus, 68, 3);
        assert_eq!(bus.read16(FRAMEBUFFER + 6), Ok(0x801F));
        assert_eq!(
            bus.replay_cpu(processor)
                .write16(FRAMEBUFFER | 0x20000000, 0xFC00),
            Err(BusError::Pending)
        );
        bus.grant_bus_transaction();
        advance(&mut bus, 4, 1);
        assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x801F));
        assert_eq!(bus.read16(FRAMEBUFFER + 8), Ok(0));
        bus.cancel_cpu_transaction(processor);
        advance(&mut bus, 1, 1);
        assert_eq!(bus.read16(FRAMEBUFFER + 8), Ok(0x801F));
        advance(&mut bus, 100, 7);
        assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x801F));
        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
    }
}

#[test]
fn command_fetch_and_untextured_drawing_use_independent_local_memories() {
    let mut bus = fixture();
    advance(&mut bus, 1, 1);
    request(&mut bus, Owner::Cpu(Processor::Master), FRAMEBUFFER);
    advance(&mut bus, 8, 3);
    bus.write16(VRAM, 0x8000).unwrap(); // CTRL already fetched at2
    advance(&mut bus, 59, 3);
    assert_eq!(bus.read16(FRAMEBUFFER + 6), Ok(0x801F));
    bus.finish_cpu_transaction(Processor::Master);
    request(&mut bus, Owner::Cpu(Processor::Master), VRAM + 0x100);
    advance(&mut bus, 8, 3);
    assert_eq!(bus.read16(FRAMEBUFFER + 22), Ok(0x801F));
    assert_eq!(bus.read16(FRAMEBUFFER + 24), Ok(0));
}
