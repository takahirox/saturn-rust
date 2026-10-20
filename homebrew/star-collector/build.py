#!/usr/bin/env python3
"""Original, bounded SH-2 assembler and ISO9660 writer. MIT: see LICENSE."""
import argparse
from pathlib import Path
import re
import struct

HERE = Path(__file__).resolve().parent
OUTPUT = HERE.parent.parent / 'web' / 'star-collector.iso'
BASE = 0x06004000


def assemble(source):
    labels, entries = {}, []
    pc = BASE
    for raw in source.splitlines():
        line = raw.split(';')[0].strip().lower()
        if not line:
            continue
        if line.endswith(':'):
            assert line[:-1] not in labels, line
            labels[line[:-1]] = pc
            continue
        op, *tail = line.split(maxsplit=1)
        args = [a.strip() for a in tail[0].split(',')] if tail else []
        size = 6 if op in ('bt.l', 'bf.l') else 2
        # load: MOV.L, BRA, NOP, optional align NOP, literal.
        if op == 'load':
            size = ((pc + 9) & ~3) + 4 - pc
        entries.append((pc, op, args, size, raw))
        pc += size
    result = bytearray()

    def reg(s):
        assert re.fullmatch(r'r(?:[0-9]|1[0-5])', s), s
        return int(s[1:])

    def num(s):
        return labels[s] if s in labels else int(s.removeprefix('#'), 0)

    def branch(at, target, bits):
        distance = num(target) - (at + 4)
        assert distance % 2 == 0 and -(1 << bits) <= distance < (1 << bits), (hex(at), target, distance)
        return (distance // 2) & ((1 << bits) - 1)

    def word(w):
        result.extend(struct.pack('>H', w))

    for pc, op, a, size, raw in entries:
        try:
            assert len(result) == pc - BASE
            if op == 'load':
                literal = (pc + 9) & ~3
                word(0xD000 | reg(a[1]) << 8 | (literal - ((pc + 4) & ~3)) // 4)
                word(0xA000 | ((pc + size - (pc + 6)) // 2))
                word(0x0009)
                while (BASE + len(result)) % 4:
                    word(0x0009)
                result.extend(struct.pack('>I', num(a[0])))
            elif op in ('bt.l', 'bf.l'):
                word((0x8B00 if op == 'bt.l' else 0x8900) | 1)
                word(0xA000 | branch(pc + 2, a[0], 12))
                word(0x0009)
            elif op == 'nop': word(0x0009)
            elif op == 'rts': word(0x000B)
            elif op == 'jmp': word(0x402B | reg(a[0][1:]) << 8)
            elif op in ('bra', 'bsr', 'bt', 'bf'):
                word({'bra': 0xA000, 'bsr': 0xB000, 'bt': 0x8900, 'bf': 0x8B00}[op] | branch(pc, a[0], 12 if op in ('bra', 'bsr') else 8))
            elif op in ('shll', 'shll2', 'shll8', 'dt'):
                word({'shll': 0x4000, 'shll2': 0x4008, 'shll8': 0x4018, 'dt': 0x4010}[op] | reg(a[0]) << 8)
            elif op == 'tst':
                assert a[1] == 'r0' and 0 <= num(a[0]) <= 255
                word(0xC800 | num(a[0]))
            elif op in ('mov', 'add') and a[0].startswith('#'):
                assert -128 <= num(a[0]) <= 127
                word((0xE000 if op == 'mov' else 0x7000) | reg(a[1]) << 8 | num(a[0]) & 255)
            elif op in ('mov', 'add', 'sub', 'cmp/eq', 'cmp/hs'):
                word({'mov': 0x6003, 'add': 0x300C, 'sub': 0x3008, 'cmp/eq': 0x3000, 'cmp/hs': 0x3002}[op] | reg(a[1]) << 8 | reg(a[0]) << 4)
            elif op in ('mov.b', 'mov.w', 'mov.l'):
                width = {'mov.b': 0, 'mov.w': 1, 'mov.l': 2}[op]
                if a[0].startswith('@'):
                    word(0x6000 | width | reg(a[1]) << 8 | reg(a[0][1:]) << 4)
                else:
                    word(0x2000 | width | reg(a[1][1:]) << 8 | reg(a[0]) << 4)
            else:
                raise ValueError('Unsupported assembly')
        except Exception as error:
            raise ValueError(f'{raw}: {error}') from error
    return bytes(result)


def both16(n): return struct.pack('<H', n) + struct.pack('>H', n)
def both32(n): return struct.pack('<I', n) + struct.pack('>I', n)


def record(name, sector, size, directory=False):
    length = 33 + len(name) + (not len(name) % 2)
    return (bytes([length, 0]) + both32(sector) + both32(size)
            + bytes([126, 1, 1, 0, 0, 0, 0, 2 if directory else 0, 0, 0])
            + both16(1) + bytes([len(name)]) + name + bytes(length - 33 - len(name)))


def build():
    game = assemble((HERE / 'game.s').read_text())
    license_text = (HERE / 'LICENSE').read_bytes()
    assert 0 < len(game) < 0xB000
    iso = bytearray(64 * 2048)
    def put(offset, data): iso[offset:offset + len(data)] = data
    def text(offset, size, value): put(offset, value.encode('ascii').ljust(size, b' '))
    text(0, 16, 'SEGA SEGASATURN')
    text(0x10, 16, 'ORIGINAL HOMEBREW')
    text(0x20, 16, 'STAR-001 V1.000')
    text(0x30, 16, '20260101CD-1/1')
    text(0x40, 16, 'U')
    text(0x50, 16, 'J')
    text(0x60, 112, 'STAR COLLECTOR')
    put(0xE0, struct.pack('>IIIIII', 0x1000, 0, 0x060FF000, 0, BASE, len(game)))
    # Original AIP: MOV.L @(1,PC),R0; JMP @R0; NOP; align; destination.
    put(0xE20, struct.pack('>HHHHI', 0xD001, 0x402B, 0x0009, 0x0009, BASE))
    pvd = 16 * 2048
    put(pvd, b'\x01CD001\x01\x00')
    text(pvd + 8, 32, 'SATURN RUST')
    text(pvd + 40, 32, 'STAR COLLECTOR')
    put(pvd + 80, both32(64))
    put(pvd + 120, both16(1) + both16(1) + both16(2048) + both32(10))
    put(pvd + 140, struct.pack('<I', 18))
    put(pvd + 148, struct.pack('>I', 19))
    put(pvd + 156, record(b'\x00', 20, 2048, True))
    # Required identifiers/date fields; fixed timestamps make builds reproducible.
    for offset, size in [(190, 128), (318, 128), (446, 128), (574, 128), (702, 37), (739, 37), (776, 37)]:
        text(pvd + offset, size, '')
    for offset in [813, 830, 847, 864]: put(pvd + offset, b'2026010100000000\x00')
    iso[pvd + 881] = 1
    put(17 * 2048, b'\xffCD001\x01')
    put(18 * 2048, b'\x01\x00' + struct.pack('<IH', 20, 1) + b'\x00\x00')
    put(19 * 2048, b'\x01\x00' + struct.pack('>IH', 20, 1) + b'\x00\x00')
    license_sector = 21 + (len(game) + 2047) // 2048
    put(20 * 2048, record(b'\x00', 20, 2048, True) + record(b'\x01', 20, 2048, True)
        + record(b'GAME.BIN;1', 21, len(game)) + record(b'LICENSE.TXT;1', license_sector, len(license_text)))
    put(21 * 2048, game)
    put(license_sector * 2048, license_text)
    assert len(iso) == 64 * 2048
    return bytes(iso)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true', help='fail if packaged image differs')
    args = parser.parse_args()
    image = build()
    if args.check:
        assert OUTPUT.read_bytes() == image, 'Rebuild Star Collector ISO'
    else:
        OUTPUT.write_bytes(image)
    print(f'{"Verified" if args.check else "Built"} {OUTPUT.name}: {len(image)} bytes')
