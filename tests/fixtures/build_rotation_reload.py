#!/usr/bin/env python3
"""Original MIT one-shot A/B origin and coefficient diagnostic. Adjacent LICENSE.

ST-058-R2 §6.3 pp.157–159; §6.4. The SH-2 constructs its gradient bitmap,
coefficient tables and parameter tables, polls beam counters, and writes MMIO.
RPRCTL is write-only: records read table operands and compute sums, never RPRCTL.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_clock import put16, read
from build_sprite_rotation import blank, line
from build_hle_system import record

HERE = Path(__file__).resolve().parent


def build():
    outputs = {}
    for parameter in [0, 1]:
        table = 0x25e60000 + parameter * 0x80
        s = '; Original MIT one-shot rotation diagnostic.\nload 0x2600dd00,r8\n'
        # RGB555: red=x%32, green=y%32, blue=x//32. Constructed by guest
        # arithmetic and stores, independently of the framebuffer oracle.
        s += 'load 0x25e00000,r1\nload 0x3e0,r6\n'
        for y in range(64):
            s += (f'load {0x8000 + (y % 32) * 32},r0\nmov #16,r7\nrow_{y}:\n'
                  f'mov #32,r2\nfill_{y}:\nmov.w r0,@r1\nadd #2,r1\n'
                  f'add #1,r0\ndt r2\nbf fill_{y}\nadd r6,r0\ndt r7\nbf row_{y}\n')
        # Short mode3 coefficients replace Xp: index%4 pixels of translation.
        s += 'load 0x25e40000,r1\nload 128,r2\ncoefficients:\n'
        for value in [0, 4, 8, 12]:
            s += f'mov #{value},r0\nmov.w r0,@r1\nadd #2,r1\n'
        s += 'dt r2\nbf coefficients\n'
        for off, value in [(0, 10 << 16), (4, 0), (0x14, 1 << 16),
                           (0x1c, 1 << 16), (0x2c, 1 << 16),
                           (0x4c, 1 << 16), (0x50, 1 << 16),
                           (0x54, 2 << 16), (0x58, 1 << 16)]:
            s += store(table + off, value)
        # ROM100 is a real guest delta operand, mutated in native and Wasm.
        s += f'load 0x100,r1\nmov.l @r1,r0\nload {table + 0xc},r1\nmov.l r0,@r1\n'
        for off, value in [(0xe, 0x303), (0x20, 0x10), (0x2a, 0x3200),
                           (0xfc, 4), (0xbc, 3), (0xbe, 0), (0xb0, parameter),
                           (0xb4, 15 << (parameter * 8)),
                           (0xb6, 2 << (parameter * 8)), (0, 0x8000)]:
            s += put16(0x25f80000 + off, value)
        s += blank('setup_blank', True) + blank('start_field', False)
        # No request: table mutation cannot move the accumulated starts.
        s += line(64)
        for off, value in [(0, 300), (4, 4), (0x54, 20)]:
            s += store(table + off, value << 16)
        # One request; mutate the table again after consumption, no host clear.
        s += line(127)
        for off, value in [(0, 100), (4, 4), (0x54, 8)]:
            s += store(table + off, value << 16)
        s += put16(0x25f800b2, 7 << (parameter * 8)) + line(128)
        for off, value in [(0, 200), (4, 8), (0x54, 12)]:
            s += store(table + off, value << 16)
        # Current deltas change independently without reloading origins.
        s += line(159) + store(table + 0xc, 3 << 16) + store(table + 0x58, 2 << 16)
        s += line(191) + store(table + 0x10, 1 << 16)
        # Reissue on consecutive lines: each new request reloads once.
        for n, (x, y, k) in enumerate([(20, 8, 16), (30, 10, 20), (40, 12, 24)]):
            if n:
                s += line(191 + n)
            for off, value in [(0, x), (4, y), (0x54, k)]:
                s += store(table + off, value << 16)
            s += put16(0x25f800b2, 7 << (parameter * 8))
        s += blank('complete', True)
        for off in [0, 4, 0x54, 0xc, 0x10, 0x58]:
            s += read(table + off)
        for off, delta in [(0, 0xc), (4, 0x10), (0x54, 0x58)]:
            s += (f'load {table + off},r1\nmov.l @r1,r0\n'
                  f'load {table + delta},r1\nmov.l @r1,r2\nadd r2,r0\n' + record())
        s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
        code = assembler.assemble(s, base=0x200)
        assert len(code) < 0x10000
        rom = bytearray(524288)
        struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
        struct.pack_into('>I', rom, 0x100, 1 << 16)
        rom[0x200:0x200 + len(code)] = code
        lines = ['# Original MIT SH-2 rotation reload BIOS. Adjacent LICENSE.']
        for at in range(0, len(rom), 16):
            if any(rom[at:at + 16]):
                lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
        stem = f'rotation-reload-{parameter}'
        outputs[stem + '.s'] = s.encode()
        outputs[stem + '.hex'] = ('\n'.join(lines) + '\n').encode()
        # Literal operand/readback and SH-2 addition records, not emulator output.
        expected = [40 << 16, 12 << 16, 24 << 16, 3 << 16, 1 << 16, 2 << 16,
                    43 << 16, 13 << 16, 26 << 16, 0x600d]
        outputs[stem + '.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    return outputs


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
