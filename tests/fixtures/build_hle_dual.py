#!/usr/bin/env python3
"""Original MIT HLE dual-CPU startup diagnostic; adjacent LICENSE.

STN-28 §§2.3/4.3–5; System Program guide interrupt table; SH7604 §§5/11.
Only original guest instructions calculate results and acknowledge interrupts.
No guest writes to TIER, IPRB, IPRA, VCRC, VCRD or ICR.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import call, smpc, record, wait
from build_clock import put8, put16

HERE = Path(__file__).resolve().parent
DATA = 0x2600B000


def get(address, register=0):
    return f'load {address:#x},r1\nmov.l @r1,r{register}\n'


def setint(index, target):
    return f'load {index:#x},r4\nload {target:#x},r5\n' + call(0x06000310)


def delay(label):
    return f'load 200,r2\n{label}:\ndt r2\nbf {label}\n'


def config(base=None):
    s = f'load {base:#x},r8\n' if base is not None else ''
    for address, width, mask in [(0xFFFFFE10,1,None),(0xFFFFFE11,1,0x80),
                                 (0xFFFFFEE2,2,None),(0xFFFFFE60,2,None),
                                 (0xFFFFFE66,2,None),(0xFFFFFE68,2,None),
                                 (0xFFFFFEE0,2,None)]:
        op = 'b' if width == 1 else 'w'
        s += f'load {address:#x},r1\nmov.{op} @r1,r0\nextu.{op} r0,r0\n'
        if mask is not None:
            s += f'mov #{-128},r2\nextu.b r2,r2\nand r2,r0\n'
        s += record()
    s += 'stc vbr,r0\n' + record() + record(15)
    s += 'stc sr,r0\nload 0xf0,r2\nand r2,r0\n' + record() + record(10)
    s += 'load 0x64,r4\n' + call(0x06000314) + record()
    s += 'load 0x70,r4\n' + call(0x06000314) + record()
    return s


def master():
    s = put8(0xFFFFFE92, 19)
    # Entry index 94 is distinct from the slave capture vector index 164.
    s += setint(0x64,0x06009000)+setint(0x164,0x06009400)
    s += setint(0x170,0x0600AC00)
    for epoch in range(4):
        base = 0x2600D000 + epoch*0x200
        if epoch == 1:
            s += smpc(3)
        if epoch >= 2:
            s += f'mov #{epoch-2},r4\n' + call(0x06000320)
        s += 'load 0xf0,r0\nldc r0,sr\n' + get(DATA,10) + config(base)
        s += f'load {base+0x40:#x},r9\n'
        s += store(DATA+0x20,base+0x100) + store(DATA+0x24,0)
        s += setint(0x94,0x06008000 if epoch == 0 else 0x06008040)
        s += smpc(2).replace('wait_smpc2',f'on_{epoch}')
        for round_ in range(2 if epoch == 0 else 1):
            label = f'round_{epoch}_{round_}'
            s += f'load {DATA+0x24:#x},r1\n{label}:\nmov.l @r1,r0\nmov #{round_+1},r2\ncmp/eq r2,r0\nbf {label}\n'
            s += delay('settled_'+label)
            s += get(DATA+8) + f'load {DATA+0x28:#x},r1\nmov.l r0,@r1\n'
            s += 'load 0xe0,r0\nldc r0,sr\n'
            s += get(DATA+12) + f'tst r0,r0\nbt omit_{label}\n'
            s += put16(0x21000000,0xA55A) + f'omit_{label}:\n'
            s += 'load 0x06007000,r1\njsr @r1\nclrt\n'
            # Ordinary code after RTE uses both CPUs' live arithmetic results.
            s += get(DATA+0x2C) + 'add r10,r0\n'
            s += f'load {base+0x60:#x},r1\nmov.l r0,@r1\n'
            s += f'load {0x2600E000+epoch*8+round_*4:#x},r1\nmov.l r0,@r1\n'
        s += store(base+0x64,0x600D)
    s += 'load 0x06006000,r1\njmp @r1\nnop\n'
    return s


def slave_entry(alternate=False):
    s = get(DATA+4,10)
    if alternate:
        s += 'add #17,r10\n'
    s += 'load 0x06008200,r1\njmp @r1\nnop\n'
    return s


def worker():
    s = get(DATA+0x20,8) + config()
    s += get(DATA+0x20,9) + 'add #64,r9\nmov #0,r12\n'
    s += put8(0xFFFFFE92,19)
    s += 'load 0xe0,r0\nldc r0,sr\n'
    return s+'load 0x06008600,r1\njmp @r1\nnop\n'


def worker_loop():
    return ('add #1,r12\n' + f'load {DATA+0x24:#x},r1\nmov.l r12,@r1\n'
            + 'load 0x06008100,r1\njmp @r1\nclrt\n')


def after_sleep():
    s = get(DATA+0x28) + 'add r10,r0\n'
    s += f'load {DATA+0x2C:#x},r1\nmov.l r0,@r1\nmov r9,r1\nadd #28,r1\nmov.l r0,@r1\n'
    s += delay('allow_master_sleep') + put16(0x21800000,0x5AA5)
    s += store(0x2600B030,0x600D)
    return s+'load 0x06008600,r1\njmp @r1\nnop\n'


def capture(add):
    # R9 points to this CPU's record; R10 is intentionally changed by handler.
    # Locate the architectural frame above the saved scratch registers.
    s = 'mov.l r0,@-r15\nmov.l r1,@-r15\nmov.l r2,@-r15\nmov.l r3,@-r15\n'
    s += 'mov r9,r3\nmov.l @r3,r0\nadd #1,r0\nmov.l r0,@r3\nadd #4,r3\n'
    s += 'stc sr,r0\nload 0xf0,r2\nand r2,r0\nmov.l r0,@r3\nadd #4,r3\n'
    s += 'mov r15,r1\nadd #16,r1\nmov.l @r1+,r0\nmov.l r0,@r3\nadd #4,r3\n'
    s += 'mov.l @r1,r0\nmov.l r0,@r3\nadd #4,r3\nadd #-4,r1\nmov.l r1,@r3\nadd #4,r3\n'
    s += 'load 0xfffffe11,r1\nmov.b @r1,r0\nmov #-128,r2\nand r2,r0\nextu.b r0,r0\nmov.l r0,@r3\nadd #4,r3\n'
    s += 'mov #0,r0\nmov.b r0,@r1\nmov.b @r1,r0\nand r2,r0\nextu.b r0,r0\nmov.l r0,@r3\n'
    s += f'add #{add},r10\n'
    s += 'mov.l @r15+,r3\nmov.l @r15+,r2\nmov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    return s


def raster_master():
    s = 'load 0xf0,r0\nldc r0,sr\n' + smpc(3)
    s += setint(0x94,0x06009800)+setint(0x141,0x0600A000)+setint(0x143,0x0600A400)
    # Both all-masked and all-enabled SCU variants retain master SR.I=15.
    s += get(DATA+0x40) + 'load 0x25fe00a0,r1\nmov.l r0,@r1\n'
    s += put16(0x25F80000,0x8000)+smpc(2)
    s += wait(0x0600E844,'raster_finished')
    s += get(0x25FE00A4)+'mov #5,r2\nand r2,r0\nload 0x2600e848,r1\nmov.l r0,@r1\n'
    s += smpc(3).replace('wait_smpc3','raster_off')
    s += get(DATA+16) + 'tst r0,r0\nbt.l all_done\n'
    s += get(0x26000C00) + 'tst r0,r0\nbf.l all_done\n'
    s += store(0x26000C00,0x12345678) + smpc(0x0D)
    s += 'all_done:\n' + get(0x26000C00) + 'load 0x2600e900,r1\nmov.l r0,@r1\n'
    s += store(0x2600E904,0x600D)+'park:\nsleep\nbra park\nnop\n'
    return s


def raster_slave():
    s = put8(0xFFFFFE92,19)
    # Actual guest instruction-cache reads fit two handler entries in HBlank.
    s += 'load 0x0600a000,r1\n' + 'mov.l @r1,r0\nadd #16,r1\n'*16
    s += get(DATA+0x34,10)+get(DATA+0x38,11)
    s += 'load 0x2600e800,r8\nload 0x2600e820,r9\n'
    s += 'load 0x25f80004,r4\nactive:\nmov.w @r4,r0\ntst #4,r0\nbf active\n'
    s += get(DATA+0x3C) + 'ldc r0,sr\nnop\n'
    s += 'raster_wait:\nsleep\nmov.l @r9,r0\nmov #2,r1\ncmp/eq r1,r0\nbf raster_wait\n'
    s += 'load 0x2600e840,r1\nmov r10,r0\nadd r11,r0\nmov.l r0,@r1\nadd #4,r1\nload 0x600d,r0\nmov.l r0,@r1\n'
    return s+'raster_park:\nsleep\nbra raster_park\nnop\n'


def raster_handler(label, base, acc, add, mask):
    s = f'mov.l @r{base},r0\nadd #1,r0\nmov.l r0,@r{base}\nadd #{add},r{acc}\n'
    s += f'mov r{base},r2\nadd #4,r2\nmov.l r{acc},@r2\n'
    s += f'mov r0,r3\nshll2 r3\nshll r3\nadd r{base},r3\n'
    s += 'load 0x25f80002,r1\nmov.w @r1,r2\n'
    s += 'load 0x25f8000a,r1\nmov.w @r1,r2\nextu.w r2,r2\nmov.l r2,@r3\n'
    s += 'add #4,r3\nload 0x25f80004,r1\nmov.w @r1,r2\nextu.w r2,r2\nmov.l r2,@r3\n'
    s += 'mov #2,r1\ncmp/eq r1,r0\nbf first_'+label+'\n'
    s += f'mov r15,r1\nadd #4,r1\nload {mask:#x},r0\nmov.l r0,@r1\n'
    return s+'first_'+label+':\nrte\nnop\n'


def build():
    # Fixed original entry points make saved-PC expectations independent literals.
    sections = {0x06004000:master(),0x06006000:raster_master(),
                0x06007000:'sleep\nrts\nnop\n',
                0x06008000:slave_entry(),0x06008040:slave_entry(True),
                0x06008100:'sleep\nbra 0x06008800\nnop\n',
                0x06008200:worker(),0x06008600:worker_loop(),
                0x06008800:after_sleep(),0x06009000:capture(13),0x06009400:capture(7),
                0x06009800:raster_slave(),
                0x0600A000:raster_handler('h',8,10,7,0x20),
                0x0600A400:raster_handler('v',9,11,11,0xF0),
                0x0600AC00:'rte\nnop\n'}
    guest = bytearray(0x7044)
    end = 0x06004000
    source = '; Original MIT HLE dual-CPU guest. Adjacent LICENSE.\n'
    for address,s in sorted(sections.items()):
        code=assembler.assemble(s,base=address)
        assert address >= end and address+len(code) <= 0x0600B000,hex(address)
        end=address+len(code)
        guest[address-0x06004000:end-0x06004000]=code
        source+=f'.section {address:#x}\n'+s
    # Authored input operands: master seed, slave seed, request, strobe enable,
    # SYSRES enable, reserved, and raster H/V seeds and SR mask.
    for off,value in {0:3,4:5,8:11,12:1,16:0,0x34:3,0x38:5,0x3C:0x10,0x40:0xFFFF}.items():
        struct.pack_into('>I',guest,0x7000+off,value)
    image=assembler.build(bytes(guest),(HERE/'LICENSE').read_bytes(),
                          'ORIGINAL HLE DUAL TEST','HLE-DUAL V1.000')
    # Literal independent expectations, never generated by running the emulator.
    rows=[]
    for epoch in range(4):
        for slave in [False,True]:
            base=0x0600D000+epoch*0x200+(0x100 if slave else 0)
            seed=(5 if epoch==0 else 22) if slave else 3
            values=[0x81,0,0,0xF00,0x6465,0x6600,0x8000 if slave else 0x8001,
                    0x06000400 if slave else 0x06000000,
                    0x06001000 if slave else 0x060FF000,0xF0,seed,
                    0x06009400 if slave else 0x06009000,0x0600AC00 if slave else 0x4B00]
            for i,v in enumerate(values):rows.append((base+i*4,v,f'epoch{epoch} {"slave" if slave else "master"} config{i}'))
            comm=[2 if epoch==0 else 1,0xF0,0x06008102 if slave else 0x06007002,
                  0xE0,0x06000FF8 if slave else 0x060FEFF8,0x80,0]
            for i,v in enumerate(comm):rows.append((base+0x40+i*4,v,f'epoch{epoch} {"slave" if slave else "master"} IRQ{i}'))
            if slave:
                rows.append((base+0x5C,30 if epoch==0 else 40,f'epoch{epoch} slave reply'))
            else:
                rows.extend([(base+0x60,59 if epoch==0 else 56,f'epoch{epoch} result'),(base+0x64,0x600D,f'epoch{epoch} done')])
    rows.extend([(0x0600E000,39,'first exchange'),(0x0600E004,59,'second exchange'),
                 (0x0600E008,56,'restarted exchange'),(0x0600E010,56,'clock320 exchange'),
                 (0x0600E018,56,'clock352 exchange'),(0x0600E848,5,'master IST retains H/V IN')])
    golden=''.join(f'{address:08X}\t{value:08X}\t{name}\n' for address,value,name in rows)
    return {'hle-dual.iso':image,'hle-dual.s':source.encode(),'hle-dual.tsv':golden.encode()}


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        target=HERE/name
        if args.check:assert target.read_bytes()==data,f'Rebuild {name}'
        else:target.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
