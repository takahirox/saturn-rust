#!/usr/bin/env python3
"""Original MIT SCSP shared-memory DMA diagnostic; adjacent LICENSE.

ST-077 §3.2 pp.20–21, §4.2 pp.101–102, tables4.39–40.
SH-2 times a 64-word RAM-to-COEF transfer using its actual FRT. The real
68k waits for DMA completion, reads a transferred coefficient and computes
PCM from it. Neither test host nor emulator supplies guest result/audio.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import smpc, wait, record

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x2080)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    words = [0x3039, 0x10, 0x420, 0x0240, 16, 0x67f4,  # wait DMA pending
             0x3039, 0x10, 0x700,                        # actual COEF0
             0x33c0, 0, 0x1000, 0x33c0, 0, 0x1002,
             0x33c0, 0, 0x808, 0x4440,                 # NEG.W D0
             0x33c0, 0, 0x1004, 0x33c0, 0, 0x1006]
    for address, value in [(0x100002, 0x1000), (0x100004, 0), (0x100006, 4),
                           (0x10000c, 0x100), (0x100010, 0), (0x100016, 0xe000),
                           (0x100000, 0x1820), (0x100422, 16)]:
        words += [0x33fc, value, address >> 16, address & 65535]
    words += [0x23fc, 0, 0x600d, 0, 0x804, 0x4e72, 0x2700, 0x60fa]
    struct.pack_into('>' + 'H' * len(words), sound, 0x100, *words)
    # Original distinct coefficient words. Their lower three bits are zero.
    struct.pack_into('>64H', sound, 0x2000, *(0x1230 + i * 8 for i in range(64)))
    host = ('load 0x4000,r1\nload 0x25a00000,r2\nload 0x820,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n'
            'load 0x25b00400,r1\nload 0x20f,r0\nmov.w r0,@r1\n')
    host += smpc(6)
    host += ('load 0x25b00412,r1\nload 0x2000,r0\nmov.w r0,@r1\n'
             'add #2,r1\nload 0x700,r0\nmov.w r0,@r1\n'
             'load 0xfffffe92,r1\nmov #19,r0\nmov.b r0,@r1\n'
             'load 0xfffffe12,r4\nload 0xfffffe13,r5\n'
             'load 0x25b00416,r6\nload 0x1080,r7\n'
             'mov #0,r0\nmov.b r0,@r4\nmov.b r0,@r5\n'
             'mov.w r7,@r6\n'
             'poll:\nmov.w @r6,r0\nload 0x1000,r2\ntst r2,r0\nbf poll\n'
             'mov.b @r4,r0\nextu.b r0,r0\nshll8 r0\nmov r0,r9\n'
             'mov.b @r5,r0\nextu.b r0,r0\nor r9,r0\n'
             'load 0x2600ec00,r8\n') + record()
    for address in [0x25b00700, 0x25b0077e, 0x25b00420]:
        host += f'load {address:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\n' + record()
    host += wait(0x05a00804, 'wait_sound')
    host += 'load 0x25a00808,r1\nmov.w @r1,r0\nextu.w r0,r0\n' + record()
    host += 'load 0x25a00804,r1\nmov.l @r1,r0\n' + record()
    host += 'finished:\nsleep\nbra finished\nnop\n'
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x3e00
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    rom[0x200:0x200 + len(code)] = code
    rom[0x4000:0x4000 + len(sound)] = sound
    lines = ['# Original SCSP memory-cycle DMA BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
    return {'scsp-dma.hex': '\n'.join(lines) + '\n',
            'scsp-dma.s': '; Original MIT: adjacent LICENSE.\n' + host}, hashlib.sha256(rom).hexdigest()


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
    print(f'{"Verified" if args.check else "Built"} SCSP memory-cycle DMA; expanded ROM SHA-256 {digest}')
