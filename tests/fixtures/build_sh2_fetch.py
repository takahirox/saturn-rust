#!/usr/bin/env python3
"""Original MIT dual SH-2 IF/ID/FRT guest; adjacent LICENSE.

The oracle below is a slot table from P figs7.1/4/6/7/8, not an emulator trace.
The master sleeps before the slave's kernels, avoiding an uncontrolled peer IF
in the timing oracle. They exchange computed data and an NMI acknowledgement.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc
from build_sh2_timing import kernel

HERE = Path(__file__).resolve().parent
BODIES = ['add #1,r6\n'*32, 'add #1,r6\n'*32,
          'mov.l @r12,r6\nnop\n'*16, 'mov.l @r12,r6\nnop\n'*16]
BASES = [0x8000, 0x8402, 0x8800, 0x8C02]
# body, route, extra software waits, cold cache
CASES = [(0,'cached',0,False), (1,'cached',0,False),
         (2,'cached',0,False), (3,'cached',0,False),
         (0,'p1',0,False), (1,'p1',0,False),
         (2,'p1',0,False), (3,'p1',0,False),
         (0,'p1',2,False), (3,'p1',2,False),
         (0,'cs0',0,False), (3,'cs0',0,False),
         (0,'cached',0,True), (3,'cached',0,True)]


def ticks(body, route, waits=0, cold=False, omit=False):
    """MA_i completes before real IF_(i+3); retained if shares MA_i.

    One loop iteration is the slot containing EX_i, ID_(i+1), IF_(i+2)
    and (unless split) MA_(i-1). Cache hit/retained slots cost one state.
    FRT is reset on the resume-store MA edge; the final high read latches it.
    """
    base = BASES[body]
    loads = body >= 2 and not omit
    ma = [0,1,0,1,2,2] + ([1,0]*16 if loads else [0]*32) + [2,2,0]
    seen = {base // 16}  # entry IF occurred before the timer was resumed
    slot = 0
    samples = []
    for n, duration in enumerate(ma):
        address = base + (n+2)*2
        real = address % 4 == 0
        fetch = 1
        if real:
            if route == 'p1':
                fetch = 2 + waits
            elif route == 'cs0':
                fetch = 4 + 2*waits  # fixed two physical CS0 word beats
            elif cold and address // 16 not in seen:
                fetch = 8  # four high-RAM longwords, two states each
                seen.add(address // 16)
        previous_ma = ma[n-1] if n else 0
        slot += previous_ma + fetch if real else max(previous_ma, 1)
        samples.append(slot + duration)
    return (samples[38] - samples[3]) // 8


def build():
    sections = {base: kernel(body) for base, body in zip(BASES, BODIES)}
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
         'load 0x2600d800,r8\nbra common\nnop\nslave:\n'
         'load 0x060fe000,r15\nload 0x2600d900,r8\ncommon:\n')
    s += store(0xFFFFFFE4, 0xA55A00FC) + store(0xFFFFFFE8, 0xA55A0000)
    s += ('load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0xc0000400,r12\nmov #3,r0\nmov.l r0,@r12\n')
    # The guest installs identical authored kernel bytes in each CPU's own RAM.
    for base, source in sections.items():
        code = assembler.assemble(source, base=base)
        # No PC-relative instructions occur in these leaf kernels.
        for i in range(0, len(code), 2):
            value = int.from_bytes(code[i:i+2], 'big')
            s += f'load {0x26000000+base+i:#x},r1\nload {value:#x},r0\nmov.w r0,@r1\n'
    for body, route, waits, cold in CASES:
        s += store(0xFFFFFFE8, 0xA55A0000 | waits << 6)
        # Purge for reproducible cold entry, or warm twice for resident entry.
        s += 'load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
        entry = BASES[body] + (0x06000000 if route == 'cached' else 0x26000000 if route == 'p1' else 0x20000000)
        for _ in range(1 if cold else 2):
            s += f'load {entry:#x},r3\nmov #0,r6\njsr @r3\nnop\n'
        s += record() + record(6)
    s += ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf consumer\n')
    s += store(0x2600DA00, 0x12345678) + smpc(2)
    s += 'load 0x600d,r0\n' + record() + 'master_wait:\nsleep\nbra master_wait\nnop\n'
    s += ('consumer:\nload 0x2600da00,r1\nmov.l @r1,r0\nadd #1,r0\n'
          'add #4,r1\nmov.l r0,@r1\n') + record() + smpc(0x18)
    s += 'slave_wait:\nsleep\nbra slave_wait\nnop\n'
    sections[0x200] = s
    sections[0x7000] = ('load 0x2600da04,r1\nmov.l @r1,r0\n' + record() + 'rte\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    struct.pack_into('>I', rom, 11*4, 0x7000)
    for address, source in sections.items():
        code = assembler.assemble(source, base=address)
        assert len(code) < (0x6E00 if address == 0x200 else 0x400)
        rom[address:address+len(code)] = code
    lines = ['# Original MIT SH-2 IF/ID guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected = [value for body,route,waits,cold in CASES for value in
                [ticks(body,route,waits,cold), 3 if body >= 2 else 32]]
    artifacts = {'sh2-fetch.hex': ('\n'.join(lines)+'\n').encode(),
                 'sh2-fetch.s': ('; Original MIT IF/ID guest; adjacent LICENSE.\n'+'\n'.join(
                     f'; section {at:08X}\n{body}' for at,body in sections.items())).encode(),
                 'sh2-fetch.tsv': ''.join(f'{value:08X}\n' for value in expected).encode()}
    artifacts['sh2-fetch.sha256'] = ''.join(f'{hashlib.sha256(data).hexdigest()}  {name}\n' for name,data in artifacts.items()).encode()
    return artifacts

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
