//! ST-162-R1 backup-library functions 1-11, executed through SH-2 calls.
//! Original MIT disc and golden records; no host filesystem/service hooks.
use saturn_rust::{Boot, Machine, bus::Memory, cpu::CpuStatus};

const ISO: &[u8] = include_bytes!("fixtures/hle-backup.iso");
const TRACE: u32 = 0x0602_4000;

fn expected() -> Vec<(&'static str, u32)> {
    include_str!("fixtures/hle-backup.tsv")
        .lines()
        .map(|l| {
            let (name, word) = l.split_once('\t').unwrap();
            (name, u32::from_str_radix(word, 16).unwrap())
        })
        .collect()
}
fn complete(m: &mut Machine, last: u32) {
    for _ in 0..300 {
        m.run_steps(10000).unwrap();
        if m.bus().read32(last) == Ok(0x600D) {
            assert_eq!(m.cpu_state().exceptions, 0);
            return;
        }
    }
    panic!("guest did not finish: {:?}", m.cpu_state());
}
fn initialized() -> Machine {
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(ISO)).unwrap();
    complete(&mut m, TRACE + (expected().len() as u32 - 1) * 4);
    for (i, (name, value)) in expected().iter().enumerate() {
        assert_eq!(
            m.bus().read32(TRACE + i as u32 * 4),
            Ok(*value),
            "{i}: {name}"
        );
    }
    m
}
fn probe(m: &mut Machine) -> [u32; 4] {
    let mut iso = ISO.to_vec();
    iso[0xE28..0xE2C].copy_from_slice(&0x0600_E000_u32.to_be_bytes());
    m.reset(Boot::HleDisc(&iso)).unwrap();
    complete(m, TRACE + 12);
    std::array::from_fn(|i| m.bus().read32(TRACE + i as u32 * 4).unwrap())
}
#[test]
fn guest_backup_abi_executes_files_capacity_prefixes_compaction_and_gregorian_dates() {
    let m = initialized();
    assert_eq!(&m.backup_ram()[..8], b"SRBUP001");
    assert_eq!(&m.backup_ram()[132..139], b"FILE_A\0");
    assert_eq!(m.backup_ram()[128], 0xA5);
    assert_eq!(&m.backup_ram()[129..131], &[0, 5]);
    assert_eq!(&m.backup_ram()[166..168], &32730_u16.to_be_bytes());
    assert_eq!(
        &m.backup_ram()[168..425],
        &(0..257).map(|i| i as u8 ^ 90).collect::<Vec<_>>()
    );
}
#[test]
fn backup_services_reopen_retained_and_exported_raw_bytes_without_formatting() {
    let mut m = initialized();
    let packed = m.backup_ram().to_vec();
    for _ in 0..2 {
        assert_eq!(probe(&mut m), [0, 0, 7, 0x600D]);
        assert_eq!(m.backup_ram(), packed);
        assert_eq!(m.bus().read32(0x0602_2000), Ok(0x5A5B5859));
    }
    let mut fresh = Machine::new();
    fresh.import_backup_ram(&packed).unwrap();
    assert_eq!(probe(&mut fresh), [0, 0, 7, 0x600D]);
    assert_eq!(fresh.backup_ram(), packed);
}
#[test]
fn corrupt_allocation_and_payload_are_bounded_and_reported_by_guest_code() {
    let m = initialized();
    let packed = m.backup_ram().to_vec();
    for (offset, bytes) in [
        (129, vec![0, 0]),
        (129, vec![2, 0]),
        (160, vec![0, 0, 1, 25]),
        (128, vec![0x7F]),
        (168, vec![0x33]),
        (143, vec![1]),
        (154, vec![1]),
        (155, vec![6]),
        (164, vec![0, 4]),
    ] {
        let mut broken = packed.clone();
        broken[offset..offset + bytes.len()].copy_from_slice(&bytes);
        let mut m = Machine::new();
        m.import_backup_ram(&broken).unwrap();
        let status = probe(&mut m);
        if offset == 168 {
            assert_eq!(status, [0, 8, 8, 0x600D]);
        } else {
            assert_eq!(status, [8, 8, 8, 0x600D]);
        }
        assert_eq!(m.backup_ram(), broken);
    }
    // The signature is not a claim of compatibility with opaque firmware data.
    let mut fresh = Machine::new();
    fresh.import_backup_ram(&vec![0xA5; 32768]).unwrap();
    assert_eq!(probe(&mut fresh), [2, 2, 2, 0x600D]);
    assert_eq!(fresh.cpu_state().status, CpuStatus::Sleeping);
}

#[test]
fn malformed_block_and_size_fields_cannot_escape_backup_ram_or_change_it() {
    let packed = initialized().backup_ram().to_vec();
    for value in [0, 511, 512, 1024, u16::MAX] {
        let mut broken = packed.clone();
        broken[129..131].copy_from_slice(&value.to_be_bytes());
        let mut m = Machine::new();
        m.import_backup_ram(&broken).unwrap();
        assert_eq!(probe(&mut m), [8, 8, 8, 0x600D]);
        assert_eq!(m.backup_ram(), broken);
    }
    for value in [281_u32, 32768, 65535, 0x80000000, u32::MAX] {
        let mut broken = packed.clone();
        broken[160..164].copy_from_slice(&value.to_be_bytes());
        let mut m = Machine::new();
        m.import_backup_ram(&broken).unwrap();
        assert_eq!(probe(&mut m), [8, 8, 8, 0x600D]);
        assert_eq!(m.backup_ram(), broken);
    }
}

#[test]
fn changed_guest_library_destination_relocates_executing_entry_points() {
    let mut image = ISO.to_vec();
    let mut changed = 0;
    for word in image[21 * 2048..].as_chunks_mut::<4>().0 {
        let value = u32::from_be_bytes(*word);
        if value == 0x0603_0000 {
            word.copy_from_slice(&(value + 0x20000).to_be_bytes());
            changed += 1;
        }
    }
    assert_eq!(changed, 2);
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    complete(&mut m, TRACE + (expected().len() as u32 - 1) * 4);
    for (i, (name, value)) in expected().iter().enumerate() {
        assert_eq!(m.bus().read32(TRACE + i as u32 * 4), Ok(*value), "{name}");
    }
    assert_eq!(m.bus().read32(0x0600_0354), Ok(0x0605_0000));
    assert_eq!(m.bus().read32(0x0605_0010), Ok(0x0605_0600));
    assert_eq!(m.bus().read32(0x0603_0000), Ok(0));
}

#[test]
fn changed_guest_xor_operand_changes_computed_and_persisted_data() {
    let mut image = ISO.to_vec();
    let offsets: Vec<_> = image[21 * 2048..]
        .windows(2)
        .enumerate()
        .filter_map(|(i, b)| (b == [0xE4, 90]).then_some(21 * 2048 + i))
        .collect();
    assert_eq!(offsets.len(), 1);
    image[offsets[0] + 1] = 91;
    let mut m = Machine::new();
    m.reset(Boot::HleDisc(&image)).unwrap();
    complete(&mut m, TRACE + (expected().len() as u32 - 1) * 4);
    for (i, (name, value)) in expected().iter().enumerate() {
        let value = if name.starts_with("A byte ") {
            value ^ 1
        } else {
            *value
        };
        assert_eq!(m.bus().read32(TRACE + i as u32 * 4), Ok(value), "{name}");
    }
    assert_eq!(m.backup_ram()[169], 90);
    assert_eq!(m.backup_ram()[424], 91);
}
