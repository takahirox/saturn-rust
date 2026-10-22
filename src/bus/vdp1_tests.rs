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
        Owner::Refresh
        | Owner::ResetDrain(_)
        | Owner::WriteAccept(_, _)
        | Owner::WriteBuffer(_, _) => unreachable!(),
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
            // SCU count=2 is two byte reads (ST-097 fig.2.1), so it
            // owns VRAM for two eight-clock grants. CPU/DMAC use a word.
            // The newly captured XC=0 also pre-clips the line to one dot.
            let finish = if owner == Owner::Scu { 113 } else { 105 };
            advance(&mut bus, finish - 19 - startup - 8 - 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x801F));
            assert_eq!(bus.read16(FRAMEBUFFER + 14), Ok(0));
        }
    }
}

#[test]
fn both_cpus_dmacs_and_scu_hold_unfetched_gouraud_words_on_the_real_bus() {
    // ST-013 §§2.1/5.3: system-controller VRAM access wins over drawing,
    // but cannot invalidate vertices already fetched. SCU activation has
    // two internal clocks before its external grant; both cases end at144.
    for owner in [
        Owner::Cpu(Processor::Master),
        Owner::Cpu(Processor::Slave),
        Owner::Dmac(Processor::Master),
        Owner::Dmac(Processor::Slave),
        Owner::Scu,
    ] {
        for chunk in [1, 3, 17] {
            let mut bus = fixture();
            bus.write16(VRAM + 36, 0xC4).unwrap();
            bus.write16(VRAM + 38, 0x8008).unwrap();
            bus.write16(VRAM + 60, 0x100).unwrap();
            for i in 0..4 {
                bus.write16(VRAM + 0x800 + i * 2, 0x4210).unwrap();
            }
            advance(&mut bus, 63, chunk);
            let startup = request(&mut bus, owner, VRAM + 0x100);
            advance(&mut bus, 8, chunk);
            bus.write16(VRAM + 0x800, 0x421F).unwrap(); // A still unfetched
            advance(&mut bus, 3 - startup, chunk); // clock74: A captured
            bus.write16(VRAM + 0x800, 0x4200).unwrap(); // cannot replace A
            bus.write16(VRAM + 0x802, 0x4200).unwrap(); // B not yet captured
            advance(&mut bus, 38, chunk); // final dot completes at112
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x8017));
            assert_eq!(bus.read16(FRAMEBUFFER + 62), Ok(0x8000));
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            assert_eq!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
            advance(&mut bus, 31, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
            assert_ne!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
        }
    }
}

fn lookup_fixture() -> Bus {
    let mut bus = fixture();
    for (offset, value) in [
        (32, 0),
        (36, 0xC8),
        (38, 0x200),
        (40, 0x100),
        (42, 0x101),
        (0x800, 0x0123),
        (0x802, 0x4567),
    ] {
        bus.write16(VRAM + offset, value).unwrap();
    }
    for i in 0..16 {
        bus.write16(VRAM + 0x1000 + i * 2, 0x8000 + i as u16)
            .unwrap();
    }
    bus
}

#[test]
fn every_board_master_holds_unfetched_lookup_words_but_preserves_captured_entries() {
    for owner in [
        Owner::Cpu(Processor::Master),
        Owner::Cpu(Processor::Slave),
        Owner::Dmac(Processor::Master),
        Owner::Dmac(Processor::Slave),
        Owner::Scu,
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut bus = lookup_fixture();
            advance(&mut bus, 63, chunk);
            let startup = request(&mut bus, owner, VRAM + 0x2000);
            advance(&mut bus, 8, chunk);
            bus.write16(VRAM + 0x1000, 0x83E0).unwrap();
            // SCU interleaves its first read with a work-RAM byte write;
            // that independent cycle permits LUT entries0/1.
            let capture = 74;
            advance(&mut bus, capture - 63 - startup - 8, chunk);
            bus.write16(VRAM + 0x1000, 0xFC00).unwrap();
            bus.write16(VRAM + 0x1002, 0x801F).unwrap();
            // Uncontended:64 command+32 LUT+4 texture+8 dots+32 END=140.
            // CPU/DMAC add one8-clock word; SCU adds two8-clock byte fringes.
            let finish = if owner == Owner::Scu { 156 } else { 148 };
            advance(&mut bus, finish - capture - 1, chunk);
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0x83E0));
            assert_eq!(
                bus.read16(FRAMEBUFFER + 2),
                Ok(0x801F),
                "scu={}, chunk={chunk}",
                owner == Owner::Scu
            );
            assert_eq!(bus.read16(FRAMEBUFFER + 14), Ok(0x8007));
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            assert_eq!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
            assert_ne!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
        }
    }
}

#[test]
fn every_board_framebuffer_owner_allows_texture_fetch_and_blocks_pixel_publication() {
    for owner in [
        Owner::Cpu(Processor::Master),
        Owner::Cpu(Processor::Slave),
        Owner::Dmac(Processor::Master),
        Owner::Dmac(Processor::Slave),
        Owner::Scu,
    ] {
        for chunk in [1, 3, 17, 10000] {
            let mut bus = lookup_fixture();
            advance(&mut bus, 96, chunk); // all LUT entries captured
            let startup = request(&mut bus, owner, FRAMEBUFFER + 0x100);
            advance(&mut bus, 8, chunk); // VRAM word captured at98
            bus.write16(VRAM + 0x800, 0x9999).unwrap();
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0));
            // SCU's byte reads are separated by a ten-clock WRAM byte
            // write under reset BSC settings. All8 dots plus the second
            // two-clock character fetch finish in that independent interval;
            // the second framebuffer read overlaps END's VRAM reads.
            let finish = if owner == Owner::Scu { 148 } else { 146 };
            advance(&mut bus, finish - 96 - startup - 8 - 1, chunk);
            for (x, color) in [
                0x8000, 0x8001, 0x8002, 0x8003, 0x8004, 0x8005, 0x8006, 0x8007,
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(bus.read16(FRAMEBUFFER + x as u32 * 2), Ok(color));
            }
            assert_eq!(
                bus.read16(REGISTERS + 0x10),
                Ok(0),
                "scu={}, chunk={chunk}",
                owner == Owner::Scu
            );
            advance(&mut bus, 1, chunk);
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(2));
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
