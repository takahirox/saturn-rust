#!/usr/bin/env python3
"""Original MIT beam-synchronized VDP2 register/VRAM diagnostic.

ST-058 §§2.5/7.2/13.1. The guest changes back color and offset registers
after polling real beam counters, then overwrites them during blanking.
Historical output must retain four independently authored color bands.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16

HERE=Path(__file__).resolve().parent


def build():
    s=put16(0x25F80000,0x8000)+put16(0x25F800AC,3)+put16(0x25F800AE,0)
    s+=put16(0x25E60000,0x801F)
    # Begin on a complete field boundary, after the initial setup field.
    s+=('load 0x25f80004,r1\nblank_in:\nmov.w @r1,r0\ntst #8,r0\nbt blank_in\n'
        'blank_out:\nmov.w @r1,r0\ntst #8,r0\nbf blank_out\n')
    for line,color in [(64,0x83E0),(128,0xFC00),(192,0xFFFF)]:
        s+=f'load {line*2},r3\n'
        s+=(f'line_{line}:\nload 0x25f80002,r1\nmov.w @r1,r0\n'
            'load 0x25f8000a,r1\nmov.w @r1,r0\n'
            f'cmp/hs r3,r0\nbf line_{line}\n')
        s+=put16(0x25E60000,color)
    s+=('load 0x25f80004,r1\nfinish:\nmov.w @r1,r0\ntst #8,r0\nbt finish\n')
    # This write must not repaint any line in the completed frame.
    s+=put16(0x25E60000,0x8000)
    s+=put16(0x25F80110,0x20)+put16(0x25F80114,31)
    s+='load 0x2600d900,r1\nload 0x600d,r0\nmov.l r0,@r1\nsleep\n'
    rom=bytearray(524288);struct.pack_into('>II',rom,0,0x200,0x060FF000)
    code=assembler.assemble(s,base=0x200);rom[0x200:0x200+len(code)]=code
    lines=['# Original VDP2 raster-history ROM. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return ('\n'.join(lines)+'\n').encode()


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    data=build();path=HERE/'raster-history.hex'
    if args.check:assert path.read_bytes()==data,'Rebuild raster-history.hex'
    else:path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} raster-history.hex; SHA-256 {hashlib.sha256(data).hexdigest()}')
