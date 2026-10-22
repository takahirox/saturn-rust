//! ST-058-R2 §3.3 tables3.2–3.5: name, character and cell-scroll
//! accesses use independent display paths. A fetched physical word is held
//! while its packed dots are consumed, independently of later CPU VRAM writes.
use super::Vdp2;

type Words = [Option<(usize, u16)>; 2];

// Functional two-word serializers, sampled on first display use. This retains
// packed data across beam flushes; it does not prefetch future cycle slots.
#[derive(Default)]
pub(super) struct DisplayFetch {
    words: [[Words; 3]; 5],
}

impl Vdp2 {
    pub(super) fn display_word(&self, layer: usize, path: usize, address: usize) -> u16 {
        if !self.beam_rendering {
            return self.vword(address);
        }
        let address = address & 0x7FFFE;
        let mut fetch = self.display_fetch.borrow_mut();
        let word = &mut fetch.words[layer][path][(address >> 1) & 1];
        if let Some((old, value)) = *word
            && old == address
        {
            return value;
        }
        let value = self.vword(address);
        *word = Some((address, value));
        value
    }

    pub(super) fn display_byte(&self, layer: usize, address: usize) -> u8 {
        (self.display_word(layer, 1, address) >> ((1 - (address & 1)) * 8)) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{devices::VDP2, vdp1::Vdp1};

    fn setup(format: u16) -> Vdp2 {
        let mut v = Vdp2::default();
        for (at, value) in [
            (0, 0x8000),
            (0x10, 0x4444),
            (0x12, 0xEEEE),
            (0x20, 0x101),
            (0x28, 2 | format << 4),
            (0x78, 1),
            (0x7C, 1),
            (0xF8, 1),
        ] {
            v.write16(VDP2 + at, value).unwrap();
        }
        for (index, value) in [(1, 0x801Fu16), (2, 0x83E0), (3, 0xFC00)] {
            v.cram[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
        v
    }
    fn pixel(v: &Vdp2, x: usize, y: usize) -> [u8; 4] {
        let at = (y * 320 + x) * 4;
        v.scanning[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn packed_dots_retain_the_fetched_word_but_an_unfetched_word_and_next_line_see_writes() {
        for (format, old, new, dots) in [(0, 0x1111u16, 0x2222u16, 4), (1, 0x0101, 0x0202, 2)] {
            for captured in 1..dots {
                let mut v = setup(format);
                v.vram[..4].copy_from_slice(&[old.to_be_bytes(), old.to_be_bytes()].concat());
                v.begin_scanline(0);
                v.scan_to(&Vdp1::default(), 0, captured);
                v.vram[..4].copy_from_slice(&[new.to_be_bytes(), new.to_be_bytes()].concat());
                v.scan_to(&Vdp1::default(), 0, dots * 2);
                for x in 0..dots * 2 {
                    assert_eq!(
                        pixel(&v, x, 0),
                        if x < dots {
                            [255, 0, 0, 255]
                        } else {
                            [0, 255, 0, 255]
                        }
                    );
                }
                // 256x vertical enlargement: line1 reads source row0 anew.
                v.write16(VDP2 + 0x7C, 0).unwrap();
                v.write16(VDP2 + 0x7E, 0x100).unwrap();
                v.begin_scanline(1);
                v.scan_to(&Vdp1::default(), 1, dots);
                assert_eq!(pixel(&v, 0, 1), [0, 255, 0, 255]);
                // Debugger snapshots inspect current memory without changing
                // or invalidating the beam's retained display words.
                assert_eq!(&v.framebuffer().unwrap()[..4], [0, 255, 0, 255]);
            }
        }
    }

    #[test]
    fn palette_11_bit_and_direct_16_32_bit_words_retain_complete_zoomed_pixels() {
        for (format, old, new, color) in [
            (2, [1u16, 0], [2u16, 0], [255, 0, 0, 255]),
            (3, [0x801F, 0], [0x83E0, 0], [255, 0, 0, 255]),
            (4, [0x80FF, 0], [0x8000, 0xFF00], [0, 0, 255, 255]),
        ] {
            let mut v = setup(format);
            for (at, value) in [
                (0x12, 0x4444),
                (0x78, 0),
                (0x7A, 0x100),
                (0x7C, 0),
                (0x7E, 0x100),
            ] {
                v.write16(VDP2 + at, value).unwrap();
            }
            v.vram[..4].copy_from_slice(&[old[0].to_be_bytes(), old[1].to_be_bytes()].concat());
            v.begin_scanline(0);
            v.scan_to(&Vdp1::default(), 0, 1);
            v.vram[..4].copy_from_slice(&[new[0].to_be_bytes(), new[1].to_be_bytes()].concat());
            v.scan_to(&Vdp1::default(), 0, 4);
            for x in 0..4 {
                assert_eq!(pixel(&v, x, 0), color, "format={format}, x={x}");
            }
            v.begin_scanline(1);
            v.scan_to(&Vdp1::default(), 1, 4);
            for x in 0..4 {
                assert_eq!(pixel(&v, x, 1), [0, 255, 0, 255], "format={format}, x={x}");
            }
        }
    }

    #[test]
    fn name_character_cell_and_layer_paths_cannot_replace_each_others_latches() {
        let mut v = Vdp2 {
            beam_rendering: true,
            ..Vdp2::default()
        };
        v.vram[..4].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(v.display_word(0, 0, 0), 0x1234);
        v.vram[..2].copy_from_slice(&[0xAB, 0xCD]);
        assert_eq!(v.display_word(0, 1, 0), 0xABCD);
        assert_eq!(v.display_word(1, 0, 0), 0xABCD);
        v.vram[..2].fill(0);
        assert_eq!(v.display_word(0, 2, 0), 0);
        assert_eq!(v.display_word(0, 0, 0), 0x1234);
        assert_eq!(v.display_word(0, 1, 0), 0xABCD);
        assert_eq!(v.display_word(1, 0, 0), 0xABCD);
        assert_eq!(v.display_word(0, 0, 2), 0x5678);
        assert_eq!(v.display_word(0, 0, 0x80000), 0x1234);
        v.begin_scanline(0);
        v.beam_rendering = true;
        assert_eq!(v.display_word(0, 0, 0), 0);
    }
}
