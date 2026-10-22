#!/usr/bin/env python3
"""Original MIT SH-2/SCU-DSP B-bus diagnostic; adjacent LICENSE.

ST-097 §4.5 pp.134/136/138/140, ST-210 #27 and §4.1 RAM widths.
All results are authored from the literal word/stride contract, not captured
emulator output. No SDK, firmware, host service or guest-result injection.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record
from build_dsp_dma import data, start, wait, read

HERE = Path(__file__).resolve().parent


def build():
    s = 'load 0x2600e800,r8\n'
    expected = []
    for code, stride in [(0, 0), (1, 2), (2, 4), (3, 8),
                         (4, 16), (5, 32), (6, 64), (7, 128)]:
        base = 0x05c02000 + code * 0x200
        addresses = [base] if code == 0 else [base, base + 4] if code == 1 else [base + i*stride for i in range(4)]
        for address in addresses:
            s += store(address | 0x20000000, 0xeeeeeeee)
        # Counter wraps from 63 to 0. Both output words originate in DSP RAM.
        s += data(63, [0x11223344, 0xaabbccdd])
        s += start([0x00001c3f, 0x9c000000 | (base//4),
                    0xc0001002 | (code << 15), 0xf8000000])
        s += wait(f'wait_output{code}')
        for address in addresses:
            s += read(address | 0x20000000)
        expected += ([0xccddeeee] if code == 0 else [0x11223344, 0xaabbccdd] if code == 1
                     else [0x1122eeee, 0x3344eeee, 0xaabbeeee, 0xccddeeee])
    # Real B-bus sound RAM input. Add2 is the final-spec permitted encoding.
    for address, value in [(0x25a01000, 0x12345678), (0x25a01004, 0xbad0bad0),
                           (0x25a01008, 0x9abcdef0)]:
        s += store(address, value)
    s += start([0x00001c3f, 0x98000000 | (0x05a01000//4), 0xc0010002, 0xf8000000])
    s += wait('wait_input') + store(0x25fe0088, 63) + read(0x25fe008c) + read(0x25fe008c)
    expected += [0x12345678, 0x9abcdef0]
    s += 'load 0x600d,r0\n' + record() + 'finished:\nsleep\nbra finished\nnop\n'
    expected += [0x600d]
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original DSP B-bus two-cycle diagnostic. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return {'dsp-bus.hex': ('\n'.join(lines)+'\n').encode(),
            'dsp-bus.tsv': ''.join(f'{word:08X}\n' for word in expected).encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for name, content in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
