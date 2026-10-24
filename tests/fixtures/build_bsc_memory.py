#!/usr/bin/env python3
"""Original MIT SH7604 memory-interface diagnostic (H §§7.5-7.7).

The same BIOS executes on both SH-2s. Cold-boot literal mutations select each
interface, bus size and CAS latency; no prohibited live interface reconfiguration.
Every expected timer interval is authored from the manual's state diagrams.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent
# name, BCR1, MCR, WCR, read ticks, write ticks. All kernels have cached
# instructions and execute eight operand accesses. Read phases include the
# fixed latch overhead. Write values below describe the old unbuffered kernel;
# the §7.11.2 buffer permits ADD/acceptance and the final FRT read to overlap
# external writes. The oracle below derives the new interval from bus states.
# SDRAM: 32-bit burst/CL1=7; CL2=7; CL3=8; CL4=9. Word bus=11.
# DRAM/pseudo normal: Tp overlaps previous closed-row Tc2 -> 3.
# BE joins the two word-bus halves; RASD holds a row across operands.
CONFIGS = [
    ('sdram32-cl1', 0x401, 0x40, 0, 7, 5),
    ('sdram32-cl2', 0x401, 0x40, 0x40, 7, 5),
    ('sdram32-cl3', 0x401, 0x40, 0x80, 8, 5),
    ('sdram32-cl4', 0x401, 0x40, 0xC0, 9, 5),
    ('sdram16', 0x401, 0, 0, 11, 9),
    ('sdram32-active', 0x401, 0x240, 0, 6, 2),
    ('dram32', 0x402, 0x40, 0, 3, 4),
    ('dram16', 0x402, 0, 0, 6, 7),
    ('dram16-page', 0x402, 0x400, 0, 5, 6),
    ('dram32-active', 0x402, 0x640, 0, 2, 3),
    ('pseudo32', 0x403, 0x40, 0, 3, 4),
    ('pseudo16-static', 0x403, 0x400, 0, 5, 6),
]


def kernel(body):
    return ('load 0xfffffe91,r7\nmov #2,r0\nmov.b r0,@r7\n'
            'mov #0,r0\nmov.b r0,@r7\nmov.b @r1,r0\nmov.b @r2,r5\nextu.b r5,r5\n'
            + body + 'mov.b @r1,r0\nmov.b @r2,r0\nextu.b r0,r0\n'
            'sub r5,r0\nextu.b r0,r0\nrts\nnop\n')


def build():
    # Initialize all keyed configuration while still fetching from CS0.
    s = store(0xFFFFFFE8, 0xA55A0000)
    s += store(0xFFFFFFE0, 0xA55A0401)
    s += store(0xFFFFFFEC, 0xA55A0040)
    s += ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
          'load 0x2600e800,r8\nload 0x2600d800,r4\nbra common\nnop\n'
          'slave:\nload 0x2600e900,r8\nload 0x2600d900,r4\n'
          'common:\nload 0x13579bdf,r0\nmov.l r0,@r4\n'
          'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n')
    for at in [0x4000, 0x4200]:
        for _ in range(2):
            s += ('load 0xfffffe12,r1\nload 0xfffffe13,r2\n'
                  f'load {at:#x},r3\nload 0x12345600,r6\njsr @r3\nnop\n')
        s += record() + record(6)
    s += 'mov.l @r4,r0\n' + record()
    s += 'load 0x600d,r0\n' + record()
    s += 'load 0x8000,r2\ntst r2,r9\nbf finished\n' + smpc(2)
    s += 'finished:\nsleep\nbra finished\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for at, source in [(0x200,s),(0x4000,kernel('mov.l @r4,r6\n'*8)),
                       (0x4200,kernel('add #1,r6\nmov.l r6,@r4\n'*8))]:
        code = assembler.assemble(source, base=at)
        assert len(code) < (0x3E00 if at==0x200 else 0x200)
        rom[at:at+len(code)] = code
    lines = ['# Original memory-interface BIOS. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    vectors = ['# name BCR1 MCR WCR read-ticks read-data write-ticks write-data final-data marker']
    for name,bcr,mcr,wcr,reads,writes in CONFIGS:
        # One ADD and one internal acceptance per write. The first external
        # write starts at clock8 after FRT resume, and the final latch is three
        # clocks after the eighth acceptance. EXTU after the first low-byte
        # load combines WB/EX with the existing IF split. Explicit MOV MA
        # lead-ins give 11+7*max(2, external-cycle); see docs/sh2-load-pipeline.md.
        # External-cycle = writes-1 from H figs7.26/7.31/7.34/7.38/7.41.
        buffered_ticks = (11 + 7 * max(2, writes - 1)) // 8
        values = [bcr,mcr,wcr,reads+1,0x13579BDF,buffered_ticks,0x12345608,0x12345608,0x600D]
        vectors.append(name+' '+' '.join(f'{v:08X}' for v in values))
    return {'bsc-memory.hex': ('\n'.join(lines)+'\n').encode(),
            'bsc-memory.tsv': ('\n'.join(vectors)+'\n').encode()}


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data, f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
