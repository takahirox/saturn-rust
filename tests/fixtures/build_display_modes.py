#!/usr/bin/env python3
"""Original MIT TV-mode/INTBACK BIOS. ST-058-R2 §§2.1–2.5/8.1;
ST-169 §§2.3.9–10/3.2. ROM word100 is the guest's TVMD operand.
No firmware, SDK objects, or host-generated frame/audio output.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16, put8, read
from build_hle_system import smpc
HERE=Path(__file__).resolve().parent

def build():
    s=put8(0x20100063,1)+'load 0x100,r1\nmov.w @r1,r0\ntst #1,r0\nbt slow_clock\nmov #14,r0\nbra set_clock\nnop\nslow_clock:\nmov #15,r0\nset_clock:\nload 0x2010001f,r1\nmov.b r0,@r1\nload 0x20100063,r1\nclock_wait:\nmov.b @r1,r0\ntst #1,r0\nbf clock_wait\n'
    s+='load 0x100,r1\nmov.w @r1,r0\nload 0x25f80000,r1\nmov.w r0,@r1\n'
    for at,value in [(0x10,0x4444),(0x12,0xEEEE),(0x18,0x4444),(0x1A,0xEEEE),
                     (0x20,1),(0x28,0x36),(0x78,1),(0x7c,1),(0xf8,3),(0xac,3),(0xae,0x8000)]:
        s+=put16(0x25f80000+at,value)
    s+=put16(0x25e70000,0x801f)
    # A real 512x512 direct-color bitmap. Each authored row has four colors;
    # the far-right word tests high-resolution access beyond the old width.
    for y in [0,223,239,255,447,479,511]:
        for x,color in [(0,0x801f),(1,0x83e0),(2,0xfc00),(3,0xffff),(511,0xfc1f)]:
            s+=put16(0x25e00000+(y*512+x)*2,color)
    s+='load 0x0600e000,r8\n'
    for n in range(2):
        s+=put8(0x20100001,0)+put8(0x20100003,8)+put8(0x20100005,0xf0)
        s+=smpc(0x10).replace('wait_smpc16',f'collect_{n}')
        s+=read(0x25f80002,2)+read(0x25f8000a,2)
        s+=read(0x20100061,1)
        for i in range(11):s+=read(0x20100021+i*2,1)
    # Capture both fields at the first visible-to-blank transition. Reads of
    # EXTEN latch real counters; the polling loop never writes result values.
    for n in range(2):
        s+=f'load 0x25f80004,r1\nwait_active_{n}:\nmov.w @r1,r0\ntst #8,r0\nbf wait_active_{n}\nwait_blank_{n}:\nmov.w @r1,r0\ntst #8,r0\nbt wait_blank_{n}\n'
        s+=read(0x25f80004,2)+read(0x25f80002,2)+read(0x25f8000a,2)
    s+='load 0x600d,r0\nmov.l r0,@r8\npark:\nsleep\nbra park\nnop\n'
    code=assembler.assemble(s,base=0x200)
    rom=bytearray(524288)
    rom[:8]=struct.pack('>II',0x200,0x060ff000)
    rom[0x100:0x102]=struct.pack('>H',0x8000)
    rom[0x200:0x200+len(code)]=code
    lines=['# Original MIT TV mode + INTBACK synthetic BIOS. Adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return {'display-modes.s':s.encode(),'display-modes.hex':('\n'.join(lines)+'\n').encode()},hashlib.sha256(rom).hexdigest()

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    files,digest=build()
    for name,data in files.items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} display-modes.hex; expanded ROM SHA-256 {digest}')
