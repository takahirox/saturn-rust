#!/usr/bin/env python3
"""Original MIT dual-SH-2/DMAC video stores and live palette phases.

ST-097 §3.2 / ST-210 item32 / SH7604 §7.11.2. Adjacent LICENSE.
Expected colors and cycle lower bounds are authored without running an emulator.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_clock import put16, read
from build_hle_system import record, smpc
from build_video_transactions import blank
from build_scu import assembler
from build_vdp2_fetch import line

HERE = Path(__file__).resolve().parent
ENDPOINTS = [0x25C06000, 0x25C86000, 0x25E06000, 0x25F00100]


def timer():
    return ('mov.b @r10,r0\nextu.b r0,r0\nshll8 r0\n'
            'mov.b @r11,r1\nextu.b r1,r1\nor r1,r0\n')


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\n'
         'bt master\nload 0x2600e100,r8\nload 0x060fe000,r15\nbra common\nnop\n'
         'master:\nload 0x2600e000,r8\ncommon:\n'
         'load 0x100,r1\nmov.l @r1,r12\n'
         'load 0x104,r1\nmov.w @r1,r13\n'
         'load 0xfffffe12,r10\nload 0xfffffe13,r11\n')
    for n, endpoint in enumerate(ENDPOINTS):
        s += f'load {endpoint:#x},r4\n'
        s += timer()+'mov r0,r5\n'
        # The omitted-write control retains exactly the same instruction count.
        s += 'mov.l r12,@r4\n' * 16
        s += 'mov.l @r4,r0\n'  # mandatory buffer drain before the second FRT
        s += timer()+'sub r5,r0\nextu.w r0,r0\n'+record()
        s += f'mov.l @r4,r0\n'+record()
        # A real 16-byte DMA unit, source initialized by the guest, destination
        # disjoint from the CPU upload. TCR is measured in longwords.
        s += 'load 0x2600c000,r1\n'
        for _ in range(4):
            s += 'mov.l r12,@r1\nadd #4,r1\n'
        for at, value in [(0xFFFFFF80, 0x2600C000), (0xFFFFFF84, endpoint+0x40),
                          (0xFFFFFF88, 4), (0xFFFFFF8C, 0x5E01)]:
            s += f'load {at:#x},r1\nload {value:#x},r0\nmov.l r0,@r1\n'
        s += 'load 0xffffffb0,r6\nmov #1,r7\n'
        s += timer()+'mov r0,r5\n'
        s += f'tst r13,r13\nbt no_dma_{n}\nmov.l r7,@r6\n'
        s += (f'load 0xffffff8c,r1\ndma_wait_{n}:\nmov.l @r1,r0\ntst #2,r0\nbt dma_wait_{n}\n'
              f'no_dma_{n}:\nload {endpoint+0x40:#x},r1\nmov.l @r1,r0\n')
        s += timer()+'sub r5,r0\nextu.w r0,r0\n'+record()
        s += read(endpoint+0x40)+read(endpoint+0x4C)
        # Clear DMAOR/CHCR before the next source/destination setup.
        s += 'mov #0,r0\nmov.l r0,@r6\nload 0xffffff8c,r1\nmov.l @r1,r2\nmov.l r0,@r1\n'
    # Only master owns display setup. Slave runs the same upload diagnostic
    # afterward, then parks. Shared records are computed exclusively by code.
    s += 'load 0x600d,r0\n'+record()
    s += 'load 0x8000,r2\ntst r2,r9\nbt display\nslave_park:\nsleep\nbra slave_park\nnop\ndisplay:\n'
    # 4bpp bitmap zooms one packed word across the screen. Mode2 palette entry2
    # has old blue in the FIRST word, new red/green in the SECOND word.
    for off,value in [(0xE,0x2000),(0x10,0x4EEE),(0x12,0xEEEE),(0x20,1),(0x28,2),
                      (0x78,0),(0x7A,0x100),(0x7C,0),(0x7E,0x100),(0xF8,1)]:
        s += put16(0x25F80000+off,value)
    s += put16(0x25E00000,0x2222)
    s += put16(0x25F00008,0x00FF)+put16(0x25F0000A,0)
    s += put16(0x25F80000,0x8000)+blank('palette')+line(16)
    # Free-running H counter avoids a stale software latch.
    s += 'load 0xfffffe92,r1\nmov #1,r0\nmov.b r0,@r1\n'
    s += ('load 0x108,r1\nmov.l @r1,r6\nload 0x25f00008,r5\n'
          'load 0x25f80008,r4\nload 300,r3\npalette_wait:\n'
          'load 0x25f80002,r1\nmov.w @r1,r0\nmov.w @r4,r0\ncmp/hs r3,r0\nbf palette_wait\nmov r0,r7\n'
          'tst r13,r13\nbt no_palette\nmov.l r6,@r5\nno_palette:\n'
          'mov.l @r5,r0\nload 0x2600e200,r1\nmov.l r0,@r1\n'
          'add #4,r1\nmov.l r7,@r1\nload 0x600d,r0\nadd #4,r1\nmov.l r0,@r1\n')
    s += smpc(2)+'park:\nsleep\nbra park\nnop\n'
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    struct.pack_into('>IHHI', rom, 0x100, 0x11223344, 1, 0, 0x000000FF)
    rom[0x200:0x200+len(code)] = code
    rows = ['# Original MIT video-write guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            rows.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'video-writes.s': ('; Original MIT: adjacent LICENSE.\n'+s).encode(),
            'video-writes.hex': ('\n'.join(rows)+'\n').encode()}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    files,digest=build()
    for name,data in files.items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} video-writes; ROM SHA-256 {digest}')
