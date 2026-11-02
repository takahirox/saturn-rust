#!/usr/bin/env python3
"""Original MIT SH-2 streaming application and authored ISO media. Adjacent LICENSE.

The unchanged external-startup firmware discovers FID2. The application discovers
STREAM.DAT;1 from real root bytes; all transfers, arithmetic and pixels are guest work.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler as a, store
from build_clock import put8, put16
from build_hle_system import smpc
from build_external_discovery import M
from build_cd_runtime import call, cmd, wait, guard, check, save, get

HERE = Path(__file__).resolve().parent
BASE, O, TRACE = 0x06020000, 0x26070000, 0x26074000
DIR, BUF = 0x06060000, 0x06080004


def sample(address, destination, width='l'):
    return f'load {address:#x},r1\nmov.{width} @r1,r0\n'+('extu.w r0,r0\n' if width=='w' else '')+save(destination)


def record(reg='r0'):
    return f'mov.l {reg},@r8\nadd #4,r8\n'


def program():
    s='bsr entered\nnop\nentered:\nsts pr,r0\n'+save(O)
    s+=f'load {TRACE:#x},r9\n'+store(O+4,1)+put8(0xfffffe92,0x11)
    # Reinitialize selectors after firmware boot; the physical medium survives.
    s+=cmd(0x4801)+cmd(0x4480)+cmd(0x3000)
    s+=put16(0x25890008,0xFFEB)+get(M['root'],'r5')+'load 150,r0\nadd r0,r5\nload 0x1080,r4\nload 0x80,r6\nmov #1,r7\n'+call('command')+wait(16)
    s+=cmd(0x6300,0,0,1)+f'load {DIR:#x},r10\nload 2048,r11\n'+call('dma')+cmd(0x0600)
    # Independently parse root record boundaries and compare the 12-byte name.
    s+=f'load {DIR:#x},r10\nload {DIR+2048:#x},r12\nfind:\ncmp/hs r12,r10\n'+guard('bt')
    s+='mov.b @r10,r0\nextu.b r0,r13\nload 34,r2\ncmp/hs r2,r13\n'+guard('bf')
    s+='mov r10,r0\nadd r13,r0\ncmp/hs r0,r12\n'+guard('bf')
    s+='mov r10,r1\nadd #32,r1\nmov.b @r1+,r0\nload 12,r2\ncmp/eq r2,r0\nbf next_record\n'
    s+=f'load {BASE+0x3000:#x},r2\nmov #12,r3\nname:\nmov.b @r1+,r0\nmov.b @r2+,r4\ncmp/eq r4,r0\nbf next_record\ndt r3\nbf name\nbra found\nnop\nnext_record:\nadd r13,r10\nbra find\nnop\nfound:\n'
    s+='mov r10,r1\nadd #6,r1\n'+call('be32')+save(O+0x10)
    s+='mov r10,r1\nadd #14,r1\n'+call('be32')+save(O+0x14)
    s+='add #1,r0\nload 2046,r2\nadd r2,r0\n'+('shlr r0\n'*11)+save(O+0x18)
    # Abort an accepted directory/drive job before its first acquisition;
    # reject an unavailable FID without publishing a host FIFO.
    s+=put16(0x25890008,0xFDFF)+cmd(0x7000,0,0x03FF,0xFFFF)+cmd(0x7500)+wait(512)
    s+=sample(0x25890008,O+0x50,'w')+cmd(0x7400,0,0x0100,0xFFFF,True)+check(0xFF00)
    s+='load 0x06080000,r1\nload 65536,r3\nload 0xa5,r0\ncanaries:\nmov.b r0,@r1\nadd #1,r1\ndt r3\nbf canaries\n'
    s+=f'load 0x26071000,r8\nmov #0,r13\n'+store(O+0x28,0)
    for slow in [False,True]:
        name='slow' if slow else 'fast'
        slot=0x8000 if slow else 0
        s+=store(O+4,3 if slow else 2)+cmd(0x4801)
        if slow:
            # Keep 198 physical pool slots occupied in independent partition2.
            # PUT's untransferred payload is unspecified and never inspected.
            s+=cmd(0x6400,0,0x0200,198)+cmd(0x0600)
        s+=cmd(0x4480,0,0x0100)+cmd(0x3000,0,0x0100)
        s+=put16(0x25890008,0xFFE3)+get(O+0x10,'r5')+'load 150,r0\nadd r0,r5\nload 0x1080,r4\nload 0x80,r6\n'+get(O+0x18,'r7')+call('command')
        if slow:
            s+=wait(4)+put16(0x25890008,0xFFFB)+wait(4)
            s+=sample(0x25890008,O+0x20,'w')+'load 12,r2\nand r2,r0\n'+check(12,'full_error')
        s+=f'load {BUF+slot:#x},r10\n'+get(O+0x18,'r12')+'mov #0,r13\nmov #0,r14\n'
        s+=name+'_sector:\n'
        if slow:
            s+='mov #2,r0\ncmp/hs r0,r12\nbf slow_tail\n'+wait(8)+'slow_tail:\n'
        else: s+=wait(4)
        s+=record('r13')
        s+='load 0x25890008,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+=cmd(0x5400,0,0x0100)+'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+=cmd(0x5100,0,0x0100)+'load 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+=put16(0x25890008,0xFF73)+cmd(0x6300,0,0x0100,1)
        s+=sample(0x26010204,O+0x30)+record()
        s+='load 0xfffffe12,r1\nmov.b @r1+,r0\nextu.b r0,r0\nshll8 r0\nmov.b @r1,r2\nextu.b r2,r2\nor r2,r0\n'+record()
        # Exact final byte count, rounded up only for the word-wide CD FIFO.
        s+='load 2048,r11\nmov #1,r0\ncmp/eq r0,r12\nbf '+name+'_length\n'+get(O+0x14,'r11')+'load 2047,r0\nand r0,r11\nadd #1,r11\nshlr r11\nshll r11\n'+name+'_length:\n'
        s+=call('dma_start')
        # Process the previous chunk while drive acquisition continues.
        s+='tst r13,r13\nbt '+name+'_first\nmov r10,r4\nload 2048,r0\nsub r0,r4\nload 16,r5\n'+call('compute')
        s+='load 2032,r5\n'+call('compute')+name+'_first:\n'
        # Independent buffer query is permitted before ending the host FIFO.
        s+=cmd(0x5000)+'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+='load 0xfffffe12,r1\nmov.b @r1+,r0\nextu.b r0,r0\nshll8 r0\nmov.b @r1,r2\nextu.b r2,r2\nor r2,r0\n'+record()+record('r14')
        s+=get(O+0x34)+record()
        s+=call('dma_wait')+cmd(0x0600)+'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+=sample(0x25fe0004,O+0x38)+record()
        s+='load 0x25890008,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
        s+=sample(0x26010204,O+0x3c)+record()+record('r10')+record('r11')
        s+='load 2048,r0\nadd r0,r10\nadd #1,r13\ndt r12\nbf.l '+name+'_sector\n'
        s+='mov r10,r4\nload 2048,r0\nsub r0,r4\n'+get(O+0x14,'r5')+'load 2047,r0\nand r0,r5\n'+call('compute')
        s+=save(O+(0x0C if slow else 8),'r14')+wait(16)+cmd(0x5100,0,0x0100)+'load 0x25890024,r1\nmov.w @r1,r0\n'+check(0)
    s+=save(O+0x24,'r8')+save(O+0x2c,'r9')
    s+='load 0x7f00,r1\nmov.l @r1,r0\ntst #1,r0\nbt no_sysres\nload 0x20180001,r1\nmov.b @r1,r0\nextu.b r0,r0\nload 0x5a,r2\ncmp/eq r2,r0\nbt no_sysres\n'
    s+=put8(0x20180001,0x5A)+smpc(0x0D)+'no_sysres:\n'+store(O+4,4)+store(O+0xFC,0x600D)
    s+='park:\nsleep\nbra park\nnop\n'
    for label,code in [('failed',0xE090),('io_error',0xE092),('full_error',0xE093)]:
        s+=label+':\n'+store(O+0xFC,code)+save(O+0x24,'r8')+save(O+0x2c,'r9')+'load park,r0\njmp @r0\nnop\n'
    s+='be32:\nmov #0,r0\nmov #4,r3\nbe_loop:\nshll8 r0\nmov.b @r1+,r2\nextu.b r2,r2\nor r2,r0\ndt r3\nbf be_loop\nrts\nnop\n'
    s+='compute:\ncompute_byte:\nmov.b @r4+,r0\nextu.b r0,r0\nadd r0,r14\ndt r5\nbf compute_byte\n'
    s+='mov r14,r0\nload 0x7fff,r2\nand r2,r0\nload 0x8000,r2\nor r2,r0\nload 0x25e00000,r1\nmov.w r0,@r1\n'+store(O+0x28,1)+'rts\nnop\n'
    s+='dma:\nsts.l pr,@-r15\n'+call('dma_start')+call('dma_wait')+'lds.l @r15+,pr\nrts\nnop\n'
    s+='dma_start:\nsts.l pr,@-r15\n'+get(0x26010204)+'add #1,r0\n'+save(O+0x40)
    for off,v in [(0,0x05818000),(12,2),(20,0x107)]: s+=store(0x25FE0000+off,v)
    s+='load 0x25fe0004,r1\nmov.l r10,@r1\nload 0x25fe0008,r1\nmov.l r11,@r1\nmov #0,r2\n'+call('dma_kernel')
    s+='load 0x25fe0010,r1\nload 0x101,r4\nmov #1,r2\n'+call('dma_kernel')+save(O+0x34,'r6')+save(O+0x48,'r5')+'lds.l @r15+,pr\nrts\nnop\n'
    # Warm the same cache lines before GO. Register-only arithmetic and local
    # FRT reads then run while SCU owns the external bus. Publication waits.
    s+='dma_kernel:\nload 0xfffffe12,r7\nmov.b @r7,r0\nextu.b r0,r0\nshll8 r0\nadd #1,r7\nmov.b @r7,r3\nextu.b r3,r3\nor r3,r0\nmov r0,r6\ntst r2,r2\nbt kernel_warm\nmov.l r4,@r1\nkernel_warm:\nmov #16,r3\nmov #0,r5\nkernel_spin:\nadd r14,r5\ndt r3\nbf kernel_spin\nload 0xfffffe12,r7\nmov.b @r7,r0\nextu.b r0,r0\nshll8 r0\nadd #1,r7\nmov.b @r7,r3\nextu.b r3,r3\nor r3,r0\nsub r6,r0\nextu.w r0,r6\nrts\nnop\n'
    s+='dma_wait:\n'+get(O+0x40,'r7')+'load 0x26010204,r1\nload 200000,r3\ndma_poll:\nmov.l @r1,r0\ncmp/eq r7,r0\nbt dma_ready\ndt r3\nbf dma_poll\nload io_error,r0\njmp @r0\nnop\ndma_ready:\nrts\nnop\n'
    s+='command:\nsts.l pr,@-r15\n'+call('issue')+'load 0x8000,r2\ntst r2,r0\n'+guard('bf','io_error')+'lds.l @r15+,pr\nrts\nnop\nissue:\n'
    for reg in range(4,8): s+=f'mov.l r{reg},@r9\nadd #4,r9\n'
    s+=put16(0x25890008,0xFFFE)+'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\nadd #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'+wait(1)
    for at in [0x25890018,0x2589001C,0x25890020,0x25890024]:
        s+=f'load {at:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\nmov.l r0,@r9\nadd #4,r9\n'
    s+='load 0x25890018,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n'
    code=a.assemble(s,base=BASE)
    assert len(code)<0x3000,len(code)
    return s,code.ljust(0x3000,b'\0')+b'STREAM.DAT;1'


def build():
    source,code=program()
    outputs={'cd-stream.s':source.encode(),'cd-stream.bin':code}
    license=(HERE/'LICENSE').read_bytes()
    for name,root,extent,length,seed in [('cd-stream',20,64,12291,11),('cd-stream-moved',27,88,16387,73)]:
        iso=bytearray(a.build(code,license,'ORIGINAL CD STREAM','STREAM-001 V1.000'))
        iso.extend(bytes(128*2048-len(iso)))
        iso[20*2048:]=bytes(len(iso)-20*2048)
        struct.pack_into('>IIIIII',iso,0xE0,0x1000,0,0x060FE000,0,BASE,0xDEADBEEC)
        aip='bsr aip_entered\nnop\naip_entered:\nsts pr,r0\nload 0x260102f0,r1\nmov.l r0,@r1\nload 0x060020f0,r1\nmov.l @r1,r1\njmp @r1\nnop\n'
        aip_code=a.assemble(aip,base=0x06002E20)
        iso[0xE20:0xE20+len(aip_code)]=aip_code
        iso[16*2048+80:16*2048+88]=a.both32(128)
        iso[16*2048+156:16*2048+190]=a.record(b'\0',root,2048,True)
        for at,endian in [(18,'<'),(19,'>')]:
            iso[at*2048:(at+1)*2048]=(b'\1\0'+struct.pack(endian+'IH',root,1)+b'\0\0').ljust(2048,b'\0')
        records=a.record(b'\0',root,2048,True)+a.record(b'\1',root,2048,True)+a.record(b'STREAM.BIN;1',32,len(code))
        files=[a.record(b'STREAM.DAT;1',extent,length),a.record(b'LICENSE.TXT;1',120,len(license))]
        if root==27:files.reverse()
        records+=b''.join(files)
        iso[root*2048:root*2048+len(records)]=records
        iso[32*2048:32*2048+len(code)]=code
        iso[extent*2048:(extent+(length+2047)//2048)*2048]=bytes((length+2047)//2048*2048)
        iso[extent*2048:extent*2048+length]=bytes(((i*31+seed)^(i>>7)^(i//2048*19))&255 for i in range(length))
        iso[120*2048:120*2048+len(license)]=license
        outputs[name+'.iso']=bytes(iso)
    return outputs


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check:assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
