#!/usr/bin/env python3
"""Original MIT VDP1 rotation and beam-driven RPRCTL guest.
ST-013 §§1.2/4.1; ST-058 §6.3 pp.152/158–159. Adjacent LICENSE applies.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_clock import put16, read
HERE=Path(__file__).resolve().parent

def blank(label, inside):
    return (f'load 0x25f80004,r1\n{label}:\nmov.w @r1,r0\ntst #8,r0\n'
            f'{"bt" if inside else "bf"} {label}\n')

def line(number):
    return (f'load {number*2},r3\nline_{number}:\nload 0x25f80002,r1\nmov.w @r1,r0\n'
            f'load 0x25f8000a,r1\nmov.w @r1,r0\ncmp/hs r3,r0\nbf line_{number}\n')

def build():
    outputs={}
    for mode in [2,3]:
        s='load 0x0600de00,r8\n'+put16(0x25d00002,2)+put16(0x25d00000,mode)
        # Commands, rather than host pixels: system clip + four polygons + END.
        for at,value in [(0,9),(20,511),(22,511),(160,0x8000)]:s+=put16(0x25c00000+at,value)
        colors=[0x801f,0x83e0,0xfc00,0xffff]
        for i,(x,y) in enumerate([(0,0),(128,0),(0,128),(128,128)]):
            at=0x25c00020+i*32
            for off,value in [(0,4),(4,0xC0),(6,colors[i] if mode==2 else i+1),
                              (12,x),(14,y),(16,x+127),(18,y),
                              (20,x+127),(22,y+127),(24,x),(26,y+127)]:
                s+=put16(at+off,value)
        s+=put16(0x25d00004,1)
        s+='load 0x25d00010,r1\ndrawing:\nmov.w @r1,r0\ntst #2,r0\nbt drawing\n'
        s+=read(0x25d00010,2)
        for x,y in [(127,127),(128,127),(127,128),(128,128)]:
            s+=read(0x25c80000+(y*512+x)*(2 if mode==2 else 1),2 if mode==2 else 1)
        s+=read(0x25d00016,2)
        for i,color in enumerate(colors):s+=put16(0x25f00002+i*2,color)
        for at,value in [(0,0x8000),(0xe0,0x20 if mode==2 else 8),(0xf0,3),
                         (0xbc,0),(0xbe,0x8000)]:s+=put16(0x25f80000+at,value)
        # Sprite rotation uses only A starts and deltas, not the matrix/kx/ky.
        for off,value in [(0,255<<16),(0xc,0xffff0000),(0x1c,0xffff0000),
                          (0x4c,7<<16),(0x80,511<<16)]:s+=store(0x25e10000+off,value)
        s+='load 0x104,r1\nmov.l @r1,r0\nload 0x25e10018,r1\nmov.l r0,@r1\n'
        s+=put16(0x25d00002,3)
        s+=blank('setup_blank',True)+blank('start_field',False)
        # Table writes without RxSTRE must leave the running origin alone.
        s+=line(64)+store(0x25e10000,0)
        s+=line(127)+put16(0x25f800b2,1)
        s+=line(191)+store(0x25e10004,128<<16)+put16(0x25f800b2,3)
        s+=blank('completed',True)
        # Later table writes cannot retroactively repaint the published field.
        s+=store(0x25e10000,511<<16)+store(0x25e10004,511<<16)
        s+=store(0x2600de18,0x600d)+'park:\nsleep\nbra park\nnop\n'
        rom=bytearray(524288);struct.pack_into('>II',rom,0,0x200,0x060ff000)
        struct.pack_into('>I',rom,0x104,1<<16)
        code=assembler.assemble(s,base=0x200);rom[0x200:0x200+len(code)]=code
        lines=['# Original MIT VDP1 rotation/RPRCTL synthetic BIOS. Adjacent LICENSE.']
        for at in range(0,len(rom),16):
            if any(rom[at:at+16]):lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
        stem=f'sprite-rotation{16 if mode==2 else 8}'
        outputs[stem+'.s']=s.encode()
        outputs[stem+'.hex']=('\n'.join(lines)+'\n').encode()
        expected=[2]+(colors if mode==2 else [1,2,3,4])+[0x1010+mode,0x600d]
        outputs[stem+'.tsv']=''.join(f'{v:08X}\n' for v in expected).encode()
    return outputs

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
