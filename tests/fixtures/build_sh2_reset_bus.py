#!/usr/bin/env python3
"""Original MIT SH7604 manual/power reset bus diagnostic (H §§4.2/7.11.1).

Both CPUs execute a warmed kernel that starts a buffered write one clock before
watchdog reset. The manual-reset handler reads the completed operand. The power
reset entry reads only the pre-reset bytes. No running result is written by host.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def dispatch(master, slave):
    return ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n'
            f'load {master:#x},r1\njmp @r1\nnop\nslave:\n'
            f'load {slave:#x},r1\njmp @r1\nnop\n')


def setup(mailbox, kernel, report):
    s = f'load {mailbox+0x3c:#x},r14\nmov.l @r14,r0\ntst r0,r0\nbt cold\n'
    s += f'mov #1,r10\nload {report:#x},r1\njmp @r1\nnop\ncold:\n'
    s += store(0xFFFFFFE4, 0xA55A007C)  # CS3 byte-wide, six Tw per beat
    s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
    s += f'load {kernel:#x},r12\nload {mailbox+0x40:#x},r7\n'
    s += 'load 0xfffffe80,r1\nload 0x5a00,r2\nmov #0,r6\n'
    s += 'jsr @r12\nnop\njsr @r12\nnop\n'  # warm code; watchdog disabled
    s += 'mov #1,r0\nmov.l r0,@r14\nmov.l @r14,r0\n'  # drain marker
    s += ('load 0xfffffe82,r1\nload 0x5a7f,r0\nmov.w r0,@r1\n'
          'load 0xfffffe80,r1\nload 0xa578,r0\nmov.w r0,@r1\n'
          'load 0x5aff,r2\nload 0x12345678,r6\njsr @r12\nnop\n'
          'wait_reset:\nsleep\nbra wait_reset\nnop\n')
    return s


def report(mailbox, master):
    s = f'load {mailbox:#x},r8\n' + record(10)
    s += f'load {mailbox+0x40:#x},r1\nmov.l @r1,r0\n' + record()
    s += 'load 0xffffffe4,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0xfffffe92,r1\nmov.b @r1,r0\nextu.b r0,r0\n' + record()
    s += 'load 0xfffffe83,r1\nmov.b @r1,r0\nextu.b r0,r0\n' + record()
    s += 'load 0x600d,r0\n' + record()
    if master:
        s += smpc(2)
    return s + 'done:\nsleep\nbra done\nnop\n'


def build():
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x400, 0x060FE000)
    sections = {
        0x200: dispatch(0x800, 0x1800),
        0x400: 'mov #2,r10\n' + dispatch(0x3000, 0x3800),
        0x800: setup(0x2601E000, 0x5000, 0x3000),
        0x1800: setup(0x2601E100, 0x5800, 0x3800),
        0x3000: report(0x2601E000, True),
        0x3800: report(0x2601E100, False),
        0x5000: 'mov.w r2,@r1\nmov.l r6,@r7\nrts\nnop\n',
        0x5800: 'mov.w r2,@r1\nmov.l r6,@r7\nrts\nnop\n',
    }
    for address, source in sections.items():
        code = assembler.assemble(source, base=address)
        assert len(code) < 0x800
        rom[address:address+len(code)] = code
    lines = ['# Original SH7604 reset-bus BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return ('\n'.join(lines)+'\n').encode(), rom


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    content, rom = build()
    path = HERE/'sh2-reset-bus.hex'
    if args.check:
        assert path.read_bytes() == content, 'Rebuild sh2-reset-bus.hex'
    else:
        path.write_bytes(content)
    print(f'{"Verified" if args.check else "Built"} sh2-reset-bus.hex; expanded ROM SHA-256 {hashlib.sha256(rom).hexdigest()}')
