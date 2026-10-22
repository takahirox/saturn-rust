#!/usr/bin/env python3
"""Original MIT dual-architecture sound-reset diagnostic; adjacent LICENSE.

MC68000UM §§5.5/6.3.1 and fig.5-30, ST-169 SNDON/SNDOFF.
The first 68k entry initializes registers and STOPs. SH-2 issues SNDOFF,
changes the actual reset vector, then SNDON. The second 68k entry records
retained registers before touching them. No host result/register writes.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent


def build():
    image = bytearray(0x4400)
    struct.pack_into('>II', image, 0, 0x7ff00, 0x100)
    words = [0x207c, 6, 0xf000, 0x4e60]  # MOVEA #6F000,A0; MOVE A0,USP
    for register in range(8):
        value = 0x13570010 + register
        words += [0x203c | register << 9, value >> 16, value & 65535]
    for register in range(7):
        value = 0x24680020 + register
        words += [0x207c | register << 9, value >> 16, value & 65535]
    words += [0x23fc, 0, 1, 0, 0x800, 0x4e72, 0x271f, 0x60fa]
    struct.pack_into('>' + 'H' * len(words), image, 0x100, *words)
    words = [0x40f9, 0, 0x6046]  # capture SR before any MOVE changes CCR
    for register in range(16):
        words += [0x23c0 | register, 0, 0x6000 + register * 4]
    words += [0x4e68, 0x23c8, 0, 0x6040]  # MOVE USP,A0; record it
    words += [0x23fc, 0, 0x600d, 0, 0x6048, 0x4e72, 0x2700, 0x60fa]
    struct.pack_into('>' + 'H' * len(words), image, 0x1000, *words)
    host = ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x1100,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    host += smpc(6).replace('wait_smpc6', 'start_first') + wait(0x05a00800, 'wait_first')
    host += smpc(7)
    host += store(0x25a00000, 0x7fe00) + store(0x25a00004, 0x1000)
    host += smpc(6).replace('wait_smpc6', 'start_second') + wait(0x05a06048, 'wait_second')
    host += ('load 0x25a06000,r1\nload 0x2600eb00,r8\nmov #19,r3\nresults:\n'
             'mov.l @r1,r0\nmov.l r0,@r8\nadd #4,r1\nadd #4,r8\ndt r3\nbf results\n'
             'finished:\nsleep\nbra finished\nnop\n')
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x1e00
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    rom[0x200:0x200 + len(code)] = code
    rom[0x2000:0x2000 + len(image)] = image
    lines = ['# Original sound-CPU reset BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    expected = ([0x13570010 + i for i in range(8)] + [0x24680020 + i for i in range(7)]
                + [0x7fe00, 0x6f000, 0x271f, 0x600d])
    return {'m68k-reset.hex': '\n'.join(lines) + '\n', 'm68k-reset.s': '; Original MIT: adjacent LICENSE.\n' + host,
            'm68k-reset.tsv': ''.join(f'{v:08X}\n' for v in expected)}, hashlib.sha256(rom).hexdigest()


def phase_fixture():
    """SH-2 updates PC after SNDON returns, before the PC vector bus reads.

    A synchronous vector snapshot runs the failure entry at 100. The correct
    timed reads run the second entry at 200, through ordinary sound RAM.
    """
    sound = bytearray(0x300)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    for entry, result in [(0x100, 0xbad), (0x200, 0x13579bdf)]:
        words = [0x23fc, result >> 16, result & 65535, 0, 0x800,
                 0x4e72, 0x2700, 0x60fa]
        struct.pack_into('>' + 'H' * len(words), sound, entry, *words)
    host = ('load 0x2000,r1\nload 0x25a00000,r2\nmov #0,r3\n'
            'load 0xc0,r3\ncopy:\nmov.l @r1,r0\nmov.l r0,@r2\n'
            'add #4,r1\nadd #4,r2\ndt r3\nbf copy\n'
            'load 0x25a00004,r5\nload 0x200,r6\n')
    host += smpc(6) + 'mov.l r6,@r5\n' + wait(0x05a00800, 'wait_sound')
    host += ('load 0x2600ed00,r1\nmov.l r0,@r1\n'
             'finished:\nsleep\nbra finished\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x1e00
    rom[0x200:0x200 + len(code)] = code
    rom[0x2000:0x2300] = sound
    lines = ['# Original sound reset bus-phase BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    return {'m68k-reset-phases.hex': '\n'.join(lines) + '\n',
            'm68k-reset-phases.s': '; Original MIT: adjacent LICENSE.\n' + host}, hashlib.sha256(rom).hexdigest()


def trace_fixture():
    """An actual traced 68k RESET must complete its pulse before trace entry."""
    sound = bytearray(0x400)
    struct.pack_into('>II', sound, 0, 0x1000, 0x200)
    struct.pack_into('>I', sound, 9 * 4, 0x300)
    # MOVE.L #operand,D3; MOVE.W #A700,SR; RESET at20A; failure store.
    words = [0x263c, 0x1357, 0x9bdf, 0x46fc, 0xa700, 0x4e70,
             0x23fc, 0, 0xbad, 0, 0x908, 0x4e72, 0x2700]
    struct.pack_into('>' + 'H' * len(words), sound, 0x200, *words)
    # Record stacked SR, PC and retained D3 before signaling completion.
    words = [0x3017, 0x33c0, 0, 0x900, 0x222f, 2, 0x23c1, 0, 0x904,
             0x23c3, 0, 0x90c, 0x23fc, 0, 0x600d, 0, 0x908,
             0x4e72, 0x2700]
    struct.pack_into('>' + 'H' * len(words), sound, 0x300, *words)
    host = ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x100,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    host += smpc(6) + wait(0x05a00908, 'wait_trace') + 'load 0x2600ee00,r8\n'
    for address, width in [(0x25a00900, 'w'), (0x25a00904, 'l'),
                           (0x25a0090c, 'l'), (0x25a00908, 'l')]:
        host += f'load {address:#x},r1\nmov.{width} @r1,r0\n'
        if width == 'w':
            host += 'extu.w r0,r0\n'
        host += 'mov.l r0,@r8\nadd #4,r8\n'
    host += 'finished:\nsleep\nbra finished\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x1e00
    rom[0x200:0x200 + len(code)] = code
    rom[0x2000:0x2400] = sound
    lines = ['# Original traced sound RESET BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    return {'m68k-traced-reset.hex': '\n'.join(lines) + '\n',
            'm68k-traced-reset.s': '; Original MIT: adjacent LICENSE.\n' + host}, hashlib.sha256(rom).hexdigest()


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
    print(f'{"Verified" if args.check else "Built"} 68k reset fixture; expanded ROM SHA-256 {digest}')
    files, digest = phase_fixture()
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} 68k reset phases; expanded ROM SHA-256 {digest}')
    files, digest = trace_fixture()
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} traced 68k RESET; expanded ROM SHA-256 {digest}')
