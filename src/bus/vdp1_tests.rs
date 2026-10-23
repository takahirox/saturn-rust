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

#[test]
fn bridge_both_words_own_the_selected_vdp1_memory_and_release_after_completion() {
    for p in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for framebuffer in [false, true] {
                let mut b = fixture();
                b.set_slave_enabled(true);
                advance(&mut b, 68, chunk);
                let at = (if framebuffer { FRAMEBUFFER } else { VRAM }) | 0x20000100;
                assert_eq!(b.replay_cpu(p).read16(at), Err(BusError::Pending));
                b.grant_bus_transaction();
                advance(&mut b, 8, chunk);
                assert_eq!(b.replay_cpu(p).read16(at), Err(BusError::Pending));
                b.grant_bus_transaction();
                advance(&mut b, 7, chunk);
                assert_eq!(b.replay_cpu(p).read16(at), Err(BusError::Pending));
                assert_eq!(
                    b.read16(FRAMEBUFFER + 8),
                    Ok(if framebuffer { 0 } else { 0x801F })
                );
                advance(&mut b, 1, chunk);
                assert_eq!(b.replay_cpu(p).read16(at), Ok(0));
                b.finish_cpu_transaction(p);
                advance(&mut b, 1, chunk);
                assert_eq!(b.read16(FRAMEBUFFER + 8), Ok(0x801F));
                advance(&mut b, 100, chunk);
                assert_eq!(b.read16(REGISTERS + 0x10), Ok(2));
            }
        }
    }
}

fn erase_registers(bus: &mut Bus, values: [u16; 3]) {
    for (offset, value) in [6, 8, 10].into_iter().zip(values) {
        bus.write16(REGISTERS + offset, value).unwrap();
    }
}
fn advance_inspected(bus: &mut Bus, clocks: u32, chunk: u32, inspect: bool) {
    let mut left = clocks;
    while left != 0 {
        let n = left.min(chunk);
        bus.advance_clocks(n).unwrap();
        if inspect {
            for offset in [0x10, 0x12, 0x14, 0x16] {
                bus.read16(REGISTERS + offset).unwrap();
            }
            let _ = bus.vdp1.display_pixel(8, 1);
            let _ = bus.read16(FRAMEBUFFER).unwrap();
        }
        left -= n;
    }
}

