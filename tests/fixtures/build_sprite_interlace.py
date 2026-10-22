#!/usr/bin/env python3
"""Original MIT VDP1/VDP2 double-density guest. ST-013 §4.2, ST-058 §2.4.
Even and odd logical texture rows are drawn into separate physical framebuffers.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_clock import put16, read
from build_sprite_rotation import blank
HERE=Path(__file__).resolve().parent

def build():
    s='load 0x0600df00,r8\n'+put16(0x25d00002,2)
    for at,value in [(0,9),(20,511),(22,511),(32,0),(36,0xE8),(40,0x100),
                     (42,0x108),(44,10),(46,430),(64,0x8000)]:s+=put16(0x25c00000+at,value)
    for y in range(8):
        for x in range(8):s+=put16(0x25c00800+(y*8+x)*2,0x801f if y%2==0 else 0x83e0)
    for at,value in [(0,0x80c0),(0xe0,0x20),(0xf0,3)]:s+=put16(0x25f80000+at,value)
    # First frame change latches even drawing. Next change displays those rows
    # in an even field while latching odd drawing for the other framebuffer.
    s+='load 0x100,r1\nmov.l @r1,r0\nload 0x25d00002,r1\nmov.w r0,@r1\n'
    for field in range(2):
        s+=blank(f'blank_in_{field}',True)+blank(f'blank_out_{field}',False)
        s+=put16(0x25d00004,1)
        s+=(f'load 0x25d00010,r1\ndraw_{field}:\nmov.w @r1,r0\ntst #2,r0\nbt draw_{field}\n')
        for y in [214,215,216,217,218,219]:s+=read(0x25c80000+(y*512+10)*2,2)
        s+=put16(0x25d00002,0xf)
    s+=blank('last_in',True)+blank('last_out',False)+blank('completed',True)
    s+=read(0x25d00016,2)+store(0x2600df34,0x600d)+'park:\nsleep\nbra park\nnop\n'
    rom=bytearray(524288);struct.pack_into('>II',rom,0,0x200,0x060ff000);struct.pack_into('>I',rom,0x100,0xb)
    code=assembler.assemble(s,base=0x200);rom[0x200:0x200+len(code)]=code
    lines=['# Original MIT double-interlace synthetic BIOS. Adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected=[0]+[0x801f]*4+[0,0]+[0x83e0]*4+[0,0x1070,0x600d]
    return {'sprite-interlace.s':s.encode(),'sprite-interlace.hex':('\n'.join(lines)+'\n').encode(),
            'sprite-interlace.tsv':''.join(f'{v:08X}\n' for v in expected).encode()}

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
