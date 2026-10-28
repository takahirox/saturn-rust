#!/usr/bin/env python3
"""Original MIT R-W guest, media and matrix worksheet. Adjacent LICENSE.

Parity is constructed by solving IEC 60908 parity-check matrices with row
reduction, independently of the production bounded-distance syndrome decoder.
The 4 reference codewords are separately frozen in native/browser oracles.
SRW1 starts AFTER figure22 permutation/delays, BEFORE P and Q correction.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler as a
from build_dma_startup import call, store
from build_clock import put16
HERE=Path(__file__).resolve().parent
BASE=0x06004000
R=0x06010000

def mul(a,b):
    r=0
    for _ in range(6):
        if b&1:r^=a
        a<<=1
        if a&64:a^=0x43
        b>>=1
    return r

def power(a,n):
    r=1
    for _ in range(n):r=mul(r,a)
    return r

def parity(data,positions):
    matrix=[]
    for row in range(len(positions)):
        coeff=[power(2,(len(data)-1-p)*row%63) for p in positions]
        rhs=0
        for p,v in enumerate(data):
            if p not in positions:rhs^=mul(v,power(2,(len(data)-1-p)*row%63))
        matrix.append(coeff+[rhs])
    for i in range(len(positions)):
        j=next(j for j in range(i,len(positions)) if matrix[j][i])
        matrix[i],matrix[j]=matrix[j],matrix[i]
        inv=power(matrix[i][i],62)
        matrix[i]=[mul(x,inv) for x in matrix[i]]
        for j in range(len(positions)):
            if i!=j:
                v=matrix[j][i]
                matrix[j]=[x^mul(v,y) for x,y in zip(matrix[j],matrix[i])]
    for i,p in enumerate(positions):data[p]=matrix[i][-1]
    return data

def codeword(k,changed=False):
    d=[56,k,0,0]+[(k*7+i+int(changed))&63 for i in range(16)]+[0]*4
    d[:4]=parity(d[:4],[2,3])
    return bytes(parity(d,[20,21,22,23]))

def build():
    s='mov #-16,r0\nldc r0,sr\nload 0x06000000,r0\nldc r0,vbr\n'
    s+=store(0x06000140,BASE+0x1800)+store(0x0600012c,BASE+0x1900)
    for off in [0x300,0x304,0x308,0x3fc]:s+=store(R+off,0)
    # Both external CD and DMA0 IRQs execute real exception/RTE paths.
    s+=store(0x25fe00a0,0xffff77ff)+store(0x25fe00a8,1)
    s+=put16(0x25890008,0)+put16(0x2589000c,0x400)
    s+='mov #0,r0\nldc r0,sr\n'
    s+=f'load {BASE+0x1f00:#x},r1\nmov.l @r1,r4\nmov #0,r5\nmov #0,r6\nmov #0,r7\nbsr command\nnop\n'
    s+=put16(0x25b00400,15)+put16(0x25b00216,0xff)+put16(0x25b00236,0xef)
    s+=call(0x1080,214,0,0)
    s+='load 0x26010300,r1\nwait_subcode:\nmov.l @r1,r0\nmov #4,r2\ncmp/hs r2,r0\nbf wait_subcode\n'
    for k in range(4):
        rec=R+k*0x80
        s+=call(0x2001)
        s+=f'load 0x25890018,r1\nmov.w @r1,r0\nload 0x8000,r2\ntst r2,r0\nbt populated{k}\nbra empty\nnop\npopulated{k}:\n'
        s+=f'load 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\nload {rec+0x34:#x},r1\nmov.l r0,@r1\n'
        if k%2:
            for off,v in [(0,0x05818000),(4,0x05e00000),(8,24),(12,1),(20,7),(16,0x101)]:
                s+=store(0x25fe0000+off,v)
            s+=f'load 0x26010304,r1\nwait_dma{k}:\nmov.l @r1,r0\nmov #{(k+1)//2},r2\ncmp/hs r2,r0\nbf wait_dma{k}\n'
        s+=f'load {0x25e00000 if k%2 else 0x25818000:#x},r4\nload {rec:#x},r5\nmov #0,r6\n'
        for i in range(12):
            s+='mov.w @r4,r0\nextu.w r0,r0\nadd r0,r6\nmov.l r0,@r5\nadd #4,r5\n'
            if k%2:s+='add #2,r4\n'
        s+=f'load {rec+0x30:#x},r1\nmov.l r6,@r1\n'
        s+=f'load {rec:#x},r1\nmov.l @r1,r0\nextu.b r0,r0\nload {rec+0x38:#x},r1\nmov.l r0,@r1\n'
        s+=call(0x0600)
        s+=f'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\nload {rec+0x3c:#x},r1\nmov.l r0,@r1\n'
    s+=store(R+0x308,4)+'bra done\nnop\nempty:\n'+store(R+0x308,0)
    s+='done:\n'+store(R+0x3fc,0x600d)+'park:\nsleep\nbra park\nnop\n'
    s+=('command:\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\n'
        'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\n'
        'add #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'
        'load 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\n'
        'load 0x25890024,r1\nmov.w @r1,r0\nrts\nnop\n')
    push='mov.l r0,@-r15\nmov.l r1,@-r15\n'
    pop='mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    handler=push+'load 0x26010300,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'
    handler+=put16(0x25890008,0xfbff)+store(0x25fe00a8,1)+pop
    dma=push+'load 0x26010304,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'+pop
    program=bytearray(0x2000)
    for offset,source in [(0,s),(0x1800,handler),(0x1900,dma)]:
        code=a.assemble(source,base=BASE+offset)
        assert len(code)<(0x1800 if offset==0 else 0x100),(offset,len(code))
        program[offset:offset+len(code)]=code
    struct.pack_into('>I',program,0x1f00,0x0402)
    iso=a.build(bytes(program),(HERE/'LICENSE').read_bytes(),'RW SUBCODE','RWTEST-001 V1.000')
    cue='REM SATURN_RW "cd-rw.srw"\nFILE "cd-rw.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "cd-rw-audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n'
    # Constant stereo has an exact independent oracle; subcode cannot alter PCM.
    audio=struct.pack('<hh',1000,-2000)*(588*30)
    files={'cd-rw.iso':iso,'cd-rw-audio.bin':audio,'cd-rw.cue':cue.encode(),
           'cd-rw.s':s.encode(),'cd-rw-irq.s':handler.encode(),'cd-rw-dma.s':dma.encode()}
    for variant in ['','-changed','-corrected','-bad','-header-bad']:
        packs=[]
        for n in range(30*4):
            d=bytearray(codeword(n%4,variant=='-changed'))
            if n==0:
                if variant=='-corrected':d[0]^=17;d[8]^=39
                if variant=='-bad':d[4]^=1;d[5]^=2;d[6]^=4
                if variant=='-header-bad':d[0]^=1;d[1]^=2;d[4]^=4
            packs.append(d)
        files['cd-rw'+variant+'.srw']=b'SRW1'+struct.pack('>II',214,30)+b''.join(packs)
    return files
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
