#!/usr/bin/env python3
"""Original MIT SMPC command/byte-lane guests. No firmware/SDK dependency.
ST-169 §1.2 fig1.3, §2.2, §§2.3–2.5; SH7604 table4.6 and §7.11.2.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_smpc import byte, issue, idle, assembler
from build_clock import read
from build_hle_system import record
from build_video_transactions import encode

HERE = Path(__file__).resolve().parent


def build():
    s = 'load 0x2601a000,r8\n'
    s += read(0x20100063, 1) + read(0x2010005f, 1)
    for i, value in enumerate([0x12, 0x34, 0x56, 0x78]):
        s += byte(0x20100001 + i * 2, value)
    s += issue(0x17) + read(0x20100063, 1) + read(0x2010005f, 1)
    s += idle('smem') + read(0x20100063, 1) + read(0x2010005f, 1)
    s += issue(9) + idle('cd_off')
    for i, value in enumerate([1, 0, 0xf0]):
        s += byte(0x20100001 + i * 2, value)
    s += issue(0x10) + idle('status')
    for address in [0x20100063, 0x2010005f, 0x20100033, 0x20100037,
                    0x20100039, 0x2010003b, 0x2010003d, 0x2010003f]:
        s += read(address, 1)
    # Poll the real SCU pending bit before allowing the actual IRQ handler.
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
    s += 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n'
    s += 'load 0x25fe00a0,r1\nload 0xbf7f,r0\nmov.l r0,@r1\nmov #0,r0\nldc r0,sr\n'
    s += 'load 0x2601a100,r1\nirq_done:\nmov.l @r1,r0\ntst r0,r0\nbt irq_done\n' + record()
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x80,r2\nand r2,r0\n' + record()
    s += issue(8) + idle('cd_on') + issue(0x19) + idle('enable')
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    handler = assembler.assemble('load 0x2601a100,r12\nmov.l @r12,r13\nadd #1,r13\nmov.l r13,@r12\nrte\nnop\n', base=0x1800)
    operands = [(0x47 * 4, 0), (0x47 * 4 + 2, 0x1800)]
    operands += [(0x1800+i, int.from_bytes(handler[i:i+2], 'big')) for i in range(0,len(handler),2)]
    files, _ = encode('smpc-transactions', s, operands)
    expected = [0, 0, 1, 0x17, 0, 0x17, 0, 0x10, 1, 0x40,
                0x12, 0x34, 0x56, 0x78, 0x80, 1, 0, 0x600d]
    files['smpc-transactions.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    # Each original fault case boots through real reset vectors. The guest
    # publishes a sentinel only if the prohibited operation silently succeeds.
    cases = {
        'read': 'load 0x0010001f,r1\nmov.b @r1,r0\n',
        'missing-sf': byte(0x0010001f, 9),
        'invalid-sf': byte(0x20100063, 0),
        'busy': issue(9) + byte(0x0010001f, 8),
        'command': byte(0x20100063, 1) + byte(0x0010001f, 0xff),
        'parameters': byte(0x20100063, 1) + byte(0x0010001f, 0x10),
        'word': byte(0x20100063, 1) + 'load 0x2010001e,r1\nmov #9,r0\nmov.w r0,@r1\n',
        'long': 'load 0x20100000,r1\nload 0x00120034,r0\nmov.l r0,@r1\n',
        'word-read': 'load 0x20100062,r1\nmov.w @r1,r0\n',
        'long-read': 'load 0x20100060,r1\nmov.l @r1,r0\n',
    }
    for name, source in cases.items():
        source += byte(0x2601a200, 0x55) + 'park:\nsleep\nbra park\nnop\n'
        extra, _ = encode('smpc-fault-' + name, source)
        files.update(extra)
    return files


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); args = p.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        if name.endswith('.hex'):
            print(f'{"Verified" if args.check else "Built"} {name}; text SHA-256 {hashlib.sha256(data).hexdigest()}')
