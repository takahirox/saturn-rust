#!/usr/bin/env python3
"""Original MIT clock/reset-domain guests; ST-169 §§2.3.9–10, ST-162-R1 §2.2.

Both versions execute the same authored writes and observations; one calls the
HLE ABI, the other drives SMPC directly from an original synthetic BIOS.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import call, smpc, record, wait

HERE = Path(__file__).resolve().parent


def put16(address, value):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.w r0,@r1\n'


def put8(address, value):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.b r0,@r1\n'


def read(address, width=4):
    op = {1:'b', 2:'w', 4:'l'}[width]
    s = f'load {address:#x},r1\nmov.{op} @r1,r0\n'
    if width != 4:
        s += f'extu.{op} r0,r0\n'
    return s + record()


# Literal hardware/ABI expectations authored independently of execution.
RESET_EXPECTED = [0, 0x1000, 0, 0, 0, 0x12345678, 0xA5, 0x56]



def source(hle):
    s = ''
    if not hle:
        s += ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\n'
              'bt master\nload 0x1800,r1\njmp @r1\nnop\nmaster:\n')
    s += 'load 0x0600e000,r8\n'
    if hle:
        s += store(0x06000250,0x06008000)
    # Both uncached physical work RAM and the packed odd-lane backup survive.
    s += store(0x2600D000, 0x12345678) + put8(0x20180001, 0xA5)
    s += put16(0x2589000C, 0x56)  # HIRQ mask survives (CD block domain).
    for mode in [0, 1]:
        s += store(0x2600D100,0)
        s += smpc(2).replace('wait_smpc2',f'wait_start_{mode}')
        s += wait(0x0600D100,f'slave_running_{mode}')
        s += put16(0x25D00000, 1) + put16(0x25F80000, 0x8000)
        for a in [0x25C00000, 0x25E00000, 0x25F00000, 0x25A00000]:
            s += put16(a, 0x5317)
        if hle:
            s += f'mov #{mode},r4\n' + call(0x06000320)
            s += read(0x06000324)
        else:
            # Raw command sequence has no firmware dependency or HLE overlay.
            s += smpc(0x0E if mode else 0x0F)
            s += f'mov #{mode},r0\n' + record()
        s += read(0x25F80000, 2) + read(0x25D00016, 2)
        for a in [0x25C00000, 0x25E00000, 0x25A00000]:
            s += read(a, 2)
        s += read(0x2600D000) + read(0x20180001, 1) + read(0x2589000C, 2)
        # INTBACK reports real DOTSEL and sound reset; register reads cannot
        # produce this report without a completed guest-issued command.
        s += put8(0x20100001, 1) + put8(0x20100003, 0) + put8(0x20100005, 0xF0)
        s += smpc(0x10).replace("wait_smpc16", f"wait_status_{mode}") + read(0x20100035, 1)
        # Rendering must also accept the selected normal-graphics clock mode.
        s += put16(0x25F80000, 0x8000 | mode)
    s += put16(0x25E00000, 0x83E0)
    s += 'load 0x600d,r0\n' + record() + 'idle:\nsleep\nbra idle\nnop\n'
    return s


def build():
    outputs = {}
    # HLE records include SYS_GETSYSCK; raw BIOS records contain the operand.
    expected = []
    for mode in [0, 1]:
        expected += [mode] + RESET_EXPECTED + [0x75 if mode else 0x35]
    expected += [0x600D]
    outputs['clock.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    slave = ('load 0x2600d100,r1\nslave_loop:\nmov.l @r1,r0\nadd #1,r0\n'
             'mov.l r0,@r1\nbra slave_loop\nnop\n')
    code = bytearray(0x4100)
    main = assembler.assemble(source(True),base=0x06004000)
    assert len(main)<0x4000
    code[:len(main)] = main
    worker = assembler.assemble(slave,base=0x06008000)
    code[0x4000:0x4000+len(worker)] = worker
    outputs['clock.iso'] = assembler.build(bytes(code), (HERE/'LICENSE').read_bytes(),
                                          'ORIGINAL CLOCK TEST', 'CLOCK V1.000')
    chunks = [(0, struct.pack('>IIII', 0x200, 0x060FF000, 0x200, 0x060FF000)),
              (0x200, assembler.assemble(source(False), base=0x200)),
              (0x1800, assembler.assemble(slave,base=0x1800))]
    text = ['# Original synthetic clock/reset BIOS. MIT: adjacent LICENSE.']
    for address, chunk in chunks:
        for i in range(0, len(chunk), 16):
            words = struct.unpack('>'+'H'*(len(chunk[i:i+16])//2),chunk[i:i+16])
            text.append(f'{address+i:04X} '+' '.join(f'{w:04X}' for w in words))
    outputs['clock.hex'] = ('\n'.join(text)+'\n').encode()
    return outputs


if __name__ == '__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
