#!/usr/bin/env python3
"""Original MIT SMPC region and CD/sound-reset status guest.
ST-169 §§2.3.4–7/2.4.2, OREG9–11; no firmware or SDK dependencies.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc
from build_clock import put8, read

HERE = Path(__file__).resolve().parent


def build():
    s = 'load 0x26017000,r8\n'
    # A separately executing 68k STOP guest makes SNDON a valid operation.
    s += store(0x25a00000, 0x7fff0) + store(0x25a00004, 0x100)
    s += store(0x25a00100, 0x4e722700)
    for n, command in enumerate([9, 6, 8, 7]):
        s += smpc(command)
        s += put8(0x20100001, 1) + put8(0x20100003, 0) + put8(0x20100005, 0xf0)
        s += smpc(0x10).replace('wait_smpc16', f'status_{n}')
        for address in [0x20100033, 0x20100035, 0x20100037, 0x2010005f, 0x20100063]:
            s += read(address, 1)
    s += 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n'
    code = assembler.assemble(s, base=0x200)
    chunks = [(0, struct.pack('>II', 0x200, 0x060ff000)), (0x200, code)]
    text = ['# Original SMPC status BIOS. MIT: adjacent LICENSE.']
    for address, chunk in chunks:
        for i in range(0, len(chunk), 16):
            words = struct.unpack('>'+'H'*(len(chunk[i:i+16])//2), chunk[i:i+16])
            text.append(f'{address+i:04X} '+' '.join(f'{w:04X}' for w in words))
    return ('\n'.join(text)+'\n').encode()


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--check', action='store_true'); args = p.parse_args()
    data = build(); path = HERE/'smpc-status.hex'
    if args.check: assert path.read_bytes() == data, 'Rebuild smpc-status.hex'
    else: path.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} smpc-status.hex; SHA-256 {hashlib.sha256(data).hexdigest()}')
