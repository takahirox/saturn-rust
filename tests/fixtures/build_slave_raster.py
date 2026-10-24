#!/usr/bin/env python3
"""Original MIT DCC slave raster diagnostic. Adjacent LICENSE.

STN-28 attachment §2.3; SH7604 §§4.4/4.6/4.7, table5.4, §5.3.8.
No firmware or guest results from the host. Both CPUs share reset code.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_clock import put16
from build_hle_system import smpc

HERE = Path(__file__).resolve().parent


def build():
    master = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\n'
              'bf.l slave\n')
    master += put16(0x25F80000, 0x8000)+smpc(2)
    master += 'master_park:\nsleep\nbra master_park\nnop\nslave:\n'
    master += 'load 0x060fe000,r15\n'
    # Slave explicitly selects normal IRL autovectors and installs RAM vectors.
    master += put16(0xFFFFFEE0, 0)
    master += store(0x26004104, 0x1000)+store(0x2600410C, 0x1400)
    master += 'load 0x26004000,r0\nldc r0,vbr\n'
    master += 'load 0xfffffe92,r1\nmov #1,r0\nmov.b r0,@r1\n'
    # Prime H-handler instruction lines through real unified-cache reads so
    # two complete entries fit within one inherited 428-clock HBlank.
    master += 'load 0x1000,r1\n' + 'mov.l @r1,r0\nadd #16,r1\n'*16
    master += ('load 0x100,r1\nmov.l @r1,r10\nadd #4,r1\nmov.l @r1,r11\n'
               'load 0x2600e000,r8\nload 0x2600e020,r9\n'
               # Start from active H phase, DISP enabled only for status evidence.
               'load 0x25f80004,r4\nactive:\nmov.w @r4,r0\ntst #4,r0\nbf active\n'
               'load 0x108,r1\nmov.l @r1,r0\nldc r0,sr\nnop\n'
               'wait_irq:\nsleep\nmov.l @r9,r0\nmov #2,r1\ncmp/eq r1,r0\nbf wait_irq\n'
               # Ordinary code only runs after two VBlank handler returns.
               'load 0x2600e040,r1\nmov r10,r0\nadd r11,r0\nmov.l r0,@r1\n'
               'add #4,r1\nload 0x600d,r0\nmov.l r0,@r1\n'
               'park:\nsleep\nbra park\nnop\n')
    # Each handler uses separate live accumulator, records two beam lines and
    # status samples, and edits stacked SR on its second entry only.
    handlers=[]
    for label, base, acc, add, mask in [('h',8,10,7,0x20),('v',9,11,11,0xF0)]:
        h=f'mov.l @r{base},r0\nadd #1,r0\nmov.l r0,@r{base}\nadd #{add},r{acc}\n'
        h+=f'mov r{base},r2\nadd #4,r2\nmov.l r{acc},@r2\n'
        # Count 1 writes slots8/12, count2 writes slots16/20.
        h+='mov r0,r3\nshll2 r3\nshll r3\n'
        h+=f'add r{base},r3\nload 0x25f80002,r1\nmov.w @r1,r2\n'
        h+='load 0x25f8000a,r1\nmov.w @r1,r2\nextu.w r2,r2\nmov.l r2,@r3\n'
        h+='add #4,r3\nload 0x25f80004,r1\nmov.w @r1,r2\nextu.w r2,r2\nmov.l r2,@r3\n'
        h+='mov #2,r1\ncmp/eq r1,r0\nbf first_'+label+'\n'
        h+=f'mov r15,r1\nadd #4,r1\nload {mask},r0\nmov.l r0,@r1\n'
        h+='first_'+label+':\nrte\nnop\n'
        handlers.append(h)
    rom=bytearray(524288)
    struct.pack_into('>IIII',rom,0,0x200,0x060FF000,0x200,0x060FE000)
    struct.pack_into('>III',rom,0x100,3,5,0x10)
    chunks=[(0x200,master),(0x1000,handlers[0]),(0x1400,handlers[1])]
    source='; Original MIT: adjacent LICENSE.\n'
    for at,s in chunks:
        code=assembler.assemble(s,base=at)
        assert len(code)<0xE00 if at==0x200 else len(code)<0x400
        rom[at:at+len(code)]=code
        source+=f'.section {at:#x}\n'+s
    rows=['# Original MIT slave raster guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):rows.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'slave-raster.s':source.encode(),'slave-raster.hex':('\n'.join(rows)+'\n').encode()},hashlib.sha256(rom).hexdigest()


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');a=p.parse_args()
    files,digest=build()
    for name,data in files.items():
        path=HERE/name
        if a.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
    print(f'{"Verified" if a.check else "Built"} slave-raster; ROM SHA-256 {digest}')
