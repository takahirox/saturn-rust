//! Original mixed-disc guest: all records and completion come from SH-2/MMIO.
use saturn_rust::{Machine, bus::Memory, media::Media};

#[test]
fn timer_triggered_cd_to_video_waits_for_finite_guest_writes_and_executes_end_handler() {
    let original = include_bytes!("fixtures/dma-startup.iso");
    let mut machine = Machine::new();
    for (source, cpu, trigger) in [
        (0x22223333u32, 0x11111111u32, true),
        (0x33334444, 0x44444444, true),
        (0x22223333, 0x11111111, false),
    ] {
        for _ in 0..2 {
            let mut iso = original.to_vec();
            iso[60 * 2048..60 * 2048 + 4].copy_from_slice(&source.to_be_bytes());
            iso[21 * 2048 + 0x1800..21 * 2048 + 0x1804].copy_from_slice(&cpu.to_be_bytes());
            iso[21 * 2048 + 0x1804..21 * 2048 + 0x1808]
                .copy_from_slice(&u32::from(trigger).to_be_bytes());
            let audio = include_bytes!("fixtures/dma-startup-audio.bin");
            let media = Media::cue(
                include_str!("fixtures/dma-startup.cue"),
                &[("dma-startup.iso", &iso), ("dma-startup-audio.bin", audio)],
            )
            .unwrap();
            machine.reset_media(media).unwrap();
            for _ in 0..300 {
                machine.run_steps(10000).unwrap();
                if machine.bus().read32(0x06010014) == Ok(0x600d) {
                    break;
                }
            }
            let r: Vec<_> = (0..6)
                .map(|i| machine.bus().read32(0x06010000 + i * 4).unwrap())
                .collect();
            assert_eq!(
                r[5],
                0x600d,
                "pc={:08x}, records={r:x?}",
                machine.cpu_state().pc
            );
            let first = if trigger { source } else { cpu };
            let second = if trigger { 0x33332222 } else { 0 };
            assert_eq!(
                r,
                [
                    u32::from(trigger),
                    first,
                    second,
                    first.wrapping_add(second),
                    if trigger { 4 } else { 0 },
                    0x600d
                ]
            );
            assert_eq!(machine.cpu_state().exceptions, u64::from(trigger));
            machine.advance_frame().unwrap();
            machine.advance_frame().unwrap();
            let frame = machine.framebuffer().unwrap();
            for x in 0..8 {
                let index = (first >> ((7 - x) * 4)) & 15;
                let color = match index {
                    1 => [255, 0, 0, 255],
                    2 => [0, 255, 0, 255],
                    3 => [0, 0, 255, 255],
                    4 => [255, 255, 255, 255],
                    _ => panic!("palette {index}"),
                };
                assert_eq!(&frame[x * 4..x * 4 + 4], color);
            }
        }
    }
}
