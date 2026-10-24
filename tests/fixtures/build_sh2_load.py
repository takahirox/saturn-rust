#!/usr/bin/env python3
"""Original MIT ordinary MOV load/FRT guest; adjacent LICENSE.

Oracle: docs/sh2-load-pipeline.md, authored stage intervals plus eight wrapper
clocks. Both CPUs execute identical kernels twice to warm their instruction
cache. Original operands live in CPU-local cache RAM and SCI BRR.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc
from build_sh2_timing import kernel

HERE = Path(__file__).resolve().parent
KERNELS = [
    ('aligned', 'mov.l @r12,r6\nnop\n'*16, 32, 3),
    ('offset', 'nop\n'+'mov.l @r12,r6\nnop\n'*16, 49, 3),
    ('dependent', 'mov.l @r12,r6\nadd #1,r6\n'*16, 48, 4),
    ('offset_dependent', 'nop\n'+'mov.l @r12,r6\nadd #1,r6\n'*16, 49, 4),
    ('repeat', 'mov.l @r12,r6\n'*32, 48, 3),
    ('separated', 'mov.l @r12,r6\nnop\nadd #1,r6\n'*8, 28, 4),
    ('peripheral', 'mov.b @r10,r6\nnop\n'*16, 48, 0x5A),
    ('peripheral_dependent', 'mov.b @r10,r6\nadd #1,r6\n'*16, 64, 0x5B),
]


def build():
    source = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
              'load 0x2600ec00,r8\n') + smpc(2) + 'bra common\nnop\n'
    source += 'slave:\nload 0x060fe000,r15\nload 0x2600ed00,r8\ncommon:\n'
    source += store(0xFFFFFFE8, 0xA55A0000)
    source += ('load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
               'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
               'load 0xc0000400,r12\nmov #3,r0\nmov.l r0,@r12\n'
               'load 0xfffffe01,r10\nmov #90,r0\nmov.b r0,@r10\n')
    for i in range(len(KERNELS)):
        for _ in range(2):
            source += f'load {0x4000+i*0x400:#x},r3\nmov #0,r6\njsr @r3\nnop\n'
        source += record() + record(6)
    source += 'load 0x600d,r0\n'+record()+'done:\nsleep\nbra done\nnop\n'
    sections = {0x200: source}
    for i, (_, body, _, _) in enumerate(KERNELS):
        sections[0x4000+i*0x400] = kernel(body)
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for address, source in sections.items():
        code = assembler.assemble(source, base=address)
        assert len(code) < (0x3E00 if address == 0x200 else 0x400)
        rom[address:address+len(code)] = code
    lines = ['# Original MIT SH-2 load pipeline guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected = [v for _,_,clocks,result in KERNELS for v in [(8+clocks)//8,result]]+[0x600D]
    artifacts = {
        'sh2-load.hex': ('\n'.join(lines)+'\n').encode(),
        'sh2-load.s': ('; Original MIT SH-2 MOV load guest; adjacent LICENSE.\n'+'\n'.join(
            f'; section {at:08X}\n{body}' for at,body in sections.items())).encode(),
        'sh2-load.tsv': ''.join(f'{v:08X}\n' for v in expected).encode(),
    }
    artifacts['sh2-load.sha256'] = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n' for name,data in artifacts.items()).encode()
    return artifacts


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args = parser.parse_args()
    for name,content in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
