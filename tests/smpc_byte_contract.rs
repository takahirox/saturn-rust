//! Independent COMREG acceptance audit, ST-169 tables 2.1–2.4, pp.37–38,
//! 42–43, 46 and 54. Uses only the inherited public API for differential runs.
use saturn_rust::{
    bus::{Bus, Memory},
    smpc::Smpc,
};

const BASE: u32 = 0x00100000;
const COMMANDS: [u8; 16] = [
    0, 2, 3, 6, 7, 8, 9, 0x0d, 0x0e, 0x0f, 0x10, 0x16, 0x17, 0x18, 0x19, 0x1a,
];

fn parameters(s: &mut Smpc, bytes: &[u8]) {
    // §2.2: claim SF first, then supply parameters, then write COMREG.
    s.write(BASE + 0x63, 1).unwrap();
    for (i, byte) in bytes.iter().enumerate() {
        s.write(BASE + 1 + i as u32 * 2, *byte).unwrap();
    }
}

#[test]
fn every_documented_command_byte_is_accepted_and_unlisted_codes_stay_unsupported() {
    for command in 0..=255 {
        let mut s = Smpc::default();
        let bytes: &[u8] = match command {
            0x10 => &[1, 0, 0xf0],
            0x16 => &[0x20, 0x24, 0x52, 0x29, 0x23, 0x59, 0x59],
            _ => &[0xa5; 7], // unused IREGs must not reject parameterless commands
        };
        parameters(&mut s, bytes);
        assert_eq!(
            s.write(BASE + 0x1f, command).is_ok(),
            COMMANDS.contains(&command),
            "COMREG={command:02x}"
        );
    }
}

#[test]
fn intback_acceptance_matches_all_documented_parameter_fields() {
    // Enumerate every IREG0 byte and every defined IREG1 combination, with
    // valid and invalid IREG2. Reserved bits 2/0 are kept zero (§2.4 p.37).
    for status in 0..=255 {
        for port1 in 0..4 {
            for port2 in 0..4 {
                for pen in 0..2 {
                    for ope in 0..2 {
                        for fixed in [0, 0xf0, 0xff] {
                            let mode = port2 << 6 | port1 << 4 | pen << 3 | ope << 1;
                            let mut s = Smpc::default();
                            parameters(&mut s, &[status, mode, fixed]);
                            let documented = status <= 1
                                && fixed == 0xf0
                                && port1 != 2
                                && port2 != 2
                                && (status == 1 || pen == 1);
                            assert_eq!(
                                s.write(BASE + 0x1f, 0x10).is_ok(),
                                documented,
                                "INTBACK {status:02x}/{mode:02x}/{fixed:02x}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn settime_accepts_real_dates_weekdays_and_boundary_times() {
    let bcd = |v: u8| ((v / 10) << 4) | (v % 10);
    // Independently authored month lengths, including the 2000 leap day.
    for (year, days) in [
        (0x00, [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]),
        (0x01, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]),
    ] {
        for (month, last) in days.into_iter().enumerate() {
            for day in 1..=last {
                for weekday in 0..7 {
                    for time in [[0, 0, 0], [0x23, 0x59, 0x59]] {
                        let mut s = Smpc::default();
                        parameters(
                            &mut s,
                            &[
                                0x20,
                                year,
                                weekday << 4 | (month + 1) as u8,
                                bcd(day),
                                time[0],
                                time[1],
                                time[2],
                            ],
                        );
                        assert!(s.write(BASE + 0x1f, 0x16).is_ok());
                    }
                }
            }
        }
    }
    // These are explicit emulator policies for undefined calendar results
    // (p.46), not claims of a hardware bus exception or invalid command code.
    for bytes in [
        [0x20, 0x01, 0x12, 0x29, 0, 0, 0],    // nonexistent date
        [0x20, 0x01, 0x1d, 0x01, 0, 0, 0],    // nonexistent month
        [0x20, 0x01, 0x71, 0x01, 0, 0, 0],    // nonexistent weekday
        [0x20, 0x01, 0x11, 0x01, 0, 0x6a, 0], // non-BCD minute
    ] {
        let mut s = Smpc::default();
        parameters(&mut s, &bytes);
        assert!(s.write(BASE + 0x1f, 0x16).is_err());
    }
}

#[test]
fn byte_lanes_and_handshake_survive_bus_aliases_and_command_deadlines() {
    for base in [BASE, BASE + 0x100, 0x20100000, 0x2017ff80] {
        let mut b = Bus::default();
        assert!(b.read8(base + 0x1f).is_err()); // no invented COMREG readback
        assert!(b.write8(base + 0x1f, 9).is_err()); // SF not claimed
        b.write8(base + 0x63, 1).unwrap();
        b.write8(base + 0x1e, 9).unwrap(); // disconnected lane
        assert_eq!(b.read8(base + 0x5f), Ok(0));
        b.write8(base + 0x1f, 9).unwrap();
        for clocks in [1, 3, 127, 943] {
            // 1074 reference clocks, one before CDOFF
            b.advance_clocks(clocks).unwrap();
            assert_eq!(b.read8(base + 0x63), Ok(1));
            assert!(b.write8(base + 0x1f, 8).is_err());
        }
        b.advance_clocks(1).unwrap();
        assert_eq!(b.read8(base + 0x63), Ok(0));
        assert_eq!(b.read8(base + 0x5f), Ok(9));
        b.advance_clocks(1).unwrap();
        b.write8(base + 0x63, 1).unwrap();
        b.write8(base + 0x1f, 8).unwrap();
        b.advance_clocks(1075).unwrap();
        assert_eq!(b.read8(base + 0x63), Ok(0));
        assert_eq!(b.read8(base + 0x5f), Ok(8));
    }
}

#[test]
fn original_byte_firmware_publishes_real_calendar_memory_and_handshake_results() {
    use saturn_rust::{Boot, Machine};
    let mut bios = vec![0; 524288];
    for line in include_str!("fixtures/smpc-byte-audit.hex")
        .lines()
        .filter(|s| !s.starts_with('#') && !s.is_empty())
    {
        let mut words = line.split_whitespace();
        let at = usize::from_str_radix(words.next().unwrap(), 16).unwrap();
        for (i, word) in words.enumerate() {
            bios[at + i * 2..at + i * 2 + 2]
                .copy_from_slice(&u16::from_str_radix(word, 16).unwrap().to_be_bytes());
        }
    }
    let expected: Vec<_> = include_str!("fixtures/smpc-byte-audit.tsv")
        .lines()
        .map(|s| u32::from_str_radix(s, 16).unwrap())
        .collect();
    let mut reference = None;
    for chunk in [1, 127, 10000] {
        let mut m = Machine::new();
        for _ in 0..2 {
            m.reset(Boot::ExternalBios(&bios)).unwrap();
            let mut left = 2000000;
            while left > 0 {
                let n = left.min(chunk);
                m.run_steps(n).unwrap();
                let _ = m.cpu_state();
                let _ = m.debug_read32(0x0601b000).unwrap();
                left -= n;
            }
            let actual: Vec<_> = (0..expected.len())
                .map(|i| m.bus().read32(0x0601b000 + i as u32 * 4).unwrap())
                .collect();
            assert_eq!(actual, expected);
            assert_eq!(m.cpu_state().exceptions, 0);
            let result = (m.clocks(), m.cpu_state().clone());
            assert_eq!(*reference.get_or_insert(result.clone()), result);
        }
    }
}
