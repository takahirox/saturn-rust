#!/usr/bin/env python3
"""Original MIT SH7604 §§7.3.1/7.11.2 guest fetch/write-buffer diagnostic.

Each real CPU executes uncached self-modifying instructions, measures peripheral
access overlapping a buffered external write, and drives internal-source DMAC.
All results are computed by guest instructions. Golden values come from the
manual's bus states, instruction encodings, and the explicitly authored operands.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def program(data, mailbox, master):
    s = f'load {mailbox:#x},r8\n'
    # Width is configured once before any CS3 access; tests select it at reset.
    s += store(0xFFFFFFE4, 0xA55A00FC) + store(0xFFFFFFE8, 0xA55A00C0)
    s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
    # Two original functions in real work RAM. The first modifies its retained
    # successor; the second branches to the word beside its modifying slot.
    for offset, value in [(0, 0x2321E607), (4, 0x000B0009),
                          (0x20, 0x0009A000), (0x24, 0x2321E607), (0x28, 0x000B0009)]:
        s += store(data + offset, value)
    s += f'load {data:#x},r1\nmov.l @r1,r0\n'  # drain initialization writes
    s += f'load {data+2:#x},r3\nload 0xe62a,r2\nload {data:#x},r4\n'
    for _ in range(2):
        s += 'jsr @r4\nnop\n' + record(6)
    s += f'load {data+0x26:#x},r3\nload {data+0x22:#x},r4\njsr @r4\nnop\n' + record(6)
    # Warm the instruction cache before each timed call. FRT starts at phase0
    # after SBYCR write. high(2)+low(2)+buffer acceptance(1)+BRR(2)+high(2)
    # =9 clocks at the final latch, hence floor(9/8)=1 at every external width.
    s += 'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\nload 0xfffffe01,r10\n'
    s += f'load {data+0x100:#x},r11\nload 0x11223344,r6\nload 0x6000,r12\n'
    for _ in range(2):
        s += 'jsr @r12\nnop\n'
    s += record() + record(9) + record(13)
    # Warm both DMA code and literal lines before measuring. FRT reset plus
    # MOV #1(1), DMAOR write(1), NOPs(4), high read(2) =8 clocks at latch.
    dma_entry = 0x7000 if master else 0x7800
    s += f'load {dma_entry:#x},r12\n'
    for _ in range(2):
        s += 'jsr @r12\nnop\n'
    s += record() + record(6)
    s += 'load 0x600d,r0\n' + record()
    if master:
        s += smpc(2)
    s += 'done:\nsleep\nbra done\nnop\n'
    return s


def dma_program(data):
    s = store(0xFFFFFFB0, 0)
    s += store(data+0x200, 0) + f'load {data+0x200:#x},r1\nmov.l @r1,r0\n'
    s += 'load 0xfffffe00,r9\nmov #49,r0\nmov.b r0,@r9\n'
    s += 'load 0xffffff8c,r1\nmov.l @r1,r0\n'  # read TE before clearing it
    for offset, value in [(0,0xFFFFFE00),(4,data+0x200),(8,2),(12,0x4201)]:
        s += store(0xFFFFFF80+offset,value)
    s += ('load 0xffffffb0,r14\nmov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
          'mov #1,r0\nmov.l r0,@r14\n') + 'nop\n'*4
    s += 'mov.b @r4,r0\nmov.b @r5,r6\nextu.b r6,r6\n'
    s += 'mov #103,r0\nmov.b r0,@r9\nload 0xffffff88,r1\n'
    s += 'dma_wait:\nmov.l @r1,r0\ntst r0,r0\nbf dma_wait\n'
    s += f'load {data+0x200:#x},r1\nmov.l @r1,r0\n'
    return s + 'rts\nnop\n'


def build():
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    entry = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n'
             'load 0x800,r1\njmp @r1\nnop\nslave:\nload 0x060fe000,r15\n'
             'load 0x2800,r1\njmp @r1\nnop\n')
    timed = ('mov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
             'mov.b @r4,r0\nmov.b @r5,r0\nmov.l r6,@r11\nmov.b @r10,r9\n'
             'mov.b @r4,r0\nmov.b @r5,r0\nextu.b r0,r0\n'
             'mov.l @r11,r13\nrts\nnop\n')
    for address, source in [(0x200,entry),(0x800,program(0x26050000,0x2601D000,True)),
                            (0x2800,program(0x26060000,0x2601D100,False)),(0x6000,timed),
                            (0x7000,dma_program(0x26050000)),(0x7800,dma_program(0x26060000))]:
        code=assembler.assemble(source,base=address)
        assert len(code)<(0x2000 if address in [0x800,0x2800] else 0x400)
        rom[address:address+len(code)]=code
    lines=['# Original SH7604 bus-pipeline BIOS; MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected=[7,42,42,1,0xFFFFFFFF,0x11223344,0x31310000,1,0x600D]
    return {'sh2-bus-pipeline.hex':('\n'.join(lines)+'\n').encode(),
            'sh2-bus-pipeline.tsv':''.join(f'{v:08X}\n' for v in expected).encode()}


if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--check',action='store_true')
    args=p.parse_args()
    for name,content in build().items():
        path=HERE/name
        if args.check:
            assert path.read_bytes()==content,f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
