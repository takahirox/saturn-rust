//! Original licensed Saturn disc calls the system ABI through guest instructions.
//! ST-162-R1 System Program §2.2; ST-210 #7 forbids earlier-library IST writes.
use saturn_rust::{
    Boot, BootMode, Machine,
    bus::{BIOS_SIZE, Memory},
    cpu::CpuStatus,
};

const ISO: &[u8] = include_bytes!("fixtures/hle-system.iso");

fn expected() -> Vec<u32> {
    include_str!("fixtures/hle-system.tsv")
        .lines()
        .map(|line| u32::from_str_radix(line.split_once('\t').unwrap().1, 16).unwrap())
        .collect()
}

fn run(machine: &mut Machine, image: &[u8]) -> Vec<u32> {
    machine.reset(Boot::HleDisc(image)).unwrap();
    machine.run_steps(10000).unwrap();
    assert_eq!(machine.cpu_state().status, CpuStatus::Sleeping);
    assert_eq!(machine.cpu_state().exceptions, 3);
    assert_eq!(machine.slave_state().status, CpuStatus::Reset);
    expected()
        .iter()
        .enumerate()
        .map(|(i, _)| machine.bus().read32(0x0600_E000 + i as u32 * 4).unwrap())
        .collect()
}

#[test]
fn guest_calls_vector_mask_semaphore_priority_and_nested_callback_services() {
    let mut m = Machine::new();
    for _ in 0..2 {
        let actual = run(&mut m, ISO);
        for (i, (actual, expected)) in actual.iter().zip(expected()).enumerate() {
            assert_eq!(*actual, expected, "ABI record {i}");
        }
        for (base, words) in [
            (0x0600_E500, vec![0, 1, 1, 1, 0x06009000, 0x4B00, 0x600D]),
            (0x0600_E600, vec![0x20, 0xFBFF, 0, 0, 1]),
            (0x0600_E700, vec![0xF0, 0xFFFF, 0, 0, 1]),
        ] {
            for (i, expected) in words.into_iter().enumerate() {
                assert_eq!(m.bus().read32(base + i as u32 * 4), Ok(expected));
            }
        }
        // PRI source was overwritten in the guest after SYS_CHGUIPR copied it.
        assert_eq!(m.bus().read32(0x0600_F02C), Ok(0x00F0FFFF));
        assert_eq!(m.bus().read32(0x0600_0BAC), Ok(0x0020F800));
        for i in 0..256 {
            assert_eq!(m.bus().read8(0x0600_0A00 + i), Ok(0));
        }
    }
}

fn mutate_literal(image: &[u8], before: u32, after: u32) -> Vec<u8> {
    let mut image = image.to_vec();
    // First-read executable only. A second copy in an ISO directory/IP must
    // never be mistaken for the executed operand.
    let start = 21 * 2048;
    let offsets: Vec<_> = image[start..start + 0x6000]
        .windows(4)
        .enumerate()
        .filter_map(|(at, bytes)| (bytes == before.to_be_bytes()).then_some(start + at))
        .collect();
    assert_eq!(offsets.len(), 1);
    image[offsets[0]..offsets[0] + 4].copy_from_slice(&after.to_be_bytes());
    image
}

#[test]
fn changed_guest_dma_data_and_copied_priority_change_computed_results() {
    let mut m = Machine::new();
    let image = mutate_literal(ISO, 0x13579BDF, 0x2468ACE0);
    let actual = run(&mut m, &image);
    let mut changed = expected();
    let count = changed.len();
    changed[count - 3] = 0x2468ACE0;
    changed[count - 2] = 0x2468ACE0;
    assert_eq!(actual, changed);

    let image = mutate_literal(ISO, 0x0020F800, 0x0030F800);
    assert_eq!(run(&mut m, &image), expected());
    assert_eq!(m.bus().read32(0x0600_E600), Ok(0x30));
    assert_eq!(m.bus().read32(0x0600_E700), Ok(0xF0));

    let mut uncached = ISO.to_vec();
    let offsets: Vec<_> = uncached
        .windows(4)
        .enumerate()
        .filter_map(|(at, bytes)| (bytes == [0xE0, 19, 0x21, 0]).then_some(at))
        .collect();
    assert_eq!(offsets.len(), 1);
    uncached[offsets[0] + 1] = 0;
    let mut expected = expected();
    expected[2] = 0;
    assert_eq!(run(&mut m, &uncached), expected);
}

#[test]
fn hle_services_are_separate_from_external_bios_and_do_not_initialize_backup() {
    let mut m = Machine::new();
    let backup = vec![0xA5; 32768];
    m.import_backup_ram(&backup).unwrap();
    assert_eq!(run(&mut m, ISO), expected());
    assert_eq!(m.backup_ram(), backup);
    let mut bios = vec![0; BIOS_SIZE];
    bios[..4].copy_from_slice(&0x100_u32.to_be_bytes());
    bios[4..8].copy_from_slice(&0x060FF000_u32.to_be_bytes());
    bios[0x100..0x102].copy_from_slice(&0x001B_u16.to_be_bytes());
    m.reset(Boot::ExternalBios(&bios)).unwrap();
    assert_eq!(m.mode(), Some(BootMode::ExternalBios));
    assert_eq!(m.cpu_state().vbr, 0);
    for pointer in [0x210, 0x280, 0x300, 0x310, 0x330, 0x340, 0x348] {
        assert_eq!(m.bus().read32(0x06000000 + pointer), Ok(0));
    }
    assert_eq!(m.bus().read32(0x4000), Ok(0));
    assert_eq!(m.backup_ram(), backup);
}

#[test]
fn power_clear_bytes_survive_guest_nmi_and_sysres_while_service_state_resets() {
    let mut image = ISO.to_vec();
    // Original AIP's jump operand selects the second original guest routine.
    image[0xE28..0xE2C].copy_from_slice(&0x0600_9600_u32.to_be_bytes());
    let mut m = Machine::new();
    for _ in 0..2 {
        m.reset(Boot::HleDisc(&image)).unwrap();
        assert_eq!(m.bus().read32(0x0600_0C00), Ok(0));
        assert_eq!(m.bus().read32(0x0600_0C04), Ok(0));
        for _ in 0..35 {
            m.run_steps(10000).unwrap();
        }
        assert_eq!(m.cpu_state().status, CpuStatus::Sleeping);
        assert_eq!(m.cpu_state().exceptions, 0); // SYSRES starts a fresh CPU.
        let expected = [0x12345678, 0x12345679, 0xBFFF, 0x4B10, 1, 0x600D];
        for (i, value) in expected.into_iter().enumerate() {
            assert_eq!(m.bus().read32(0x0600_E800 + i as u32 * 4), Ok(value));
        }
    }
}
