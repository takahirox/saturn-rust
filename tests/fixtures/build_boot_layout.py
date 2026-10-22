#!/usr/bin/env python3
"""Original MIT multi-area/interleaved-first-read/dual-stack HLE disc.

ST-040 §§3.2.4/4.1/4.3/4.5–4.7 and ISO9660 §9.1. Only original SH-2
instructions and no proprietary area/security object, firmware or SDK code.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent


def build():
    source = ('load 0x2600a000,r1\nmov.l r15,@r1\n'
              'load 0x06005000,r1\nmov.l @r1,r0\n'
              'load 0x06005800,r1\nmov.l @r1,r2\nadd r2,r0\n'
              'load 0x2600a004,r1\nmov.l r0,@r1\n'
              'load 0x06004200,r0\nload 0x06000250,r1\nmov.l r0,@r1\n')
    source += smpc(2) + wait(0x0600A014, 'slave_finished')
    source += 'load 0x2600a008,r1\nload 0x600d,r0\nmov.l r0,@r1\nsleep\n'
    slave = ('load 0x2600a010,r1\nmov.l r15,@r1\n'
             'mov #9,r0\nload 0x2600a014,r1\nmov.l r0,@r1\nsleep\n')
    payload = bytearray(0x1804)
    master = assembler.assemble(source, base=0x06004000)
    assert len(master) < 0x200
    payload[:len(master)] = master
    code = assembler.assemble(slave, base=0x06004200)
    payload[0x200:0x200+len(code)] = code
    struct.pack_into('>I',payload,0x1000,0x12345678)
    struct.pack_into('>I',payload,0x1800,0x87654321)
    image = bytearray(assembler.build(bytes(payload),(HERE/'LICENSE').read_bytes(),
                                     'BOOT LAYOUT DIAGNOSTIC','BLAYOUT V1.000'))
    image[0x40:0x50] = b'JTUE' + b' '*12
    image[0xE00:0xF00] = bytes(256)
    # Original AIP after the four 32-byte area blocks, with an authored literal.
    struct.pack_into('>HHHHI',image,0xE80,0xD001,0x402B,9,9,0x06004000)
    struct.pack_into('>I',image,0xEC,0x060FD000)
    record = 20*2048+68
    image[record+2:record+10] = assembler.both32(31)
    image[record+1] = 1
    image[record+26:record+28] = bytes([2,1])
    image[21*2048:25*2048] = b'\xa5'*(4*2048)
    image[31*2048:32*2048] = b'\xbb'*2048
    image[34*2048:35*2048] = b'\xee'*2048
    for logical in range(4):
        lba = 32 + (logical//2)*3 + logical%2
        data = payload[logical*2048:(logical+1)*2048]
        image[lba*2048:lba*2048+len(data)] = data
    return {'boot-layout.iso': bytes(image),
            'boot-layout.tsv': b'060FF000\n99999999\n0000600D\n00000000\n060FD000\n00000009\n'}


if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
