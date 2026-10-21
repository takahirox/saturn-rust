#!/usr/bin/env python3
"""Original MIT guest for ST-169 §3.2/table3.2, fig3.11 and fig3.13.

Exercises status/continue BREAK, pre-collection BREAK, active collection BREAK,
and packed digital/ID16 port records under all selected zero-byte modes.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import record
from build_smpc import byte, issue, idle

HERE=Path(__file__).resolve().parent


def params(status,mode):
    return byte(0x100001,status)+byte(0x100003,mode|0x0A)+byte(0x100005,0xF0)+issue(0x10)


def read_byte(at):
    return f'load {at:#x},r1\nmov.b @r1,r0\nextu.b r0,r0\n'+record()


def build():
    s='load 0x2600ec00,r8\n'+params(1,0)
    s+='load 0x100061,r1\nstatus:\nmov.b @r1,r0\ntst #64,r0\nbt status\n'
    s+=read_byte(0x100063)+byte(0x100001,0x41)+read_byte(0x100063)
    s+=params(0,0)+byte(0x100001,0x40)+read_byte(0x100063)
    s+=params(0,0)
    s+=('load 0x25f80004,r1\nblank:\nmov.w @r1,r0\ntst #8,r0\nbt blank\n'
        'active:\nmov.w @r1,r0\ntst #8,r0\nbf active\n')
    s+=read_byte(0x100063)+byte(0x100001,0x40)+read_byte(0x100063)
    records=[1,0,0,1,0]
    reports=[
        [0xF1,2,0xEE,0xFF,0xF1,0x16,0x77,0xFF,0x12,0x34,0x56,0x78],
        [0xF1,0x16,0x77,0xFF,0x12,0x34,0x56,0x78],
        [0xF1,2,0xEE,0xFF],
    ]
    for n,(mode,report) in enumerate(zip([0,0x30,0xC0],reports)):
        s+=params(0,mode)+idle(f'packet{n}')
        for i in range(16): s+=read_byte(0x100021+i*2)
        s+=read_byte(0x100061)+read_byte(0x100063)
        records+=report+[0xFF]*(16-len(report))+[0xC0|(mode>>4),0]
    s+='load 0x600d,r0\n'+record()+'done:\nsleep\nbra done\nnop\n'
    records += [0x600D]
    rom=bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x060ff000)
    code=assembler.assemble(s,base=0x200)
    assert len(code)<0x8000
    rom[0x200:0x200+len(code)]=code
    lines=['# Original SMPC boundary BIOS. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'smpc-boundaries.hex':('\n'.join(lines)+'\n').encode(),
            'smpc-boundaries.tsv':''.join(f'{v:08X}\n' for v in records).encode()}


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
