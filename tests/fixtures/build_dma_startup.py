#!/usr/bin/env python3
"""Original MIT mixed-disc A-to-B startup diagnostic; adjacent LICENSE.

SH-2 enables its cache, requests CD data, arms Timer0 DMA, and continuously
writes the overlapping bitmap operand. Real DMA-end exception code records IRQ.
No production arbiter helper is used to derive data or source-count expectations.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler as a
from build_clock import put16
from build_video_transactions import blank

HERE = Path(__file__).resolve().parent
BASE = 0x06004000
R = 0x06010000


def store(address, value):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.l r0,@r1\n'


def call(command, b=0, c=0, d=0):
    return (f'load {command:#x},r4\nload {b:#x},r5\nload {c:#x},r6\nload {d:#x},r7\n'
            'bsr command\nnop\n')


def build():
    s = ('mov #-16,r0\nldc r0,sr\nload 0x06000000,r0\nldc r0,vbr\n'
         'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n')
    s += store(0x0600012c, BASE+0x1400) + store(R,0)
    s += store(0x25fe00a0,0xfffff7ff)
    # 4bpp bitmap: palette1 red, 2 green, 3 blue, 4 white.
    for off, value in [(0x10,0x4eee),(0x12,0xeeee),(0x20,1),(0x28,2),
                       (0x78,1),(0x7a,0),(0x7c,1),(0x7e,0),(0xf8,1)]:
        s += put16(0x25f80000+off,value)
    for index, color in [(1,0x801f),(2,0x83e0),(3,0xfc00),(4,0xffff)]:
        s += put16(0x25f00000+index*2,color)
    s += put16(0x25f80000,0x8000)
    s += call(0x3000)
    s += call(0x1080,210,0x80,1)
    s += 'sector_wait:\n'+call(0x5100)+'mov #1,r2\ncmp/eq r2,r0\nbf sector_wait\n'
    s += call(0x6300,0,0,1)
    for off,value in [(0,0x05818000),(4,0x05e00000),(8,8),(12,1),(20,3),(16,0x100)]:
        s += store(0x25fe0000+off,value)
    s += blank('start')
    s += store(0x25fe0090,2)
    # Omitted-trigger control changes only this guest operand.
    s += f'load {BASE+0x1804:#x},r1\nmov.l @r1,r0\nload 0x25fe0098,r1\nmov.l r0,@r1\n'
    s += (f'load {BASE+0x1800:#x},r1\nmov.l @r1,r12\nload 0x25e00000,r4\n'
          'load 512,r3\ncontinuous:\nmov.l r12,@r4\ndt r3\nbf continuous\n'
          'mov.l @r4,r0\n')  # drain before waiting/recording
    s += store(0x25fe0098,0)
    s += f'load {BASE+0x1804:#x},r1\nmov.l @r1,r0\ntst r0,r0\nbt no_trigger\n'
    s += 'mov #0,r0\nldc r0,sr\nload 0x26010000,r1\nirq_wait:\nmov.l @r1,r0\ntst r0,r0\nbt irq_wait\nno_trigger:\n'
    for at,record in [(0x25e00000,R+4),(0x25e00004,R+8)]:
        s += f'load {at:#x},r1\nmov.l @r1,r0\nload {record:#x},r1\nmov.l r0,@r1\n'
    s += ('load 0x25e00000,r1\nmov.l @r1+,r0\nmov.l @r1,r2\nadd r2,r0\n'
          f'load {R+12:#x},r1\nmov.l r0,@r1\n')
    s += call(0x0600)
    s += 'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\nload 0x26010010,r1\nmov.l r0,@r1\n'
    s += store(R+20,0x600d)+'park:\nsleep\nbra park\nnop\n'
    s += ('command:\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\n'
          'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\n'
          'add #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'
          'load 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\n'
          'load 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n')
    handler = ('mov.l r0,@-r15\nmov.l r1,@-r15\nload 0x26010000,r1\n'
               'mov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'
               'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n')
    program = bytearray(0x2000)
    code = a.assemble(s,base=BASE)
    assert len(code)<0x1400
    program[:len(code)] = code
    code = a.assemble(handler,base=BASE+0x1400)
    program[0x1400:0x1400+len(code)] = code
    struct.pack_into('>II',program,0x1800,0x11111111,1)
    iso = bytearray(a.build(bytes(program),(HERE/'LICENSE').read_bytes(),'DMA STARTUP','DMASTART-001 V1.000'))
    struct.pack_into('>III',iso,60*2048,0x22223333,0x33332222,0x44445555)
    cue = 'FILE "dma-startup.iso" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\nFILE "dma-startup-audio.bin" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n'
    return {'dma-startup.iso':bytes(iso),'dma-startup-audio.bin':bytes(2352*2),
            'dma-startup.cue':cue.encode(),'dma-startup.s':s.encode(),
            'dma-startup-handler.s':handler.encode()}


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
