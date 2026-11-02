//! Original MIT streaming guest. ST-162 §§3.2/3.4/4.2/8.2.7.
use saturn_rust::{
    Boot, BootMode, Machine,
    bus::{BIOS_SIZE, Memory},
    media::Media,
};
const O: u32 = 0x06070000;
const IMAGES: [&[u8]; 2] = [
    include_bytes!("fixtures/cd-stream.iso"),
    include_bytes!("fixtures/cd-stream-moved.iso"),
];
fn firmware() -> Vec<u8> {
    let mut rom = vec![0; BIOS_SIZE];
    for line in include_str!("fixtures/external-startup.hex")
        .lines()
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
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
fn word(m: &Machine, at: u32) -> u32 {
    m.bus().read32(at).unwrap()
}
fn bytes(m: &Machine, at: u32, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| m.bus().read8(at + i as u32).unwrap())
        .collect()
}
fn start(image: &[u8]) -> Machine {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&firmware())).unwrap();
    m.mount_media(Media::iso(image).unwrap());
    m
}
fn finish(m: &mut Machine) {
    for _ in 0..800 {
        m.run_steps(10000).unwrap();
        if word(m, O + 0xfc) != 0 || word(m, 0x060100fc) != 0 {
            return;
        }
    }
    panic!(
        "deadline pc {:08x} phase {}",
        m.cpu_state().pc,
        word(m, O + 4)
    );
}
fn payload(layout: usize) -> Vec<u8> {
    let (n, seed) = if layout == 0 {
        (12291, 11)
    } else {
        (16387, 73)
    };
    (0..n)
        .map(|i| ((i * 31 + seed) ^ (i >> 7) ^ (i / 2048 * 19)) as u8)
        .collect()
}
fn records(m: &Machine) -> Vec<Vec<u32>> {
    let end = word(m, O + 0x24) & 0x1fffffff;
    assert!((0x06071000..0x06072000).contains(&end));
    (0x06071000..end)
        .step_by(64)
        .map(|at| (0..16).map(|i| word(m, at + i * 4)).collect())
        .collect()
}
fn success(m: &Machine, layout: usize, data: &[u8]) {
    assert_eq!(word(m, 0x060100fc), 0, "firmware error");
    if word(m, O + 0xfc) != 0x600d {
        let end = word(m, O + 0x2c) & 0x1fffffff;
        eprintln!(
            "commands {:x?} fields {:x?}",
            (0x06074000..end.min(0x06075000))
                .step_by(4)
                .map(|at| word(m, at))
                .collect::<Vec<_>>(),
            (0..22).map(|i| word(m, O + i * 4)).collect::<Vec<_>>()
        );
    }
    assert_eq!(
        word(m, O + 0xfc),
        0x600d,
        "pc {:08x} phase {} BFUL sample {:04x} trace {:08x}",
        m.cpu_state().pc,
        word(m, O + 4),
        word(m, O + 0x20),
        word(m, O + 0x2c)
    );
    assert_eq!(m.mode(), Some(BootMode::ExternalBios));
    assert_eq!(word(m, 0x06000320), 0);
    assert_eq!(word(m, O), 0x06020004);
    assert_eq!(word(m, 0x060102f0), 0x06002e24);
    let code = include_bytes!("fixtures/cd-stream.bin");
    assert_eq!(bytes(m, 0x06020000, code.len()), code);
    assert_eq!(word(m, O + 0x10), if layout == 0 { 64 } else { 88 });
    assert_eq!(word(m, O + 0x14), data.len() as u32);
    let n = data.len().div_ceil(2048);
    assert_eq!(
        word(m, O + 0x48),
        16 * data[..(n - 2) * 2048]
            .iter()
            .map(|v| u32::from(*v))
            .sum::<u32>(),
        "cached DMA kernel computes from prior transferred bytes"
    );
    assert_eq!(word(m, O + 0x18), n as u32);
    assert_eq!(word(m, O + 0x20) & 12, 12, "second sector fills real pool");
    let sum = data.iter().map(|v| u32::from(*v)).sum::<u32>();
    assert_eq!(word(m, O + 8), sum);
    assert_eq!(word(m, O + 12), sum);
    let mut expected = vec![0xa5; 65536];
    for at in [4, 0x8004] {
        expected[at..at + data.len()].copy_from_slice(data);
        expected[at + data.len()] = 0;
    }
    assert_eq!(
        bytes(m, 0x06080000, 65536),
        expected,
        "all payload bytes, final FIFO word and canaries"
    );
    let trace = records(m);
    assert_eq!(trace.len(), n * 2);
    for (pass, group) in trace.chunks(n).enumerate() {
        for (i, r) in group.iter().enumerate() {
            assert_eq!(r[0], i as u32);
            assert!(
                (6..100).contains(&r[9]),
                "cached arithmetic FRT ticks while DMA owns bus: pass {pass} sector {i}, {}",
                r[9]
            );
            assert_eq!(
                r[2],
                (if layout == 0 { 214 } else { 238 }) + i as u32,
                "actual sector FAD identity"
            );
            assert!(r[3] >= 1 && r[3] <= if pass == 0 { n as u32 } else { 2 });
            assert_ne!(r[5], r[7], "FRT while CPU computes and DMA advances");
            let prior = data[..i * 2048].iter().map(|v| u32::from(*v)).sum::<u32>();
            assert_eq!(r[8], prior);
            let length = if i + 1 == n { 4 } else { 2048 };
            assert_eq!(r[10], if length == 4 { 3 } else { 1024 });
            assert_eq!(
                r[11],
                0x06080004 + pass as u32 * 0x8000 + i as u32 * 2048 + length
            );
            assert_eq!(r[12] & 0x82, 0x80);
            assert_eq!(r[13], r[4] + 1);
            assert_eq!(r[14], r[11] - length);
            assert_eq!(r[15], length);
        }
    }
    assert!(
        trace[..n - 1]
            .iter()
            .skip(1)
            .any(|r| r[8] != 0 && r[1] & 16 == 0),
        "CPU computed and changed video before PEND"
    );
    assert_ne!(
        word(m, O + 0x50) & 512,
        0,
        "accepted directory abort completed"
    );
    assert_eq!(
        word(m, 0x06010204),
        13 + 2 * n as u32,
        "one real interrupt per completed DMA, including unchanged firmware"
    );
    assert_eq!(
        word(m, 0x06010208),
        1,
        "firmware CD A-bus interrupt is acknowledged once"
    );
    assert_eq!(
        word(m, 0x25fe00a4) & 0x10800,
        0,
        "no duplicated pending DMA/CD acknowledgment"
    );
    assert_eq!(
        word(m, 0x25fe0000),
        0x05818000,
        "fixed FIFO source never advances"
    );
    let end = word(m, O + 0x2c) & 0x1fffffff;
    let commands: Vec<_> = (0x06074000..end)
        .step_by(32)
        .map(|at| (0..8).map(|i| word(m, at + i * 4)).collect::<Vec<_>>())
        .collect();
    assert!(
        commands
            .iter()
            .any(|r| r[..4] == [0x7400, 0, 0x100, 0xffff] && r[4] == 0xff00)
    );
    assert!(commands.iter().all(|r| r[4] != 0xff00 || r[0] == 0x7400));
    assert!(
        commands
            .iter()
            .all(|r| r[4] & 0x8000 == 0 || r[0] == 0x7400),
        "no swallowed WAIT"
    );
    assert_eq!(m.debug_read32(0x25818000).unwrap(), u32::MAX);
    assert_eq!(
        m.bus().read16(0x25e00000).unwrap(),
        0x8000 | (sum as u16 & 0x7fff)
    );
}
#[test]
fn firmware_guest_streams_base_layout_with_dma_computation_video_and_pool_backpressure() {
    let mut m = start(IMAGES[0]);
    finish(&mut m);
    success(&m, 0, &payload(0));
    rendered(&mut m, &payload(0));
}
#[test]
fn firmware_guest_streams_moved_layout_with_dma_computation_video_and_pool_backpressure() {
    let mut m = start(IMAGES[1]);
    finish(&mut m);
    success(&m, 1, &payload(1));
    rendered(&mut m, &payload(1));
}

