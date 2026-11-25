//! Owned renderer command transport. Metadata is little endian; guest memory
//! payload bytes retain their original order. No pointers cross an instance.
use super::*;

const MAGIC: &[u8; 4] = b"SRV1";
const MAX_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_CLOCKS: u32 = 2_000_000;

fn number(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn data(out: &mut Vec<u8>, bytes: &[u8]) {
    number(
        out,
        u32::try_from(bytes.len()).expect("bounded renderer payload"),
    );
    out.extend_from_slice(bytes);
}
fn patches(out: &mut Vec<u8>, items: Vec<FrontPatch>) {
    number(
        out,
        u32::try_from(items.len()).expect("bounded front changes"),
    );
    for item in items {
        number(out, item.offset as u32);
        data(out, &item.bytes);
    }
}
fn surface(out: &mut Vec<u8>, image: DisplaySurface) {
    number(out, u32::from(image.mode));
    data(out, &image.bytes);
}

pub(super) fn encode(journal: &mut Journal) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    while let Some(command) = journal.commands.pop_front() {
        match command {
            Command::Advance {
                clocks,
                before,
                endpoint,
            } => {
                out.push(0);
                number(&mut out, clocks);
                patches(&mut out, before);
                match endpoint {
                    Endpoint::None => out.push(0),
                    Endpoint::HBlankErase(items) => {
                        out.push(1);
                        patches(&mut out, items);
                    }
                    Endpoint::VBlankSurface(image) => {
                        out.push(2);
                        surface(&mut out, image);
                    }
                }
            }
            Command::Vram { offset, bytes } => {
                out.push(1);
                number(&mut out, offset as u32);
                data(&mut out, &bytes);
            }
            Command::Cram { offset, bytes } => {
                out.push(2);
                number(&mut out, offset as u32);
                data(&mut out, &bytes);
            }
            Command::Register { address, value } => {
                out.push(3);
                number(&mut out, address);
                number(&mut out, u32::from(value));
            }
            Command::FrontMode(mode) => {
                out.push(4);
                number(&mut out, u32::from(mode));
            }
            Command::FrontSurface(image) => {
                out.push(5);
                surface(&mut out, image);
            }
            Command::Pal(pal) => {
                out.push(6);
                out.push(u8::from(pal));
            }
            Command::Clock(fast) => {
                out.push(7);
                out.push(u8::from(fast));
            }
            Command::Reset { pal } => {
                out.push(8);
                out.push(u8::from(pal));
            }
            Command::Flush => out.push(9),
            Command::LineRendering(enabled) => {
                out.push(10);
                out.push(u8::from(enabled));
            }
        }
    }
    assert!(
        out.len() <= MAX_BYTES,
        "renderer chunk exceeds transport budget"
    );
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or("truncated video command")?;
        let bytes = &self.bytes[self.at..end];
        self.at = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn number(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn boolean(&mut self) -> Result<bool, String> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("invalid video flag".into()),
        }
    }
    fn word(&mut self) -> Result<u16, String> {
        u16::try_from(self.number()?).map_err(|_| "invalid video word".into())
    }
    fn data(&mut self, offset: usize, limit: usize) -> Result<Vec<u8>, String> {
        let n = self.number()? as usize;
        if offset.checked_add(n).is_none_or(|end| end > limit) {
            return Err("video memory span out of range".into());
        }
        Ok(self.take(n)?.to_vec())
    }
    fn patches(&mut self) -> Result<Vec<FrontPatch>, String> {
        let n = self.number()? as usize;
        if n > self.bytes.len().saturating_sub(self.at) / 8 {
            return Err("invalid front change count".into());
        }
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            let offset = self.number()? as usize;
            items.push(FrontPatch {
                offset,
                bytes: self.data(offset, crate::vdp1::BUFFER_SIZE)?,
            });
        }
        Ok(items)
    }
    fn surface(&mut self) -> Result<DisplaySurface, String> {
        let mode = self.word()?;
        Vdp1::validate_write(crate::vdp1::REGISTERS, mode).map_err(|_| "invalid front mode")?;
        let bytes = self.data(0, crate::vdp1::BUFFER_SIZE)?;
        if bytes.len() != crate::vdp1::BUFFER_SIZE {
            return Err("incomplete front surface".into());
        }
        Ok(DisplaySurface { mode, bytes })
    }
}

