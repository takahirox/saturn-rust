//! Cooked ISO9660 + Saturn IP loader, independently implemented.
//! ST-040-R4 §§3–4 (system area, IP header, first-read file identifier 2).
use crate::bus::{HIGH_WRAM, WRAM_SIZE};
use std::borrow::Cow;
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
    pub executable: Cow<'a, [u8]>,
    pub load_address: u32,
    pub stack: u32,
    pub slave_stack: u32,
    pub aip_entry: u32,
}

#[derive(Clone, Copy)]
struct FileExtent {
    sector: u32,
    size: u32,
    unit: u8,
    gap: u8,
}
impl FileExtent {
    fn sector_at(self, index: u32) -> Option<u32> {
        let unit = u32::from(self.unit);
        let offset = match index.checked_div(unit) {
            None => index,
            Some(units) => units
                .checked_mul(unit + u32::from(self.gap))?
                .checked_add(index % unit)?,
        };
        self.sector.checked_add(offset)
    }
    fn from_record(record: &[u8], image: &[u8]) -> Result<Self, DiscError> {
        let file = Self {
            sector: dual32(record, 2)?
                .checked_add(u32::from(record[1]))
                .ok_or_else(|| invalid("ISO extended-attribute extent overflow"))?,
            size: dual32(record, 10)?,
            unit: record[26],
            gap: record[27],
        };
        if (file.unit == 0 && file.gap != 0) || (record[25] & 2 != 0 && file.unit != 0) {
            return Err(invalid("invalid ISO9660 interleave unit/gap"));
        }
        let end = if file.size == 0 {
            file.sector
        } else {
            file.sector_at(file.size.div_ceil(SECTOR as u32) - 1)
                .and_then(|s| s.checked_add(1))
                .ok_or_else(|| invalid("ISO interleave extent overflow"))?
        };
        if end as u64 * SECTOR as u64 > image.len() as u64 {
            return Err(invalid("truncated ISO file/interleave extent"));
        }
        Ok(file)
    }
    fn bytes(self, image: &[u8]) -> Result<Cow<'_, [u8]>, DiscError> {
        if self.unit == 0 || self.gap == 0 {
            return Ok(Cow::Borrowed(extent(image, self.sector, self.size)?));
        }
        let mut bytes = Vec::with_capacity(self.size as usize);
        for i in 0..self.size.div_ceil(SECTOR as u32) {
            let length = (self.size as usize - bytes.len()).min(SECTOR);
            bytes.extend_from_slice(extent(image, self.sector_at(i).unwrap(), length as u32)?);
        }
        Ok(Cow::Owned(bytes))
    }
}

/// ST-040 §3.2.1: scan the volume descriptor set, rather than assuming its
/// terminator immediately follows the primary. Boot/supplementary/partition
/// descriptors do not replace the primary ISO9660 namespace used at startup.
fn primary_volume(image: &[u8]) -> Result<(&[u8], usize), DiscError> {
    let mut primary: Option<&[u8]> = None;
    for (index, descriptor) in image[16 * SECTOR..]
        .as_chunks::<SECTOR>()
        .0
        .iter()
        .enumerate()
    {
        if &descriptor[1..7] != b"CD001\x01" {
            return Err(invalid(
                "invalid ISO9660 volume descriptor identifier/version",
            ));
        }
        match descriptor[0] {
            0 | 2 | 3 => (),
            1 if primary.is_none() => primary = Some(descriptor),
            255 => {
                return primary
                    .map(|p| (p, index + 17))
                    .ok_or_else(|| invalid("missing ISO9660 primary volume descriptor"));
            }
            _ => return Err(invalid("reserved or duplicate ISO9660 volume descriptor")),
        }
    }
    Err(invalid("missing ISO9660 volume descriptor terminator"))
}

