#!/usr/bin/env python3
"""Original MIT VDP1 HSS/EOS guest. ST-013 §6.3/fig6.5 and §4.2.
All command lists, textures, draw polling and results are guest MMIO/instructions.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16,read
HERE=Path(__file__).resolve().parent

def build():
    s='load 0x0600d000,r8\n'
    s+=put16(0x25d00002,2)
    for at,value in [(0,9),(20,511),(22,255),(64,0x8000)]:s+=put16(0x25c00000+at,value)
    for x in range(8):s+=put16(0x25c00800+x*2,0x8000+x)
    expected=[]
    for n,(kind,hss,eos,flip,indices) in enumerate([
        (1,False,False,False,[0,2,4,7]),(1,True,False,False,[0,2,4,6]),
        (1,True,True,False,[1,3,5,7]),(2,True,False,False,[0,2,4,6]),
        (2,True,True,False,[1,3,5,7]),(2,True,False,True,[6,4,2,0]),
        (2,True,True,True,[7,5,3,1]),
    ]):
        s+=put16(0x25d00002,2|(16 if eos else 0))
        for at,value in [(0,kind|(16 if flip else 0)),(4,0x68|(0x1000 if hss else 0)),
                         (8,0x100),(10,0x101),(12,10),(14,20),(16,13),(18,20),
                         (20,13),(22,20),(24,10),(26,20)]:s+=put16(0x25c00020+at,value)
        s+=put16(0x25d00004,1)
        s+=f'load 0x25d00010,r1\ndraw_wait_{n}:\nmov.w @r1,r0\ntst #2,r0\nbt draw_wait_{n}\n'
        for x in range(4):s+=read(0x25c80000+(20*512+10+x)*2,2)
        expected += [0x8000+i for i in indices]
    s+=read(0x25d00016,2)
    expected += [0x1090,0x600d] # EOS + manual-change mode, PTMR=1, TVMR=0.
    s+=put16(0x25d00002,0x13) # publish last draw buffer on next field
    for at,value in [(0,0x8000),(0xe0,0x20),(0xf0,3)]:s+=put16(0x25f80000+at,value)
    s+='load 0x600d,r0\nmov.l r0,@r8\npark:\nsleep\nbra park\nnop\n'
    code=assembler.assemble(s,base=0x200)
    rom=bytearray(524288);rom[:8]=struct.pack('>II',0x200,0x060ff000)
    rom[0x200:0x200+len(code)]=code
    lines=['# Original MIT HSS/EOS synthetic BIOS. Adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return {'sprite-shrink.s':s.encode(),'sprite-shrink.hex':('\n'.join(lines)+'\n').encode(),
            'sprite-shrink.tsv':''.join(f'{v:08X}\n' for v in expected).encode()},hashlib.sha256(rom).hexdigest()

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    files,digest=build()
    for name,data in files.items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} sprite-shrink.hex; expanded ROM SHA-256 {digest}')
