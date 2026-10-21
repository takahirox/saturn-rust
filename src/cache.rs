//! SH7604 §8: 4 KiB, four-way, write-through cache. No hardware snooping.
//! The LRU masks below are derived from tables 8.3/8.4, not another core.
use crate::bus::BusError;

pub const CCR: u32 = 0xFFFF_FE92;
#[derive(Clone, Default)]
struct Line {
    tag: u32,
    valid: bool,
    data: [u8; 16],
}
pub(crate) struct Cache {
    control: u8,
    lines: Vec<[Line; 4]>,
    lru: [u8; 64],
}
impl Default for Cache {
    fn default() -> Self {
        Self {
            control: 0,
            lines: (0..64)
                .map(|_| std::array::from_fn(|_| Line::default()))
                .collect(),
            lru: [0; 64],
        }
    }
}
impl Cache {
    pub fn control(&self) -> u8 {
        self.control
    }
    pub fn set_control(&mut self, value: u8) {
        self.control = value & 0xCF;
        if value & 0x10 != 0 {
            for set in &mut self.lines {
                for line in set {
                    line.valid = false;
                }
            }
            self.lru.fill(0);
        }
    }
    pub fn reset_control(&mut self) {
        self.control = 0;
    }
    fn parts(address: u32) -> (usize, u32, usize) {
        (
            ((address >> 4) & 63) as usize,
            address & 0x1FFF_FC00,
            (address & 15) as usize,
        )
    }
    fn touch(&mut self, set: usize, way: usize) {
        let (mask, value) = [(0x38, 0), (0x26, 0x20), (0x15, 0x14), (0x0B, 0x0B)][way];
        self.lru[set] = (self.lru[set] & !mask) | value;
    }
    fn victim(&self, set: usize) -> usize {
        let lru = self.lru[set];
        if self.control & 8 != 0 {
            return if lru & 1 == 0 { 3 } else { 2 };
        }
        [(0x38, 0x38), (0x26, 0x06), (0x15, 0x01), (0x0B, 0)]
            .into_iter()
            .position(|(mask, value)| lru & mask == value)
            .unwrap_or(3)
    }
    pub fn read<const N: usize>(
        &mut self,
        address: u32,
        instruction: bool,
        mut fetch: impl FnMut(u32) -> Result<[u8; 16], BusError>,
    ) -> Result<Option<[u8; N]>, BusError> {
        if address >= 0x2000_0000 || self.control & 1 == 0 {
            return Ok(None);
        }
        let (set, tag, offset) = Self::parts(address);
        let way = if let Some(way) = self.lines[set]
            .iter()
            .position(|line| line.valid && line.tag == tag)
        {
            way
        } else {
            let disable = if instruction { 2 } else { 4 };
            if self.control & disable != 0 {
                return Ok(None);
            }
            let data = fetch(address & !15)?;
            let way = self.victim(set);
            self.lines[set][way] = Line {
                tag,
                valid: true,
                data,
            };
            way
        };
        let mut bytes = [0; N];
        bytes.copy_from_slice(&self.lines[set][way].data[offset..offset + N]);
        self.touch(set, way);
        Ok(Some(bytes))
    }
    pub fn written<const N: usize>(&mut self, address: u32, bytes: [u8; N]) {
        if address >= 0x2000_0000 || self.control & 1 == 0 {
            return;
        }
        let (set, tag, offset) = Self::parts(address);
        for way in 0..4 {
            if self.lines[set][way].valid && self.lines[set][way].tag == tag {
                self.lines[set][way].data[offset..offset + N].copy_from_slice(&bytes);
                self.touch(set, way);
            }
        }
    }
    pub fn array_read<const N: usize>(&self, address: u32) -> Result<Option<[u8; N]>, BusError> {
        let mut bytes = [0; N];
        match address >> 29 {
            3 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                let set = ((address >> 4) & 63) as usize;
                let line = &self.lines[set][(self.control >> 6) as usize];
                let value =
                    line.tag | (u32::from(self.lru[set]) << 4) | (u32::from(line.valid) << 2);
                bytes.copy_from_slice(&value.to_be_bytes());
            }
            6 => {
                let offset = (address & 0xFFF) as usize;
                let line = &self.lines[(offset >> 4) & 63][offset >> 10];
                bytes.copy_from_slice(&line.data[offset & 15..(offset & 15) + N]);
            }
            _ => return Ok(None),
        }
        Ok(Some(bytes))
    }
    pub fn array_write<const N: usize>(
        &mut self,
        address: u32,
        bytes: [u8; N],
    ) -> Result<bool, BusError> {
        let (set, tag, offset) = Self::parts(address);
        match address >> 29 {
            2 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                for line in &mut self.lines[set] {
                    if line.tag == tag {
                        line.valid = false;
                    }
                }
            }
            3 => {
                if N != 4 {
                    return Err(BusError::UnsupportedDevice { address, width: N });
                }
                let value = u32::from_be_bytes(bytes.as_slice().try_into().unwrap());
                let line = &mut self.lines[set][(self.control >> 6) as usize];
                line.tag = tag;
                line.valid = address & 4 != 0;
                self.lru[set] = ((value >> 4) & 63) as u8;
            }
            6 => {
                let way = ((address >> 10) & 3) as usize;
                self.lines[set][way].data[offset..offset + N].copy_from_slice(&bytes);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
