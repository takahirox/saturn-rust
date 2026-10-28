#!/usr/bin/env python3
"""Original MIT VDP1 VBE raster-IRQ guest. Adjacent LICENSE applies.

ST-013-R3 ch.4/§§4.1/4.2/4.4; SCU VBlank-in/out and SH-2 FRT.
Every register write, interrupt response, checksum and completion is guest code.
No SDK, firmware, host-supplied completion or framebuffer results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_clock import put16, read
from build_hle_system import record
from build_vdp1_erase_latch import fill, observations, operand

HERE = Path(__file__).resolve().parent


def wait_out(label):
    # R10 is modified only by the real VBlank-out handler. SLEEP/RTE resumes
    # this guest; no host polling result is used to trigger a state transition.
    return (f'mov r10,r7\nwait_{label}:\nsleep\ncmp/eq r7,r10\n'
            f'bt wait_{label}\n')


def source():
    s = 'load 0x0600e800,r8\n'
    for o, v in [(0, 0), (2, 3), (4, 0), (8, 0x0201), (10, 0x0602)]:
        s += put16(0x25D00000 + o, v)
    s += operand(0x100, 0) + 'load 0x25d00006,r1\nmov.w r0,@r1\n'
    s += fill('vbe_a')
    s += put16(0x25F800E0, 0x20) + put16(0x25F800F0, 3) + put16(0x25F80000, 0x8000)
    # Enable external-vector mode, real RAM exception vectors, and only SCU
    # VBlank-in/out. The handlers preserve R0/R1 and use dedicated registers.
    s += put16(0xFFFFFEE0, 1)
    s += store(0x26004100, 0x1800) + store(0x26004104, 0x1C00)
    s += 'load 0x26004000,r0\nldc r0,vbr\n'
    s += ('mov #0,r9\nmov #0,r10\nmov #3,r11\nmov #0,r12\n'
          'load 0x25d00000,r13\nload 0x25d00002,r14\n')
    s += store(0x25FE00A0, 0xBFFC) + 'mov #0,r0\nldc r0,sr\n'
    s += wait_out('capture_a') + 'mov #0,r11\n' + fill('vbe_b')
    # A is captured. B remains written-only until the next physical swap.
    for o, a in [(6, 0x102), (8, 0x104), (10, 0x106)]:
        s += operand(a, 0) + f'load {0x25D00000+o:#x},r1\nmov.w r0,@r1\n'
    # Handler installs VBE at the next VBlank-in, before first HBlank-in.
    s += operand(0x108, 12) + 'mov #3,r11\n' + wait_out('erase_a')
    # Request next manual change immediately after OUT, before checksumming.
    s += put16(0x25D00002, 3) + 'mov #0,r11\n' + observations('vbe_a').replace('r9', 'r5')
    # The handler still writes the same enabled VBE. Native vectors additionally
    # cover persistence without writes. B erases the other physical buffer.
    s += wait_out('erase_b')
    # §4.2 permits clearing immediately after OUT, preserving TVM.
    s += put16(0x25D00000, 0) + 'mov #0,r12\n' + put16(0x25D00002, 3)
    s += observations('vbe_b').replace('r9', 'r5')
    # With VBE clear, A must remain A, rather than being recolored to B.
    s += wait_out('disabled') + observations('vbe_disabled').replace('r9', 'r5')
    # B is already displayed; hold both buffers so the production Canvas is stable.
    s += wait_out('publish_b')
    for a in [0x25D00016, 0x25D00010, 0x25D00014]:
        s += read(a, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += 'mov r9,r0\n' + record() + 'mov r10,r0\n' + record()
    # Difference of live FRT samples taken by IN/OUT handlers, modulo16 bits.
    s += ('load 0x2600e900,r1\nmov.l @r1,r0\nadd #4,r1\nmov.l @r1,r2\n'
          'sub r0,r2\nextu.w r2,r0\n') + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def handler(which):
    s = 'add #-8,r15\nmov.l r0,@r15\nadd #4,r15\nmov.l r1,@r15\n'
    if which == 'in':
        # The control write is the first handler operation after saving scratch.
        s += ('mov.w r12,@r13\ntst r11,r11\nbt no_change\nmov.w r11,@r14\n'
              'no_change:\nadd #1,r9\n')
    else:
        s += 'add #1,r10\n'
    s += ('load 0xfffffe12,r1\nmov.b @r1,r0\nextu.b r0,r0\nshll8 r0\n'
          'add #1,r1\nmov.b @r1,r1\nextu.b r1,r1\nor r1,r0\n'
          f'load {0x2600E900 + (4 if which=="out" else 0):#x},r1\nmov.l r0,@r1\n')
    return s + 'mov.l @r15,r1\nadd #-4,r15\nmov.l @r15,r0\nadd #8,r15\nrte\nnop\n'


def build():
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    for a, v in [(0x100, 0x801F), (0x102, 0x83E0), (0x104, 0x0402), (0x106, 0x0803), (0x108, 8)]:
        struct.pack_into('>H', rom, a, v)
    src = '; Original MIT VDP1 sampled VBE diagnostic; adjacent LICENSE.\n'
    for at, text in [(0x200, source()), (0x1800, handler('in')), (0x1C00, handler('out'))]:
        code = assembler.assemble(text, base=at)
        assert len(code) < (0x1600 if at == 0x200 else 0x400)
        rom[at:at+len(code)] = code
        src += f'.section {at:#x}\n' + text
    rows = ['# Original MIT VDP1 VBE raster IRQ diagnostic; adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            rows.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at+16])))
    return {'vdp1-vbe.s': src.encode(), 'vdp1-vbe.hex': ('\n'.join(rows)+'\n').encode()}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); a = p.parse_args()
    files, digest = build()
    for name, data in files.items():
        path = HERE / name
        if a.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if a.check else "Built"} {name}: {hashlib.sha256(data).hexdigest()}')
    print(f'Expanded ROM SHA-256: {digest}')
