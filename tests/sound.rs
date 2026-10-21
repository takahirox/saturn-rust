//! Same original dual-architecture fixture runs in scripts/browser-smoke.mjs.
use saturn_rust::{
    Boot, Machine,
    bus::{BIOS_SIZE, Memory},
    sound::{SCSP, SOUND_RAM},
};
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/sound.hex").lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let mut a = usize::from_str_radix(parts.next().unwrap(), 16).unwrap();
        for word in parts {
            let v = u16::from_str_radix(word, 16).unwrap();
            bytes[a..a + 2].copy_from_slice(&v.to_be_bytes());
            a += 2;
        }
    }
    bytes
}
#[test]
fn master_uploads_starts_sound_cpu_and_observes_scsp_irq_guest_computation_and_stereo_pcm() {
    let rom = rom();
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::ExternalBios(&rom)).unwrap();
        for _ in 0..10 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.bus().read32(0x06007000).unwrap(), 1);
        assert_eq!(m.bus().read32(0x06007004).unwrap(), 0x600D);
        assert_eq!(m.bus().read32(SOUND_RAM + 0x4000).unwrap(), 0x1000F000);
        assert_eq!(m.bus().read32(SOUND_RAM + 0x4004).unwrap(), 0xE0002000);
        assert_eq!(m.sound_state().last_vector, Some(28));
        assert_eq!(m.sound_state().a[7], 0x7FF00);
        assert!(m.sound_state().stopped);
        assert!(!m.sound_state().halted);
        // With table8 bus costs the timer is pending when SR unmasks it:
        // 23 setup instructions, three in its handler, then one STOP.
        assert_eq!(m.sound_state().instructions, 27);
        assert_eq!(m.bus().read16(SCSP + 0x420).unwrap() & 0x40, 0);
        let audio = m.drain_audio(8192);
        let nonzero = audio.iter().position(|v| *v != 0).unwrap();
        assert_eq!(
            &audio[nonzero..nonzero + 16],
            &[
                256, 4096, -256, -4096, -512, -8192, 512, 8192, 256, 4096, -256, -4096, -512,
                -8192, 512, 8192
            ]
        );
        assert!(m.audio_generated() > 1000);
        assert_eq!(m.audio_dropped(), 0);
    }
}
#[test]
fn changing_guest_arithmetic_changes_emulated_samples_without_host_game_detection() {
    let mut rom = rom();
    // Change sound guest MOVEQ #16,D0 to #8; both CPU code and output must change.
    assert_eq!(&rom[0x2106..0x2108], &[0x70, 0x10]);
    rom[0x2107] = 8;
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    for _ in 0..10 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read32(SOUND_RAM + 0x4000).unwrap(), 0x0800F800);
    let audio = m.drain_audio(8192);
    let first = audio.iter().position(|v| *v != 0).unwrap();
    assert_eq!(&audio[first..first + 4], &[128, 2048, -128, -2048]);
}

#[test]
fn sound_guest_programs_half_sample_pitch_and_generates_interpolated_stereo() {
    let mut rom = rom();
    // Literal guest MOVE.W #0,$100010 programs OCT/FNS. Change its operand
    // to OCT=-1; the actual 68k executes the write. No host SCSP write is used.
    let instruction = [0x33, 0xFC, 0, 0, 0, 0x10, 0, 0x10];
    let offsets: Vec<_> = rom
        .windows(instruction.len())
        .enumerate()
        .filter_map(|(at, bytes)| (bytes == instruction).then_some(at))
        .collect();
    assert_eq!(offsets.len(), 1);
    rom[offsets[0] + 2..offsets[0] + 4].copy_from_slice(&0x7800u16.to_be_bytes());
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    for _ in 0..10 {
        m.run_steps(10000).unwrap();
    }
    assert_eq!(m.bus().read16(SCSP + 0x10).unwrap(), 0x7800);
    assert_eq!(m.bus().read32(SOUND_RAM + 0x4000).unwrap(), 0x1000F000);
    let audio = m.drain_audio(8192);
    let first = audio.iter().position(|v| *v != 0).unwrap();
    assert_eq!(
        &audio[first..first + 16],
        &[
            256, 4096, 0, 0, -256, -4096, -384, -6144, -512, -8192, 0, 0, 512, 8192, 384, 6144
        ]
    );
}
