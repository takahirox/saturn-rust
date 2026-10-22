#!/usr/bin/env python3
"""Original MIT A-bus timing guest; ST-097 §3.2 pp.62–72.

Six cached kernels measure real A-bus cycles with the SH7604 FRT phi/8.
Expected states count normal/burst waits and post-read precharge; no result
was collected from emulator output. The base console has no CS0 cartridge:
reads return the board's pulled-up data pins, rather than an injected device.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record
from build_sh2_timing import kernel

HERE = Path(__file__).resolve().parent
# ASR0, read instruction, main states, FRT ticks.
# The shared kernel latches FRT after six internal states (H §7.11.2),
# so a 52-state body records floor((6+52)/8)=7, not six ticks.
CASES = [
    (0, 'mov.w @r12,r6\n', 32, 4),
    (0x00300000, 'mov.w @r12,r6\n', 80, 10),
    (0x00510000, 'mov.w @r12,r6\n', 192, 24),
    (0x01540000, 'mov.w @r12+,r6\n', 80, 10),
    (0x015C0000, 'mov.w @r12+,r6\n', 52, 7),
    (0x20000000, 'mov.w @r12,r6\n', 47, 6),
]


def build():
    s = store(0xFFFFFFE8, 0xA55A0000)
    s += ('load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0x2600d700,r8\n')
    for i, (config, _, _, _) in enumerate(CASES):
        s += store(0x25FE00B0, config)
        for _ in range(2):
            s += (f'load {0x4000+i*0x400:#x},r3\nload 0x22000000,r12\n'
                  'jsr @r3\nnop\n')
        s += record() + record(6)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    sections = {0x200: s}
    for i, (_, instruction, _, _) in enumerate(CASES):
        sections[0x4000+i*0x400] = kernel(instruction*16)
    for base, source in sections.items():
        code = assembler.assemble(source, base=base)
        assert len(code) < 0x400
        rom[base:base+len(code)] = code
    lines = ['# Original MIT A-bus timing synthetic BIOS; adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected = [v for _, _, _, ticks in CASES for v in [ticks, 0xFFFFFFFF]] + [0x600D]
    return {'abus.hex': ('\n'.join(lines)+'\n').encode(),
            'abus.tsv': ''.join(f'{v:08X}\n' for v in expected).encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check',action='store_true'); args=p.parse_args()
    for name, data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data, f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
