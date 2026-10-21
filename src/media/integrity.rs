//! ECMA-130 §14.3 and annex A: CRC-32 EDC and the two interleaved RS(26,24)
//! / RS(45,43) product codes. All arithmetic is independently implemented here.
//! XA Form 1 uses the same parity with a zeroed header; Form 2 has EDC only.
use super::{DiscError, RAW_SECTOR, TrackMode, invalid};

pub(super) fn edc(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0xD801_8001 } else { 0 };
        }
    }
    crc
}
fn double(value: u8) -> u8 {
    ((u16::from(value) << 1) ^ if value & 128 != 0 { 0x11D } else { 0 }) as u8
}
fn inverse() -> [u8; 256] {
    let mut result = [0; 256];
    for value in 0..=255u8 {
        result[(value ^ double(value)) as usize] = value;
    }
    result
}
const CODES: [(usize, usize, usize, usize, usize); 2] =
    [(86, 24, 2, 86, 2076), (52, 43, 86, 88, 2248)];
fn parity(data: impl Iterator<Item = u8>, inverse: &[u8; 256]) -> [u8; 2] {
    let (mut a, mut b) = (0, 0);
    for value in data {
        a = double(a ^ value);
        b ^= value;
    }
    a = inverse[(double(a) ^ b) as usize];
    [a, a ^ b]
}
fn indices(major: usize, code: (usize, usize, usize, usize, usize)) -> impl Iterator<Item = usize> {
    let (majors, minors, multiplier, increment, _) = code;
    let start = (major >> 1) * multiplier + (major & 1);
    (0..minors).map(move |minor| 12 + (start + minor * increment) % (majors * minors))
}
pub(super) fn encode(sector: &mut [u8; RAW_SECTOR]) {
    let inverse = inverse();
    for code in CODES {
        for major in 0..code.0 {
            let [a, b] = parity(indices(major, code).map(|i| sector[i]), &inverse);
            sector[code.4 + major] = a;
            sector[code.4 + major + code.0] = b;
        }
    }
}
fn correct(sector: &mut [u8; RAW_SECTOR]) -> bool {
    let inverse = inverse();
    let mut changed = false;
    // Q can repair symbols that share a P codeword; the next pass then resolves
    // that P codeword. Bound passes so adversarial sectors cannot spin forever.
    for code in CODES {
        for major in 0..code.0 {
            let expected = parity(indices(major, code).map(|i| sector[i]), &inverse);
            let first = code.4 + major;
            let second = first + code.0;
            let syndrome = [expected[0] ^ sector[first], expected[1] ^ sector[second]];
            let correction = match syndrome {
                [0, 0] => continue,
                [error, 0] => Some((first, error)),
                [0, error] => Some((second, error)),
                [a, b] => {
                    let error = a ^ b;
                    indices(major, code)
                        .enumerate()
                        .find_map(|(position, index)| {
                            let delta = parity(
                                (0..code.1).map(|i| if i == position { error } else { 0 }),
                                &inverse,
                            );
                            (delta == syndrome).then_some((index, error))
                        })
                }
            };
            if let Some((index, error)) = correction {
                sector[index] ^= error;
                changed = true;
            }
        }
    }
    changed
}
fn edc_matches(sector: &[u8; RAW_SECTOR], mode: TrackMode) -> bool {
    let (start, end) = match mode {
        TrackMode::Mode1 => (0, 2064),
        TrackMode::Mode2 if sector[18] & 0x20 == 0 => (16, 2072),
        TrackMode::Mode2 => (16, 2348),
        TrackMode::Audio => return true,
    };
    sector[end..end + 4] == edc(&sector[start..end]).to_le_bytes()
}
pub(super) fn validate(sector: &mut [u8; RAW_SECTOR], mode: TrackMode) -> Result<(), DiscError> {
    if mode == TrackMode::Audio {
        return Ok(());
    }
    if sector[..12] != [0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0] {
        return Err(invalid("Malformed raw data sector sync"));
    }
    let header: [u8; 4] = sector[12..16].try_into().unwrap();
    // One damaged copy of the Form bit must not prevent Form 1 parity from
    // repairing its subheader. Two copies selecting Form 2 mean EDC only: never
    // reinterpret corrupted Form 2 payload bytes as a Form 1 parity trailer.
    let form2 = mode == TrackMode::Mode2 && sector[18] & sector[22] & 0x20 != 0;
    if !form2 {
        if mode == TrackMode::Mode2 {
            sector[12..16].fill(0);
        }
        // A clean EDC does not excuse corrupt parity: raw transfers must expose
        // corrected P/Q as well as corrected user bytes.
        for _ in 0..4 {
            if !correct(sector) {
                break;
            }
        }
        let mut encoded = *sector;
        encode(&mut encoded);
        if encoded[2076..] != sector[2076..] {
            return Err(invalid("Uncorrectable raw sector parity"));
        }
        if mode == TrackMode::Mode2 {
            sector[12..16].copy_from_slice(&header);
        }
    }
    let valid_bcd = sector[12..15]
        .iter()
        .all(|byte| byte & 15 < 10 && byte >> 4 < 10)
        && sector[13] < 0x60
        && sector[14] < 0x75;
    if !valid_bcd
        || sector[15] != if mode == TrackMode::Mode1 { 1 } else { 2 }
        || !edc_matches(sector, mode)
        || (mode == TrackMode::Mode1 && sector[2068..2076] != [0; 8])
        || (mode == TrackMode::Mode2 && sector[16..20] != sector[20..24])
    {
        return Err(invalid("Uncorrectable raw sector EDC/header"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // Independent polynomial multiplication, used to evaluate the parity-check
    // matrix in ECMA-130 annex A.3, rather than the encoder's recurrence.
    fn multiply(mut a: u16, mut b: u16) -> u8 {
        let mut result = 0;
        while b != 0 {
            if b & 1 != 0 {
                result ^= a;
            }
            b >>= 1;
            a <<= 1;
            if a & 256 != 0 {
                a ^= 0x11D;
            }
        }
        result as u8
    }
    #[test]
    fn edc_standard_check_and_both_parity_check_matrix_rows() {
        // CRC-32/CD-ROM-EDC check value for the source string, not an emulator.
        assert_eq!(edc(b"123456789"), 0x6EC2_EDC4);
        let mut raw = [0; RAW_SECTOR];
        for (i, byte) in raw[12..2076].iter_mut().enumerate() {
            *byte = ((i * 73) ^ (i >> 4)) as u8;
        }
        encode(&mut raw);
        for code in CODES {
            for major in 0..code.0 {
                let values: Vec<_> = indices(major, code)
                    .map(|i| raw[i])
                    .chain([raw[code.4 + major], raw[code.4 + major + code.0]])
                    .collect();
                assert_eq!(values.iter().fold(0, |sum, v| sum ^ v), 0);
                let (mut sum, mut power) = (0, 1);
                for value in values.into_iter().rev() {
                    sum ^= multiply(u16::from(value), power);
                    power = u16::from(multiply(power, 2));
                }
                assert_eq!(sum, 0, "code {code:?}, row {major}");
            }
        }
    }
    #[test]
    fn xa_form_one_corrects_data_subheader_and_parity_and_form_two_detects_errors() {
        let mut xa = [0; RAW_SECTOR];
        xa[1..11].fill(255);
        xa[15] = 2;
        xa[16..24].copy_from_slice(&[1, 2, 8, 0, 1, 2, 8, 0]);
        xa[24..2072].fill(0x69);
        let crc = edc(&xa[16..2072]);
        xa[2072..2076].copy_from_slice(&crc.to_le_bytes());
        xa[12..16].fill(0);
        encode(&mut xa);
        xa[15] = 2;
        for index in [
            16, 17, 18, 19, 20, 21, 22, 23, 24, 2000, 2072, 2075, 2076, 2247, 2351,
        ] {
            let mut damaged = xa;
            damaged[index] ^= 0x6A;
            validate(&mut damaged, TrackMode::Mode2).unwrap();
            assert_eq!(damaged, xa, "offset {index}");
        }
        xa[18] = 0x28;
        xa[22] = 0x28;
        let crc = edc(&xa[16..2348]);
        xa[2348..].copy_from_slice(&crc.to_le_bytes());
        validate(&mut xa, TrackMode::Mode2).unwrap();
        xa[42] ^= 1;
        assert!(validate(&mut xa, TrackMode::Mode2).is_err());
    }
}
