#!/usr/bin/env python3
"""Original MIT HDTV commands/composition and automatic erase guests.

ST-013 §§1.2/4.1/4.4 and ST-058 §2.1. Adjacent LICENSE applies.
All drawing, MMIO, checksums, status and IRQ observations execute on SH-2.
"""
import argparse
import hashlib
from pathlib import Path
from build_clock import put16, read
from build_hle_system import record
from build_video_transactions import blank, encode
from build_vdp1_erase_latch import operand

HERE = Path(__file__).resolve().parent
# Native and browser expected colors are independently authored, not generated.
TEXTURE = [0x801F, 0x83E0, 0, 1, 0x4001, 0xFFFF, 0xFC1F, 0xFFE0,
           0x83FF, 0xFC00, 1, 0, 0x4001, 0x801F, 0xFFFF, 0x83E0]


def in_blank(label):
    return (f'load 0x25f80004,r1\nin_{label}:\nmov.w @r1,r0\n'
            f'tst #8,r0\nbt in_{label}\n')


def out_blank(label):
    return (f'load 0x25f80004,r1\nout_{label}:\nmov.w @r1,r0\n'
            f'tst #8,r0\nbf out_{label}\n')


def mode_window(label):
    s = in_blank(label)
    # Do not write TVM until the second HBlank-IN after VBlank-IN.
    # Full low/high transitions tolerate entering the first blank partway.
    for n in range(2):
        s += (f'hout_{label}{n}:\nmov.w @r1,r0\ntst #4,r0\nbf hout_{label}{n}\n'
              f'hin_{label}{n}:\nmov.w @r1,r0\ntst #4,r0\nbt hin_{label}{n}\n')
    return s + put16(0x25D00000, 4)


def finish():
    s = ''
    for a in [0x25D00016, 0x25D00010, 0x25D00014]:
        s += read(a, 2)
    s += ('load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n'
          + record())
    return s


def sprite():
    s = 'load 0x0600ea00,r8\n' + put16(0x25D00002, 2)
    # Valid exclusive-hires NBG0/NBG1 bitmap multiplexing. Both backgrounds
    # are opaque blue (index0); high-priority sprites win, low ones lose.
    for off,v in [(0xE,0), (0x10,0x4444), (0x12,0xFFFF), (0x14,0xFFFF),
                  (0x16,0xFFFF), (0x18,0x5555), (0x1A,0xFFFF), (0x1C,0xFFFF),
                  (0x1E,0xFFFF), (0x20,0x303), (0x28,0x1616), (0x3C,0x20),
                  (0xF8,0x303), (0x7C,1), (0x8C,1), (0xAC,3), (0xAE,0xF800),
                  (0xE0,0x20), (0xF0,0x0204)]:
        s += put16(0x25F80000+off,v)
    s += put16(0x25F00000,0x7C00) + put16(0x25F00002,0x7C1F)
    s += operand(0x106,9) + 'mov r9,r0\nmov #2,r2\nand r2,r0\ntst r0,r0\nbt low\n'
    for off,v in [(0x78,2),(0x88,2),(0x80,1)]:
        s += put16(0x25F80000+off,v)
    s += 'bra configured\nnop\nlow:\n'
    for off,v in [(0x78,1),(0x88,1),(0x80,0)]:
        s += put16(0x25F80000+off,v)
    s += 'configured:\nload 0x25f80000,r1\nmov.w r9,@r1\n' + mode_window('sprites')
    for off,v in [(0,9),(20,511),(22,255)]:
        s += put16(0x25C00000+off,v)
    for i,(x,y) in enumerate([(0,0),(312,238),(344,238)],1):
        for off,v in [(0,0),(4,0xA8),(8,0x100),(10,0x102),(12,x),(14,y)]:
            s += put16(0x25C00000+i*32+off,v)
    s += operand(0x102,0)+'load 0x25c0002c,r1\nmov.w r0,@r1\n'
    s += put16(0x25C00080,0x8000)
    for i,v in enumerate(TEXTURE):
        s += put16(0x25C00800+i*2,v)
    s += operand(0x100,0)+'load 0x25c00800,r1\nmov.w r0,@r1\n'
    # A CPU word at the final storage location detects enlarged/8-bit layouts.
    s += put16(0x25CBFFFE,0xABCD)
    s += operand(0x104,0)+'tst r0,r0\nbt omitted\nload 0x25d00004,r1\nmov.w r0,@r1\n'
    s += ('load 0x25d00010,r1\ndrawn:\nmov.w @r1,r0\ntst #2,r0\nbt drawn\nomitted:\n')
    # Read actual framebuffer words and compute an unsigned checksum on guest.
    s += 'mov #0,r10\n'
    for y in [0,1]:
        for x in range(8):
            s += read(0x25C80000+y*1024+x*2,2).replace('mov.l r0,@r8','add r0,r10\nmov.l r0,@r8')
    s += 'mov r10,r0\n'+record()
    for x,y in [(319,238),(319,239),(351,238),(351,239),(511,255)]:
        s += read(0x25C80000+y*1024+x*2,2)
    s += finish()
    s += put16(0x25D00002,3)+blank('publish')+put16(0x25D00002,2)
    s += 'load 0x600d,r0\n'+record()+'park:\nsleep\nbra park\nnop\n'
    return encode('vdp1-hdtv',s,[(0x100,0x801F),(0x102,0),(0x104,1),(0x106,0x8006)])


def erase():
    s = 'load 0x0600eb00,r8\n'
    s += operand(0x100,0)+'load 0x25f80000,r1\nmov.w r0,@r1\n'+mode_window('erase')
    # Automatic changes capture A at OUT. VBE stays clear throughout. No draw
    # command is involved: display erase would incorrectly consume the suffix.
    for off,v in [(2,0),(6,0x1234),(8,0),(10,64<<9|255)]:
        s += put16(0x25D00000+off,v)
    words = [0,29339,29340,53135,53136,131071]
    for word in words:
        s += put16(0x25C80000+word*2,0xFFFF)
    s += out_blank('capture')
    s += put16(0x25D00006,0x5678)+put16(0x25D00008,0x0201)+put16(0x25D0000A,0x0602)
    # Only display physical0 should be erased using retained A, not pending B.
    for word in words:
        s += put16(0x25C80000+word*2,0xDDDD)
    s += blank('automatic')+put16(0x25D00002,2)
    for word in words:
        s += read(0x25C80000+word*2,2)
    s += finish()
    # Holding buffers and advancing beyond display cannot restart A on CPU0.
    s += blank('hold')+read(0x25C80000+53136*2,2)
    s += 'load 0x600d,r0\n'+record()+'park:\nsleep\nbra park\nnop\n'
    return encode('vdp1-hdtv-erase',s,[(0x100,0x8006)])


def build():
    files,a=sprite(); other,b=erase(); files.update(other)
    return files,[a,b]


if __name__ == '__main__':
    parser=argparse.ArgumentParser(); parser.add_argument('--check',action='store_true'); args=parser.parse_args()
    files,digests=build()
    for name,data in files.items():
        path=HERE/name
        if args.check:
            assert path.read_bytes()==data,f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{name}: {hashlib.sha256(data).hexdigest()}')
    print('Expanded ROM SHA-256:',*digests)
