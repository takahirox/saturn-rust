use super::*;
use crate::bus::{Bus, Memory};

fn original_image(source: &str) -> Vec<u8> {
    let mut bytes = vec![0; crate::bus::BIOS_SIZE];
    for line in source.lines().filter(|s| !s.starts_with('#')) {
        let mut words = line.split_whitespace();
        let Some(at) = words.next() else { continue };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    bytes
}

#[test]
fn long_paused_self_refresh_run_stays_within_each_video_chunk_clock_budget() {
    let mut image = original_image(include_str!("../../../tests/fixtures/refresh.hex"));
    assert_eq!(&image[0x4242..0x4244], &0x2A92u16.to_be_bytes());
    image[0x4242..0x4244].copy_from_slice(&9u16.to_be_bytes());
    let (mut reference, mut producer) = (crate::Machine::new(), crate::Machine::new());
    let mut consumer = Renderer::default();
    assert!(producer.enable_video_transport(true));
    for _ in 0..2 {
        for m in [&mut reference, &mut producer] {
            m.reset(crate::Boot::ExternalBios(&image)).unwrap();
            m.run_steps(6000).unwrap();
        }
        let chunks = producer.take_video_chunks();
        assert!(
            chunks.len() > 1,
            "clock budget must split long debugger runs"
        );
        for bytes in chunks {
            consumer.replay_bytes(&bytes).unwrap();
        }
        assert_eq!(reference.cpu, producer.cpu);
        assert_eq!(reference.slave, producer.slave);
        assert_eq!(reference.clocks, producer.clocks);
        assert_eq!(reference.loop_state_hashes(), producer.loop_state_hashes());
        assert_eq!(
            reference.framebuffer().unwrap(),
            consumer.framebuffer().unwrap()
        );
        assert_eq!(reference.drain_audio(8192), producer.drain_audio(8192));
        assert_eq!(producer.frames(), consumer.frames());
    }
}

#[test]
fn external_video_chunks_replay_original_guests_and_cold_reset_without_live_ram() {
    for (source, marker) in [
        (
            include_str!("../../../tests/fixtures/rotation-fetch-tile.hex"),
            0x0600DC0C,
        ),
        (
            include_str!("../../../tests/fixtures/rotation-fetch-rbg1.hex"),
            0x0600DC0C,
        ),
        (
            include_str!("../../../tests/fixtures/sprite-rotation16.hex"),
            0x0600DE18,
        ),
        (
            include_str!("../../../tests/fixtures/sprite-interlace.hex"),
            0x0600DF34,
        ),
    ] {
        let image = original_image(source);
        let (mut reference, mut producer) = (crate::Machine::new(), crate::Machine::new());
        let mut consumer = Renderer::default();
        assert!(producer.enable_video_transport(true));
        let mut chunk_count = 0;
        for _ in 0..2 {
            for m in [&mut reference, &mut producer] {
                m.reset(crate::Boot::ExternalBios(&image)).unwrap();
            }
            for _ in 0..32 {
                reference.advance_video_field().unwrap();
                producer.advance_video_field().unwrap();
                for bytes in producer.take_video_chunks() {
                    consumer.replay_bytes(&bytes).unwrap();
                    chunk_count += 1;
                }
                assert_eq!(reference.cpu, producer.cpu);
                assert_eq!(reference.slave, producer.slave);
                assert_eq!(reference.clocks, producer.clocks);
                assert_eq!(reference.loop_state_hashes(), producer.loop_state_hashes());
                assert_eq!(reference.video_dimensions(), consumer.dimensions());
                assert_eq!(
                    reference.framebuffer().unwrap(),
                    consumer.framebuffer().unwrap()
                );
                assert_eq!(reference.drain_audio(8192), producer.drain_audio(8192));
                assert_eq!(consumer.frames(), producer.frames());
                assert!(producer.video_transport_enabled());
                if reference.bus.read32(marker) == Ok(0x600D) {
                    break;
                }
            }
            assert_eq!(producer.bus.read32(marker), Ok(0x600D));
            assert!(
                producer.framebuffer().is_err(),
                "external output must not fall back to a stale local image"
            );
        }
        assert!(chunk_count > 1, "real producer chunks must be replayed");
    }
}

#[test]
fn queued_original_rotation_and_interlace_guests_match_pixels_cpu_and_reset() {
    for (source, marker) in [
        (
            include_str!("../../../tests/fixtures/rotation-fetch-tile.hex"),
            0x0600DC0C,
        ),
        (
            include_str!("../../../tests/fixtures/rotation-fetch-bitmap.hex"),
            0x0600DC0C,
        ),
        (
            include_str!("../../../tests/fixtures/rotation-fetch-rbg1.hex"),
            0x0600DC0C,
        ),
        (
            include_str!("../../../tests/fixtures/sprite-rotation16.hex"),
            0x0600DE18,
        ),
        (
            include_str!("../../../tests/fixtures/sprite-rotation8.hex"),
            0x0600DE18,
        ),
        (
            include_str!("../../../tests/fixtures/sprite-interlace.hex"),
            0x0600DF34,
        ),
    ] {
        let image = original_image(source);
        let (mut reference, mut queued) = (crate::Machine::new(), crate::Machine::new());
        assert!(queued.enable_video_journal(true));
        for _ in 0..2 {
            for m in [&mut reference, &mut queued] {
                m.reset(crate::Boot::ExternalBios(&image)).unwrap();
            }
            for _ in 0..32 {
                reference.advance_video_field().unwrap();
                queued.advance_video_field().unwrap();
                if reference.bus.read32(marker) == Ok(0x600D) {
                    break;
                }
            }
            assert_eq!(reference.bus.read32(marker), Ok(0x600D));
            assert_eq!(queued.bus.read32(marker), Ok(0x600D));
            assert_eq!(reference.cpu, queued.cpu);
            assert_eq!(reference.slave, queued.slave);
            assert_eq!(reference.clocks, queued.clocks);
            assert_eq!(reference.loop_state_hashes(), queued.loop_state_hashes());
            assert_eq!(reference.video_dimensions(), queued.video_dimensions());
            assert_eq!(reference.framebuffer(), queued.framebuffer());
            assert_eq!(reference.drain_audio(8192), queued.drain_audio(8192));
            reference.advance_video_field().unwrap();
            queued.advance_video_field().unwrap();
            assert_eq!(reference.framebuffer(), queued.framebuffer());
            assert!(queued.video_journal_enabled());
        }
    }
}

fn same_queued_board(reference: &Bus, queued: &Bus) {
    queued.drain_video_journal();
    let domain = queued.video_domain.as_ref().unwrap().borrow();
    same(reference, &domain.renderer);
    assert_eq!(reference.main_clocks, queued.main_clocks);
    assert_eq!(reference.vdp2.vram, queued.vdp2.vram);
    assert_eq!(reference.vdp2.cram, queued.vdp2.cram);
    assert_eq!(
        reference.vdp1.display_surface().bytes,
        queued.vdp1.display_surface().bytes
    );
    assert_eq!(
        queued.vdp1.display_surface().bytes,
        domain.renderer.sprites.display_surface().bytes
    );
    assert!(
        queued.vdp2.display_fetch.trace.borrow().is_empty(),
        "acquisition belongs to consumer"
    );
}

#[test]
fn queued_board_preserves_future_board_clock_catchup_and_video_mutations() {
    for advanced in [false, true] {
        for fast in [false, true] {
            let (mut reference, mut queued) = (Bus::default(), Bus::default());
            for board in [&mut reference, &mut queued] {
                board.change_system_clock(fast);
                for (offset, value) in [
                    (0, 0x8000),
                    (0x10, 0x4444),
                    (0x12, 0x4444),
                    (0x20, 0x101),
                    (0x28, 2),
                    (0x78, 1),
                    (0x7C, 1),
                    (0xF8, 1),
                ] {
                    board.write16(VDP2 + offset, value).unwrap();
                }
                board.vdp2.vram[..0x40000].fill(0x11);
                board.write16(crate::vdp2::CRAM + 2, 0x801F).unwrap();
            }
            reference.literal_display = true;
            assert!(queued.enable_video_journal(true));
            for (index, span) in [1, 3, 7, 13, 31, 64].into_iter().enumerate() {
                for board in [&mut reference, &mut queued] {
                    if advanced {
                        board.main_clocks += u64::from(span);
                        board
                            .advance_isolated_segment::<true>(span, false, false)
                            .unwrap();
                    } else {
                        board
                            .advance_isolated_segment::<false>(span, false, false)
                            .unwrap();
                    }
                }
                same_queued_board(&reference, &queued);
                for board in [&mut reference, &mut queued] {
                    board.write16(VDP2 + 0x70, (index * 3) as u16).unwrap();
                    board
                        .write16(
                            crate::vdp2::CRAM + 2,
                            if index & 1 == 0 { 0x83E0 } else { 0x801F },
                        )
                        .unwrap();
                }
            }
            for board in [&mut reference, &mut queued] {
                board.set_pal(true);
                board.change_system_clock(!fast);
            }
            same_queued_board(&reference, &queued);
            for board in [&mut reference, &mut queued] {
                board.system_reset();
            }
            same_queued_board(&reference, &queued);
            assert!(queued.video_journal_enabled());
        }
    }
}

#[test]
fn queued_board_installs_a_change_latched_after_vblank_out_before_line_zero() {
    // ST-013 §4.2: FBCR written right after VBlank-OUT (a retail FMV writes it
    // from that interrupt) is latched at the first HBlank-in and changes this
    // field. The consumer must compose line 0 from the new front.
    const FIELD: u32 = 263 * 1708;
    let (mut reference, mut queued) = (Bus::default(), Bus::default());
    for board in [&mut reference, &mut queued] {
        for (address, value) in [(VDP2, 0x8000), (VDP2 + 0xE0, 0x20), (VDP2 + 0xF0, 1)] {
            board.write16(address, value).unwrap();
        }
        board.write16(crate::vdp1::REGISTERS + 2, 2).unwrap();
        board.vdp1.install_display_surface(surface(0x801F));
        board.vdp1.draw_buffer_mut()[..2].copy_from_slice(&0xFC00u16.to_be_bytes());
    }
    assert!(queued.enable_video_journal(true));
    let mut previous = 0;
    for (target, request, front) in [
        (FIELD, true, 0x801F),
        (FIELD + 1279, false, 0x801F),
        (FIELD + 1280, false, 0xFC00),
        // After the prohibited window, a request waits for the next field.
        (FIELD + 3 * 1708, true, 0xFC00),
        (2 * FIELD + 1279, false, 0xFC00),
        (2 * FIELD + 1280, false, 0x801F),
    ] {
        for board in [&mut reference, &mut queued] {
            board.advance_main_clocks(target - previous, 0).unwrap();
        }
        same_queued_board(&reference, &queued);
        assert_eq!(queued.vdp1.display_pixel(0, 0), front);
        if target == FIELD + 1280 {
            assert_eq!(&reference.vdp2.scanning[..4], &[0, 0, 255, 255]);
        }
        if request {
            for board in [&mut reference, &mut queued] {
                board.write16(crate::vdp1::REGISTERS + 2, 3).unwrap();
            }
        }
        previous = target;
    }
}

#[test]
fn queued_board_records_captured_front_pixel_at_its_real_completion() {
    let (mut reference, mut queued) = (Bus::default(), Bus::default());
    for board in [&mut reference, &mut queued] {
        for (address, value) in [(VDP2, 0x8000), (VDP2 + 0xE0, 0x20), (VDP2 + 0xF0, 1)] {
            board.write16(address, value).unwrap();
        }
        for (at, words) in [
            (0, &[(0, 9), (20, 511), (22, 255)][..]),
            (32, &[(0, 6), (4, 0xC3), (6, 0x801F), (16, 1)][..]),
            (64, &[(0, 0x8000)][..]),
        ] {
            for &(offset, value) in words {
                board
                    .write16(crate::vdp1::VRAM + at + offset, value)
                    .unwrap();
            }
        }
        board.write16(crate::vdp1::REGISTERS + 2, 2).unwrap();
        board.write16(crate::vdp1::REGISTERS + 4, 1).unwrap();
        board.vdp1.advance(65).unwrap();
        board.write16(crate::vdp1::REGISTERS + 2, 0).unwrap();
        board.vdp1.vblank_out();
    }
    assert!(queued.enable_video_journal(true));
    for (span, pixel) in [(4, 0), (1, 0x801F), (7, 0x801F)] {
        for board in [&mut reference, &mut queued] {
            board.advance_main_clocks(span, 0).unwrap();
        }
        same_queued_board(&reference, &queued);
        assert_eq!(queued.vdp1.display_pixel(0, 0), pixel);
        if pixel == 0 {
            // This CPU write targets the draw buffer, but the established
            // flush still captures the old front's first dot before the
            // accepted pixel completes on that front at clock five.
            for board in [&mut reference, &mut queued] {
                board
                    .write16(crate::vdp1::FRAMEBUFFER + 0x2000, 0xABCD)
                    .unwrap();
            }
        }
    }
    for board in [&mut reference, &mut queued] {
        board.advance_main_clocks(1280 - 12, 0).unwrap();
    }
    same_queued_board(&reference, &queued);
    assert_eq!(&reference.vdp2.scanning[..4], &[0, 0, 0, 255]);
}

#[test]
fn queued_board_retains_display_and_blank_erase_before_front_swap() {
    for mode in [0, 8, 11] {
        let (mut reference, mut queued) = (Bus::default(), Bus::default());
        for board in [&mut reference, &mut queued] {
            for (address, value) in [
                (VDP2, 0x8000),
                (VDP2 + 0xE0, 0x20),
                (VDP2 + 0xF0, 1),
                (crate::vdp1::REGISTERS, mode),
                (crate::vdp1::REGISTERS + 6, 0x1234),
                (crate::vdp1::REGISTERS + 8, 0),
                (
                    crate::vdp1::REGISTERS + 10,
                    (if mode & 1 == 0 { 64 } else { 32 }) << 9 | 255,
                ),
            ] {
                board.write16(address, value).unwrap();
            }
            board.vdp1.draw_buffer_mut().fill(255);
            board.write16(crate::vdp1::REGISTERS + 2, 3).unwrap();
            board.vdp1.vblank_out();
            board.write16(crate::vdp1::REGISTERS + 2, 2).unwrap();
            if mode == 0 {
                board.vdp1.vblank_out();
            }
        }
        assert!(queued.enable_video_journal(true));
        let mut previous = 0;
        for target in [
            1279,
            1280,
            2988,
            224 * 1708,
            224 * 1708 + 1281,
            224 * 1708 + 1282,
            449204,
            450000,
        ] {
            for board in [&mut reference, &mut queued] {
                board.advance_main_clocks(target - previous, 0).unwrap();
            }
            same_queued_board(&reference, &queued);
            if mode == 0 && target == 1280 {
                assert_eq!(queued.vdp1.display_pixel(0, 0), 0x1234);
            }
            if mode != 0 && target == 224 * 1708 + 1282 {
                assert_eq!(
                    queued.vdp1.display_pixel(0, 0),
                    if mode == 8 { 0x1234 } else { 0x12 }
                );
            }
            previous = target;
        }
    }
}

#[test]
fn queued_machine_preserves_original_dual_cpu_pcm_and_complete_fields() {
    let mut bytes = vec![0; crate::bus::BIOS_SIZE];
    for line in include_str!("../../../tests/fixtures/dual-poll.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let Some(at) = words.next() else { continue };
        let at = usize::from_str_radix(at, 16).unwrap();
        for (i, word) in words.enumerate() {
            bytes[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    for fast in [false, true] {
        let (mut reference, mut queued) = (crate::Machine::new(), crate::Machine::new());
        for m in [&mut reference, &mut queued] {
            m.reset(crate::Boot::ExternalBios(&bytes)).unwrap();
            m.bus.change_system_clock(fast);
            for (offset, value) in [
                (0, 0x8000),
                (0x10, 0x4444),
                (0x12, 0x4444),
                (0x20, 0x101),
                (0x28, 0x32),
                (0x78, 1),
                (0x7C, 1),
                (0xF8, 1),
            ] {
                m.bus.write16(VDP2 + offset, value).unwrap();
            }
            for word in m.bus.vdp2.vram[..0x40000].as_chunks_mut::<2>().0 {
                word.copy_from_slice(&0x801Fu16.to_be_bytes());
            }
            let sound = &mut m.bus.sound;
            sound.ram[..4].copy_from_slice(&0x70000u32.to_be_bytes());
            sound.ram[4..8].copy_from_slice(&0x100u32.to_be_bytes());
            sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
            sound.ram[0x1000..0x1008].copy_from_slice(&[0x10, 0, 0xF0, 0, 0x20, 0, 0xE0, 0]);
            for (offset, value) in [
                (2, 0x1000),
                (6, 4),
                (8, 31),
                (12, 0x100),
                (16, 0),
                (22, 0xE000),
                (0, 0x1820),
                (0x400, 0x20F),
            ] {
                sound.scsp.write16(offset, value).unwrap();
            }
            sound.set_enabled(true).unwrap();
        }
        assert!(queued.enable_video_journal(true));
        let mut heard = false;
        for field in 0..3 {
            reference.advance_frame().unwrap();
            queued.advance_frame().unwrap();
            assert_eq!(reference.cpu, queued.cpu);
            assert_eq!(reference.slave, queued.slave);
            assert_eq!(reference.clocks, queued.clocks);
            assert_eq!(reference.loop_state_hashes(), queued.loop_state_hashes());
            assert_eq!(reference.sound_state(), queued.sound_state());
            assert_eq!(reference.video_dimensions(), queued.video_dimensions());
            assert_eq!(reference.framebuffer(), queued.framebuffer());
            let pcm = reference.drain_audio(8192);
            heard |= pcm.iter().any(|&v| v != 0);
            assert_eq!(pcm, queued.drain_audio(8192));
            assert!(queued.video_journal_enabled());
            assert_eq!(
                queued
                    .bus
                    .video_domain
                    .as_ref()
                    .unwrap()
                    .borrow()
                    .renderer
                    .frames(),
                queued.frames()
            );
            assert!(queued.bus.vdp2.display_fetch.trace.borrow().is_empty());
            for m in [&mut reference, &mut queued] {
                m.bus.write16(0x2100_0000, 0).unwrap();
                m.bus.write16(VDP2 + 0x70, (field * 8) as u16).unwrap();
                m.bus
                    .write32(crate::devices::VRAM + 0x400, 0xFC0083E0)
                    .unwrap();
            }
        }
        assert!(heard);
        assert!(
            !queued.enable_video_journal(false),
            "only reset-boundary changes"
        );
    }
}

fn reg(board: &mut Bus, renderer: &mut Renderer, offset: u32, value: u16) {
    board.write16(VDP2 + offset, value).unwrap();
    renderer
        .apply(Command::Register {
            address: VDP2 + offset,
            value,
        })
        .unwrap();
}
fn same(board: &Bus, renderer: &Renderer) {
    assert_eq!(board.raster.fields, renderer.raster.fields);
    assert_eq!(board.raster.counters(), renderer.raster.counters());
    assert_eq!(board.raster.display_slot(), renderer.raster.display_slot());
    assert_eq!(board.vdp2.hblank, renderer.video.hblank);
    assert_eq!(board.vdp2.vblank, renderer.video.vblank);
    assert_eq!(
        board.vdp2.display_fetch.trace.borrow().as_slice(),
        renderer.video.display_fetch.trace.borrow().as_slice(),
        "physical acquisition trace"
    );
    assert_eq!(board.vdp2.scanning, renderer.video.scanning);
    assert_eq!(board.vdp2.output_dimensions(), renderer.dimensions());
    assert_eq!(board.vdp2.scanout(&board.vdp1), renderer.framebuffer());
}

#[test]
fn serial_video_commands_replay_logical_spans_and_live_writes_on_original_board() {
    fn record_reg(board: &mut Bus, journal: &mut Journal, offset: u32, value: u16) {
        board.write16(VDP2 + offset, value).unwrap();
        journal.push(Command::Register {
            address: VDP2 + offset,
            value,
        });
    }
    for (fast, pal, mode) in [
        (false, false, 0),
        (true, false, 2),
        (false, true, 6),
        (true, true, 0xC0),
    ] {
        for format in [0, 3] {
            let (mut board, mut renderer, mut journal) =
                (Bus::default(), Renderer::default(), Journal::default());
            board.change_system_clock(fast);
            board.set_pal(pal);
            journal.push(Command::Clock(fast));
            journal.push(Command::Pal(pal));
            for (offset, value) in [
                (0, 0x8000 | mode),
                (0x10, 0x4444),
                (0x12, 0x4444),
                (0x20, 0x101),
                (0x28, format << 4 | 2),
                (0x78, 1),
                (0x7C, 1),
                (0xF8, 1),
            ] {
                record_reg(&mut board, &mut journal, offset, value);
            }
            let mut initial = vec![0x11; 0x40000];
            if format == 3 {
                for word in initial.as_chunks_mut::<2>().0 {
                    word.copy_from_slice(&0x801Fu16.to_be_bytes());
                }
            }
            board.vdp2.vram[..initial.len()].copy_from_slice(&initial);
            journal.push(Command::Vram {
                offset: 0,
                bytes: initial,
            });
            board.write16(crate::vdp2::CRAM + 2, 0x801F).unwrap();
            journal.push(Command::Cram {
                offset: 2,
                bytes: 0x801Fu16.to_be_bytes().to_vec(),
            });
            let mut previous = 0;
            let mut acquired = false;
            for target in [
                3, 7, 31, 75, 127, 512, 1023, 1279, 1280, 1340, 1643, 1675, 1716, 3000, 450000,
            ] {
                board.advance_main_clocks(target - previous, 0).unwrap();
                journal.push(Command::Advance {
                    clocks: target - previous,
                    before: Vec::new(),
                    endpoint: Endpoint::None,
                });
                // Delay consumption across later producer writes as well as spans.
                if [3, 75, 1279, 1716, 450000].contains(&target) {
                    journal.drain_into(&mut renderer).unwrap();
                    same(&board, &renderer);
                    acquired |= !board.vdp2.display_fetch.trace.borrow().is_empty();
                }
                if target == 75 {
                    record_reg(&mut board, &mut journal, 0x70, 8);
                }
                if target == 127 {
                    board.write32(crate::devices::VRAM, 0xFC0083E0).unwrap();
                    journal.push(Command::Vram {
                        offset: 0,
                        bytes: 0xFC0083E0u32.to_be_bytes().to_vec(),
                    });
                }
                if target == 512 {
                    record_reg(&mut board, &mut journal, 0x10, 0x4544);
                }
                if target == 1023 {
                    board.write16(crate::vdp2::CRAM + 2, 0x83E0).unwrap();
                    journal.push(Command::Cram {
                        offset: 2,
                        bytes: 0x83E0u16.to_be_bytes().to_vec(),
                    });
                }
                if target == 1340 {
                    board.set_pal(!pal);
                    journal.push(Command::Pal(!pal));
                }
                previous = target;
            }
            assert!(acquired, "actual physical requests must be replayed");
        }
    }
}

fn surface(value: u16) -> DisplaySurface {
    let mut bytes = vec![0; crate::vdp1::BUFFER_SIZE];
    bytes[..2].copy_from_slice(&value.to_be_bytes());
    DisplaySurface { mode: 0, bytes }
}

#[test]
fn front_write_precedes_hblank_scan_and_display_erase_follows_it() {
    let (mut board, mut renderer) = (Bus::default(), Renderer::default());
    for (offset, value) in [(0, 0x8000), (0xE0, 0x20), (0xF0, 1)] {
        reg(&mut board, &mut renderer, offset, value);
    }
    board.vdp1.install_display_surface(surface(0x801F));
    renderer
        .apply(Command::FrontSurface(surface(0x801F)))
        .unwrap();
    // The last VDP1 pixel at HBlank changes red to blue before scanout.
    board
        .vdp1
        .patch_display_surface(0, &0xFC00u16.to_be_bytes());
    board.advance_main_clocks(1280, 0).unwrap();
    // Display erase follows the line; it must not retroactively make it black.
    board.vdp1.patch_display_surface(0, &[0, 0]);
    renderer
        .apply(Command::Advance {
            clocks: 1280,
            before: vec![FrontPatch {
                offset: 0,
                bytes: 0xFC00u16.to_be_bytes().to_vec(),
            }],
            endpoint: Endpoint::HBlankErase(vec![FrontPatch {
                offset: 0,
                bytes: vec![0, 0],
            }]),
        })
        .unwrap();
    same(&board, &renderer);
    assert_eq!(&renderer.video.scanning[..4], &[0, 0, 255, 255]);
    assert_eq!(renderer.sprites.display_pixel(0, 0), 0);
    assert_eq!(&renderer.video.scanning[..4], &[0, 0, 255, 255]);
}

#[test]
fn queued_clock_spans_coalesce_without_crossing_owned_memory_writes_or_reset() {
    let mut q = Journal::default();
    let span = |clocks| Command::Advance {
        clocks,
        before: Vec::new(),
        endpoint: Endpoint::None,
    };
    q.push(span(1));
    q.push(span(3));
    assert_eq!(q.len(), 1);
    let mut live = vec![0x80, 0x1F];
    q.push(Command::Vram {
        offset: 0,
        bytes: live.clone(),
    });
    live.copy_from_slice(&[0xFC, 0]);
    q.push(span(4));
    q.push(span(8));
    assert_eq!(q.len(), 3);
    let mut renderer = Renderer::default();
    q.drain_into(&mut renderer).unwrap();
    assert!(q.is_empty());
    assert_eq!(
        &renderer.video.vram[..2],
        &[0x80, 0x1F],
        "consumer owns historical bytes"
    );
    q.push(Command::Reset { pal: true });
    q.push(Command::Vram {
        offset: 2,
        bytes: vec![0x83, 0xE0],
    });
    q.drain_into(&mut renderer).unwrap();
    assert_eq!(&renderer.video.vram[..4], &[0, 0, 0x83, 0xE0]);
    assert_eq!(renderer.frames(), 0);
    assert!(renderer.video.pal);
}
