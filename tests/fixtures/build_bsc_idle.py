#!/usr/bin/env python3
"""Original MIT SH7604 intercycle-turnaround BIOS; §§7.2.3/7.9 fig.7.47.

Both CPUs execute a warmed, instruction-cached TAS loop against cache-through
RAM. FRT measures real read/write turnaround. No SDK, firmware or host results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x2600e200,r8\n'
         'load 0x8000,r2\ntst r2,r9\nbt master\n'
         'load 0x2600e300,r8\nmaster:\n')
    s += 'load 0xffffffe8,r1\nmov.l @r1,r0\n' + record()
    s += store(0xFFFFFFE8, 0xA55A0000)
    s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
    s += 'load 0x2600d000,r4\nload 0x25,r0\nmov.b r0,@r4\n'
    s += 'load 0xfffffe12,r1\nload 0xfffffe13,r2\nload 0x4000,r3\n'
    # Warm all four instruction-cache lines once. Then every timed fetch hits,
    # leaving precisely the sixteen actual TAS read/write pairs on the bus.
    s += 'jsr @r3\nnop\n'
    for idle in range(3):
        s += store(0xFFFFFFE8, 0xA55A0000 | idle << 14)
        # store() uses r1; restore the FRT high-byte address.
        s += 'load 0xfffffe12,r1\njsr @r3\nnop\n' + record()
    s += 'mov.b @r4,r0\nextu.b r0,r0\n' + record()
    s += 'load 0x600d,r0\n' + record()
    s += 'load 0x8000,r2\ntst r2,r9\nbf finished\n' + smpc(2)
    s += 'finished:\nsleep\nbra finished\nnop\n'
    timed = ('mov.b @r1,r0\nmov.b @r2,r5\nextu.b r5,r5\n'
             + 'tas.b @r4\n' * 16
             + 'mov.b @r1,r0\nmov.b @r2,r0\nextu.b r0,r0\n'
               'sub r5,r0\nextu.b r0,r0\nrts\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x20000200, 0x060FF000, 0x20000200, 0x060FE000)
    for at, base, source in [(0x200, 0x20000200, s), (0x4000, 0x4000, timed)]:
        code = assembler.assemble(source, base=base)
        rom[at:at+len(code)] = code
    lines = ['# Original BSC intercycle-idle BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    # Between FRC high-byte latches: 19 issue turns + 16 read/write gaps.
    # (19*8 + 16*max(1,IW3))/8 = 21/21/23 FRT ticks. Source-authored.
    return {'bsc-idle.hex': ('\n'.join(lines)+'\n').encode(),
            'bsc-idle.tsv': b'0000AAFF\n00000015\n00000015\n00000017\n000000A5\n0000600D\n'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
