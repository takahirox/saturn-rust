#!/usr/bin/env python3
"""Original cartridge and battery reset-vector guest. MIT: adjacent LICENSE."""
import argparse
from pathlib import Path
import struct

from build_clock import put8, read
from build_hle_system import smpc, wait
from build_scu import assembler, store

HERE = Path(__file__).resolve().parent
TRACE = 0x26028000
PEER = 0x26028100
DMAC = 0xFFFFFF80
SCU = 0x25FE0000


def put(address, value, width):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.{width} r0,@r1\n'


def dmac(source, destination, label):
    s = ''.join(store(DMAC + offset, val) for offset, val in
                [(12, 0), (0, source), (4, destination), (8, 1), (12, 0x0A01), (0x30, 1)])
    return s + f'load {DMAC+12:#x},r1\n{label}:\nmov.l @r1,r0\ntst #2,r0\nbt {label}\n'


def status():
    s = put8(0x20100001, 1) + put8(0x20100003, 0) + put8(0x20100005, 0xF0) + smpc(0x10)
    return s + ''.join(read(0x20100021 + i*2, 1) for i in range(16))


def identity():
    return (read(0x24FFFFFF, 1) + read(0x04FFFFFE, 2) + read(0x24FFFFFC, 4)
            + read(0x20180000, 4) + read(0x24000000, 4))


def master():
    s = f'load {TRACE:#x},r8\n' + identity()
    s += put(0x257EFFFE, 1, 'w') + store(SCU + 0xB0, 0x23301FF0) + store(SCU + 0xB8, 0x13)
    s += store(0x20180000, 0x11223344) + store(0x24000000, 0xA1B2C3D4)
    s += put(0x24000000, 42, 'b') + put(0x240FFFFE, 0x1234, 'w') + put8(0x24FFFFFF, 0)
    s += store(0x22400000, 0x01020304) + put(0x22400000, 0xAB, 'b') + put(0x22400002, 0x5678, 'w')
    s += store(0x2247FFFC, 0x10203040) + store(0x22600000, 0x50607080)
    s += store(0x227FFFFC, 0xDEADBEEF)
    # Seed SH-2 DMAC data from ordinary WRAM, then read it back through DMAC.
    s += store(0x26028200, 0x9ABCDEF0)
    s += dmac(0x06028200, 0x02400008, 'dmac_to_cart')
    s += dmac(0x02400008, 0x06028204, 'dmac_from_cart')
    # SCU supports A-bus sources, with two ordered halfword reads per longword.
    s += ''.join(store(SCU + off, val) for off, val in
                 [(0, 0x02400000), (4, 0x06028208), (8, 4), (12, 0x102), (20, 7), (16, 0x101)])
    s += wait(0x06028208, 'scu_done')
    s += put8(0xFFFFFE92, 17)  # Enable cache. The P0 read fills a real cache line.
    for addr, width in [(0x22400000, 4), (0x02400000, 4), (0x2247FFFC, 4), (0x22480000, 4),
                        (0x224FFFFC, 4), (0x22500000, 4), (0x22600000, 4), (0x22680000, 4),
                        (0x227FFFFC, 4), (0x22800000, 4), (0x24000000, 4), (0x24100000, 4),
                        (0x241FFFFE, 2), (0x24000000, 1), (0x24FFFFFF, 1), (0x257EFFFE, 2),
                        (0x26028204, 4), (0x26028208, 4), (0x20200000, 4)]:
        s += read(addr, width)
    s += smpc(2) + wait(PEER + 12, 'slave_done')
    for addr in [0x24000004, 0x22400004]:
        s += read(addr, 2)
    for addr, byte in enumerate([0x20, 0x24, 0x32, 0x29, 0x23, 0x59, 0x59]):
        s += put8(0x20100001 + addr*2, byte)
    s += smpc(0x16)
    for addr, byte in enumerate([0x12, 0x34, 0x56, 0x78]):
        s += put8(0x20100001 + addr*2, byte)
    s += smpc(0x17)
    return s + status() + 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n'


def slave():
    return (f'load {PEER:#x},r8\n' + read(0x24FFFFFF, 1) + read(0x24000000, 4) + read(0x22400000, 4)
            + put8(0x24000005, 0x67) + put8(0x22400005, 0x89)
            + 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n')


def reset_guest():
    # The battery marker selects the post-SYSRES path because WRAM clears.
    s = ('load 0x20180009,r1\nmov.b @r1,r0\ntst r0,r0\nbf retained\n'
         'mov #1,r0\nmov.b r0,@r1\n')
    s += store(0x22400000, 0xC0DEBEEF) + put8(0x24000009, 0xA5)
    s += put8(0x20100001, 0x67) + smpc(0x17) + smpc(0x0D)
    s += 'wait_system:\nsleep\nbra wait_system\nnop\nretained:\n'
    s += f'load {TRACE:#x},r8\n' + read(0x22400000) + read(0x24000009, 1)
    s += read(0x20180009, 1) + status()
    # Execute an actual SH-2 watchdog manual reset after the system reset.
    s += put(0xFFFFFE82, 0x5A7F, 'w') + put(0xFFFFFE80, 0xA578, 'w') + put(0xFFFFFE80, 0x5AFF, 'w')
    return s + 'wait_manual:\nsleep\nbra wait_manual\nnop\n'


def manual():
    return (f'load {TRACE + 19*4:#x},r8\n' + read(0x22400000) + read(0x24000009, 1)
            + read(0x20180009, 1) + status() + 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n')


def build():
    entry = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n'
             'load 0x800,r1\njmp @r1\nnop\nslave:\nload 0x060fe000,r15\nload 0x1800,r1\njmp @r1\nnop\n')
    probe = f'load {TRACE:#x},r8\n' + identity() + status() + 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n'
    chunks = [(0, struct.pack('>IIII', 0x200, 0x060FF000, 0x200, 0x060FE000))]
    for address, source in [(0x200, entry), (0x800, master()), (0x1400, probe), (0x1800, slave()),
                            (0x1C00, reset_guest()), (0x2400, manual())]:
        chunks.append((address, assembler.assemble(source, base=address)))
    lines = ['# Original cartridge and SMPC battery synthetic BIOS; MIT: adjacent LICENSE.']
    end = 0
    for address, code in chunks:
        assert address >= end, hex(address)
        end = address + len(code)
        for i in range(0, len(code), 16):
            words = struct.unpack('>' + 'H' * (len(code[i:i+16])//2), code[i:i+16])
            lines.append(f'{address+i:04X} ' + ' '.join(f'{word:04X}' for word in words))
    return '\n'.join(lines) + '\n'


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    content = build()
    path = HERE / 'cartridge.hex'
    if parser.parse_args().check:
        assert path.read_text() == content, f'Rebuild {path.name}'
    else:
        path.write_text(content)
