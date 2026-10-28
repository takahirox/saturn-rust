#!/usr/bin/env python3
"""Original MIT SH-2/68k level-seven diagnostic; adjacent LICENSE.

Oracle authored from MC68000UM 3.5/6.3.2 before scheduler changes:
first assertion -> IACK -> handler at I=7 -> DIVU(7/2) completes ->
second IACK -> nested handler saves PC after DIVU, SR=2700, D0=00010003.
No clear or no reassertion gives one entry. DIVU(9/2) gives 00010004.
The SH-2 polls a guest marker, clears SCIPD, waits 16 NOPs, reasserts.
Both requests stay asserted through their IACK; no short-pulse oracle.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x1000)
    struct.pack_into('>II', sound, 0, 0x1000, 0x200)
    struct.pack_into('>I', sound, 31 * 4, 0x300)
    def put(at, words):
        struct.pack_into('>' + 'H' * len(words), sound, at, *words)
    # A0 points to guest-owned handler records. No host expected-result stores.
    put(0x200, [0x203c, 0, 7, 0x7202, 0x7400, 0x207c, 0, 0x900,
                0x23fc, 0, 1, 0, 0x948, 0x4e72, 0x2700, 0x60fa])
    # Each entry publishes SR (word), PC (long), D0 (long), count (long).
    put(0x300, [0x5282, 0x30d7, 0x20ef, 2, 0x20c0, 0x20c2,
                0x0c82, 0, 2, 0x6700, 0x002c,
                0x23fc, 0, 1, 0, 0x940, 0x80c1,
                0x23c0, 0, 0x944, 0x4e72, 0x2700, 0x60fa])
    put(0x340, [0x4e72, 0x2700, 0x60fa])
    # BEQ at 312 branches from extension address 314 to second-entry STOP340.
    # CS0 two four-state phases; deterministic room inside DIV's 140 clocks.
    host = 'load 0xffffffe8,r4\nload 0xa55aaafe,r0\nmov.l r0,@r4\n'
    host += ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x400,r3\ncopy:\n'
             'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    for offset in [0x41e, 0x424, 0x426, 0x428]:
        host += f'load {0x25b00000 + offset:#x},r1\nmov #32,r0\nmov.w r0,@r1\n'
    host += smpc(6)
    host += ('load 0x25a00948,r1\nwait_ready:\nmov.l @r1,r0\ntst r0,r0\nbt wait_ready\n')
    host += ('load 0x25b00420,r4\nload 0x25b00422,r5\nmov #32,r6\n'
             'mov.w r6,@r4\nload 0x25a00940,r1\nwait_handler:\n'
             'mov.l @r1,r0\ntst r0,r0\nbt wait_handler\n'
             'mov.w r6,@r5\n')
    host += 'nop\n' * 16
    host += 'mov.w r6,@r4\nfinished:\nsleep\nbra finished\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x1e00
    rom[0x200:0x200 + len(code)] = code
    rom[0x2000:0x3000] = sound
    lines = ['# Original level-seven rearm diagnostic. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    return {'m68k-irq.hex': '\n'.join(lines) + '\n',
            'm68k-irq.s': '; Original MIT: adjacent LICENSE.\n' + host}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} 68k IRQ; expanded ROM SHA-256 {digest}')
