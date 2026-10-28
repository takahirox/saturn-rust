#!/usr/bin/env python3
"""Original MIT ordinary MOV store/FRT guest; adjacent LICENSE.

Independent oracle: docs/sh2-store-pipeline.md. Both SH-2s execute warmed
kernels, retain readback records, and exchange data through cache-through RAM.
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
    ('aligned', 'mov.l r6,@r12\nnop\n'*16, 32, 3),
    ('offset', 'nop\n'+'mov.l r6,@r12\nnop\n'*16, 49, 3),
    ('repeat', 'mov.l r6,@r12\n'*32, 48, 3),
    ('dependent', 'mov.l @r13,r6\nmov.l r6,@r12\n'*16, 64, 3),
    ('independent', 'mov.l @r13,r6\nmov.l r9,@r12\n'*16, 48, 7),
    ('peripheral', 'mov.b r6,@r10\n'*32, 48, 3),
]


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
         'load 0x2600ee00,r8\n') + smpc(2) + 'bra common\nnop\n'
    s += 'slave:\nload 0x060fe000,r15\nload 0x2600ef00,r8\ncommon:\n'
    s += store(0xFFFFFFE8, 0xA55A0000)
    s += ('load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0xc0000400,r12\nload 0xc0000404,r13\n'
          'mov #3,r0\nmov.l r0,@r13\nload 0xfffffe01,r10\n')
    for i, (name, _, _, _) in enumerate(KERNELS):
        for _ in range(2):
            s += ('mov #-1,r0\nmov.l r0,@r12\nmov.b r0,@r10\n'
                  f'load {0x4000+i*0x400:#x},r3\nmov #3,r6\nmov #7,r9\njsr @r3\nnop\n')
        s += record()
        s += 'mov.b @r10,r6\n' if name == 'peripheral' else 'mov.l @r12,r6\n'
        s += record(6)
    # Guest-only publication handoff. The master publishes data then a flag;
    # slave polls the external flag, consumes the data and publishes arithmetic
    # acknowledgement. No host synthesizes completion records.
    s += ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf consumer\n'
          'load 0x2600f000,r12\nload 0x12345678,r6\nmov.l r6,@r12\n'
          'mov #1,r0\nadd #4,r12\nmov.l r0,@r12\n'
          'add #4,r12\nwait_ack:\nmov.l @r12,r0\ntst r0,r0\nbt wait_ack\n'
          'bra handoff_done\nnop\nconsumer:\nload 0x2600f000,r12\n'
          'add #4,r12\nwait_flag:\nmov.l @r12,r0\ntst r0,r0\nbt wait_flag\n'
          'add #-4,r12\nmov.l @r12,r0\nadd #1,r0\nadd #8,r12\nmov.l r0,@r12\n'
          'handoff_done:\n'+record()+'load 0x600d,r0\n'+record()+
          'done:\nsleep\nbra done\nnop\n')
    sections = {0x200: s}
    for i, (_, body, _, _) in enumerate(KERNELS):
        sections[0x4000+i*0x400] = kernel(body)
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for address, source in sections.items():
        code = assembler.assemble(source, base=address)
        assert len(code) < (0x3E00 if address == 0x200 else 0x400)
        rom[address:address+len(code)] = code
    lines = ['# Original MIT SH-2 store pipeline guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected = [v for _,_,clocks,result in KERNELS for v in [(8+clocks)//8,result]]+[0x12345679,0x600D]
    artifacts = {
        'sh2-store.hex': ('\n'.join(lines)+'\n').encode(),
        'sh2-store.s': ('; Original MIT SH-2 MOV store guest; adjacent LICENSE.\n'+'\n'.join(
            f'; section {at:08X}\n{body}' for at,body in sections.items())).encode(),
        'sh2-store.tsv': ''.join(f'{v:08X}\n' for v in expected).encode(),
    }
    artifacts['sh2-store.sha256'] = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n' for name,data in artifacts.items()).encode()
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
