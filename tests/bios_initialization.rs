//! Narrow initialization endpoints and the original reset-vector startup guest.
use saturn_rust::{
    Boot, Machine,
    bus::{Bus, Memory},
    devices::VDP2,
    sound::Scsp,
};

#[test]
fn vdp2_initialization_words_preserve_counters_latches_and_live_neighbors() {
    let mut b = Bus::default();
    b.write16(VDP2 + 2, 0x200).unwrap();
    for (at, value) in [(0xe, 0x123), (0xfc, 3), (0x100, 0x17)] {
        b.write16(VDP2 + at, value).unwrap();
    }
    let before = b.read32(VDP2 + 8).unwrap();
    for value in [0, 0x7fff, 0xffff] {
        for at in [4, 8, 0xa, 0xc, 0xfe] {
            b.write16(VDP2 + at, value).unwrap();
        }
        assert_eq!(b.read32(VDP2 + 8), Ok(before));
        assert_eq!(b.read16(VDP2 + 4).unwrap() & 0x200, 0);
        assert_eq!(b.read16(VDP2 + 0xc), Ok(0));
        assert_eq!(b.read16(VDP2 + 0xfe), Ok(0));
    }
    b.write16(VDP2 + 6, 0x7fff).unwrap();
    assert_eq!(b.read16(VDP2 + 6), Ok(0));
    assert!(b.write16(VDP2 + 6, 0x8000).is_err());
    assert!(b.write16(VDP2 + 7, 0).is_err());
    assert!(b.write16(VDP2 + 0x120, 0).is_err());
    for (at, value) in [(0xe, 0x123), (0xfc, 3), (0x100, 0x17)] {
        assert_eq!(b.read16(VDP2 + at), Ok(value));
    }
}

#[test]
fn scsp_unused_words_do_not_modify_neighboring_live_words() {
    let mut s = Scsp::default();
    for slot in 0..32 {
        s.write16(slot * 32 + 0x14, 0x53).unwrap();
        s.write16(slot * 32 + 0x16, 0x1200).unwrap();
        for word in [0x18, 0x1a, 0x1c, 0x1e] {
            for value in [0, 0xffff, 0x1357] {
                s.write16(slot * 32 + word, value).unwrap();
                assert_eq!(s.read16(slot * 32 + word), Ok(0));
            }
        }
        assert_eq!(s.read16(slot * 32 + 0x14), Ok(0x53));
        assert_eq!(s.read16(slot * 32 + 0x16), Ok(0x1200));
    }
    for at in [0x77e, 0x780, 0x7be, 0x800] {
        s.write16(at, 0x1230).unwrap();
    }
    for at in (0x7c0..=0x7fe).step_by(2) {
        s.write16(at, 0xffff).unwrap();
        assert_eq!(s.read16(at), Ok(0));
    }
    for at in [0x77e, 0x780, 0x7be, 0x800] {
        assert_eq!(s.read16(at), Ok(0x1230));
    }
    assert!(s.write16(0x7c1, 0).is_err());
    assert!(s.read16(0x430).is_err());
    assert!(s.write16(0x430, 0).is_err());
}

fn firmware() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/bios-initialization.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, w) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(w, 16).unwrap().to_be_bytes());
        }
    }
    rom
}

#[test]
fn original_bios_initialization_executes_real_devices_then_settles_without_disc() {
    let expected: Vec<_> = include_str!("fixtures/bios-initialization.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for chunk in [1, 127, 10000] {
        let mut m = Machine::new();
        let rom = firmware();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&rom)).unwrap();
            for start in (0..400000).step_by(chunk) {
                m.run_steps((400000 - start).min(chunk) as u32).unwrap();
                let _ = m.debug_read16(0x05890024).unwrap();
                let _ = m.bus().smpc_command_state();
            }
            let got: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x06020000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(
                got,
                expected,
                "PC={:08x}, clocks={}",
                m.cpu_state().pc,
                m.clocks()
            );
            assert_eq!(m.cpu_state().exceptions, 0);
            assert!(m.bus().cd.media().is_none());
            assert!(
                m.framebuffer()
                    .unwrap()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == [165, 16, 0, 255])
            );
        }
    }
}

#[test]
fn changed_dsp_operand_omitted_sound_start_and_pending_reset_are_guest_sensitive() {
    let expected: Vec<_> = include_str!("fixtures/bios-initialization.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    for omit_sound in [false, true] {
        let mut rom = firmware();
        if omit_sound {
            rom[0x104] = 7;
        } else {
            rom[0x103] = 9;
        }
        let mut m = Machine::new();
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..40 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(
            m.bus().read32(0x06020000 + 24 * 4),
            Ok(if omit_sound { 84 } else { 108 })
        );
        if omit_sound {
            assert_eq!(m.bus().read32(0x06020ffc), Ok(0));
            assert_eq!(m.bus().read32(0x05a00804), Ok(0));
        } else {
            assert_eq!(m.bus().read32(0x06020ffc), Ok(0x600d));
            let mut changed = expected.clone();
            changed[24] = 108;
            assert_eq!(
                (0..changed.len())
                    .map(|i| m.bus().read32(0x06020000 + i as u32 * 4).unwrap())
                    .collect::<Vec<_>>(),
                changed
            );
        }
    }
    let rom = firmware();
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    for _ in 0..400000 {
        m.step().unwrap();
        if m.bus().smpc_command_state().deferred_control.is_some() {
            break;
        }
    }
    assert_eq!(m.bus().smpc_command_state().deferred_control, Some(0x1a));
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    assert_eq!(m.bus().smpc_command_state().deferred_control, None);
    for _ in 0..40 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(0x06020ffc), Ok(0x600d));
    assert_eq!(m.cpu_state().exceptions, 0);
}
