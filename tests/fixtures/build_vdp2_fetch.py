#!/usr/bin/env python3
"""Original MIT beam/fetch diagnostic; ST-058-R2 §§3.3/5.3/8.1.

All pixels, tables, bus grants and observations come from SH-2 execution.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16

HERE = Path(__file__).resolve().parent


def line(n):
    return (f'load {n},r3\nline_{n}:\nload 0x25f80002,r1\nmov.w @r1,r0\n'
            'load 0x25f8000a,r1\nmov.w @r1,r0\n'
            f'cmp/hs r3,r0\nbf line_{n}\n')


def build():
    s = '; Original MIT VDP2 display-fetch and line-window guest.\n'
    for at,value in [(0x10,0x4444),(0x12,0xEEEE),(0x14,0xFFFF),(0x16,0xFFFF),
                     (0x18,0xFFFF),(0x1A,0xFFFF),(0x1C,0xFFFF),(0x1E,0xFFFF),
                     (0x20,0x101),(0x28,0x32),(0xF8,1),(0xAC,3),(0xAE,0xC000),
                     (0xC6,223),(0xD8,0x8003)]:
        s += put16(0x25F80000+at,value)
    s += put16(0x25E00000,0x801F)+put16(0x25E20000,0x801F)
    s += put16(0x25F00100,0x801F)+put16(0x25F00106,0x801F)
    # RBG0 later samples source X=1*kx, distinguishing coefficients 1/2
    # through two authored pixels. The start/matrix is guest-written RAM.
    s += put16(0x25E00002,0x801F)+put16(0x25E00004,0x83E0)
    for at,value in [(0x2A,0x3200),(0xFC,2),(0xBC,3),(0xBE,0x8000),(0xB4,3),(0xB6,2)]:
        s += put16(0x25F80000+at,value)
    for address in [0x25E70000,0x25E7001C]:
        s += f'load {address:#x},r1\nload 0x10000,r0\nmov.l r0,@r1\n'
    s += 'load 0x25e40000,r1\nload 512,r2\nload 0x0800,r0\ncoefficient_fill:\n'
    s += 'mov.w r0,@r1\nadd #2,r1\ndt r2\nbf coefficient_fill\n'
    s += put16(0x25E40000,0x0400)
    # A full-width window, read separately for every output line.
    s += 'load 0x25e60000,r1\nload 224,r2\nload 638,r4\nmov #0,r0\nwindow_fill:\n'
    s += 'mov.w r0,@r1\nadd #2,r1\nmov.w r4,@r1\nadd #2,r1\ndt r2\nbf window_fill\n'
    s += put16(0x25F80000,0x8000)
    s += ('load 0x25f80004,r1\nblank_in:\nmov.w @r1,r0\ntst #8,r0\nbt blank_in\n'
          'blank_out:\nmov.w @r1,r0\ntst #8,r0\nbf blank_out\n')
    s += line(16)+put16(0x25F80010,0x4FFF) # one read cannot supply RGB555
    s += line(32)+put16(0x25F80010,0xFFFF)+put16(0x25F80018,0x4444) # wrong bank
    s += line(48)+put16(0x25F80010,0x4444)
    s += line(64)+put16(0x25F8000E,0x100)+put16(0x25F8003C,1) # A1 has no fetch
    s += line(80)+put16(0x25F80014,0x4444)+put16(0x25F80016,0xEEEE)
    s += line(96)+put16(0x25F80028,0x12)+put16(0x25F80014,0x4FFF) # 8-bit needs two
    s += line(112)+put16(0x25F80014,0x44FF)
    # A table write after line start must not shrink that already-read window.
    s += line(120)+put16(0x25F800D0,2)
    s += line(128)+put16(0x25E60000+128*4+2,0)+put16(0x25E60000+129*4+2,0)
    s += line(144)+put16(0x25F8000E,0x103)+put16(0x25F80020,0x1111)
    s += put16(0x25E40000,0x0800) # this line already captured coefficient 1
    s += line(160)+put16(0x25E40000,0x0400) # this line already captured coefficient 2
    s += line(176)+'load 0x25e7005c,r1\nload 0x10000,r0\nmov.l r0,@r1\n'
    s += line(192)+put16(0x25F8000E,0x113) # allow actual per-dot coefficient reads
    s += line(208)+put16(0x25F8000E,0x103) # remove the per-dot port again
    s += ('load 0x25f80004,r1\nfinish:\nmov.w @r1,r0\ntst #8,r0\nbt finish\n'
          'load 0x2600df00,r1\nload 0x600d,r0\nmov.l r0,@r1\npark:\nsleep\nbra park\nnop\n')
    code=assembler.assemble(s,base=0x200)
    rom=bytearray(524288);struct.pack_into('>II',rom,0,0x200,0x060FF000)
    rom[0x200:0x200+len(code)]=code
    lines=['# Original MIT VDP2 display-fetch synthetic BIOS. Adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return {'vdp2-fetch.s':s.encode(),'vdp2-fetch.hex':('\n'.join(lines)+'\n').encode()},hashlib.sha256(rom).hexdigest()


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    files,digest=build()
    for name,data in files.items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} vdp2-fetch.hex; expanded ROM SHA-256 {digest}')
