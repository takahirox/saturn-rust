#!/usr/bin/env python3
"""Original MIT throughput guest combining authored CPU, video and sound fixtures.

No firmware or SDK is used. The existing project SH-2 assembler emits all setup
and arithmetic instructions. Sound code/data comes from the original effects
fixture. This is a performance workload, not a hardware timing oracle.
"""
import argparse
import hashlib
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / 'tests' / 'fixtures'
sys.path.insert(0, str(FIXTURES))
from build_scu import assembler
from build_clock import put16


def read_hex(path):
    data = bytearray(524288)
    for line in path.read_text().splitlines():
        if line.startswith('#') or not line.strip():
            continue
        address, *words = line.split()
        for index, word in enumerate(words):
            struct.pack_into('>H', data, int(address, 16) + index * 2, int(word, 16))
    return data


def build(poll=False, normal=False, sound_busy=False):
    data = read_hex(FIXTURES / 'scsp-effects.hex')
    if sound_busy:
        marker = bytes.fromhex('4e72270060fa')
        at = data.find(marker, 0x2000)
        assert at >= 0x2000 and data.find(marker, at + 1) < 0
        # Original sound image: continue with NOP/NOP/BRA instead of STOP.
        data[at:at + 4] = bytes.fromhex('4e714e71')
    data[:0x2000] = bytes(0x2000)
    data[:8] = struct.pack('>II', 0x200, 0x060FF000)
    if normal:
        source = (FIXTURES / 'scsp-effects.s').read_text().split('finished:', 1)[0]
        for offset, value in [
            (0x10, 0x4444), (0x12, 0x4444), (0x14, 0x5555), (0x16, 0x5555),
            (0xE, 0x300), (0x20, 0x303), (0x28, 0x3232), (0x3C, 0x10),
            (0x78, 1), (0x7A, 0), (0x7C, 1), (0x7E, 0),
            (0x88, 1), (0x8A, 0), (0x8C, 1), (0x8E, 0), (0xF8, 0x201),
        ]:
            source += put16(0x25F80000 + offset, value)
        for base in [0x25E00000, 0x25E20000]:
            for i, value in enumerate([0x801F, 0x83E0, 0xFC00, 0xFFFF] * 4):
                source += put16(base + i * 2, value)
        source += put16(0x25F80000, 0x8000)
        source += 'load 0xfffffe92,r1\nmov #1,r0\nmov.b r0,@r1\nnormal_park:\nbra normal_park\nnop\n'
    else:
        data[0x100:0x108] = read_hex(FIXTURES / 'rotation-fetch-bitmap.hex')[0x100:0x108]
        source = (FIXTURES / 'rotation-fetch-bitmap.s').read_text().rsplit('park:', 1)[0]
        source += (FIXTURES / 'scsp-effects.s').read_text().split('finished:', 1)[0]
        # Repeated 64x64 textured drawing while RBG0 and the effects DSP remain live.
        for address, value in [(0, 9), (20, 511), (22, 255), (32, 0x1000),
                               (34, 4), (36, 0xC0), (38, 0x100), (40, 0x2000), (42, 0x840)]:
            source += put16(0x25C00000 + address, value)
        source += put16(0x25D00002, 2) + put16(0x25D00004, 1)
        if poll:
            source += 'load 0x25d00010,r4\npoll_scene:\nmov.w @r4,r0\ntst #2,r0\nbt poll_scene\nbra poll_scene\nnop\n'
        else:
            source += 'mov #-7,r2\nmov #13,r3\ndiv0s r2,r3\ncompute_scene:\n'
            source += 'rotcl r3\ndiv1 r2,r3\n' * 32
            source += 'add #1,r0\nbra compute_scene\nnop\n'
    code = assembler.assemble(source, base=0x200)
    assert 0x200 + len(code) < 0x2000, 'SH-2 setup overlaps the authored sound image'
    data[0x200:0x200 + len(code)] = code
    lines = ['# SPDX-License-Identifier: MIT', '# Original CPU/rotation/VDP1/SCSP throughput guest.']
    for at in range(0, len(data), 16):
        if any(data[at:at + 16]):
            lines.append(f'{at:08X} ' + ' '.join(f'{word:04X}' for word in struct.unpack('>8H', data[at:at + 16])))
    if sound_busy:
        source = '; Sound image runs NOP/NOP/BRA continuously instead of STOP.\n' + source
    return source, '\n'.join(lines) + '\n', data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--normal', action='store_true', help='two normal bitmaps and effects sound')
    mode.add_argument('--poll', action='store_true', help='use draw-status polling instead of division')
    parser.add_argument('--sound-busy', action='store_true', help='keep the original sound CPU loop running')
    parser.add_argument('--binary', type=Path, help='also write the original guest as a raw ROM')
    args = parser.parse_args()
    source, hex_text, data = build(args.poll, args.normal, args.sound_busy)
    prefix = "normal-" if args.normal else "poll-" if args.poll else ""
    if args.sound_busy:
        prefix += 'sound-busy-'
    for suffix, text in [('s', source), ('hex', hex_text)]:
        path = Path(__file__).with_name(f'runtime-{prefix}scene.{suffix}')
        if args.check:
            assert path.read_text() == text, f'Rebuild {path.name}'
        else:
            path.write_text(text)
    if args.binary:
        args.binary.parent.mkdir(parents=True, exist_ok=True)
        args.binary.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} original runtime scene; SHA-256 {hashlib.sha256(data).hexdigest()}')


if __name__ == '__main__':
    main()
