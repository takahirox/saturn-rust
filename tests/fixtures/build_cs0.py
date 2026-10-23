#!/usr/bin/env python3
"""Original MIT CS0 word-bus/BSTROM diagnostic, SH7604 §7.8 figs7.44–46.

Both CPUs actually read ROM and low RAM, compute their records and synchronize
through low RAM. A warmed cache isolates sixteen ROM + sixteen low-RAM MOV.L
reads timed by FRT. No host calculation is embedded in the guest's results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def program(master):
    mailbox = 0x2601C000 if master else 0x2601C100
    ram = 0x20210000 if master else 0x20211000
    s = f'load {mailbox:#x},r8\n'
    s += store(0xFFFFFFE0, 0xA55A0BF0) + store(0xFFFFFFE8, 0xA55A0001)
    s += store(ram, 0x13579BDF)
    s += f'load {ram:#x},r11\nmov.l @r11,r0\n'  # drain the buffered store
    s += ('load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0x2001f000,r10\nload 0x8000,r12\n')
    # The first call warms every instruction line. The second has no refill.
    s += 'jsr @r12\nnop\njsr @r12\nnop\n' + record()
    s += record(6) + record(9) + 'xor r9,r6\n' + record(6)
    if master:
        s += store(0x20212000, 0x51) + smpc(2)
        # Stay on local cached instructions during the slave's FRT interval.
        s += 'load 0x1000,r3\nquiet:\ndt r3\nbf quiet\n'
        s += ('load 0x20212000,r1\nload 0xa3,r2\n'
              'sync:\nmov.l @r1,r0\ncmp/eq r2,r0\nbf sync\n')
    else:
        s += 'load 0x20212000,r1\nmov.l @r1,r0\n' + record()
        s += store(0x20212000, 0xA3)
    s += 'load 0x600d,r0\n' + record()
    return s + 'done:\nsleep\nbra done\nnop\n'


def build():
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    entry = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n'
             'load 0x800,r1\njmp @r1\nnop\nslave:\nload 0x2800,r1\njmp @r1\nnop\n')
    timed = ('mov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
             'mov.b @r4,r0\nmov.b @r5,r0\n' +
             'mov.l @r10,r6\n'*16 + 'mov.l @r11,r9\n'*16 +
             'mov.b @r4,r0\nmov.b @r5,r0\nextu.b r0,r0\nrts\nnop\n')
    result = {}
    for at, source in [(0x200, entry), (0x800, program(True)),
                       (0x2800, program(False)), (0x8000, timed)]:
        code = assembler.assemble(source, base=at)
        assert len(code) < 0x1800
        rom[at:at+len(code)] = code
        result[f'cs0-{at:x}.s'] = ('; Original MIT CS0 guest; adjacent LICENSE.\n'+source).encode()
    struct.pack_into('>I', rom, 0x1F000, 0x11223344)
    lines = ['# Original CS0 word-bus/BSTROM BIOS; MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    result['cs0.hex'] = ('\n'.join(lines)+'\n').encode()
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for name, data in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