fn pixel(sum: u32) -> [u8; 4] {
    let c = |shift: u32| {
        let n = ((sum >> shift) & 31) as u8;
        (n << 3) | (n >> 2)
    };
    [c(0), c(5), c(10), 255]
}
fn rendered(m: &mut Machine, data: &[u8]) {
    m.advance_frame().unwrap();
    m.advance_frame().unwrap();
    assert_eq!(
        m.framebuffer().unwrap(),
        pixel(data.iter().map(|v| u32::from(*v)).sum()).repeat(320 * 224)
    );
}
#[test]
fn changed_disc_bytes_change_all_guest_computation_and_pixels_and_cold_reset_reuses_medium() {
    let mut image = IMAGES[0].to_vec();
    image[64 * 2048] ^= 0x1f;
    let mut data = payload(0);
    data[0] ^= 0x1f;
    let mut m = start(&image);
    for _ in 0..2 {
        finish(&mut m);
        success(&m, 0, &data);
        rendered(&mut m, &data);
        m.reset(Boot::ExternalBios(&firmware())).unwrap();
    }
}
#[test]
fn reset_during_slow_fifo_cancels_dma_and_guest_sysres_reboots_retained_medium() {
    let mut m = start(IMAGES[0]);
    let mut reached = false;
    for _ in 0..3000000 {
        m.run_steps(1).unwrap();
        if word(&m, O + 4) == 3
            && word(&m, O + 0x20) & 12 == 12
            && m.debug_read16(0x25890008).unwrap() & 2 != 0
            && m.debug_read32(0x25818000).unwrap() != word(&m, 0x06080004)
            && word(&m, 0x06010204) < word(&m, O + 0x40)
        {
            reached = true;
            break;
        }
    }
    assert!(reached, "slow FIFO entered");
    m.reset(Boot::ExternalBios(&firmware())).unwrap();
    assert_eq!(word(&m, O), 0);
    assert_eq!(m.debug_read32(0x25818000).unwrap(), u32::MAX);
    finish(&mut m);
    success(&m, 0, &payload(0));
    let mut rom = firmware();
    rom[0x7f00..0x7f04].copy_from_slice(&1u32.to_be_bytes());
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    finish(&mut m);
    success(&m, 0, &payload(0));
    assert_eq!(m.bus().read8(0x20180001).unwrap(), 0x5a);
    rendered(&mut m, &payload(0));
    let mut empty = Machine::new();
    empty.reset(Boot::ExternalBios(&firmware())).unwrap();
    finish(&mut empty);
    assert_eq!(word(&empty, O), 0);
    assert_ne!(word(&empty, 0x060100fc), 0);
    assert_eq!(empty.debug_read32(0x25818000).unwrap(), u32::MAX);
}
#[test]
fn fixed_guest_snapshot_is_invariant_to_step_chunks_and_passive_inspection() {
    let mut expected = None;
    for chunk in [1, 3, 17, 10000] {
        let mut m = start(IMAGES[0]);
        let mut left = 2_000_000;
        while left != 0 {
            let n = left.min(chunk);
            m.run_steps(n).unwrap();
            left -= n;
            if chunk != 10000 {
                m.debug_read32(0x25818000).unwrap();
                m.debug_read16(0x25890024).unwrap();
            }
        }
        success(&m, 0, &payload(0));
        let snapshot = (
            m.clocks(),
            m.cpu_state().clone(),
            m.slave_state().clone(),
            bytes(&m, O, 0x8000),
            bytes(&m, 0x06080000, 65536),
            m.framebuffer().unwrap(),
        );
        if let Some(previous) = expected {
            assert_eq!(snapshot, previous);
        }
        expected = Some(snapshot);
    }
}

#[test]
fn guest_computation_changes_rendered_pixels_before_stream_end() {
    let data = payload(0);
    let mut m = start(IMAGES[0]);
    let mut reached = false;
    for _ in 0..3000 {
        m.run_steps(1000).unwrap();
        if word(&m, 0x06071060) != 0 {
            reached = true;
            break;
        }
    }
    assert!(reached);
    let before = m.cpu_state().instructions;
    let frame = m.frames();
    m.advance_frame().unwrap();
    m.advance_frame().unwrap();
    assert!(m.cpu_state().instructions > before);
    assert!(m.frames() > frame);
    assert_eq!(word(&m, O + 4), 2);
    assert_eq!(
        m.debug_read16(0x25890008).unwrap() & 16,
        0,
        "later sectors still pending"
    );
    let expected: Vec<_> = (1..7)
        .flat_map(|i| [i * 2048, (i - 1) * 2048 + 16])
        .map(|end| pixel(data[..end].iter().map(|v| u32::from(*v)).sum()))
        .collect();
    assert!(
        m.framebuffer()
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| expected.contains(p)),
        "actual guest-derived raster pixels before PEND"
    );
    finish(&mut m);
    success(&m, 0, &data);
}
