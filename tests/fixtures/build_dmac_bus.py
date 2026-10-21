#!/usr/bin/env python3
"""Original MIT dual-SH-2/on-chip DMA bus guest; H §§9.1.1/9.3.4–9.3.8.

Each CPU runs its own DMAC through short/overlapping 16-byte, longword,
byte/word decrement and burst transfers, handling and acknowledging five IRQs.
The source-authored oracle is independent of the emulator.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc

HERE = Path(__file__).resolve().parent


def program(data, mailbox):
    s = f'load {mailbox:#x},r8\n'
    s += store(0xFFFFFFE0, 0xA55A0000) + store(0xFFFFFFE8, 0xA55A0080)
    s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'  # enable/purge cache
    s += 'load 0xfffffee2,r1\nload 0x600,r0\nmov.w r0,@r1\n'
    s += store(0xFFFFFFA0, 0x44)
    s += 'mov #0,r0\nldc r0,sr\n'
    for i, value in enumerate([0x11223344, 0x55667788, 0x99AABBCC, 0xDDEEFF00, 0x13579BDF]):
        s += store(data + i * 4, value)
    record = mailbox + 8
    # A final 16-byte unit with TCR=3 still reads four source longwords.
    # Source/destination overlap by one: interleaving R/W corrupts the result.
    for case, (source, dest, count, control, readback) in enumerate([
        (data, data + 4, 3, 0x5E05, [data + 4, data + 8, data + 12, data + 16]),
        (data, data + 0x100, 2, 0x4A05, [data + 0x100, data + 0x104]),
        (data + 3, data + 0x110, 4, 0x6205, [data + 0x110]),
        (data + 2, data + 0x120, 2, 0x6605, [data + 0x120]),
        (data, data + 0x130, 2, 0x5A15, [data + 0x130, data + 0x134]),
    ], 1):
        for offset, value in [(0, source), (4, dest), (8, count), (12, control), (0x30, 1)]:
            s += store(0xFFFFFF80 + offset, value)
        s += (f'wait{case}:\nmov.l @r8,r0\nmov #{case},r2\ncmp/eq r2,r0\nbf wait{case}\n')
        for address in readback:
            s += f'load {address:#x},r1\nmov.l @r1,r0\nload {record:#x},r1\nmov.l r0,@r1\n'
            record += 4
    # Each CPU must observe its peer's completed device work before sleeping.
    s += store(mailbox + 4, 0x600D)
    peer = 0x2601C104 if mailbox == 0x2601C000 else 0x2601C004
    s += (f'load {peer:#x},r1\nload 0x600d,r2\npeer:\nmov.l @r1,r0\ncmp/eq r2,r0\nbf peer\n'
          'done:\nsleep\nbra done\nnop\n')
    return s


def build():
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    struct.pack_into('>I', rom, 0x44 * 4, 0x3000)
    entry = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n')
    entry += smpc(2) + 'load 0x800,r1\njmp @r1\nnop\n'
    entry += 'slave:\nload 0x060fe000,r15\nload 0x1800,r1\njmp @r1\nnop\n'
    handler = ('mov.l r0,@-r15\nmov.l r1,@-r15\n'
               'load 0xffffff8c,r1\nmov.l @r1,r0\nmov #0,r0\nmov.l r0,@r1\n'
               'mov.l @r8,r0\nadd #1,r0\nmov.l r0,@r8\n'
               'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n')
    for address, source in [(0x200, entry), (0x800, program(0x2601A000, 0x2601C000)),
                            (0x1800, program(0x2601B000, 0x2601C100)), (0x3000, handler)]:
        code = assembler.assemble(source, base=address)
        assert len(code) < (0x1000 if address in [0x800, 0x1800] else 0x600)
        rom[address:address + len(code)] = code
    lines = ['# Original dual-SH-2 on-chip DMAC BIOS; MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{word:04X}' for word in struct.unpack('>8H', rom[at:at + 16])))
    # Literal architectural results, not values measured from an emulator.
    golden = [5, 0x600D, 0x11223344, 0x55667788, 0x99AABBCC, 0x13579BDF,
              0x11223344, 0x11223344, 0x44332211, 0x33441122, 0x11223344, 0x11223344]
    return {'dmac-bus.hex': ('\n'.join(lines) + '\n').encode(),
            'dmac-bus.tsv': ''.join(f'{word:08X}\n' for word in golden).encode()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, content in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
