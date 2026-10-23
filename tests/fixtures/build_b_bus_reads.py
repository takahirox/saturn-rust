#!/usr/bin/env python3
"""Original MIT dual-SH-2 memory-bridge diagnostic; adjacent LICENSE.

ST-210 item32, ST-077 §3.1, SH7604 FRT. No SDK or firmware.
Golden arithmetic is authored here, never obtained by executing an emulator.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16, read
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent
ENDPOINTS = [0x25A06000, 0x25C06000, 0x25C86000, 0x25E06000, 0x25F00100]


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\n'
         'bt master\nload 0x2600e100,r8\nload 0x060fe000,r15\nbra common\nnop\n'
         'master:\nload 0x2600e000,r8\ncommon:\n')
    s += put16(0x25B00400, 0x20F)
    s += 'load 0x100,r1\nmov.w @r1,r12\nload 0x102,r1\nmov.w @r1,r13\n'
    for endpoint in ENDPOINTS:
        s += (f'load {endpoint:#x},r4\nmov.w r12,@r4\nadd #2,r4\n'
              'mov.w r13,@r4\nadd #-2,r4\n')
        if endpoint != ENDPOINTS[0]:
            for lane in range(4):
                s += read(endpoint + lane, 1)
        for lane in [0, 2]:
            s += read(endpoint + lane, 2)
        s += read(endpoint, 4)
        # Each FRC low observation first reads high to latch a coherent pair.
        s += ('load 0xfffffe12,r10\nload 0xfffffe13,r11\n'
              'mov.b @r10,r0\nmov.b @r11,r5\nextu.b r5,r5\n')
        s += 'mov.w @r4,r0\n' * 16
        s += ('mov.b @r10,r0\nmov.b @r11,r0\nextu.b r0,r0\n'
              'sub r5,r0\nextu.b r0,r0\n') + record()
    s += 'load 0x600d,r0\n' + record()
    s += 'load 0x8000,r2\ntst r2,r9\nbf park\n' + smpc(2)
    s += 'park:\nsleep\nbra park\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    struct.pack_into('>HH', rom, 0x100, 0xA1B2, 0xC3D4)
    code = assembler.assemble(s, base=0x200)
    rom[0x200:0x200 + len(code)] = code
    lines = ['# Original MIT B-bus memory read guest; adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
    # Timing is separately bounded: sixteen full bridge reads alone require
    # 16*16/8=32 FRT ticks at reset CS2, before IF/peripheral costs.
    expected = [0xA1B2, 0xC3D4, 0xA1B2C3D4, 0]
    expected += [0xA1, 0xB2, 0xC3, 0xD4, 0xA1B2, 0xC3D4, 0xA1B2C3D4, 0] * 4
    expected += [0x600D]
    return {'b-bus-reads.s': '; Original MIT: adjacent LICENSE.\n' + s,
            'b-bus-reads.hex': '\n'.join(lines) + '\n',
            'b-bus-reads.tsv': ''.join(f'{v:08X}\n' for v in expected)}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    for name, data in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == data, f'Rebuild {name}'
        else:
            path.write_text(data)
    print(f'{"Verified" if args.check else "Built"} b-bus-reads; ROM SHA-256 {digest}')