pub fn parse(image: &[u8]) -> Result<DiscBoot<'_>, DiscError> {
    if image.len() > MAX_DISC_SIZE
        || image.len() < 18 * SECTOR
        || !image.len().is_multiple_of(SECTOR)
    {
        return Err(invalid(
            "expected a complete 2048-byte-sector ISO (18 sectors to 16 MiB)",
        ));
    }
    if &image[..16] != b"SEGA SEGASATURN " {
        return Err(invalid(
            "missing Saturn hardware identifier in IP system area",
        ));
    }
    let field = &image[0x40..0x50];
    let count = field.iter().position(|c| *c == b' ').unwrap_or(field.len());
    let areas = &field[..count];
    if areas.is_empty()
        || areas.len() > 4
        || field[count..].iter().any(|c| *c != b' ')
        || areas
            .iter()
            .enumerate()
            .any(|(i, c)| !b"JTUE".contains(c) || areas[..i].contains(c))
    {
        return Err(invalid(
            "Saturn area symbols must be distinct J/T/U/E codes followed by spaces",
        ));
    }
    let ip_size = be32(image, 0xE0) as usize;
    if !(0x1000..=0x8000).contains(&ip_size) || !ip_size.is_multiple_of(4) {
        return Err(invalid(
            "Saturn IP size must be aligned and within 0x1000..0x8000",
        ));
    }
    let ip = &image[..ip_size];
    // ST-040 §§4.1/4.5/4.6: each area block adds 32 bytes before AIP.
    // Independent HLE never executes or substitutes Sega security/area objects.
    let aip_entry = IP_ADDRESS + 0xE00 + areas.len() as u32 * 0x20;
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
    let slave_stack = match be32(image, 0xEC) {
        0 => HIGH_WRAM + 0x1000,
        n => n,
    };
    if !slave_stack.is_multiple_of(4)
        || !ram_range(slave_stack.wrapping_sub(4), 4)
        || (slave_stack > IP_ADDRESS && slave_stack <= IP_ADDRESS + ip_size as u32)
    {
        return Err(invalid(
            "slave stack must be aligned in high work RAM outside the IP",
        ));
    }
    let load_address = be32(image, 0xF0);
    if load_address != 0
        && (!load_address.is_multiple_of(4) || load_address < IP_ADDRESS + ip_size as u32)
    {
        return Err(invalid(
            "HLE requires a first-read destination above the loaded IP in high work RAM",
        ));
    }
    let (pvd, descriptors_end) = primary_volume(image)?;
    let volume_size = dual32(pvd, 80)? as usize;
    if volume_size < descriptors_end
        || volume_size > image.len() / SECTOR
        || pvd[128..132] != [0, 8, 8, 0]
    {
        return Err(invalid(
            "ISO volume size/block size exceeds the medium or excludes its descriptors",
        ));
    }
    // Padding after a volume is legal, but directory/file extents must not use
    // it to escape the declared logical volume, even when bytes are present.
    let image = &image[..volume_size * SECTOR];
    let root = &pvd[156..190];
    if root[0] != 34 || root[1] != 0 || root[25] != 2 || root[32..34] != [1, 0] {
        return Err(invalid("invalid ISO9660 root directory record"));
    }
    let directory_sector = dual32(root, 2)?;
    let directory_size = dual32(root, 10)?;
    if directory_sector < descriptors_end as u32
        || directory_size == 0
        || directory_size > 64 * SECTOR as u32
    {
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
            || record[28..32] != [1, 0, 0, 1]
            || record[25] & !3 != 0
        {
            return Err(invalid("unsupported ISO record flags or volume sequence"));
        }
        let file = FileExtent::from_record(record, image)?;
        let (sector, size) = (file.sector, file.size);
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
        if record_index == 2 && load_address != 0 {
            // The boot system loads file identifier 2: first root child, not a magic name.
            if record[25] & 2 != 0
                || sector < descriptors_end as u32
                || (sector as u64 * SECTOR as u64)
                    < directory_sector as u64 * SECTOR as u64 + directory_size as u64
            {
                return Err(invalid(
                    "first root entry must be an executable file after the directory",
                ));
            }
            first_file = Some(file);
        }
        record_index += 1;
        offset += length;
    }
    // ST-040 §4.3: a zero first-read address disables that transfer. The
    // original guest AIP still executes; it can read its own files at runtime.
    if load_address == 0 {
        return Ok(DiscBoot {
            ip,
            executable: Cow::Borrowed(&[]),
            load_address,
            stack,
            slave_stack,
            aip_entry,
        });
    }
    let file =
        first_file.ok_or_else(|| invalid("missing first-read executable in root directory"))?;
    let size = file.size;
    if size == 0
        || !ram_range(load_address, size as usize)
        || [stack, slave_stack]
            .iter()
            .any(|&sp| sp > load_address && sp <= load_address.saturating_add(size))
    {
        return Err(invalid(
            "first-read executable is empty, exceeds high work RAM, or overlaps the stack",
        ));
    }
    Ok(DiscBoot {
        ip,
        executable: file.bytes(image)?,
        load_address,
        stack,
        slave_stack,
        aip_entry,
    })
}
