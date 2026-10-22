#!/usr/bin/env python3
"""Original single-address-DMA request ROM. MIT: adjacent LICENSE.

No firmware, SDK objects or external assets. All MMIO/result writes are guest
instructions. Board-level DACK transfer vectors live in src/onchip/dma.rs.
"""
import argparse
import hashlib
from pathlib import Path
import struct
from build_sh7604 import assembler

HERE = Path(__file__).resolve().parent


def build():
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x06100000)
    code = assembler.assemble((HERE / 'dack.s').read_text(), base=0x200)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original DACK-request ROM; MIT: adjacent LICENSE.',
             '# Generated from dack.s by build_dack.py; unspecified bytes zero.']
    for at in range(0, len(rom), 16):
        chunk = rom[at:at+16]
        if any(chunk):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', chunk)))
    return '\n'.join(lines)+'\n', hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text, digest = build()
    target = HERE / 'dack.hex'
    if args.check:
        assert target.read_text() == text, 'Rebuild dack.hex'
    else:
        target.write_text(text)
    print(f'{"Verified" if args.check else "Built"} dack.hex; expanded ROM SHA-256 {digest}')
