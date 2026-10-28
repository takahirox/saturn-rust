#!/usr/bin/env python3
"""Original MIT timed interrupt guest; adjacent LICENSE.

H Rev4 table5.8/fig5.7: recognition5 + entry8 + handler IF1 + ID1
+ load EX1 + peripheral read2 = 18 states from FRT request to FRC latch.
FRT compare K asserts at the edge leaving K (H figs11.6/11.7), so the
handler observes K+1+floor(18/8) = K+3, with single-state cache RAM stack
and vector accesses. No emulator import or measured-output generation.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc
HERE = Path(__file__).resolve().parent
K = 32
HANDLER = '''mov.b @r4,r10
mov.b @r5,r11
extu.b r11,r11
shll8 r10
or r10,r11
mov.l r11,@r8
add #4,r8
mov r15,r12
add #4,r12
mov.l @r12,r13
mov.l r13,@r8
add #4,r8
mov.l @r15,r13
mov.l r13,@r8
add #4,r8
stc sr,r13
mov.l r13,@r8
load 0xfffffe10,r12
mov #0,r13
mov.b r13,@r12
rte
nop
'''
KERNEL = '''mov #80,r0
ldc r0,sr
nop
mov #0,r0
mov.b r0,@r7
sleep
mov #7,r6
rts
nop
'''

def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
         'load 0x2600d000,r8\nbra common\nnop\nslave:\nload 0x2600d100,r8\ncommon:\n')
    s += store(0xFFFFFFE8,0xA55A0000)
    s += 'load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
    # Each CPU installs the original bytes into its private cache RAM.
    for address, source in [(0xC0000400,HANDLER),(0xC0000500,KERNEL)]:
        code = assembler.assemble(source,base=address)
        for i in range(0,len(code),2):
            s += f'load {address+i:#x},r1\nload {int.from_bytes(code[i:i+2],"big"):#x},r0\nmov.w r0,@r1\n'
    s += store(0xC0000180,0xC0000400) # vector 0x60
    s += 'load 0xc0000000,r0\nldc r0,vbr\nload 0xc0000780,r15\n'
    # Reset divider and all FRT state, then stop its external clock selection.
    s += 'load 0xfffffe91,r1\nmov #2,r0\nmov.b r0,@r1\nmov #0,r0\nmov.b r0,@r1\n'
    for addr,value in [(0xFFFFFE16,3),(0xFFFFFE14,0),(0xFFFFFE15,K),(0xFFFFFE10,8)]:
        s += f'load {addr:#x},r1\nmov #{value},r0\nmov.b r0,@r1\n'
    s += 'load 0xfffffe60,r1\nload 0x0600,r0\nmov.w r0,@r1\n'
    s += 'load 0xfffffe66,r1\nload 0x0060,r0\nmov.w r0,@r1\n'
    s += 'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe16,r7\nload 0xc0000500,r3\njsr @r3\nnop\n'
    s += 'add #4,r8\nmov.l r6,@r8\nadd #4,r8\nload 0x600d,r0\nmov.l r0,@r8\n'
    # Return to a board stack/vector table for the master SCU DMA interrupt.
    s += 'mov #0,r0\nldc r0,vbr\nload 0x060ff000,r15\n'
    s += 'mov #0,r0\nldc r0,sr\nnop\nload 0x8000,r2\ntst r2,r9\nbf.l finished\n'
    s += 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n'
    s += store(0x25FE00A0,0xB7FF) + store(0x2600d200,0x13579BDF)
    for off,val in [(0,0x0600D200),(4,0x0600D204),(8,4),(12,0x102),(20,7),(16,0x101)]:
        s += store(0x25FE0000+off,val)
    s += 'load 0x2600d018,r1\nwait_scu:\nmov.l @r1,r0\ntst #255,r0\nbt wait_scu\n'
    s += smpc(2)
    s += 'finished:\nmov #-16,r0\nldc r0,sr\nnop\nsleep\nbra finished\nnop\n'
    scu = 'load 0x2600d018,r12\nmov #75,r13\nmov.l r13,@r12\nrte\nnop\n'
    rom=bytearray(524288)
    struct.pack_into('>IIII',rom,0,0x200,0x060FF000,0x200,0x060FE000)
    struct.pack_into('>I',rom,0x4B*4,0x7000)
    for at,source in [(0x200,s),(0x7000,scu)]:
        code=assembler.assemble(source,base=at)
        assert at+len(code)<(0x7000 if at==0x200 else 0x7400)
        rom[at:at+len(code)]=code
    lines=['# Original MIT timed interrupt guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    artifacts={'interrupt-entry.hex': ('\n'.join(lines)+'\n').encode(),
        'interrupt-entry.s': ('; Original MIT; adjacent LICENSE.\n'+s+'\n; private handler\n'+HANDLER+'\n; private kernel\n'+KERNEL+'\n; SCU handler\n'+scu).encode()}
    artifacts['interrupt-entry.sha256']=''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n' for name,data in artifacts.items()).encode()
    return artifacts
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}')
