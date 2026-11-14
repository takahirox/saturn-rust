//! Original active graphics/audio differential on the actual board.
use super::*;

pub fn verify() -> Vec<u32> {
    let mut count = 0;
    let mut hash = 2166136261u32;
    for dual in [false, true] {
        for control_writes in [false, true] {
            let mut e = Experiment::new(true, dual, false);
            for bus in [&mut e.board, &mut e.reference.bus] {
                for (offset, value) in [(0, 0x8000), (0x20, 0x101), (0x28, 0x32), (0xF8, 3)] {
                    bus.vdp2
                        .write16(crate::devices::VDP2 + offset, value)
                        .unwrap();
                }
                for (i, bytes) in bus.vdp2.vram[..0x40000]
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .enumerate()
                {
                    bytes.copy_from_slice(&[0x801Fu16, 0x83E0, 0xFC00][i % 3].to_be_bytes());
                }
                for (command, words) in [
                    (
                        0usize,
                        [9u16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 511, 255, 0, 0, 0, 0],
                    ),
                    (
                        32,
                        [
                            0x1004, 4, 0xC0, 0x801F, 0, 0, 0, 0, 511, 0, 511, 255, 0, 255, 0, 0,
                        ],
                    ),
                ] {
                    for (i, word) in words.into_iter().enumerate() {
                        bus.vdp1.vram[command + i * 2..command + i * 2 + 2]
                            .copy_from_slice(&word.to_be_bytes());
                    }
                }
                bus.vdp1.write16(crate::vdp1::REGISTERS + 2, 2).unwrap();
                bus.vdp1.write16(crate::vdp1::REGISTERS + 4, 1).unwrap();
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
                    bus.sound.scsp.write16(offset, value).unwrap();
                }
                bus.sound.ram[..8].copy_from_slice(&[0, 7, 0xFF, 0, 0, 0, 1, 0]);
                if control_writes {
                    bus.sound.scsp.write16(0x42A, 0x20).unwrap();
                    bus.scu.write32(crate::scu::SCU + 0xA0, !(1 << 6)).unwrap();
                    for (i, word) in [
                        0x33FCu16, 0x20, 0x10, 0x42C, 0x33FC, 0x20, 0x10, 0x42E, 0x60EE,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        bus.sound.ram[0x100 + i * 2..0x102 + i * 2]
                            .copy_from_slice(&word.to_be_bytes());
                    }
                } else {
                    bus.sound.ram[0x100..0x104].copy_from_slice(&[0x4E, 0x71, 0x60, 0xFC]);
                }
                bus.sound.ram[0x1000..0x1008]
                    .copy_from_slice(&[0x10, 0, 0xF0, 0, 0x20, 0, 0xE0, 0]);
                bus.sound.set_enabled(true).unwrap();
            }
            for _ in 0..2 {
                e.run_reference(1);
                e.run_candidate_until(e.reference_clock());
                let pcm = e.board.sound.scsp.drain(16384);
                assert!(pcm.iter().any(|&value| value != 0));
                assert_eq!(pcm, e.reference.drain_audio(16384));
                let pixels = e.board.vdp2.scanout(&e.board.vdp1).unwrap();
                assert!(
                    pixels
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|pixel| pixel[..3] != [0, 0, 0])
                );
                assert_eq!(pixels, e.reference.framebuffer().unwrap());
                assert_eq!(
                    e.board.vdp1.draw_buffer(),
                    e.reference.bus.vdp1.draw_buffer()
                );
                assert!(e.board.vdp1.busy());
                assert_eq!(&e.board.sound.cpu.state, e.reference.sound_state());
                assert_eq!(
                    e.board.sound.scsp.read16(0x42C),
                    e.reference.bus.sound.scsp.read16(0x42C)
                );
                assert_eq!(
                    e.board.recognized_interrupt(Processor::Master),
                    e.reference.bus.recognized_interrupt(Processor::Master)
                );
                for value in pcm
                    .into_iter()
                    .map(|v| u32::from(v as u16))
                    .chain(
                        pixels
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|bytes| u32::from_be_bytes(*bytes)),
                    )
                    .chain(e.digest())
                {
                    hash = (hash ^ value).wrapping_mul(16777619);
                }
                count += 1;
                e.verify();
            }
        }
    }

    vec![count, hash]
}
