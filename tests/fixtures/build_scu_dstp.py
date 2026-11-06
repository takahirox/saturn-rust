#!/usr/bin/env python3
"""Original MIT legacy-DSTP diagnostics; no Sega firmware or copied emulator code.

Final SCU compatibility policy: +60 longword read=0, writes ignored. The cache
negative control loads +6C, generating +60 before failing honestly at +64.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record

HERE = Path(__file__).resolve().parent


def build():
    result = {}
    for name in ['access', 'refill', 'read', 'write']:
        sections = {}
        s = 'load 0x2600d000,r8\n'
        expected = []
        if name == 'access':
            # P1 execution prevents unrelated instruction refills. CE=0 covers
            # ordinary P0 alias; CE=1/CD=1 tests explicit data-fill disable.
            for ccr in [0, 0x15]:
                s += f'load 0xfffffe92,r1\nmov #{ccr},r0\nmov.b r0,@r1\n'
                for address in [0x05fe0060, 0x25fe0060]:
                    for value in [0, 1, 0xffffffff, 0xa55a1234]:
                        s += store(address, value)
                        s += f'load {address:#x},r1\nmov.l @r1,r0\n' + record()
                        expected.append(0)
            # Each DMA completion must still deliver the real handler and data.
            s += 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\nmov #0,r0\nldc r0,sr\n' + store(0x26008000, 0x12345678)
            for channel in range(3):
                s += 'mov #0,r10\n'
                s += store(0x25fe00a0, 0xbfff & ~(1 << (11-channel)))
                for off, value in [(0, 0x06008000), (4, 0x06009000+channel*4),
                                   (8, 4), (12, 0x102), (20, 7), (16, 0x101)]:
                    s += store(0x25fe0000+channel*32+off, value)
                s += store(0x25fe0060, 1)
                s += (f'wait_{channel}:\nmov r10,r0\ntst r0,r0\nbt wait_{channel}\n'
                      f'load {0x26009000+channel*4:#x},r1\nmov.l @r1,r0\n') + record() + record(10)
                expected += [0x12345678, 1]
            s += 'load 0x600d,r0\n' + record()
            expected.append(0x600d)
        elif name == 'refill':
            s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\nload 0x05fe006c,r1\nmov.l @r1,r0\n'
        elif name == 'read':
            s += 'load 0x25fe0060,r1\nmov.l @r1,r0\n' + record()
            expected.append(0)
        else:
            s += ('load 0x25fe0060,r1\nload 0xa55a1234,r0\nmov #100,r2\n'
                  'repeat:\nmov.l r0,@r1\ndt r2\nbf repeat\n')
            s += 'load 0x600d,r0\n' + record()
            expected.append(0x600d)
        s += 'done:\nsleep\nbra done\nnop\n'
        sections[0x200] = s
        sections[0x1800] = 'add #1,r10\nrte\nnop\n'
        rom = bytearray(524288)
        struct.pack_into('>II', rom, 0, 0x20000200, 0x260ff000)
        for vector in [0x49, 0x4a, 0x4b]:
            struct.pack_into('>I', rom, vector*4, 0x20001800)
        for at, source in sections.items():
            data = assembler.assemble(source, base=at)
            assert at != 0x200 or len(data) < 0x1600
            rom[at:at+len(data)] = data
        lines = ['# Original MIT legacy DSTP firmware; adjacent LICENSE.']
        for at in range(0, len(rom), 16):
            if any(rom[at:at+16]):
                lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
        result[f'scu-dstp-{name}.hex'] = ('\n'.join(lines)+'\n').encode()
        result[f'scu-dstp-{name}.asm'] = ''.join(f'; physical section {at:08X}\n{source}' for at, source in sections.items()).encode()
        if expected:
            result[f'scu-dstp-{name}.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
