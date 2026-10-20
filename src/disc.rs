//! Strict cooked ISO9660 + single-area Saturn IP loader, independently implemented.
//! ST-040-R4 §§3–4 (system area, IP header, first-read file identifier 2).
use crate::bus::{HIGH_WRAM, WRAM_SIZE};
use std::fmt;

pub const SECTOR: usize = 2048;
pub const MAX_DISC_SIZE: usize = 16 * 1024 * 1024;
pub const IP_ADDRESS: u32 = HIGH_WRAM + 0x2000;
pub const AIP_ENTRY: u32 = IP_ADDRESS + 0xE20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscError(pub String);
impl fmt::Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cannot boot disc: {}", self.0)
    }
}
impl std::error::Error for DiscError {}
fn invalid(message: &str) -> DiscError {
    DiscError(message.into())
}
fn be32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn dual32(bytes: &[u8], offset: usize) -> Result<u32, DiscError> {
    let little = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    if little != be32(bytes, offset + 4) {
        return Err(invalid("ISO9660 endian copies disagree"));
    }
    Ok(little)
}
fn extent(image: &[u8], sector: u32, length: u32) -> Result<&[u8], DiscError> {
    let start = (sector as usize)
        .checked_mul(SECTOR)
        .ok_or_else(|| invalid("ISO extent overflow"))?;
    let end = start
        .checked_add(length as usize)
        .ok_or_else(|| invalid("ISO extent overflow"))?;
    image
        .get(start..end)
        .ok_or_else(|| invalid("truncated ISO file/directory extent"))
}
fn ram_range(address: u32, size: usize) -> bool {
    address >= HIGH_WRAM && (address as u64 + size as u64) <= HIGH_WRAM as u64 + WRAM_SIZE as u64
}

pub struct DiscBoot<'a> {
    pub ip: &'a [u8],
    pub executable: &'a [u8],
    pub load_address: u32,
    pub stack: u32,
}

pub fn parse(image: &[u8]) -> Result<DiscBoot<'_>, DiscError> {
    if image.len() > MAX_DISC_SIZE
        || image.len() < 22 * SECTOR
        || !image.len().is_multiple_of(SECTOR)
    {
        return Err(invalid(
            "expected a complete 2048-byte-sector ISO (22 sectors to 16 MiB); raw BIN/CUE and compressed images are unsupported",
        ));
    }
    if &image[..16] != b"SEGA SEGASATURN " {
        return Err(invalid(
            "missing Saturn hardware identifier in IP system area",
        ));
    }
    let areas = image[0x40..0x50]
        .iter()
        .filter(|&&c| c != b' ')
        .copied()
        .collect::<Vec<_>>();
    if areas.len() != 1 || !b"JTUE".contains(&areas[0]) {
        return Err(invalid(
            "HLE currently requires exactly one Saturn area code (J, T, U or E)",
        ));
    }
    let ip_size = be32(image, 0xE0) as usize;
    if !(0x1000..=0x8000).contains(&ip_size) || !ip_size.is_multiple_of(4) {
        return Err(invalid(
            "Saturn IP size must be aligned and within 0x1000..0x8000",
        ));
    }
    let ip = &image[..ip_size];
    let stack = match be32(image, 0xE8) {
        0 => HIGH_WRAM + 0x2000,
        n => n,
    };
    if !stack.is_multiple_of(4)
        || !ram_range(stack.wrapping_sub(4), 4)
        || (stack > IP_ADDRESS && stack <= IP_ADDRESS + ip_size as u32)
    {
        return Err(invalid(
            "master stack must be aligned in high work RAM outside the IP",
        ));
    }
    let load_address = be32(image, 0xF0);
    if !load_address.is_multiple_of(4) || load_address < IP_ADDRESS + ip_size as u32 {
        return Err(invalid(
            "HLE requires a first-read destination above the loaded IP in high work RAM",
        ));
    }
    let pvd = &image[16 * SECTOR..17 * SECTOR];
    if &pvd[..7] != b"\x01CD001\x01" || &image[17 * SECTOR..17 * SECTOR + 7] != b"\xFFCD001\x01" {
        return Err(invalid(
            "unsupported ISO9660 volume descriptors: need primary at sector 16 and terminator at 17",
        ));
    }
    if dual32(pvd, 80)? as usize != image.len() / SECTOR || pvd[128..132] != [0, 8, 8, 0] {
        return Err(invalid(
            "ISO volume size/2048-byte logical block size does not match file",
        ));
    }
    let root = &pvd[156..190];
    if root[0] != 34 || root[1] != 0 || root[25] != 2 || root[32..34] != [1, 0] {
        return Err(invalid("invalid ISO9660 root directory record"));
    }
    let directory_sector = dual32(root, 2)?;
    let directory_size = dual32(root, 10)?;
    if directory_sector < 18 || directory_size == 0 || directory_size > 64 * SECTOR as u32 {
        return Err(invalid("unsupported root directory extent/size"));
    }
    let directory = extent(image, directory_sector, directory_size)?;
    let mut offset = 0;
    let mut first_file = None;
    let mut record_index: usize = 0;
    while offset < directory.len() {
        let length = directory[offset] as usize;
        if length == 0 {
            offset = (offset / SECTOR + 1) * SECTOR;
            continue;
        }
        if length < 34 || offset + length > directory.len() || offset % SECTOR + length > SECTOR {
            return Err(invalid("truncated/invalid ISO9660 directory record"));
        }
        let record = &directory[offset..offset + length];
        let name_length = record[32] as usize;
        if name_length == 0
            || 33 + name_length > length
            || record[1] != 0
            || record[26..28] != [0, 0]
            || record[28..32] != [1, 0, 0, 1]
            || record[25] & !2 != 0
        {
            return Err(invalid(
                "unsupported ISO record (extended attributes, flags, interleave or multi-volume)",
            ));
        }
        let sector = dual32(record, 2)?;
        let size = dual32(record, 10)?;
        extent(image, sector, size)?;
        let name = &record[33..33 + name_length];
        if record_index < 2 {
            if name != [record_index as u8]
                || record[25] != 2
                || sector != directory_sector
                || size != directory_size
            {
                return Err(invalid(
                    "root directory must begin with matching self and parent records",
                ));
            }
        } else if name == [0] || name == [1] {
            return Err(invalid("duplicate ISO root self/parent record"));
        }
        if record_index == 2 {
            // The boot system loads file identifier 2: first root child, not a magic name.
            if record[25] & 2 != 0
                || sector < 18
                || (sector as u64 * SECTOR as u64)
                    < directory_sector as u64 * SECTOR as u64 + directory_size as u64
            {
                return Err(invalid(
                    "first root entry must be a contiguous executable file after the directory",
                ));
            }
            first_file = Some((sector, size));
        }
        record_index += 1;
        offset += length;
    }
    let (sector, size) =
        first_file.ok_or_else(|| invalid("missing first-read executable in root directory"))?;
    if size == 0
        || !ram_range(load_address, size as usize)
        || (stack > load_address && stack <= load_address.saturating_add(size))
    {
        return Err(invalid(
            "first-read executable is empty, exceeds high work RAM, or overlaps the stack",
        ));
    }
    Ok(DiscBoot {
        ip,
        executable: extent(image, sector, size)?,
        load_address,
        stack,
    })
}
