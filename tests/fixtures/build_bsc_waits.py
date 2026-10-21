#!/usr/bin/env python3
"""Original MIT ordinary-bus wait diagnostic; SH7604 §§7.2/7.4.2 fig.7.11.

Both SH-2s execute the authored register writes and measure eight external reads
with their own FRT. No host timing observation, result write, SDK or firmware.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc
from build_clock import put8, put16, read

HERE = Path(__file__).resolve().parent


def control(value):
    # Preserve the reset-time ENDIAN choice while changing the permitted AnLW.
    return (f'load {value:#x},r0\nor r14,r0\n'
            'load 0xffffffe0,r1\nmov.l r0,@r1\n')


def raw_word():
    # Byte operands do not undergo endian conversion (figs.7.5/7.6), so guest
    # byte reads reconstruct the actual physical backing bytes in either mode.
    s = 'load 0x25a07000,r1\nmov #0,r6\n'
    for _ in range(4):
        s += 'shll8 r6\nmov.b @r1,r0\nextu.b r0,r0\nor r0,r6\nadd #1,r1\n'
    return s + record(6)


def build():
    # H §§7.2.1/7.2.2: establish endian and widths ONCE before accessing
    # anything outside CS0. Mutated ROM operands select separate cold boots;
    # the guest never relies on prohibited live width/endian changes.
    s = 'load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x1000,r14\n'
    s += store(0xFFFFFFE4, 0xA55A00FC) + control(0xA55A03F0)
    s += ('mov r9,r0\nload 0x2600e000,r8\nload 0x8000,r2\ntst r2,r0\nbt master\n'
         'load 0x2600e100,r8\nload 0x060fe000,r15\nmaster:\n')
    s += record() + 'load 0xffffffe8,r1\nmov.l @r1,r0\n' + record()
    s += store(0x2600D000, 0x13579BDF)
    for waits in range(7):
        # Fetches in CS0 have zero Tw; only the eight CS3 reads vary.
        bcr = 0x3F0 if waits < 3 else 0xF0 | ((waits-3) << 8)
        wcr = 0xAA00 | (min(waits, 3) << 6)
        s += control(0xA55A0000 | bcr)
        s += store(0xFFFFFFE8, 0xA55A0000 | wcr)
        s += ('load 0xfffffe12,r1\nload 0xfffffe13,r2\nload 0x2600d000,r4\n'
              'mov.b @r1,r0\nmov.b @r2,r5\nextu.b r5,r5\n')
        s += 'mov.l @r4,r0\n' * 8
        s += ('mov r0,r6\nmov.b @r1,r0\nmov.b @r2,r0\nextu.b r0,r0\n'
              'sub r5,r0\nextu.b r0,r0\n')
        s += record() + record(6)
    for i, byte in enumerate([0x11, 0x22, 0x33, 0x44]):
        s += put8(0x25A07000+i, byte)
    for address, size in [(0x25A07000, 4), (0x25A07000, 2), (0x25A07002, 2), (0x25A07000, 1)]:
        s += read(address, size)
    for write in [store(0x25A07000, 0xA1B2C3D4), put16(0x25A07000, 0x1234), put8(0x25A07000, 0x56)]:
        s += write + raw_word()
    s += 'load 0x600d,r0\n' + record()
    s += 'load 0x8000,r2\ntst r2,r9\nbf finished\n' + smpc(2)
    s += 'finished:\nsleep\nbra finished\nnop\n'
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original BSC software-wait BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    # The first FRC latch precedes twelve issue turns plus eight Tw groups:
    # 12*8+8*(IW0=2+wait) clocks -> 14+wait FRT ticks. Each CS0 fetch
    # precedes a CS3 read; H §7.9 requires two turnaround cycles. The gap
    # after each data read satisfies IW3 before the next fetch.
    # Source-authored, no emulator run.
    expected = [14+w for w in range(7)]
    return {'bsc-waits.hex': ('\n'.join(lines)+'\n').encode(),
            'bsc-waits.tsv': ''.join(f'{v:08X}\n' for v in expected).encode(),
            'bsc-endian.tsv': ''.join(f'{v:08X}\n' for v in
                [0x44332211, 0x2211, 0x4433, 0x11, 0xD4C3B2A1, 0x3412B2A1, 0x5612B2A1]).encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); args = p.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check: assert path.read_bytes() == data, f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