#[test]
fn both_sh2_adapters_capture_only_physically_completed_erase_writes_at_field_change() {
    // Emulator equal-edge convention: raster capture precedes external data
    // completion. ST-013 ch.4 specifies the boundary, not an electrical phase.
    const FIELD: u32 = 1708 * 263;
    for processor in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            for inspect in [false, true] {
                for (offset, value) in [(6, 0x5678), (8, 0x0402), (10, 0x0803)] {
                    for completion in [FIELD - 1, FIELD, FIELD + 1] {
                        let mut bus = Bus::default();
                        bus.set_slave_enabled(true);
                        bus.write16(REGISTERS, 8).unwrap();
                        bus.write16(REGISTERS + 4, 0).unwrap();
                        erase_registers(&mut bus, [0x1234, 0x0201, 0x0602]);
                        bus.write16(REGISTERS + 2, 3).unwrap();
                        bus.vdp1.vblank_out(); // initialize captured A
                        bus.vdp1.draw_buffer_mut().fill(0xEE);
                        bus.write16(REGISTERS + 2, 3).unwrap();
                        advance_inspected(&mut bus, completion - 8, chunk, inspect);
                        let address = (REGISTERS + offset) | 0x20000000;
                        assert_eq!(
                            bus.replay_cpu(processor).write16(address, value),
                            Err(BusError::Pending)
                        );
                        bus.grant_bus_transaction();
                        assert_eq!(bus.transactions.active.as_ref().unwrap().remaining, 8);
                        advance_inspected(&mut bus, 7, chunk, inspect);
                        let active = bus.transactions.active.as_ref().unwrap();
                        assert_eq!(active.remaining, 1);
                        assert_eq!(active.transfer.address, address);
                        assert_eq!(active.transfer.write, Some(u32::from(value)));
                        // SH-2 write-buffer acceptance already completed, but
                        // the physical register write still has one clock left.
                        assert_eq!(bus.replay_cpu(processor).write16(address, value), Ok(()));
                        bus.finish_cpu_transaction(processor);
                        advance_inspected(&mut bus, 1, chunk, inspect);
                        assert!(bus.transactions.active.is_none());
                        advance_inspected(
                            &mut bus,
                            FIELD + 224 * 1708 + 300 - completion,
                            chunk,
                            inspect,
                        );
                        let captured = completion < FIELD;
                        for y in 0..5 {
                            for x in [7, 8, 15, 16, 23, 24, 31, 32] {
                                let left = if captured && offset == 8 { 16 } else { 8 };
                                let right = if captured && offset == 10 { 32 } else { 24 };
                                let top = if captured && offset == 8 { 2 } else { 1 };
                                let bottom = if captured && offset == 10 { 4 } else { 3 };
                                let color = if captured && offset == 6 {
                                    0x5678
                                } else {
                                    0x1234
                                };
                                let expected =
                                    if (left..right).contains(&x) && (top..bottom).contains(&y) {
                                        color
                                    } else {
                                        0xEEEE
                                    };
                                assert_eq!(
                                    bus.vdp1.display_pixel(x, y),
                                    expected,
                                    "processor={processor:?} chunk={chunk} inspect={inspect} reg={offset} completion={completion} x{x} y{y}"
                                );
                            }
                        }
                        // CPU-visible destination is the other physical buffer.
                        assert_eq!(bus.read16(FRAMEBUFFER + 1040), Ok(0x1234));
                        assert_eq!(bus.read16(FRAMEBUFFER + 1038), Ok(0));
                        assert_eq!(bus.read16(REGISTERS + 0x16), Ok(0x1018));
                        assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
                        assert_eq!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn reset_cancels_pending_erase_register_transactions_and_unfinished_captured_erase() {
    for processor in [Processor::Master, Processor::Slave] {
        for chunk in [1, 3, 17, 10000] {
            let mut bus = Bus::default();
            bus.set_slave_enabled(true);
            bus.write16(REGISTERS, 8).unwrap();
            erase_registers(&mut bus, [0x1234, 0, 0x8000 | 255]);
            bus.write16(REGISTERS + 2, 3).unwrap();
            bus.vdp1.vblank_out();
            erase_registers(&mut bus, [0x5678, 0x0402, 0x0803]); // pending B
            bus.vdp1.vblank_in(1708);
            advance(&mut bus, 201, chunk); // unfinished accepted A job
            assert_eq!(bus.vdp1.display_pixel(0, 0), 0x1234);
            let address = (REGISTERS + 6) | 0x20000000;
            assert_eq!(
                bus.replay_cpu(processor).write16(address, 0x9ABC),
                Err(BusError::Pending)
            );
            advance(&mut bus, 4, chunk); // C is still physically in flight
            assert!(bus.transactions.active.is_some());
            bus.system_reset();
            bus.write16(REGISTERS, 8).unwrap();
            erase_registers(&mut bus, [0x2468, 0x0201, 0x0401]); // new run D
            bus.vdp1.draw_buffer_mut().fill(0xEE);
            bus.write16(REGISTERS + 2, 3).unwrap();
            bus.vdp1.vblank_out();
            bus.vdp1.vblank_in(1708);
            advance(&mut bus, 1708, chunk);
            assert_eq!(bus.vdp1.display_pixel(7, 1), 0xEEEE);
            assert_eq!(bus.vdp1.display_pixel(8, 1), 0x2468);
            assert_eq!(bus.vdp1.display_pixel(15, 1), 0x2468);
            assert_eq!(bus.vdp1.display_pixel(16, 1), 0xEEEE);
            assert_eq!(bus.vdp1.display_pixel(0, 0), 0xEEEE);
            assert_eq!(bus.read16(FRAMEBUFFER), Ok(0));
            assert_eq!(bus.read16(REGISTERS + 0x10), Ok(0));
            assert_eq!(bus.read32(SCU + 0xA4).unwrap() & 0x2000, 0);
        }
    }
}
