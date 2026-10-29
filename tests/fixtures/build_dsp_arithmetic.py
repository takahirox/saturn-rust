#!/usr/bin/env python3
"""Original MIT SH-2/SCU DSP arithmetic diagnostic; adjacent LICENSE.

ST-097 §4.5, SOA #8 §§3/5.1 and ST-240-B-SP1 printed p.5 (Bugs).
V read-clear: ST-210-110194 item 36, printed p.10.
Golden records are literal, source-authored arithmetic, never emulator captures.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_dsp_dma import data, start
from build_hle_system import record

HERE = Path(__file__).resolve().parent
# Reset counters, load sign-extended A/P, then use parallel ALU -> A / ALL -> MC2.
LOAD = [0x00001C00, 0x00001D00, 0x00001E00, 0x00001F00, 0x01960000]
# RX=+/-65536, RY=2^30; P=MUL, A=0 gives +/-2^46 through real instructions.
WIDE = [0x00001C00, 0x00001D00, 0x00001E00, 0x00001F00,
        0x02084000, 0x01020000]
# label, A/RX, P/RY, instructions, low result, upper32 result, S/Z/C/V.
CASES = [
    ('add_overflow', None, 1, LOAD+[0x10043209], 0x80000000, 0x00008000, 0x480000),
    ('add_replace', None, 1, LOAD+[0x10040000, 0x10043209], 0x80000001, 0x00008000, 0x400000),
    ('sub_positive', 0x7fffffff, 0xffffffff, LOAD+[0x14043209], 0x80000000, 0x00008000, 0x580000),
    ('sub_positive_replace', 0x7fffffff, 0xffffffff, LOAD+[0x14040000, 0x14043209], 0x80000001, 0x00008000, 0x500000),
    ('sub_negative', 0x80000000, 1, LOAD+[0x14043209], 0x7fffffff, 0xffff7fff, 0x080000),
    ('sub_negative_replace', 0x80000000, 1, LOAD+[0x14040000, 0x14043209], 0x7ffffffe, 0xffff7fff, 0),
    ('ad2_positive', 0x10000, 0x40000000, WIDE+[0x18040000, 0x18043209], 0, 0x80000000, 0x480000),
    ('ad2_positive_replace', 0x10000, 0x40000000, WIDE+[0x18040000, 0x18040000, 0x18043209], 0, 0xc0000000, 0x400000),
    ('ad2_negative', 0xffff0000, 0x40000000, WIDE+[0x18040000, 0x18040000, 0x18043209], 0, 0x40000000, 0x180000),
    ('ad2_negative_replace', 0xffff0000, 0x40000000, WIDE+[0x18040000]*3+[0x18043209], 0, 0, 0x300000),
    # Without MOV ALU,A the stored ALU result and flags still reflect ADD.
    ('parallel_no_writeback', 0x7fffffff, 1, LOAD+[0x10003209], 0x80000000, 0x00007fff, 0x480000),
    # AND does not overwrite A, changes S/Z/C, and preserves V; moves preserve all.
    ('logical_preserve', 0x7fffffff, 1, LOAD+[0x10040000, 0x04000000, 0, 0x00003209], 0x80000000, 0x00008000, 0x280000),
]


def read(address):
    return f'load {address:#x},r1\nmov.l @r1,r0\n' + record()


def build():
    # Only DSP-end IRQ enabled. Each ENDI is observed by a real RTE handler.
    # SCU consumption requires an external-vector acquisition, not an
    # internally generated autovector that happens to have the same number.
    s = ('load 0x2600ec00,r8\nload 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n'
         'mov #0,r0\nldc r0,sr\n'
         + store(0x25fe00a0, 0xbfdf))
    # Record cold-reset flags and interrupt status before executing any DSP work.
    s += read(0x25fe0080)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x20,r2\nand r2,r0\n' + record()
    golden = [0, 0]
    for label, a, p, words, low, high, flags in CASES:
        s += data(0, [] if a is None else [a])
        if a is None:
            # Mutation is a ROM operand, read and uploaded by the guest itself.
            s += ('load 0x1f000,r1\nmov.l @r1,r0\n'
                  'load 0x25fe008c,r1\nmov.l r0,@r1\n')
        s += data(64, [p])
        # Wait beyond these short kernels without reading program control:
        # polling EX would clear V before the halted result could be sampled.
        # Include EX in the status mask to require that the wait reached END.
        code = words+[0x0000330a, 0xf0000000]
        s += start(code)
        s += (f'load 100,r2\n{label}:\ndt r2\nbf {label}\n'
              'load 0x25fe0080,r1\nmov.l @r1,r0\n'
              'load 0x7d0000,r2\nand r2,r0\n') + record()
        for _ in range(2):
            s += 'mov.l @r1,r0\nand r2,r0\n' + record()
        s += store(0x25fe0088, 128) + read(0x25fe008c)
        s += store(0x25fe0088, 192) + read(0x25fe008c)
        golden += [flags, flags & ~0x080000, flags & ~0x080000, low, high]
    # ENDI with overflow and then ENDI with replacement. No control-port polls
    # while running: main waits for handler's count. Handler reads status twice,
    # consumes the result and computes result+1. Control reads clear V and acknowledge E;
    # The external-vector acquisition consumes the modeled SCU request.
    for i, words in enumerate([LOAD+[0x10043209], LOAD+[0x10040000, 0x10043209]]):
        s += data(0, [0x7fffffff])+data(64, [1])+start(words+[0xf8000000])
        s += (f'load 0x2600f000,r1\nirq_wait{i}:\nmov.l @r1,r0\n'
              f'mov #{i+1},r2\ncmp/eq r2,r0\nbf irq_wait{i}\n')
        for addr in [0x2600f004, 0x2600f008, 0x2600f00c]:
            s += read(addr)
        golden += [[0x4c0000, 0x400000, 0x80000001], [0x440000, 0x400000, 0x80000002]][i]
    # Give a stale/duplicate end event ample time to arrive before counting.
    s += 'load 100,r2\nsettle:\ndt r2\nbf settle\n'
    s += read(0x2600f000)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x20,r2\nand r2,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'finished:\nsleep\nbra finished\nnop\n'
    golden += [2, 0, 0x600d]
    push = 'add #-12,r15\nmov.l r0,@r15\nadd #4,r15\nmov.l r1,@r15\nadd #4,r15\nmov.l r2,@r15\n'
    pop = 'mov.l @r15,r2\nadd #-4,r15\nmov.l @r15,r1\nadd #-4,r15\nmov.l @r15,r0\nadd #12,r15\nrte\nnop\n'
    handler = push
    for dest in [0x2600f004, 0x2600f008]:
        handler += ('load 0x25fe0080,r1\nmov.l @r1,r0\nload 0x7d0000,r2\nand r2,r0\n'
                    f'load {dest:#x},r1\nmov.l r0,@r1\n')
    handler += store(0x25fe0088, 128)
    handler += ('load 0x25fe008c,r1\nmov.l @r1,r0\nadd #1,r0\n'
                'load 0x2600f00c,r1\nmov.l r0,@r1\n')
    handler += 'load 0x2600f000,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'+pop
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    struct.pack_into('>I', rom, 0x45*4, 0x18000)
    struct.pack_into('>I', rom, 0x1f000, 0x7fffffff)
    for at, source in [(0x200,s),(0x18000,handler)]:
        code = assembler.assemble(source, base=at)
        assert at+len(code)<(0x18000 if at==0x200 else 0x1f000)
        rom[at:at+len(code)] = code
    lines = ['# Original SH-2/SCU DSP arithmetic diagnostic. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'dsp-arithmetic.s': ('# Original SCU DSP arithmetic guest. SPDX-License-Identifier: MIT; see LICENSE.\n# Main at 0x200.\n'+s+'\n# IRQ at 0x18000\n'+handler).encode(),
            'dsp-arithmetic.hex': ('\n'.join(lines)+'\n').encode(),
            'dsp-arithmetic.tsv': ''.join(f'{v:08X}\n' for v in golden).encode()}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    outputs,digest=build()
    for name,content in outputs.items():
        path=HERE/name
        if args.check:
            assert path.read_bytes()==content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}')
    print(f'Expanded ROM SHA-256 {digest}')
