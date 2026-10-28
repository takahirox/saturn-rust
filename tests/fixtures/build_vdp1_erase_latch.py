#!/usr/bin/env python3
"""Original MIT SH-2 erase-capture guest; ST-013-R3 ch.4, §§4.2/4.4.

Guest instructions initialize both buffers, synchronize through TVSTAT, write
A then pending B, checksum/read both physical results and publish B. No SDK,
firmware or emulator-recorded goldens. Adjacent LICENSE applies to all outputs.
"""
import argparse
import hashlib
from pathlib import Path
from build_clock import put16, read
from build_hle_system import record
from build_video_transactions import blank, encode

HERE = Path(__file__).resolve().parent


def operand(address, register):
    return f'load {address:#x},r1\nmov.w @r1,r{register}\n'


def fill(label):
    return ('load 0x25c80000,r4\nmov #6,r3\nload 944,r6\nload 0x8000,r0\n'
            f'fill_row_{label}:\nmov #40,r2\nfill_dot_{label}:\n'
            'mov.w r0,@r4\nadd #2,r4\ndt r2\n'
            f'bf fill_dot_{label}\nadd r6,r4\ndt r3\nbf fill_row_{label}\n')


def observations(label):
    s = ('load 0x25c80000,r4\nmov #6,r3\nload 944,r6\nmov #0,r9\n'
         f'sum_row_{label}:\nmov #40,r2\nsum_dot_{label}:\n'
         'mov.w @r4,r0\nextu.w r0,r0\nadd r0,r9\nadd #2,r4\ndt r2\n'
         f'bf sum_dot_{label}\nadd r6,r4\ndt r3\nbf sum_row_{label}\n'
         'mov r9,r0\n') + record()
    for x, y in [(7, 1), (8, 1), (23, 2), (24, 2), (16, 2), (31, 3), (32, 3), (0, 4)]:
        s += read(0x25C80000 + y * 1024 + x * 2, 2)
    return s


def build():
    s = '; Original MIT VDP1 erase data/window capture diagnostic.\nload 0x0600e700,r8\n'
    for offset, value in [(0, 0), (2, 2), (4, 0), (8, 0x0201), (10, 0x0602)]:
        s += put16(0x25D00000 + offset, value)
    s += operand(0x100, 0) + 'load 0x25d00006,r1\nmov.w r0,@r1\n'
    s += fill('a') + put16(0x25F800E0, 0x20) + put16(0x25F800F0, 3)
    s += put16(0x25F80000, 0x8000)
    # Display A's initialized buffer and capture A. Manual change does not erase.
    s += put16(0x25D00002, 3) + blank('capture_a') + fill('b')
    s += operand(0x108, 0) + 'tst r0,r0\nbt omit_b\n'
    for offset, address in [(6, 0x102), (8, 0x104), (10, 0x106)]:
        s += operand(address, 0) + f'load {0x25D00000 + offset:#x},r1\nmov.w r0,@r1\n'
    # §4.1/§4.2: legal post-IN enable and erase/change, unchanged TVM.
    s += 'omit_b:\nload 0x25f80004,r1\nwait_enable:\nmov.w @r1,r0\ntst #8,r0\nbt wait_enable\n'
    s += put16(0x25D00000, 8) + put16(0x25D00002, 3) + blank('erase_a')
    # The erase/change captured B at OUT. CPU now accesses the A result.
    s += put16(0x25D00002, 3) + observations('a') + blank('inspect_b')
    s += put16(0x25D00000, 0) + put16(0x25D00002, 3) + observations('b')
    # Publish B without further erase; the parked guest leaves stable Canvas data.
    s += blank('publish_b')
    for address in [0x25D00016, 0x25D00010, 0x25D00014]:
        s += read(address, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return encode('vdp1-erase-latch', s,
                  [(0x100, 0x801F), (0x102, 0x83E0), (0x104, 0x0402),
                   (0x106, 0x0803), (0x108, 1)])


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, expanded_hash = build()
    for name, data in files.items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
    print(f'Expanded BIOS SHA-256: {expanded_hash}')
