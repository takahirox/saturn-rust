#!/usr/bin/env python3
"""Original MIT two-port direct-pad diagnostic; SMPC §3.4 table3.36.

Consecutive guest bus reads sample the model's retained bank before settling;
a guest delay then samples the selected bank. Host supplies only button pins.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put8

HERE=Path(__file__).resolve().parent


def build():
    s='load 0x0600e000,r8\n'
    for a,v in [(0x20100079,0x60),(0x2010007B,0x60),(0x2010007D,3),
                (0x20100075,0x60),(0x20100077,0x60)]:
        s+=put8(a,v)
    s+='mov #32,r7\ninitial_settle:\ndt r7\nbf initial_settle\n'
    s+='load 0x20100075,r1\nload 0x20100077,r2\n'
    for bank in [0x20,0x40,0,0x60]:
        s+=f'mov #{bank},r0\nmov.b r0,@r1\nmov.b @r1,r3\nmov.b r0,@r2\nmov.b @r2,r4\n'
        s+=f'mov #32,r7\nsettle_{bank}:\ndt r7\nbf settle_{bank}\n'
        s+='mov.b @r1,r5\nmov.b @r2,r6\n'
        for r in [3,4,5,6]:
            s+=f'extu.b r{r},r{r}\nmov.l r{r},@r8\nadd #4,r8\n'
    s+='load 0x600d,r0\nmov.l r0,@r8\nidle:\nsleep\nbra idle\nnop\n'
    code=assembler.assemble(s,base=0x200)
    text=['# Original digital pad settling BIOS. MIT: adjacent LICENSE.']
    for address,chunk in [(0,struct.pack('>IIII',0x200,0x060FF000,0x200,0x060FF000)),(0x200,code)]:
        for i in range(0,len(chunk),16):
            words=struct.unpack('>'+'H'*(len(chunk[i:i+16])//2),chunk[i:i+16])
            text.append(f'{address+i:04X} '+' '.join(f'{w:04X}' for w in words))
    expected=[0x3C,0x3C,0x3D,0x3B,0x5D,0x5B,0x5B,0x57,
              0x1B,0x17,0x17,0x1E,0x77,0x7E,0x7C,0x7C,0x600D]
    return {'digital-timing.hex':('\n'.join(text)+'\n').encode(),
            'digital-timing.tsv':''.join(f'{v:08X}\n' for v in expected).encode()}


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
