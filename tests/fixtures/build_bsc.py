#!/usr/bin/env python3
"""Build original SH7604 BSC/module-stop diagnostic ROM text. MIT: adjacent LICENSE.

Uses only the project's original assembler, no SDK, firmware or compiler core.
Exercises processor identification, refresh IRQ/RTE and FRT/SCI module stops.
All control and result writes execute as guest SH-2 instructions.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('assembler', HERE.parents[1] / 'homebrew/star-collector/build.py')
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)


def build():
    sections = {}
    for line in (HERE / 'bsc.s').read_text().splitlines():
        if line.startswith('.section '):
            address = int(line.split()[1], 0)
            sections[address] = []
        elif sections:
            sections[address].append(line)
    chunks = [(0, struct.pack('>IIII', 0x200, 0x06100000, 0x200, 0x06100000)),
              (0x50 * 4, struct.pack('>I', 0x1800))]
    for address, source in sections.items():
        chunks.append((address, assembler.assemble('\n'.join(source), base=address)))
    chunks.sort()
    end = 0
    text = ['# Original SH7604 BSC/module-stop BIOS fixture. MIT: adjacent LICENSE.',
            '# Generated from bsc.s by build_bsc.py; unspecified bytes zero.']
    rom = bytearray(524288)
    for address, chunk in chunks:
        assert address >= end and address + len(chunk) <= len(rom)
        end = address + len(chunk)
        rom[address:end] = chunk
        for offset in range(0, len(chunk), 16):
            words = struct.unpack('>' + 'H' * (len(chunk[offset:offset+16]) // 2), chunk[offset:offset+16])
            text.append(f'{address+offset:04X} ' + ' '.join(f'{word:04X}' for word in words))
    return '\n'.join(text) + '\n', hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text, digest = build()
    path = HERE / 'bsc.hex'
    if args.check:
        assert path.read_text() == text, 'Rebuild bsc.hex'
    else:
        path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} bsc.hex; expanded ROM SHA-256 {digest}')
