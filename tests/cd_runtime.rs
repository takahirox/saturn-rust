//! Original application reader; ST-162 §8.2.1/8.2.8 and ST-040 §3.2.
use saturn_rust::{Boot, BootMode, Machine, bus::Memory, media::Media};
const O: u32 = 0x06070000;
const IMAGES: [&[u8]; 2] = [
    include_bytes!("fixtures/cd-runtime.iso"),
    include_bytes!("fixtures/cd-runtime-moved.iso"),
];
fn firmware() -> Vec<u8> {
    let mut rom = vec![0; 524288];
    for line in include_str!("fixtures/external-startup.hex")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            rom[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    rom
}
fn word(m: &Machine, at: u32) -> u32 {
    m.bus().read32(at).unwrap()
}
fn bytes(m: &Machine, at: u32, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| m.bus().read8(at + i as u32).unwrap())
        .collect()
}
fn finish(m: &mut Machine) {
    for _ in 0..600 {
        m.run_steps(10000).unwrap();
        if word(m, O + 0xFC) != 0 || word(m, 0x060100FC) != 0 {
            return;
        }
    }
    panic!(
        "deadline pc {:08X}, phase {}",
        m.cpu_state().pc,
        word(m, O + 4)
    );
}
fn start(image: &[u8], rom: &[u8]) -> Machine {
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(rom)).unwrap();
    m.mount_media(Media::iso(image).unwrap());
    m
}
fn assets(layout: usize) -> (Vec<u8>, Vec<u8>) {
    // Independent literal formulas and lengths, not the ISO or a core helper.
    let (a, b) = if layout == 0 {
        (5003, 2051)
    } else {
        (4096, 6145)
    };
    (
        (0..a)
            .map(|i| ((i * 17 + 3 + (i >> 8)) & 255) as u8)
            .collect(),
        (0..b)
            .map(|i| (((i * 29 + 7) ^ (i >> 4)) & 255) as u8)
            .collect(),
    )
}
fn success(m: &Machine, layout: usize, alpha: &[u8], beta: &[u8]) {
    assert_eq!(word(m, 0x060100FC), 0, "no firmware failure");
    assert_eq!(
        word(m, O + 0xFC),
        0x600D,
        "pc {:08X}, phase {}, trace end {:08X}, host words {}, CD words {}",
        m.cpu_state().pc,
        word(m, O + 4),
        word(m, O + 0x28),
        word(m, O + 0xC08),
        word(m, O + 0xC0C)
    );
    assert_eq!(word(m, O), 0x06020004, "actual loaded program entry");
    assert_eq!(word(m, 0x060102F0), 0x06002E24, "actual on-disc AIP");
    assert_eq!(
        word(m, 0x06010328),
        include_bytes!("fixtures/cd-runtime.bin").len() as u32
    );
    assert_eq!(
        bytes(
            m,
            0x06020000,
            include_bytes!("fixtures/cd-runtime.bin").len()
        ),
        include_bytes!("fixtures/cd-runtime.bin")
    );
    assert_eq!(m.mode(), Some(BootMode::ExternalBios));
    assert_eq!(word(m, 0x06000320), 0, "no installed HLE service pointer");
    assert_eq!(word(m, O + 4), 3);
    assert_eq!(word(m, O + 0xC), u32::MAX);
    assert_eq!(word(m, O + 0x10), if layout == 0 { 3 } else { 4 });
    assert_eq!(
        (word(m, O + 0x18), word(m, O + 0x1C)),
        (2, 3),
        "host versus CD-side words: inherited-main fails here"
    );
    assert_eq!(
        (word(m, O + 0x20), word(m, O + 0x24)),
        (alpha.len() as u32, beta.len() as u32)
    );
    assert_eq!(
        (word(m, O + 0x60), word(m, O + 0x64), word(m, O + 0x68)),
        (u32::MAX, 0, u32::MAX)
    );
    let counts = [
        3,
        2050,
        2057.min(alpha.len() - 2046),
        7,
        alpha.len(),
        13,
        beta.len(),
    ];
    for (i, count) in counts.iter().enumerate() {
        assert_eq!(word(m, O + 0x40 + i as u32 * 4), *count as u32);
    }
    let mut expected = vec![0xA5; 65536];
    for (slot, data) in [
        (0, &alpha[..3]),
        (1, &beta[..2050]),
        (2, &alpha[2046..2046 + counts[2]]),
        (3, &beta[1..8]),
        (4, alpha),
        (5, &alpha[..13]),
        (6, beta),
    ] {
        let offset = 4 + slot * 0x2000;
        expected[offset..offset + data.len()].copy_from_slice(data);
    }
    assert_eq!(
        bytes(m, 0x06080000, 65536),
        expected,
        "all partial/full bytes, tails and every neighboring canary"
    );
    let sum = alpha.iter().chain(beta).map(|v| u32::from(*v)).sum::<u32>();
    assert_eq!(word(m, O + 8), sum, "SH-2 byte arithmetic");
    assert_eq!(
        m.bus().read16(0x25E00000).unwrap(),
        0x8000 | (sum as u16 & 0x7FFF)
    );
    let handles = if layout == 0 {
        [
            (2, 64, alpha.len(), 13, 1),
            (3, 72, beta.len(), beta.len(), 2),
        ]
    } else {
        [
            (3, 80, alpha.len(), 13, 1),
            (2, 96, beta.len(), beta.len(), 2),
        ]
    };
    for (i, (fid, extent, length, cursor, selector)) in handles.into_iter().enumerate() {
        let at = O + 0x800 + i as u32 * 0x20;
        assert_eq!(
            (0..6).map(|j| word(m, at + j * 4)).collect::<Vec<_>>(),
            [fid, extent, length as u32, cursor as u32, 1, selector]
        );
    }
    assert_eq!(m.debug_read32(0x25818000).unwrap(), u32::MAX);
    assert_eq!(m.debug_read16(0x25890008).unwrap() & 0x82, 0x80);
    assert_eq!(
        word(m, 0x25FE0000),
        0x05818000,
        "actual fixed-source DMA transactions"
    );
    assert_eq!(
        word(m, 0x25FE0004),
        0x06060000 + if layout == 0 { 0 } else { 2048 }
    );
    assert_eq!(
        word(m, 0x06010204),
        16 + layout as u32,
        "one interrupt per completed sector DMA"
    );
    assert_eq!(
        word(m, 0x06010208),
        1,
        "independent CD A-bus handler, acknowledged once"
    );
    assert_eq!(
        word(m, 0x25FE00A4) & 0x10800,
        0,
        "DMA0 and A-bus acknowledgment"
    );
    let end = word(m, O + 0x28) & 0x1FFF_FFFF;
    assert!((0x06072000..0x06074000).contains(&end));
    let trace: Vec<Vec<u32>> = (0x06072000..end)
        .step_by(32)
        .map(|at| (0..8).map(|i| word(m, at + i * 4)).collect())
        .collect();
    let expected = [
        [0, 0, 0, 0, 0x0180, 0x4101, 0x0100, 191],
        [0x0100, 0, 0, 0, 0x0180, 2, 0, 0],
        [0x0400, 0xFFFF, 0, 0x0201, 0x0180, 0x4101, 0x0100, 191],
        [0x0480, 0xFFFF, 0, 0xFFFF, 0x0180, 0x4101, 0x0100, 191],
        [0x0400, 0xFFFF, 0, 0x0600, 0xFF00, 0x4101, 0x0100, 191],
    ];
    assert_eq!(trace[..5], expected);
    assert!(
        trace
            .iter()
            .any(|r| r[..4] == [0x7400, 0, 0x1700, 0xFFFF] && r[4] == 0xFF00)
    );
    assert!(trace.iter().any(|r| r[0] == 0x7500 && r[4] & 0x8000 == 0));
    assert!(
        trace
            .windows(2)
            .any(|r| r[0][..4] == [0x6300, 0, 0x100, 1] && r[1][0] == 0x600 && r[1][5] == 3)
    );
    assert!(trace.iter().all(|r| r[4] != 0x8180), "no swallowed WAIT");
}
#[test]
fn same_application_reads_all_bytes_and_independent_handles_in_two_layouts_and_cold_restarts() {
    let rom = firmware();
    for (layout, image) in IMAGES.into_iter().enumerate() {
        let (alpha, beta) = assets(layout);
        let mut m = start(image, &rom);
        for _ in 0..2 {
            finish(&mut m);
            success(&m, layout, &alpha, &beta);
            m.advance_frame().unwrap();
            m.advance_frame().unwrap();
            let sum = word(&m, O + 8);
            let channel = |shift: u32| {
                let v = ((sum >> shift) & 31) as u8;
                (v << 3) | (v >> 2)
            };
            let pixel = [channel(0), channel(5), channel(10), 255];
            assert_eq!(m.framebuffer().unwrap(), pixel.repeat(320 * 224));
            m.reset(Boot::ExternalBios(&rom)).unwrap();
        }
    }
}
#[test]
fn runtime_chunks_and_passive_fifo_and_response_inspection_are_identical() {
    let rom = firmware();
    for (layout, image) in IMAGES.into_iter().enumerate() {
        let mut reference = None;
        for chunk in [1, 3, 17, 10000] {
            let mut m = start(image, &rom);
            let mut left = 2_600_000;
            while left != 0 {
                let n = left.min(chunk);
                m.run_steps(n).unwrap();
                left -= n;
                if chunk != 10000 {
                    m.debug_read32(0x25818000).unwrap();
                    m.debug_read16(0x25890024).unwrap();
                }
            }
            let (a, b) = assets(layout);
            success(&m, layout, &a, &b);
            let snapshot = (
                m.clocks(),
                m.cpu_state().clone(),
                m.slave_state().clone(),
                bytes(&m, O, 0x4000),
                bytes(&m, 0x06080000, 65536),
            );
            if let Some(previous) = &reference {
                assert_eq!(&snapshot, previous);
            }
            reference = Some(snapshot);
        }
    }
}
#[test]
fn sysres_reexecutes_runtime_from_media_and_preserves_only_backup() {
    let mut rom = firmware();
    rom[0x7F00..0x7F04].copy_from_slice(&1u32.to_be_bytes());
    let mut m = start(IMAGES[1], &rom);
    m.import_backup_ram(&vec![0xA5; 32768]).unwrap();
    finish(&mut m);
    let (a, b) = assets(1);
    success(&m, 1, &a, &b);
    assert_eq!(m.backup_ram()[0], 0x5A);
    assert!(m.backup_ram()[1..].iter().all(|v| *v == 0xA5));
    assert!(m.clocks() > 26_000_000);
}
#[test]
fn reset_during_runtime_fifo_cancels_transfer_and_cold_boot_can_repeat() {
    let rom = firmware();
    let mut m = start(IMAGES[0], &rom);
    let mut reached = false;
    for _ in 0..1_600_000 {
        m.step().unwrap();
        if word(&m, O + 4) == 2 && m.debug_read16(0x25890008).unwrap() & 2 != 0 {
            reached = true;
            break;
        }
    }
    assert!(reached);
    assert_eq!(word(&m, O + 0xFC), 0);
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    assert_eq!(word(&m, O), 0);
    assert_eq!(m.debug_read32(0x25818000).unwrap(), u32::MAX);
    finish(&mut m);
    let (a, b) = assets(0);
    success(&m, 0, &a, &b);
    m.clear_media();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    finish(&mut m);
    assert_eq!(word(&m, 0x060100FC), 0xE001);
    assert_eq!(word(&m, O), 0);
}
#[test]
fn runtime_directory_name_and_medium_failures_never_publish_stale_file_bytes() {
    let rom = firmware();
    for control in 0..5 {
        let mut image = IMAGES[0].to_vec();
        match control {
            0 => image[48 * 2048 + 68 + 33] = b'X', // name missing, valid record
            1 => image[48 * 2048 + 68] = 33,        // malformed child, not boot metadata
            2 => image[48 * 2048 + 68..49 * 2048].fill(0), // missing runtime records
            3 => {
                let at = 48 * 2048 + 68 + 2;
                image[at..at + 4].copy_from_slice(&127u32.to_le_bytes());
                image[at + 4..at + 8].copy_from_slice(&127u32.to_be_bytes());
            } // child file extent exceeds actual medium
            4 => image[20 * 2048 + 114 + 33] = b'X', // DATA name missing in root
            _ => unreachable!(),
        }
        let mut m = start(&image, &rom);
        finish(&mut m);
        assert_eq!(
            word(&m, 0x060100FC),
            0,
            "firmware still loads original program, control{control}"
        );
        assert_ne!(word(&m, O), 0);
        assert_eq!(
            word(&m, O + 0xFC),
            if matches!(control, 0 | 2) {
                0xE090
            } else if control == 4 {
                0xE093
            } else {
                0xE092
            },
            "control{control}"
        );
        assert!(bytes(&m, 0x06080000, 65536).iter().all(|v| *v == 0xA5));
        assert_eq!(word(&m, O + 0x800 + 12), 0);
    }
    let medium = Media::iso(IMAGES[0]).unwrap();
    let mut raw = Vec::new();
    for lba in 0..128 {
        let mut sector = medium.raw_sector(lba + 150).unwrap().to_vec();
        if lba == 65 {
            sector[16..2064].fill(0xB7);
        }
        raw.extend(sector);
    }
    let mut m = Machine::new();
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    m.mount_media(
        Media::cue(
            "FILE \"d\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00",
            &[("d", &raw)],
        )
        .unwrap(),
    );
    finish(&mut m);
    assert_eq!(word(&m, O + 0xFC), 0xE092);
    assert_eq!(word(&m, O + 4), 2);
    assert_eq!(word(&m, 0x060100FC), 0);
    assert!(bytes(&m, 0x06080000, 65536).iter().all(|v| *v == 0xA5));
    assert_eq!(word(&m, O + 0x800 + 12), 0);
    // Recovery uses the selected original ROM on a fresh valid medium.
    m.mount_media(Media::iso(IMAGES[0]).unwrap());
    m.reset(Boot::ExternalBios(&rom)).unwrap();
    finish(&mut m);
    let (a, b) = assets(0);
    success(&m, 0, &a, &b);
    assert!(
        Media::iso(&IMAGES[0][..IMAGES[0].len() - 1]).is_err(),
        "truncated physical sector rejected at mount"
    );
}
#[test]
fn changing_actual_runtime_bytes_changes_guest_arithmetic_and_visible_color() {
    let mut image = IMAGES[0].to_vec();
    image[64 * 2048] ^= 0x1F;
    let mut m = start(&image, &firmware());
    finish(&mut m);
    let (mut a, b) = assets(0);
    a[0] ^= 0x1F;
    success(&m, 0, &a, &b);
    let (original_a, original_b) = assets(0);
    assert_ne!(
        word(&m, O + 8),
        original_a
            .iter()
            .chain(&original_b)
            .map(|v| u32::from(*v))
            .sum::<u32>()
    );
}
