#!/usr/bin/env python3
"""Original SCU guest + DSP fixture; MIT: adjacent LICENSE.

Only the project's original SH-2 assembler and Python's standard library are used.
DSP words follow ST-097 §4.5; indirect DMA follows ST-210 #25, not the old table.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('assembler', HERE.parents[1] / 'homebrew/star-collector/build.py')
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)

# Independently authored DSP code. Parallel buses: X=M0,Y=M1; P=X*Y,A=0;
# AD2 ALU->A; ADD ALU->A ALL->MC2; ENDI. Inputs 7,6 -> 42+42=84.
PRODUCT = [0x02084000, 0x01020000, 0x18040000, 0x10043209, 0xF8000000]
# RA0=data/4; DMA D0,M0,4; JMP T0,2; NOP; reset CT0; CLR A;
# four MOV MC0,P / ADD ALU,A pairs; MOV ALL,MC1; WA0=result/4;
# reset CT1; DMA M1,D0,1; JMP T0,18; NOP; ENDI. Sum 7+11+13+17=48.
SUM = [0x99801400, 0xC0008004, 0xD3400002, 0,
       0x00001C00, 0x00020000, 0x01C00000, 0x10040000,
       0x01C00000, 0x10040000, 0x01C00000, 0x10040000,
       0x01C00000, 0x10040000, 0x00003109,
       0x9D801800, 0x00001D00, 0xC0009101, 0xD3400012, 0, 0xF8000000]


def store(address, value):
    return f'load {address:#x},r1\nload {value & 0xffffffff:#x},r0\nmov.l r0,@r1\n'


def program():
    s = 'mov #0,r0\nldc r0,sr\n'
    # Enable only DSP, DMA0/1 and timer0/1 interrupts; blanking remains masked.
    s += store(0x25FE00A0, 0xBFFF & ~0xC38)
    for address, value in [(0x06005000, 7), (0x06005004, 11), (0x06005008, 13), (0x0600500C, 17)]:
        s += store(address, value)
    for a, v in [(0x25FE0088, 0), (0x25FE008C, 7), (0x25FE0088, 64), (0x25FE008C, 6)]: s += store(a, v)
    s += store(0x25FE0080, 0x8000)
    for word in PRODUCT: s += store(0x25FE0084, word)
    s += store(0x25FE0080, 0x8000) + store(0x25FE0080, 0x10000)
    s += 'load 0x26004000,r1\nwait_product:\nmov.l @r1,r0\nmov #1,r2\ncmp/eq r2,r0\nbf wait_product\n'
    # Direct DMA0 from high work RAM into VDP2 VRAM, real guest IRQ handler.
    for off, val in [(0, 0x06005000), (4, 0x05E00090), (8, 16), (12, 0x101), (20, 7), (16, 0x101)]:
        s += store(0x25FE0000 + off, val)
    s += 'load 0x26004004,r1\nwait_dma0:\nmov.l @r1,r0\nmov #1,r2\ncmp/eq r2,r0\nbf wait_dma0\n'
    for i, val in enumerate([4, 0x05E000A0, 0x06005000, 4, 0x05E000A4, 0x86005004]):
        s += store(0x06007000 + i * 4, val)
    for off, val in [(4, 0x06007000), (12, 0x101), (20, 0x1000007), (16, 0x101)]:
        s += store(0x25FE0020 + off, val)
    s += 'load 0x26004004,r1\nwait_dma1:\nmov.l @r1,r0\nmov #2,r2\ncmp/eq r2,r0\nbf wait_dma1\n'
    s += store(0x25FE0080, 0x8000)
    for word in SUM: s += store(0x25FE0084, word)
    s += store(0x25FE0080, 0x8000) + store(0x25FE0080, 0x10000)
    s += 'load 0x26004000,r1\nwait_sum:\nmov.l @r1,r0\nmov #2,r2\ncmp/eq r2,r0\nbf wait_sum\n'
    s += store(0x25FE0090, 10) + store(0x25FE0094, 16) + store(0x25FE0098, 0x101)
    s += 'load 0x26004008,r1\nwait_t0:\nmov.l @r1,r0\nmov #1,r2\ncmp/eq r2,r0\nbf wait_t0\n'
    s += 'load 0x2600401c,r1\nwait_t1:\nmov.l @r1,r0\nmov #1,r2\ncmp/eq r2,r0\nbf wait_t1\n'
    s += store(0x25FE0098, 0) + store(0x06004018, 0x600D)
    s += 'mov #-16,r0\nldc r0,sr\nsleep\n'
    return s


def build():
    sections = {0x200: program()}
    # Handlers preserve R0–R2 on the real guest stack, leaving SR/PC for RTE.
    push = 'add #-12,r15\nmov.l r0,@r15\nadd #4,r15\nmov.l r1,@r15\nadd #4,r15\nmov.l r2,@r15\n'
    pop = 'mov.l @r15,r2\nadd #-4,r15\nmov.l @r15,r1\nadd #-4,r15\nmov.l @r15,r0\nadd #12,r15\nrte\nnop\n'
    dsp = push + 'load 0x25fe0080,r1\nmov.l @r1,r0\nload 0x06004020,r1\nmov.l r0,@r1\n'
    dsp += 'load 0x26004000,r1\nmov.l @r1,r0\nmov #0,r2\ncmp/eq r2,r0\nbf sum_irq\n'
    dsp += store(0x25FE0088, 128) + 'load 0x25fe008c,r1\nmov.l @r1,r0\nload 0x0600400c,r1\nmov.l r0,@r1\nbra count_dsp\nnop\n'
    dsp += 'sum_irq:\nload 0x26006000,r1\nmov.l @r1,r0\nload 0x06004014,r1\nmov.l r0,@r1\n'
    dsp += 'count_dsp:\nload 0x26004000,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n' + pop
    sections[0x1800] = dsp
    dma0 = push + 'load 0x25e00090,r1\nmov.l @r1,r0\nload 0x06004010,r1\nmov.l r0,@r1\n'
    dma0 += 'load 0x26004004,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n' + pop
    sections[0x1C00] = dma0
    dma1 = push
    for src, dst in [(0x25E000A0, 0x06004024), (0x25E000A4, 0x06004028)]:
        dma1 += f'load {src:#x},r1\nmov.l @r1,r0\nload {dst:#x},r1\nmov.l r0,@r1\n'
    dma1 += 'load 0x26004004,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n' + pop
    sections[0x1E00] = dma1
    for addr, mailbox in [(0x2000, 0x06004008), (0x2200, 0x0600401C)]:
        sections[addr] = push + f'load {mailbox:#x},r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n' + pop
    chunks = [(0, struct.pack('>II', 0x200, 0x060FF000))]
    for vector, target in [(0x45, 0x1800), (0x4B, 0x1C00), (0x4A, 0x1E00), (0x43, 0x2000), (0x44, 0x2200)]:
        chunks.append((vector*4, struct.pack('>I', target)))
    for address, source in sections.items():
        chunks.append((address, assembler.assemble(source, base=address)))
    chunks.sort()
    rom = bytearray(524288)
    lines = ['# Original SCU CPU/DMA/DSP/timer BIOS fixture; MIT: adjacent LICENSE.',
             '# Generated by build_scu.py; unspecified bytes are zero.']
    end = 0
    for address, data in chunks:
        assert address >= end and address + len(data) <= len(rom), hex(address)
        end = address + len(data)
        rom[address:end] = data
        for i in range(0, len(data), 16):
            words = struct.unpack('>' + 'H' * (len(data[i:i+16])//2), data[i:i+16])
            lines.append(f'{address+i:04X} ' + ' '.join(f'{word:04X}' for word in words))
    return '\n'.join(lines) + '\n', hashlib.sha256(rom).hexdigest()

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text, digest = build()
    path = HERE / 'scu.hex'
    if args.check:
        assert path.read_text() == text, 'Rebuild scu.hex'
    else:
        path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} scu.hex; expanded ROM SHA-256 {digest}')
