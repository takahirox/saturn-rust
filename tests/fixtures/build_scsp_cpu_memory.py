#!/usr/bin/env python3
"""Original MIT main/68k DRAM cycle diagnostic; adjacent LICENSE.

A burst DMAC fills 256 longwords from a fixed on-chip DIVU quotient. The main
bridge is configured for two-clock physical words, making its demand exceed
DRAM supply. 512 distinct memory cycles take >=2438 reference clocks (FRT/8
>=304, even before refresh and setup). The inherited busy gate has no such
budget. A separately executing 68k consumes the uploaded data, runs PCM and a
half-gain DSP read, and toggles the sample using real instructions.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc, wait, record

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x900)
    struct.pack_into('>II',sound,0,0x7ff00,0x100)
    code = [0x4a79,0,0x800,0x67f8, # TST.W $800; BEQ poll
            0x2039,0,0x1000,       # MOVE.L $1000,D0
            0x23c0,0,0x804]        # retain independently read guest operand
    def write(address,value):
        code.extend([0x33fc,value,address>>16,address&65535])
    for address,value in [(0x100400,0x20f),(0x100002,0x1000),(0x100006,1),
            (0x10000c,0x100),(0x100016,0xffef),(0x100700,0x4000),(0x100780,0x800)]:
        write(address,value)
    for step,words in [(1,[0,0,0xa002,0x8000]),(3,[0,0x20,2,0]),
                       (5,[0,0xa000,2,0]),(6,[0,0,0x1002,0])]:
        for part,value in enumerate(words):
            write(0x100800+step*8+part*2,value)
    write(0x100000,0x1820)
    # Ensure the DSP has actually read the initial uploaded operand.
    code.extend([0x3039,0x10,0xec0,0x4a40,0x67f6])
    code.extend([0x303c,1023])
    loop = 0x100+len(code)*2
    first = 0x4000+0x100+len(code)*2+2
    write(0x1000,0x2000)
    second = 0x4000+0x100+len(code)*2+2
    write(0x1000,0x4000)
    dbra = 0x100+len(code)*2
    code.extend([0x51c8,(loop-(dbra+2))&65535,0x23fc,0,0x600d,0,0x808,
                 0x4e72,0x2700,0x60fa])
    struct.pack_into('>'+'H'*len(code),sound,0x100,*code)
    host = ('load 0x4000,r1\nload 0x25a00000,r2\nload 0x240,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    host += smpc(6)
    for address,value in [(0xffffffe0,0xa55a0000),(0xffffffe8,0xa55a0000),
            (0xffffff00,1),(0xffffff04,0x20002000),(0xffffff80,0xffffff04),(0xffffff84,0x25a01000),
            (0xffffff88,256),(0xffffff8c,0x4a15)]:
        host += store(address,value)
    host += ('load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xffffffb0,r6\n'
             'mov #1,r7\nmov #0,r0\nmov.b r0,@r4\nmov.b r0,@r5\n')
    # The operation's unique encoding is also recorded for omitted-start control.
    host += 'load 0xffffff88,r1\nload 0x25a013fc,r2\nmov.l r7,@r6\n'
    host += ('poll_dma:\n'
             'mov.l @r1,r0\ntst r0,r0\nbf poll_dma\n'
             'mov.l @r2,r0\n' # drains final buffered destination before timer read
             'mov.b @r4,r0\nextu.b r0,r0\nshll8 r0\nmov r0,r9\n'
             'mov.b @r5,r0\nextu.b r0,r0\nor r9,r0\nload 0x2600ef00,r8\n')+record()
    host += store(0x25a00800,0x10000)+wait(0x05a00808,'wait_sound')
    for address in [0x25a00804,0x25a00808]:
        host += f'load {address:#x},r1\nmov.l @r1,r0\n'+record()
    host += 'finished:\nsleep\nbra finished\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x060ff000)
    hostcode = assembler.assemble(host,base=0x200)
    rom[0x200:0x200+len(hostcode)] = hostcode
    rom[0x4000:0x4000+len(sound)] = sound
    # Locate the unique initiating instruction in the actual final assembly.
    starts = [at for at in range(0x200,0x200+len(hostcode),2) if rom[at:at+2] == bytes.fromhex('2672')]
    assert len(starts)==1
    lines=['# Original main/68k shared DRAM allocation guest. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return {'scsp-cpu-memory.hex':'\n'.join(lines)+'\n',
            'scsp-cpu-memory.s':'; Original MIT: adjacent LICENSE.\n'+host,
            'scsp-cpu-memory.tsv':f'{first:08X}\n{second:08X}\n{starts[0]:08X}\n'},hashlib.sha256(rom).hexdigest()

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    files,digest=build()
    for name,content in files.items():
        path=HERE/name
        if args.check: assert path.read_text()==content,f'Rebuild {name}'
        else: path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} SCSP CPU memory guest; ROM SHA-256 {digest}')
