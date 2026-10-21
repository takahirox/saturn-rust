#!/usr/bin/env python3
"""Original MIT ECMA-130 Mode 2 runtime-read guest and mixed-track media.

ECMA-130 §14 fig12 / §14.2: 2336 unrestricted bytes, without XA coding.
ST-162 §5.4 / §8.2: host transfer size 2336 includes all bytes after header.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_cd_drive import command

HERE=Path(__file__).resolve().parent


def build():
    base=0x0600D000
    def cmd(words,name,offset): return command(words,name,base+offset)
    s=cmd([0x0400,0,0,0],'init',0)
    s+=cmd([0x6001,0,0,0],'length',0x10)
    s+=cmd([0x3000,0,0,0],'connect',0x20)
    s+=cmd([0x1080,150,0x80,1],'play',0x30)
    s+='wait:\n'+cmd([0x5100,0,0,0],'count',0x40)
    s+='tst #255,r0\nbt wait\n'
    s+=cmd([0x6300,0,0,1],'get',0x50)
    s+=('load 0x25818000,r1\nload 1168,r3\nmov #0,r4\n'
        'load 0x26010000,r2\ncopy:\nmov.w @r1,r0\nextu.w r0,r0\n'
        'add r0,r4\nmov.w r0,@r2\nadd #2,r2\ndt r3\nbf copy\n'
        'load 0x2600d080,r2\nmov.l r4,@r2\n')
    s+=cmd([0x0600,0,0,0],'end',0x60)
    s+=cmd([0x5100,0,0,0],'after',0x70)
    s+='load 0x2600d084,r1\nload 0x600d,r0\nmov.l r0,@r1\nsleep\n'
    rom=bytearray(524288);struct.pack_into('>II',rom,0,0x200,0x060FF000)
    code=assembler.assemble(s,base=0x200);rom[0x200:0x200+len(code)]=code
    lines=['# Original non-XA Mode2 runtime-read ROM. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    raw=bytearray(2352);raw[1:11]=b'\xff'*10;raw[12:16]=bytes([0,2,0,2])
    raw[16:]=bytes(((i*73)^(i>>3))&255 for i in range(2336))
    cue=b'FILE "mode2.bin" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00\nFILE "mode2-audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n'
    return {'mode2.hex':('\n'.join(lines)+'\n').encode(), 'mode2.bin':raw,
            'mode2-audio.bin':bytes([0x34,0x12,0xCC,0xED])*1176,'mode2.cue':cue}


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data, f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
