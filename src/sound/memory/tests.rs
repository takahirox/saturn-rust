//! Independently enumerated boundaries: docs/scsp-cpu-memory.md.
use super::*;

fn advance(s: &mut Sound, target: u64, chunk: u64) {
    while s.time < target {
        s.advance((target - s.time).min(chunk) as u32).unwrap();
    }
}
fn request(address: usize, width: usize, write: Option<u32>) -> Request {
    Request {
        address,
        width,
        write,
    }
}
fn sound() -> Sound {
    let mut s = Sound::default();
    s.scsp.write16(0x400, 0x20F).unwrap();
    s
}
#[test]
fn arrival_edges_wake_idle_allocation_and_reads_survive_later_acknowledgement() {
    for chunk in [1, 3, 17, 10000] {
        // At5 means after that boundary's allocation has run. Even a request
        // at4 must wait until10 to sample. No sample-frame event is required.
        for (arrival, grant, completion) in [(4, 1, 10), (5, 2, 15), (6, 2, 15)] {
            let mut s = sound();
            advance(&mut s, arrival, chunk);
            s.ram[0x1000..0x1002].copy_from_slice(&0x1111u16.to_be_bytes());
            s.main_ram_request(Some(request(0x1000, 2, None)));
            assert!(u64::from(s.clocks_to_event()) <= 5);
            advance(&mut s, completion - 1, chunk);
            assert!(!s.main_ram_ready());
            s.ram[0x1000..0x1002].copy_from_slice(&0x2222u16.to_be_bytes());
            advance(&mut s, completion, chunk);
            s.ram[0x1000..0x1002].copy_from_slice(&0x3333u16.to_be_bytes());
            advance(&mut s, 100, chunk);
            assert_eq!(s.complete_main_ram(), Ok(0x2222));
            assert_eq!(
                s.memory.trace,
                [(grant, Owner::Main, false), (grant + 1, Owner::Main, true)]
            );
            assert_eq!(s.scsp.generated, 0);
        }
    }
}
#[test]
fn simultaneous_priority_and_late_high_priority_request_do_not_replace_a_grant() {
    for late in [false, true] {
        for chunk in [1, 3, 17, 10000] {
            let mut s = sound();
            s.memory.submit(Owner::Cpu, request(0x1001, 1, Some(0x34)));
            if late {
                advance(&mut s, 5, chunk);
            }
            s.main_ram_request(Some(request(0x1000, 2, Some(0xABCD))));
            advance(&mut s, 14, chunk);
            let expected = if late { 0xABCDu16 } else { 0xAB34 };
            advance(&mut s, 15, chunk);
            assert_eq!(s.read_ram8(0x1000), (expected >> 8) as u8);
            assert_eq!(s.read_ram8(0x1001), expected as u8);
            let (a, b) = if late {
                (Owner::Cpu, Owner::Main)
            } else {
                (Owner::Main, Owner::Cpu)
            };
            assert_eq!(
                s.memory.trace,
                [(1, a, false), (2, a, true), (2, b, false), (3, b, true)]
            );
            s.write_ram8(0x1001, 0x56);
            assert_eq!(s.memory.take(Owner::Cpu), Some(0x34));
            assert_eq!(s.complete_main_ram(), Ok(0xABCD));
            advance(&mut s, 500, chunk);
            assert_eq!(s.read_ram8(0x1001), 0x56); // no phantom replay/publication
            assert_eq!(s.memory.trace.len(), 4);
        }
    }
}
#[test]
fn internal_pcm_dsp_refresh_and_dma_precede_queued_cpu_clients_then_release_them() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = sound();
        // Retain PG/address work, then collide requests at OP3's first read.
        for (at, value) in [(2, 0x1000), (6, 1), (12, 0x100), (22, 0xE000), (0, 0x1820)] {
            s.scsp.write16(at, value).unwrap();
        }
        advance(&mut s, (8 * REFERENCE_HZ).div_ceil(DSP_HZ), chunk);
        // PCM ticks9/11; DSP10; internal DMA12/13; CPU clients14/15.
        for (at, value) in [
            (0x84C, 0xA002),
            (0x84E, 0x8000),
            (0x412, 0x2000),
            (0x414, 0x700),
            (0x416, 0x3004),
        ] {
            s.scsp.write16(at, value).unwrap();
        }
        s.main_ram_request(Some(request(0x1000, 2, Some(0x1234))));
        s.memory.submit(Owner::Cpu, request(0x1000, 2, None));
        advance(&mut s, (16 * REFERENCE_HZ).div_ceil(DSP_HZ), chunk);
        let grants: Vec<_> = s
            .memory
            .trace
            .iter()
            .filter(|e| !e.2)
            .map(|e| (e.0, e.1))
            .collect();
        assert_eq!(
            grants,
            [
                (9, Owner::Pcm),
                (10, Owner::Dsp),
                (11, Owner::Pcm),
                (12, Owner::Dma),
                (13, Owner::Dma),
                (14, Owner::Main),
                (15, Owner::Cpu)
            ]
        );
        assert_eq!(s.memory.take(Owner::Cpu), Some(0x1234));
        assert_eq!(s.complete_main_ram(), Ok(0x1234));
        // Next refresh at596 (cycle125), main600..605, refresh605..610,
        // sound610..615. Both queued before596.
        advance(&mut s, 592, chunk);
        s.main_ram_request(Some(request(0x1000, 2, None)));
        s.memory
            .submit(Owner::Cpu, request(0x1000, 2, Some(0x5678)));
        advance(&mut s, 615, chunk);
        // Next sample's OP3 slot0 starts at137, retaining the original
        // assertion that PCM resumes after both CPU clients are released.
        advance(&mut s, (137 * REFERENCE_HZ).div_ceil(DSP_HZ), chunk);
        let grants: Vec<_> = s
            .memory
            .trace
            .iter()
            .filter(|e| !e.2 && e.0 >= 125)
            .map(|e| (e.0, e.1))
            .collect();
        assert_eq!(
            grants,
            [
                (125, Owner::Refresh),
                (126, Owner::Main),
                (127, Owner::Refresh),
                (128, Owner::Cpu),
                (137, Owner::Pcm),
            ]
        );
        assert_eq!(s.complete_main_ram(), Ok(0x1234));
        assert_eq!(s.memory.take(Owner::Cpu), Some(0x5678));
        // Exclusive, one-cycle grants; complete before equal-edge reassignment.
        let mut previous_grant = None;
        for &(tick, owner, completion) in &s.memory.trace {
            if completion {
                assert_eq!(previous_grant.take(), Some((tick - 1, owner)));
            } else {
                assert!(previous_grant.replace((tick, owner)).is_none());
            }
        }
    }
}
#[test]
fn geometry_and_byte_lanes_are_latched_on_admission_and_absent_chip_reads_high() {
    for chunk in [1, 3, 17, 10000] {
        let mut s = sound();
        s.main_ram_request(Some(request(0x7FFFE, 2, Some(0x1234))));
        advance(&mut s, 5, chunk);
        s.scsp.write16(0x400, 15).unwrap();
        advance(&mut s, 10, chunk);
        assert_eq!(&s.ram[0x7FFFE..], &[0x12, 0x34]);
        assert_eq!(s.complete_main_ram(), Ok(0x1234));
        s.main_ram_request(Some(request(0x20000, 2, None)));
        advance(&mut s, 20, chunk);
        assert_eq!(s.complete_main_ram(), Ok(65535));
        s.main_ram_request(Some(request(0x1001, 1, Some(0xAB))));
        advance(&mut s, 29, chunk);
        assert_eq!(&s.ram[0x1000..0x1002], &[0, 0xAB]);
    }
}
#[test]
fn cancel_queued_granted_and_completed_operations_preserves_only_published_writes() {
    for owner in [Owner::Main, Owner::Cpu] {
        for cancel_at in [4, 5, 9, 10] {
            let mut s = sound();
            s.memory.submit(owner, request(0x1000, 2, Some(0x1234)));
            advance(&mut s, cancel_at, 3);
            if owner == Owner::Cpu {
                s.set_enabled(false).unwrap();
            } else {
                s.main_ram_request(None);
            }
            advance(&mut s, 100, 17);
            assert_eq!(
                &s.ram[0x1000..0x1002],
                if cancel_at == 10 {
                    &[0x12, 0x34]
                } else {
                    &[0, 0]
                }
            );
            assert!(!s.memory.pending());
            assert!(!s.memory.ready(owner));
        }
    }
    for reset_at in [4, 5, 9, 10] {
        let mut s = sound();
        s.main_ram_request(Some(request(0x1000, 2, Some(0x5678))));
        advance(&mut s, reset_at, 1);
        s.reset_output_start = Some(reset_at + 1);
        s.reset_output_end = Some(reset_at + 20);
        // Cancel the external request during RESET. It must not return as a
        // phantom operation when the device is released.
        advance(&mut s, reset_at + 2, 1);
        s.main_ram_request(None);
        advance(&mut s, 100, 17);
        assert_eq!(
            &s.ram[0x1000..0x1002],
            if reset_at == 10 {
                &[0x56, 0x78]
            } else {
                &[0, 0]
            }
        );
        assert!(!s.main_ram_ready());
        s.write_ram8(0x1000, 0xAB);
        advance(&mut s, 200, 17);
        assert_eq!(s.read_ram8(0x1000), 0xAB);
    }
}
#[test]
fn real_68k_fetch_byte_word_and_split_long_cycles_complete_only_on_allocated_boundaries() {
    let mut previous = None;
    for chunk in [1, 3, 17, 10000] {
        let mut s = sound();
        s.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 2, 0]);
        let words = [
            0x13FCu16, 0xAB, 0, 0x1001, // MOVE.B #AB,$1001
            0x33FC, 0x1234, 0, 0x1002, // MOVE.W #1234,$1002
            0x23FC, 0x5678, 0x9ABC, 0, 0x1004, // MOVE.L #56789ABC,$1004
            0x2039, 0, 0x1004, // MOVE.L $1004,D0
            0x4E72, 0x2700, 0x4E71,
        ];
        for (i, w) in words.iter().enumerate() {
            s.ram[0x200 + i * 2..0x202 + i * 2].copy_from_slice(&w.to_be_bytes());
        }
        s.set_enabled(true).unwrap();
        advance(&mut s, 1000, chunk);
        assert!(s.cpu.state.stopped && !s.cpu.state.halted);
        assert_eq!(s.cpu.state.d[0], 0x56789ABC);
        assert_eq!(
            &s.ram[0x1000..0x1008],
            &[0, 0xAB, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC]
        );
        let grants = s
            .memory
            .trace
            .iter()
            .filter(|e| e.1 == Owner::Cpu && !e.2)
            .count();
        let completions = s
            .memory
            .trace
            .iter()
            .filter(|e| e.1 == Owner::Cpu && e.2)
            .count();
        assert_eq!(grants, completions);
        // Six reset reads + seventeen subsequent fetches + six data cycles.
        assert_eq!(completions, 29);
        let result = (s.ram.clone(), s.memory.trace.clone(), s.scsp.drain(10));
        if let Some(old) = &previous {
            assert_eq!(&result, old);
        }
        previous = Some(result);
    }
}

#[test]
fn scsp_reset_readmits_only_the_unfinished_half_of_a_still_selected_main_longword() {
    let mut s = sound();
    s.main_ram_request(Some(request(0x1000, 4, Some(0x12345678))));
    advance(&mut s, 10, 1);
    assert_eq!(&s.ram[0x1000..0x1004], &[0x12, 0x34, 0, 0]);
    s.reset_output_start = Some(12);
    s.reset_output_end = Some(30);
    advance(&mut s, 20, 1);
    assert!(!s.main_ram_ready());
    s.write_ram8(0x1000, 0xAB);
    advance(&mut s, 38, 1);
    assert_eq!(&s.ram[0x1000..0x1004], &[0xAB, 0x34, 0, 0]);
    advance(&mut s, 39, 1);
    assert_eq!(s.complete_main_ram(), Ok(0x12345678));
    advance(&mut s, 100, 17);
    assert_eq!(&s.ram[0x1000..0x1004], &[0xAB, 0x34, 0x56, 0x78]);
    assert!(!s.memory.pending());
}