pub(super) fn replay(renderer: &mut Renderer, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_BYTES || !bytes.starts_with(MAGIC) {
        return Err("invalid video command envelope".into());
    }
    let mut input = Reader { bytes, at: 4 };
    let mut commands = Vec::new();
    let mut clocks = 0u32;
    // Parse and validate the whole chunk before changing renderer state.
    while input.at < bytes.len() {
        let command = match input.byte()? {
            0 => {
                let delta = input.number()?;
                clocks = clocks
                    .checked_add(delta)
                    .filter(|&n| n <= MAX_CLOCKS)
                    .ok_or("video clock span out of range")?;
                if delta == 0 {
                    return Err("empty video clock span".into());
                }
                let before = input.patches()?;
                let endpoint = match input.byte()? {
                    0 => Endpoint::None,
                    1 => Endpoint::HBlankErase(input.patches()?),
                    2 => Endpoint::VBlankSurface(input.surface()?),
                    _ => return Err("invalid video endpoint".into()),
                };
                Command::Advance {
                    clocks: delta,
                    before,
                    endpoint,
                }
            }
            1 => {
                let offset = input.number()? as usize;
                Command::Vram {
                    offset,
                    bytes: input.data(offset, 0x80000)?,
                }
            }
            2 => {
                let offset = input.number()? as usize;
                Command::Cram {
                    offset,
                    bytes: input.data(offset, 0x1000)?,
                }
            }
            3 => {
                let address = input.number()?;
                let value = input.word()?;
                if !(VDP2..VDP2 + 0x120).contains(&address) || address & 1 != 0 {
                    return Err("invalid video register".into());
                }
                Vdp2::validate_value(address, value).map_err(|_| "invalid video register value")?;
                Command::Register { address, value }
            }
            4 => {
                let mode = input.word()?;
                Vdp1::validate_write(crate::vdp1::REGISTERS, mode)
                    .map_err(|_| "invalid front mode")?;
                Command::FrontMode(mode)
            }
            5 => Command::FrontSurface(input.surface()?),
            6 => Command::Pal(input.boolean()?),
            7 => Command::Clock(input.boolean()?),
            8 => Command::Reset {
                pal: input.boolean()?,
            },
            9 => Command::Flush,
            10 => Command::LineRendering(input.boolean()?),
            _ => return Err("unknown video command".into()),
        };
        if commands.len() >= 262144 {
            return Err("too many video commands".into());
        }
        commands.push(command);
    }
    let mut raster = renderer.raster.clone();
    for command in &commands {
        match command {
            Command::Advance {
                clocks, endpoint, ..
            } => {
                let mut remaining = *clocks;
                let mut final_event = None;
                while remaining != 0 {
                    let delta = remaining.min(raster.clocks_to_event());
                    final_event = raster.advance(delta);
                    remaining -= delta;
                }
                let valid = match endpoint {
                    Endpoint::None => true,
                    Endpoint::HBlankErase(_) => final_event == Some(RasterEvent::HBlankIn),
                    Endpoint::VBlankSurface(_) => final_event == Some(RasterEvent::VBlankOut),
                };
                if !valid {
                    return Err("video endpoint does not match raster".into());
                }
            }
            Command::Register { address, value } if *address == VDP2 => raster.configure(*value),
            Command::Pal(pal) => raster.pal = *pal,
            Command::Clock(fast) => {
                raster.select_clock(*fast);
                raster.configure(0);
            }
            Command::Reset { pal } => {
                raster = Raster::default();
                raster.pal = *pal;
            }
            _ => (),
        }
    }
    for command in commands {
        renderer
            .apply(command)
            .map_err(|_| "invalid video command application")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_has_literal_little_endian_encoding_and_preserves_guest_bytes() {
        let mut journal = Journal::default();
        journal.push(Command::Vram {
            offset: 0x1234,
            bytes: vec![0xAB, 0xCD],
        });
        journal.push(Command::Register {
            address: VDP2,
            value: 0x8000,
        });
        let bytes = encode(&mut journal);
        assert_eq!(
            bytes,
            [
                b'S', b'R', b'V', b'1', 1, 0x34, 0x12, 0, 0, 2, 0, 0, 0, 0xAB, 0xCD, 3, 0, 0, 0xF8,
                5, 0, 0x80, 0, 0
            ]
        );
        assert!(journal.is_empty());
        let mut renderer = Renderer::default();
        renderer.replay_bytes(&bytes).unwrap();
        assert_eq!(&renderer.video.vram[0x1234..0x1236], &[0xAB, 0xCD]);
        assert_eq!(renderer.video.read16(VDP2).unwrap(), 0x8000);
    }
    #[test]
    fn malformed_trailing_payload_or_wrong_endpoint_cannot_publish_a_prefix() {
        let mut journal = Journal::default();
        journal.push(Command::Vram {
            offset: 0,
            bytes: vec![1, 2],
        });
        let mut bytes = encode(&mut journal);
        bytes.push(3);
        let mut renderer = Renderer::default();
        assert!(renderer.replay_bytes(&bytes).is_err());
        assert_eq!(&renderer.video.vram[..2], &[0, 0]);
        journal.push(Command::Advance {
            clocks: 1,
            before: Vec::new(),
            endpoint: Endpoint::HBlankErase(Vec::new()),
        });
        assert!(renderer.replay_bytes(&encode(&mut journal)).is_err());
        assert_eq!(renderer.raster.counters(), (0, 0));
    }
}
