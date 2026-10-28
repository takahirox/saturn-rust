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



# Separate lifecycle records leave the earlier reset-domain oracle intact.
LIFECYCLE = [0xFF, 0xFF, 0xFF, 2, 0, 0x2600D200, 0x2600D204, 1,
             0, 0x13579BDF, 0xF0, 0xB7FF]


def observe(address, width=4):
    return read(address, width).replace('@r8', '@r9').replace('#4,r8', '#4,r9')


def raw_clock(mode):
    # Independently authored external firmware: no HLE address or service code.
    s = put16(0xFFFFFE80, 0xA506) + put8(0xFFFFFE92, 0)
    s += put8(0x20100063, 1) + put8(0x2010001F, 14 if mode else 15)
    s += put8(0xFFFFFE91, 0xE0)
    s += 'load 0xf0,r0\nldc r0,sr\nnop\nsleep\n'
    s += f'raw_resume_{mode}:\nload raw_resume_{mode},r0\n'
    s += 'load 0x0600e200,r1\nmov.l @r1,r1\ncmp/eq r1,r0\nmovt r0\n'
    s += f'load {0x0600e220 + mode*8:#x},r1\nmov.l r0,@r1\n'
    s += 'load 0x0600e204,r1\nmov.l @r1,r0\nload 0xf0,r1\ncmp/eq r1,r0\nmovt r0\n'
    s += f'load {0x0600e224 + mode*8:#x},r1\nmov.l r0,@r1\n'
    s += put8(0xFFFFFE91, 0x60)
    s += store(0x25FE00A0, 0xB7FF)
    s += smpc(0x19).replace('wait_smpc25', f'enable_reset_{mode}')
    return s


def source(hle):
    s = ''
    if not hle:
        s += ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\n'
              'bt master\nload 0x1800,r1\njmp @r1\nnop\nmaster:\n')
    s += 'load 0x0600e000,r8\nload 0x0600e100,r9\n'
    s += 'load 0xf0,r0\nldc r0,sr\n'
    for reg in range(10,15):
        s += f'load {0x1000+reg:#x},r{reg}\n'
    s += store(0x06000348, 0xB7FF)
    if not hle:
        s += 'load 0x06000000,r0\nldc r0,vbr\n'
        s += store(0x0600002C, 0x1C00)
    if hle:
        s += store(0x06000250,0x06008000)
    # Both uncached physical work RAM and the packed odd-lane backup survive.
    s += store(0x2600D000, 0x12345678) + put8(0x20180001, 0xA5)
    s += put16(0x2589000C, 0x56)  # HIRQ mask survives (CD block domain).
    for mode in [0, 1]:
        s += smpc(0x1A).replace('wait_smpc26',f'disable_reset_{mode}')
        s += store(0x2600D100,0)
        s += smpc(2).replace('wait_smpc2',f'wait_start_{mode}')
        s += wait(0x0600D100,f'slave_running_{mode}')
        s += put16(0x25D00000, 1) + put16(0x25F80000, 0x8000)
        for a in [0x25C00000, 0x25E00000, 0x25F00000, 0x25A00000]:
            s += put16(a, 0x5317)
        s += put8(0xFFFFFE14, 0x12) + put8(0xFFFFFE15, 0x34) + put8(0xFFFFFE01, 0x17)
        s += store(0xFFFFFF80, 0x2600D200) + store(0xFFFFFF84, 0x2600D204)
        s += store(0xFFFFFF88, 1) + store(0x2600D200, 0x13579BDF)
        s += store(0x2600D204, 0)
        if hle:
            s += f'mov #{mode},r4\nstc sr,r0\nload {0x0600e280+mode*4:#x},r1\nmov.l r0,@r1\n'
            s += call(0x06000320)
            s += f'stc sr,r0\nload {0x0600e288+mode*4:#x},r1\nmov.l r0,@r1\n'
            s += read(0x06000324)
        else:
            # Raw command sequence has no firmware dependency or HLE overlay.
            s += raw_clock(mode)
            s += f'mov #{mode},r0\n' + record()
        s += read(0x25F80000, 2) + read(0x25D00016, 2)
        for a in [0x25C00000, 0x25E00000, 0x25A00000]:
            s += read(a, 2)
        s += read(0x2600D000) + read(0x20180001, 1) + read(0x2589000C, 2)
        # INTBACK reports real DOTSEL and sound reset; register reads cannot
        # produce this report without a completed guest-issued command.
        s += put8(0x20100001, 1) + put8(0x20100003, 0) + put8(0x20100005, 0xF0)
        s += smpc(0x10).replace("wait_smpc16", f"wait_status_{mode}") + read(0x20100035, 1)
        for address, width in [(0xFFFFFE14,1),(0xFFFFFE15,1),(0xFFFFFE01,1),
                               (0xFFFFFFB0,4),(0xFFFFFF8C,4),
                               (0xFFFFFF80,4),(0xFFFFFF84,4),(0xFFFFFF88,4)]:
            s += observe(address,width)
        # Enabling channels alone cannot override NMIF; only guest read/clear
        # and a fresh DME/DE sequence can resume transfer.
        s += store(0xFFFFFF8C, 0x5A01) + store(0xFFFFFFB0, 3)
        s += 'mov #20,r2\nblocked_' + str(mode) + ':\ndt r2\nbf blocked_' + str(mode) + '\n'
        s += observe(0x2600D204)
        s += 'load 0xFFFFFFB0,r1\nmov.l @r1,r0\nmov #1,r0\nmov.l r0,@r1\n'
        s += wait(0x2600D204,f'restarted_{mode}')
        s += observe(0x2600D204)
        s += 'stc sr,r0\nmov.l r0,@r9\nadd #4,r9\n'
        s += observe(0x06000348)
        # Rendering must also accept the selected normal-graphics clock mode.
        s += f'load 0x20100021,r1\nmov.b @r1,r0\nextu.b r0,r0\nload {0x0600e240+mode*4:#x},r1\nmov.l r0,@r1\n'
        s += put16(0x25F80000, 0x8000 | mode)
    # Constant original PCM16 loop after recovery, deterministic stereo 4096.
    s += put16(0x25A01000, 0x1000) + put16(0x25A01002, 0x1000)
    for offset,value in [(2,0x1000),(4,0),(6,2),(8,0),(10,0),(12,0x100),(16,0),(22,0xE000),(0x400,0x20F),(0,0x1820)]:
        s += put16(0x25B00000+offset,value)
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
    outputs['clock-lifecycle.tsv'] = ''.join(f'{v:08X}\n' for v in LIFECYCLE*2).encode()
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
              (0x1800, assembler.assemble(slave,base=0x1800)),
              (0x1C00, assembler.assemble('mov.l r0,@-r15\nmov.l r1,@-r15\n'
                 'mov r15,r1\nadd #8,r1\nmov.l @r1,r0\nload 0x0600e200,r1\nmov.l r0,@r1\n'
                 'mov r15,r1\nadd #12,r1\nmov.l @r1,r0\nload 0x0600e204,r1\nmov.l r0,@r1\n'
                 'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop',base=0x1C00))]
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
