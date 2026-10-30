#!/usr/bin/env python3
"""Frozen inherited-main negative control (260f1a3), MIT. Adjacent LICENSE.

Imported only by build_external_startup.py to reproduce the fixed ROM/ISO.
This is not the active discovery firmware.

No HLE installation: the ROM issues SMPC/CD commands and SCU DMA, then jumps
into the first-read file at LBA21. All observations are SH-2 instructions.
"""
import struct
from pathlib import Path
from build_scu import assembler as a, store
from build_clock import put8, put16
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent
R = 0x26010000
BASE = 0x06004000


def observe(address, destination, width=4):
    op = {1:'b', 2:'w', 4:'l'}[width]
    s = f'load {address:#x},r1\nmov.{op} @r1,r0\n'
    if width != 4: s += f'extu.{op} r0,r0\n'
    return s + f'load {destination:#x},r1\nmov.l r0,@r1\n'


def check(value, label='failed'):
    return f'load {value:#x},r2\ncmp/eq r2,r0\nbf.l {label}\n'


def command(cmd, b=0, c=0, d=0):
    return (f'load {cmd:#x},r4\nload {b:#x},r5\nload {c:#x},r6\nload {d:#x},r7\n'
            'bsr command\nnop\n')


def build():
    # The payload executes only after DMA has copied these bytes from media.
    g = ('bsr entered\nnop\nentered:\nsts pr,r0\n'
         f'load {R:#x},r8\nmov.l r0,@r8\nadd #4,r8\n')
    g += check(BASE+4)
    g += 'mov r15,r0\nmov.l r0,@r8\nadd #4,r8\n' + check(0x060FE000)
    g += 'stc vbr,r0\nmov.l r0,@r8\nadd #4,r8\n' + check(0x06000000)
    for index, (address, value, width) in enumerate([(R+0x200,1,4), (R+0x204,1,4), (R+0x208,1,4),
                                  (R+0x20C,0x5353,4), (0x25F80000,0x8000,2),
                                  (R+0x210,2048,4), (R+0x214,0x41000096,4),
                                  (BASE,0xB0000009,4)]):
        g += observe(address, R+0x10+index*4, width) + check(value)
    g += f'load {BASE+0xf00:#x},r1\nmov #0,r0\nmov #4,r3\nsum:\nmov.l @r1+,r2\nadd r2,r0\ndt r3\nbf sum\n'
    g += f'load {R+12:#x},r1\nmov.l r0,@r1\n' + check(48)
    # Native and browser compare the actual guest-produced back-screen color.
    g += ('load 0x7f00,r1\nmov.l @r1,r0\ntst #1,r0\nbt no_sysres\n'
          'load 0x20180001,r1\nmov.b @r1,r0\nextu.b r0,r0\nload 0x5a,r2\ncmp/eq r2,r0\nbt no_sysres\n')
    g += put8(0x20180001,0x5A)+smpc(0x0D)
    g += 'no_sysres:\n'+put16(0x25E00000,0x83E0)
    g += store(R+0xFC,0x600D)+'park:\nsleep\nbra park\nnop\n'
    g += 'failed:\n'+store(R+0xFC,0xBAD)+'bra park\nnop\n'
    payload = bytearray(4096)
    code = a.assemble(g,base=BASE)
    assert len(code)<0xF00
    payload[:len(code)] = code
    struct.pack_into('>4I',payload,0xF00,7,11,13,17)
    # Original ISO writer supplies Saturn IP + ISO9660 with GAME.BIN at LBA21.
    iso = a.build(bytes(payload),(HERE/'LICENSE').read_bytes(),'EXTERNAL STARTUP','EXTBOOT-001 V1.000')
    checksum = sum(struct.unpack('>1024I',payload)) & 0xffffffff

    s = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\n'
         'bt master\nload 0x7800,r1\njmp @r1\nnop\nmaster:\n'
         'load 0x06000000,r0\nldc r0,vbr\nload 0x26010400,r9\n')
    s += put8(0xFFFFFE92,0) + put16(0xFFFFFEE0,1)
    for vector,handler in [(0x47,0x7000),(0x4b,0x7200),(0x50,0x7400)]:
        s += store(0x06000000+vector*4,handler)
    s += store(0x25FE00A0,0xB77F)  # SMPC + DMA0; CD initially masked.
    s += 'mov #0,r0\nldc r0,sr\n'
    for cmd in [2,7,9,8]:
        s += smpc(cmd)
        s += observe(0x2010005F,R+0x240+cmd*4,1)
    s += wait(R+0x20C,'slave_started')
    # Status-only INTBACK has a real SCU interrupt and no CONTINUE phase.
    s += put8(0x20100001,1)+put8(0x20100003,0)+put8(0x20100005,0xF0)+smpc(0x10)
    s += wait(R+0x200,'status_irq')
    s += observe(0x20100035,R+0x218,1) # sound reset / fixed system flags
    s += put16(0x25F80000,0x8000)+put16(0x25F800EC,0)+put16(0x25F800EE,0)
    s += put16(0x25E00000,0x801F)
    # Reset selectors/transfer state, then enable exactly CMOK for an A-bus IRQ.
    s += command(0x0401)
    s += put16(0x25890008,0)+put16(0x2589000C,1)
    s += store(0x25FE00A8,1)+store(0x25FE00A0,0x377F)
    s += 'load 0xf0,r0\nldc r0,sr\n' + command(0) + 'mov #0,r0\nldc r0,sr\n'
    s += wait(R+0x208,'cd_irq')
    s += put16(0x2589000C,0)+store(0x25FE00A0,0xB77F)
    s += command(0x0200)
    s += 'load 0x25890018,r1\nmov.w @r1,r0\nextu.w r0,r0\nload 0xff00,r2\ncmp/eq r2,r0\nbt.l no_media\n'
    s += observe(0x25818000,R+0x214)
    s += 'load 202,r3\nload 0x25818000,r1\nread_toc:\nmov.w @r1,r0\ndt r3\nbf read_toc\n'
    s += command(0x0600)
    s += command(0x3000)
    s += put16(0x25890008,0xFFEB) # clear CSCT/PEND before timed read
    s += command(0x1080,171,0x80,2)
    s += ('load 0x25890008,r1\nread_wait:\nmov.w @r1,r0\ntst #16,r0\nbt read_wait\n')
    s += command(0x5100)
    s += 'mov r0,r0\n' + check(2,'no_media')
    s += command(0x6300,0,0,2)
    for off,value in [(0,0x05818000),(4,BASE),(8,4096),(12,2),(20,7),(16,0x101)]:
        s += store(0x25FE0000+off,value)
    s += wait(R+0x204,'dma_done')
    s += command(0x0600)
    s += observe(0x2589001C,R+0x210,2)
    # Guest checksum fences all copied instruction/data bytes before entry.
    s += (f'load {BASE:#x},r1\nload 1024,r3\nmov #0,r0\nchecksum:\n'
          'mov.l @r1+,r2\nadd r2,r0\ndt r3\nbf checksum\n')
    s += f'load {R+0x21c:#x},r1\nmov.l r0,@r1\n'+check(checksum,'corrupt')
    s += 'load 0x060fe000,r15\nload 0x06004000,r1\njmp @r1\nnop\n'
    s += 'no_media:\n'+store(R+0xFC,0xE001)+'bra stopped\nnop\n'
    s += 'corrupt:\n'+store(R+0xFC,0xE002)+'stopped:\nsleep\nbra stopped\nnop\n'
    s += ('command:\nmov.l r4,@r9\nadd #4,r9\nmov.l r5,@r9\nadd #4,r9\n'
          'mov.l r6,@r9\nadd #4,r9\nmov.l r7,@r9\nadd #4,r9\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\n'
          'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\n'
          'add #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'
          'load 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\n' +
          ''.join(f'load {0x25890018+i*4:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\nmov.l r0,@r9\nadd #4,r9\n' for i in range(4)) + 'rts\nnop\n')
    push='mov.l r0,@-r15\nmov.l r1,@-r15\n'
    pop='mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    chunks=[(0,struct.pack('>IIII',0x200,0x060FF000,0x200,0x060FF000)),
            (0x200,a.assemble(s,base=0x200))]
    assert len(chunks[1][1])<0x6e00
    for address,mailbox in [(0x7000,R+0x200),(0x7200,R+0x204),(0x7400,R+0x208)]:
        h=push+f'load {mailbox:#x},r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'
        if address==0x7400:
            h+=put16(0x25890008,0xFFFE)+store(0x25FE00A8,1)
        chunks.append((address,a.assemble(h+pop,base=address)))
    slave=store(R+0x20C,0x5353)+'slave_park:\nsleep\nbra slave_park\nnop\n'
    chunks.append((0x7800,a.assemble(slave,base=0x7800)))
    rom=bytearray(524288)
    lines=['# Original MIT external startup firmware. Adjacent LICENSE; expanded size 512 KiB.']
    for at,data in chunks:
        rom[at:at+len(data)]=data
        for i in range(0,len(data),16):
            words=struct.unpack('>'+'H'*(len(data[i:i+16])//2),data[i:i+16])
            lines.append(f'{at+i:05X} '+' '.join(f'{w:04X}' for w in words))
    return {'external-startup.hex':('\n'.join(lines)+'\n').encode(),
            'external-startup.iso':iso,'external-startup.s':s.encode(),
            'external-startup-disc.s':g.encode()}, bytes(rom)
