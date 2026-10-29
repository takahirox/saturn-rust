#!/usr/bin/env python3
"""Original MIT external firmware and first-read guest. Adjacent LICENSE.

No HLE installation: the ROM issues SMPC/CD commands and SCU DMA, then jumps
through disc-derived IP/AIP and ISO9660 file identifier 2. All observations
are SH-2 instructions. build_external_fixed.py preserves the inherited control.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler as a, store
from build_clock import put8, put16
from build_hle_system import smpc, wait
from build_external_discovery import discovery, routines, call, guard, M
from build_external_fixed import build as build_fixed

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
    for index, (address, value, width) in enumerate([(R+0x200,1,4), (R+0x204,7,4), (R+0x208,1,4),
                                  (R+0x20C,0x5353,4), (0x25F80000,0x8000,2),
                                  (R+0x210,1024,4), (R+0x214,0x41000096,4),
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
    # Original discs: same ROM for all, independently assembled guest operands.
    images = {}
    for name,extent,length,base,sp,root,descriptors,areas,padding in [
        ('external-startup',21,4096,BASE,0x060FE000,20,2,'U',0),
        ('external-startup-moved',35,4093,0x06030000,0x060FC000,27,3,'JT',24),
        ('external-startup-long',48,6145,0x06050000,0x060FA000,31,2,'JTUE',48),
    ]:
        directory_sectors = 2 if padding else 1
        reads = 2 + descriptors + directory_sectors + (length+2047)//2048
        guest = g.replace(f'{BASE:#x}',f'{base:#x}').replace(f'{BASE+4:#x}',f'{base+4:#x}').replace(f'{BASE+0xf00:#x}',f'{base+0xf00:#x}')
        guest = guest.replace('0x60fe000',f'{sp:#x}').replace('load 0x7,r2\ncmp/eq',f'load {reads:#x},r2\ncmp/eq')
        program = bytearray((i*37+11)&255 for i in range(length))
        code = a.assemble(guest,base=base)
        assert len(code)<0xF00
        program[:len(code)] = code
        struct.pack_into('>4I',program,0xF00,7,11,13,17)
        iso=bytearray(a.build(bytes(program),(HERE/'LICENSE').read_bytes(),name.upper(),'EXTBOOT-001 V1.000'))
        iso[21*2048:] = bytes(len(iso)-21*2048)
        iso[0x40:0x50] = areas.encode().ljust(16,b' ')
        struct.pack_into('>IIIIII',iso,0xE0,0x1000,0,sp,0,base,0xDEADBEEC) # ignored for CD
        # The original AIP owns the jump to the first-read destination. A fetched
        # BSR/PR proves firmware entered it, rather than jumping straight to file.
        aip=0x06002000+0xE00+len(areas)*32
        aip_source=('bsr aip_entered\nnop\naip_entered:\nsts pr,r0\n'
                    'load 0x260102f0,r1\nmov.l r0,@r1\n'
                    'load 0x060020f0,r1\nmov.l @r1,r1\njmp @r1\nnop\n')
        iso[aip-0x06002000:aip-0x06002000+len(a.assemble(aip_source,base=aip))]=a.assemble(aip_source,base=aip)
        if descriptors==3:
            iso[18*2048:19*2048]=iso[17*2048:18*2048]
            iso[17*2048:18*2048]=iso[16*2048:17*2048]
            iso[16*2048:17*2048]=bytes(2048)
            iso[16*2048:16*2048+7]=b'\x00CD001\x01'
        pvd=(16+(descriptors==3))*2048
        def dir_record(identifier,sector,size,directory=False,extra=0):
            record=bytearray(a.record(identifier,sector,size,directory))
            record[0]+=extra
            return record+bytes(extra)
        directory=bytearray(dir_record(b'\x00',root,directory_sectors*2048,True,padding))
        directory+=dir_record(b'\x01',root,directory_sectors*2048,True)
        directory+=dir_record(b'GAME.BIN;1',extent,length)
        directory+=dir_record(b'LICENSE.TXT;1',extent+(length+2047)//2048,len((HERE/'LICENSE').read_bytes()))
        # Valid extra files make the larger root span a sector boundary. Records
        # never straddle a sector; zero fill is only the remaining sector tail.
        if directory_sectors==2:
            for i in range(48):
                record=dir_record(f'X{i:03}.BIN;1'.encode(),63,1)
                if len(directory)%2048+len(record)>2048:
                    directory+=bytes(2048-len(directory)%2048)
                directory+=record
        assert len(directory)<=directory_sectors*2048
        iso[root*2048:root*2048+len(directory)]=directory
        iso[pvd+156:pvd+190]=a.record(b'\x00',root,directory_sectors*2048,True)
        # Path tables also describe the actual root, even though ROM doesn't use them.
        for at,endian in [(18 if descriptors==2 else 19,'<'),(19 if descriptors==2 else 20,'>')]:
            iso[at*2048:at*2048+10]=b'\x01\x00'+struct.pack(endian+'IH',root,1)+b'\x00\x00'
        struct.pack_into('<I',iso,pvd+140,18 if descriptors==2 else 19)
        struct.pack_into('>I',iso,pvd+148,19 if descriptors==2 else 20)
        iso[extent*2048:extent*2048+length]=program
        license_sector=extent+(length+2047)//2048
        license=(HERE/'LICENSE').read_bytes()
        iso[license_sector*2048:license_sector*2048+len(license)]=license
        images[name+'.iso']=bytes(iso)
        images[name+'-disc.s']=guest.encode()

    s = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\n'
         'bt master\nload 0x7800,r1\njmp @r1\nnop\nmaster:\n'
         'load 0x06000000,r0\nldc r0,vbr\nload 0x26010400,r9\n')
    s += put8(0xFFFFFE92,0) + put16(0xFFFFFEE0,1)
    for vector,handler in [(4,0x7600),(6,0x7600),(9,0x7600),(0x47,0x7000),(0x4b,0x7200),(0x50,0x7400)]:
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
    s += put16(0x25890008,0xFFFE)+put16(0x2589000C,1)
    s += store(0x25FE00A8,1)+store(0x25FE00A0,0x377F)
    s += 'load 0xf0,r0\nldc r0,sr\n' + command(0) + 'mov #0,r0\nldc r0,sr\n'
    s += wait(R+0x208,'cd_irq')
    s += put16(0x2589000C,0)+store(0x25FE00A0,0xB77F)
    s += command(0x0200)
    s += 'load 0x25890018,r1\nmov.w @r1,r0\nextu.w r0,r0\nload 0xff00,r2\ncmp/eq r2,r0\nbf medium_present\nload no_media,r0\njmp @r0\nnop\nmedium_present:\n'
    s += observe(0x25818000,R+0x214)
    s += 'load 200,r3\nload 0x25818000,r1\nread_toc:\nmov.w @r1,r0\ndt r3\nbf read_toc\n'
    s += observe(0x25818000,R+0x2F8)
    s += command(0x0600)
    s += put16(0x25890008,0xFFBF)+command(0x3000)
    s += discovery()
    s += 'no_media:\n'+store(R+0xFC,0xE001)+'bra stopped\nnop\n'
    s += 'stopped:\nsleep\nbra stopped\nnop\n'
    s += routines()
    s += ('command:\nmov.l r4,@r9\nadd #4,r9\nmov.l r5,@r9\nadd #4,r9\n'
          'mov.l r6,@r9\nadd #4,r9\nmov.l r7,@r9\nadd #4,r9\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\n'
          'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\n'
          'add #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'
          'load 0x25890008,r1\nload 200000,r3\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbf command_ready\ndt r3\nbf command_wait\nload io_error,r0\njmp @r0\nnop\ncommand_ready:\n' +
          ''.join(f'load {0x25890018+i*4:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\nmov.l r0,@r9\nadd #4,r9\n' + (('load 0x200,r2\ncmp/eq r2,r4\nbt command_status_ok\nload 0x8000,r2\ntst r2,r0\n'+guard('bf','io_error')+'command_status_ok:\n') if i==0 else '') for i in range(4)) + 'rts\nnop\n')
    s=s.replace('bsr command\nnop\n',call('command'))
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
    trap=store(R+0xFC,0xE004)+'trap_park:\nsleep\nbra trap_park\nnop\n'
    chunks.append((0x7600,a.assemble(trap,base=0x7600)))
    slave=store(R+0x20C,0x5353)+f'load {M["slave_ready"]:#x},r1\nslave_wait:\nmov.l @r1,r0\ntst r0,r0\nbt slave_wait\nmov r0,r15\nload 0x260102f4,r2\nmov.l r0,@r2\nmov #0,r0\nmov.l r0,@r1\nslave_park:\nsleep\nbra slave_park\nnop\n'
    chunks.append((0x7800,a.assemble(slave,base=0x7800)))
    rom=bytearray(524288)
    lines=['# Original MIT external startup firmware. Adjacent LICENSE; expanded size 512 KiB.']
    for at,data in chunks:
        rom[at:at+len(data)]=data
        for i in range(0,len(data),16):
            words=struct.unpack('>'+'H'*(len(data[i:i+16])//2),data[i:i+16])
            lines.append(f'{at+i:05X} '+' '.join(f'{w:04X}' for w in words))
    fixed,_=build_fixed()
    # Negative hardware probes issue a second ordinary command while the play
    # engine is seeking, rather than merely substituting/omitting the read.
    # These are separate original ROMs, never selected for a valid-layout oracle.
    fence='load 0x25890008,r1\nload 200000,r3\nsector_wait:'
    assert s.count(fence)==1
    for name,opcode in [('abort',0x7500),('read-reset',0x0401)]:
        extra=command(opcode).replace('bsr command\nnop\n',call('command'))
        probe=s.replace(fence,extra+fence)
        data=a.assemble(probe,base=0x200)
        assert len(data)<0x6e00
        probe_chunks=[(at,data if at==0x200 else chunk) for at,chunk in chunks]
        probe_lines=[f'# Original MIT {name} during scheduled seek probe. Adjacent LICENSE.']
        for at,chunk in probe_chunks:
            for i in range(0,len(chunk),16):
                words=struct.unpack('>'+'H'*(len(chunk[i:i+16])//2),chunk[i:i+16])
                probe_lines.append(f'{at+i:05X} '+' '.join(f'{w:04X}' for w in words))
        images[f'external-startup-{name}.hex']=('\n'.join(probe_lines)+'\n').encode()
        images[f'external-startup-{name}.s']=extra.encode()
    images['external-startup-fixed.hex']=fixed['external-startup.hex']
    images['external-startup-fixed.iso']=fixed['external-startup.iso']
    return {**images,'external-startup.hex':('\n'.join(lines)+'\n').encode(),
            'external-startup.s':s.encode()}, bytes(rom)


if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--check',action='store_true')
    p.add_argument('--rom-output',type=Path,help='also write/check an expanded raw 512 KiB ROM')
    args=p.parse_args()
    outputs,rom=build()
    for name,data in outputs.items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else:path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
    if args.rom_output:
        if args.check: assert args.rom_output.read_bytes()==rom,'Rebuild raw ROM'
        else:
            args.rom_output.parent.mkdir(parents=True,exist_ok=True)
            args.rom_output.write_bytes(rom)
    print('Expanded firmware SHA-256 '+hashlib.sha256(rom).hexdigest())
