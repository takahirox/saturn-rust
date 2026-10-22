#!/usr/bin/env python3
"""Original MIT dual-SH-2 partial-share diagnostic (SH7604 §§7.2.1/7.8/7.10).

External-master UBC comparisons distinguish actual shared cycles from private
CS3 cycles. Local CPU comparisons remain enabled. No firmware, SDK or host
result writes. The BCR literal can be changed before boot to select total share.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc, wait

HERE = Path(__file__).resolve().parent


def watch(address, cycle, control):
    return (store(0xFFFFFF40, address)
            + f'load 0xffffff48,r1\nload {cycle:#x},r0\nmov.w r0,@r1\n'
            + f'load 0xffffff78,r1\nload {control:#x},r0\nmov.w r0,@r1\nmov.w @r1,r0\n')


def status():
    return 'load 0xffffff78,r1\nmov.w @r1,r0\nextu.w r0,r0\n' + record()


def build():
    s = store(0xFFFFFFE0, 0xA55A0FF0)
    s += 'load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r0\ntst r0,r9\nbf slave\n'
    s += 'load 0x2600e600,r8\n' + record(9)
    s += watch(0x0600D000, 0xA8, 0x3000) + smpc(2)
    s += wait(0x0600E400, 'master_ready')
    s += store(0x2600D004, 0x2468ACE0) + store(0x2600E404, 1)
    s += wait(0x0600E408, 'master_done') + status()
    s += 'bra local\nnop\nslave:\nload 0x2600e700,r8\n' + record(9)
    s += watch(0x0600D004, 0xA8, 0x3000)
    s += store(0x2600D000, 0x13579BDF) + store(0x2600E400, 1)
    s += wait(0x0600E404, 'slave_go') + status()
    s += store(0x2600E408, 1)
    s += 'local:\n' + watch(0x2600D000, 0x67, 0x1000)
    s += 'load 0x2600d000,r1\nmov.l @r1,r6\n' + status() + record(6)
    s += 'load 0x600d,r0\n' + record()
    s += 'finished:\nsleep\nbra finished\nnop\n'
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original MIT BSC sharing diagnostic. License: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at+16])))
    return {'bsc-share.hex': ('\n'.join(lines)+'\n').encode(),
            'bsc-share.tsv': b'00003000\n00009000\n13579BDF\n0000600D\n'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
