#!/usr/bin/env python3
"""Original MIT DISP/status/raster guest. ST-058-R2 §2.5; ST-097 table2.1.

Only guest SH-2 instructions write results. ROM word100 selects whether the
two active-period writes disable display; no proprietary firmware is used.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16
from build_hle_system import record

HERE = Path(__file__).resolve().parent


def observe(address, width, mask):
    return (f'load {address:#x},r1\nmov.{width} @r1,r0\n'
            f'mov #{mask},r2\nand r2,r0\n' + record())


def build():
    status = observe(0x25F80004, 'w', 8)
    irq = observe(0x25FE00A4, 'l', 3)
    disable = ('load 0x100,r1\nmov.w @r1,r0\n'
               'load 0x25f80000,r1\nmov.w r0,@r1\n')
    s = 'load 0x2600e000,r8\n' + put16(0x25F80000, 0x8000)
    s += status + irq + disable + status + irq
    # Status is forced blank while disabled. Poll the real vertical counter
    # instead, proving that disabling display does not synthesize a raster IRQ.
    s += ('load 224,r3\nwait_line:\nload 0x25f80002,r1\nmov.w @r1,r0\n'
          'load 0x25f8000a,r1\nmov.w @r1,r0\n'
          'cmp/hs r3,r0\nbf wait_line\n')
    s += status + irq + put16(0x25F80000, 0x8000) + status
    s += ('load 0x25f80004,r1\nwait_active:\nmov.w @r1,r0\n'
          'tst #8,r0\nbf wait_active\n')
    s += status + irq + disable + status + irq
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    rom[0x200:0x200 + len(code)] = code
    lines = ['# Original MIT VDP2 DISP/status BIOS. Adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    return {'vdp2-status.s': s.encode(), 'vdp2-status.hex': ('\n'.join(lines) + '\n').encode()}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    files, digest = build()
    for name, data in files.items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} vdp2-status.hex; expanded ROM SHA-256 {digest}')
