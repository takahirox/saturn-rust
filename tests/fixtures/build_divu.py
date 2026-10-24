#!/usr/bin/env python3
"""Original MIT dual-SH-2 DIVU/UBC guest, H §§6.3.7/10.2–10.4.

Literal expected partial values are authored from three division steps, never
obtained from the emulator. No SDK, third-party core or proprietary firmware.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc
from build_clock import read, put16

HERE = Path(__file__).resolve().parent
# width, high, low, divisor, partial quotient, partial remainder, saturated
OVERFLOW = [
    (32, 0, 0, 0, 7, 0, 0x7fffffff),
    (32, 0, 1, 0, 15, 0, 0x7fffffff),
    (32, 0xffffffff, 0xffffffff, 0, 0xfffffff8, 0xffffffff, 0x80000000),
    (32, 0xffffffff, 0x80000000, 0xffffffff, 3, 0xffffffff, 0x7fffffff),
    (64, 2, 0, 1, 7, 9, 0x7fffffff),
    (64, 0xffffffff, 0, 1, 0, 0xffffffff, 0x80000000),
    (64, 1, 0, 0xffffffff, 0, 1, 0x80000000),
    (64, 0xffffffff, 1, 0xfffffffe, 11, 0xfffffffe, 0x7fffffff),
    (64, 0x12345678, 0x9abcdef0, 0, 0xd5e6f787, 0x91a2b3c4, 0x7fffffff),
]
NORMAL = [
    (32, 0, 100, 7, 14, 2),
    (32, 0xffffffff, 0xffffff9c, 7, 0xfffffff2, 0xfffffffe),
    (32, 0, 100, 0xfffffff9, 0xfffffff2, 2),
    (32, 0xffffffff, 0xffffff9c, 0xfffffff9, 14, 0xfffffffe),
    (64, 1, 0, 3, 0x55555555, 1),
    (64, 0xffffffff, 0, 3, 0xaaaaaaab, 0xffffffff),
    (64, 1, 0, 0xfffffffd, 0xaaaaaaab, 1),
    (64, 0xffffffff, 0, 0xfffffffd, 0x55555555, 0xffffffff),
]


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\n'
         'mov r9,r0\ntst r2,r0\nbt master\n'
         'load 0x26013000,r8\nload 0x26014004,r10\nload 0x2601400c,r11\n'
         'load 0x060fe000,r15\nbra ready\nnop\n'
         'master:\nload 0x26012000,r8\nload 0x26014000,r10\nload 0x26014008,r11\nready:\n'
         'mov #0,r0\nmov.l r0,@r10\n')
    s += put16(0xfffffee2, 0xa000) + put16(0xffffff0e, 0x45)
    expected = []
    for control in [0, 2]:
        for width, high, low, divisor, partial, remainder, saturated in OVERFLOW:
            s += put16(0xffffff0a, control) + store(0xffffff00, divisor)
            if width == 64:
                s += store(0xffffff10, high)
            s += store(0xffffff14 if width == 64 else 0xffffff04, low)
            s += read(0xffffff04) + read(0xffffff10) + read(0xffffff14)
            s += read(0xffffff08) + read(0xffffff00) + read(0xffffff0e, 2)
            quotient = partial if control else saturated
            expected += [quotient, remainder, quotient, control | 1, divisor, 0x45]
        for width, high, low, divisor, quotient, remainder in NORMAL:
            # A successful divide leaves an uncleared overflow sticky.
            s += put16(0xffffff0a, control | 1) + store(0xffffff00, divisor)
            if width == 64:
                s += store(0xffffff10, high)
            s += store(0xffffff14 if width == 64 else 0xffffff04, low)
            s += read(0xffffff04) + read(0xffffff10) + read(0xffffff08)
            expected += [quotient, remainder, control | 1]
    # Keep unused DCC raster inputs masked while accepting DIVU level10.
    s += put16(0xffffff0a, 2) + 'mov #96,r0\nldc r0,sr\n'
    s += store(0xffffff00, 0) + store(0xffffff04, 1)
    s += ('wait_irq:\nmov.l @r10,r0\ntst r0,r0\nbt wait_irq\n'
          'mov #-16,r0\nldc r0,sr\n')
    s += read(0xffffff04) + read(0xffffff10) + read(0xffffff08)
    s += 'mov.l @r10,r0\n' + record()
    expected += [15, 0, 2, 1]
    # Both CPUs program their own UBC. The documented final-register read
    # synchronizes configuration; the real data write triggers vector 12.
    s += ('load 0xffffff40,r1\nmov.l r11,@r1\n')
    s += put16(0xffffff48, 0x6b) + put16(0xffffff78, 0x1000)
    s += read(0xffffff78, 2)
    s += ('mov #96,r0\nldc r0,sr\nload 0xaa,r0\nmov.l r0,@r11\n'
          'mov #-16,r0\nldc r0,sr\nmov.l @r11,r0\n') + record()
    s += read(0xffffff78, 2)
    s += 'load 0x600d,r0\n' + record()
    expected += [0x1000, 0xab, 0x9000, 0x600d]
    s += 'load 0x8000,r2\nmov r9,r0\ntst r2,r0\nbf done\n' + smpc(2)
    s += 'done:\nsleep\nbra done\nnop\n'
    # ISR acknowledges the DIVU flag itself, preserving all caller registers.
    irq = ('add #-8,r15\nmov.l r0,@r15\nadd #4,r15\nmov.l r1,@r15\n'
           'load 0xffffff0a,r1\nmov #2,r0\nmov.w r0,@r1\n'
           'mov.l @r10,r0\nadd #1,r0\nmov.l r0,@r10\n'
           'mov.l @r15,r1\nadd #-4,r15\nmov.l @r15,r0\nadd #8,r15\nrte\nnop\n')
    ubc = ('add #-8,r15\nmov.l r0,@r15\nadd #4,r15\nmov.l r1,@r15\n'
           'load 0xffffff48,r1\nmov #0,r0\nmov.w r0,@r1\nmov.w @r1,r0\n'
           'mov.l @r11,r0\nadd #1,r0\nmov.l r0,@r11\n'
           'mov.l @r15,r1\nadd #-4,r15\nmov.l @r15,r0\nadd #8,r15\nrte\nnop\n')
    code = assembler.assemble(s, base=0x200)
    assert len(code) < 0x1fe00
    chunks = [(0, struct.pack('>IIII', 0x200, 0x060ff000, 0x200, 0x060fe000)),
              (0x45*4, struct.pack('>I', 0x20000)), (0x200, code),
              (0x20000, assembler.assemble(irq, base=0x20000)),
              (12*4, struct.pack('>I', 0x22000)),
              (0x22000, assembler.assemble(ubc, base=0x22000))]
    text = ['# Original dual-SH-2 DIVU diagnostic. MIT: adjacent LICENSE.']
    for address, chunk in chunks:
        for at in range(0, len(chunk), 16):
            words = struct.unpack('>'+'H'*(len(chunk[at:at+16])//2), chunk[at:at+16])
            text.append(f'{address+at:04X} '+' '.join(f'{w:04X}' for w in words))
    return {'divu.hex': ('\n'.join(text)+'\n').encode(),
            'divu.tsv': ''.join(f'{v:08X}\n' for v in expected).encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); args = p.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check: assert path.read_bytes() == data, f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
