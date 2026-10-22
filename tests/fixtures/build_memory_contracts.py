#!/usr/bin/env python3
"""Original MIT backup aliases + SCSP DRAM geometry boot ROM.

SCSP ST-077 §4.2 table4.32/fig4.58. Both CPUs make the accesses;
the 68k programs PCM, SCSP DMA and an executing effects-DSP memory read.
No firmware, SDK or host-computed guest results are included.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def build():
    words = []
    def emit(*values): words.extend(values)
    def movw(value, address): emit(0x33FC, value & 65535, address >> 16, address & 65535)
    def movl(value, address): emit(0x23FC, value >> 16, value & 65535, address >> 16, address & 65535)
    def read(address, mailbox):
        emit(0x7000, 0x3039, address >> 16, address & 65535,
             0x23C0, 0, mailbox)
    movw(0x20F, 0x100400)
    movl(0x2345DCBB, 0x20000)
    movl(0x2345DCBB, 0x20004)
    read(0x20000, 0x900)
    movw(0xF, 0x100400)
    read(0x20000, 0x904)
    movw(0x9876, 0x20000)  # uninstalled second chip, no write strobe
    movw(0x20F, 0x100400)
    read(0x20000, 0x908)
    # DMA 0x20000 -> COEF0, with the geometry selected by the 68k.
    movw(0, 0x100412); movw(0x2700, 0x100414); movw(0x1002, 0x100416)
    # Wait for DEXE to clear via actual SCSP MMIO.
    loop = len(words) * 2 + 0x100
    emit(0x3039, 0x10, 0x416, 0x0240, 0x1000)
    next_pc = 0x100 + len(words) * 2 + 2
    emit(0x6600 | ((loop - next_pc) & 255))
    read(0x100700, 0x90C)
    # TABLE sound-memory read at RBP16; N+2 -> MEMS0.
    for address, value in [(0x402,16), (0x80C,0xA002), (0x80E,0x8000), (0x81A,0x20)]:
        movw(value, 0x100000 + address)
    # PCM16 at the same high-RAM address, bipolar loop, opposite-pan slots.
    for slot, pan, start in [(0,15,0), (1,31,1)]:
        for offset, value in [(2,0), (4,start), (6,4), (12,0x100),
                              (16,0), (22,0xE000 | pan << 8), (0,0x1822)]:
            movw(value, 0x100000 + slot*32 + offset)
    movl(0x600D, 0x910)
    emit(0x4E72, 0x2700, 0x60FA)
    sound = bytearray(0x1000)
    struct.pack_into('>II', sound, 0, 0x7FF00, 0x100)
    struct.pack_into('>'+'H'*len(words), sound, 0x100, *words)
    s = 'load 0x2600f000,r8\n'
    # Eight aliases with both CPU-visible cache-through lanes. The guest
    # increments battery RAM on each boot; it never writes host state.
    s += 'load 0x201f0001,r1\nmov.b @r1,r0\nadd #1,r0\nmov.b r0,@r1\n'
    for alias in range(8):
        s += f'load {0x20180000+alias*0x10000:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\n' + record()
    s += 'load 0x25b00400,r1\nload 0x200,r0\nmov.w r0,@r1\n'
    s += ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x400,r3\ncopy:\n'
          'mov.l @r1+,r0\nmov.l r0,@r2\nadd #4,r2\ndt r3\nbf copy\n')
    s += smpc(6)
    s += ('load 0x25a00910,r1\nwait:\nmov.l @r1,r0\ntst r0,r0\nbt wait\n')
    for address in [0x25A00900,0x25A00904,0x25A00908,0x25A0090C,0x25A00910]:
        s += f'load {address:#x},r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x22000000,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x20000000,r1\nmov #0,r0\nmov.l r0,@r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'sleep\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    code = assembler.assemble(s, base=0x200)
    assert len(code) < 0x1E00
    rom[0x200:0x200+len(code)] = code
    rom[0x2000:0x3000] = sound
    lines = ['# Original memory-contracts ROM. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return ('\n'.join(lines)+'\n').encode()


if __name__ == '__main__':
    p=argparse.ArgumentParser(); p.add_argument('--check',action='store_true'); args=p.parse_args()
    data=build(); path=HERE/'memory-contracts.hex'
    if args.check: assert path.read_bytes()==data, 'Rebuild memory-contracts.hex'
    else: path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} memory-contracts.hex; SHA-256 {hashlib.sha256(data).hexdigest()}')
