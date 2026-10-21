#!/usr/bin/env python3
"""Original MIT refresh arbitration guest, SH7604 §§7.5.7/7.10.

Both SH-2s measure actual uncached reads using their own FRT. Only the master
drives refresh. The master also enters/exits self-refresh from warmed cached
instructions; no host callback performs the exit or computes guest results.
Timing assertions are inequalities against the same guest with refresh disabled,
not sampled golden intervals. Exact bus-state vectors live in refresh_tests.rs.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent


def calls():
    return (('mov #0,r11\nload 0x4000,r3\njsr @r3\nnop\n') * 2
            + record() + record(11))


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r13\nload 0x8000,r2\ntst r2,r13\nbf slave\n'
         'load 0x2600e400,r8\nload 0x2600dc00,r12\nbra common\nnop\n'
         'slave:\nload 0x2600e500,r8\nload 0x2600dd00,r12\ncommon:\n')
    s += store(0xFFFFFFE0, 0xA55A0004) + store(0xFFFFFFE8, 0xA55A0000)
    s += store(0xFFFFFFEC, 0xA55A0048)
    s += ('load 0x013579bd,r0\nmov.l r0,@r12\n'
          'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0x8000,r2\ntst r2,r13\nbt master_measure\nbra slave_measure\nnop\nmaster_measure:\n')
    s += calls()
    # Warm this exact routine before entering self-refresh. No memory operand
    # or instruction miss is needed between the keyed MCR entry and exit.
    s += ('load 0xffffffec,r10\nload 0xa55a0048,r9\nmov r9,r6\n'
          'load 0x4200,r3\nmov #0,r11\njsr @r3\nnop\n'
          'load 0xa55a004c,r6\nmov #0,r11\njsr @r3\nnop\n') + record(11)
    s += store(0xFFFFFFF8, 0xA55A0004) + store(0xFFFFFFF4, 0xA55A0000)
    s += store(0xFFFFFFF0, 0xA55A0008)  # unique timer-enable mutation
    s += calls()
    s += 'load 0xfffffff0,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + smpc(2) + 'bra done\nnop\n'
    s += 'slave_measure:\n' + calls()
    s += 'load 0xfffffff0,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'done:\nsleep\nbra done\nnop\n'
    kernel = ('mov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
              'mov.b @r4,r0\nmov.b @r5,r0\n'
              + 'mov.l @r12,r6\nadd r6,r11\n' * 32
              + 'mov.b @r4,r0\nmov.b @r5,r0\nextu.b r0,r0\nrts\nnop\n')
    self_refresh = ('mov.l r6,@r10\n' + 'add #1,r11\n' * 32
                    + 'mov.l r9,@r10\nrts\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for at, source in [(0x200, s), (0x4000, kernel), (0x4200, self_refresh)]:
        code = assembler.assemble(source, base=at)
        assert len(code) < (0x3E00 if at == 0x200 else 0x200)
        rom[at:at+len(code)] = code
    lines = ['# Original refresh/self-refresh BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return ('\n'.join(lines) + '\n').encode()


def monitor():
    # H §7.10: master repeatedly owns the external bus; slave's UBC
    # monitor competes with its own peripheral reads on its internal bus.
    s = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf monitor_slave\n'
         'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n')
    s += smpc(2)
    s += ('load 0x2600e608,r12\nmonitor_poll:\nmov.l @r12,r0\ntst r0,r0\nbt monitor_poll\n'
          'load 0x2600e700,r1\nload 0x600d,r0\nmov.l r0,@r1\nbra monitor_done\nnop\n'
          'monitor_slave:\nload 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
          'load 0xffffff78,r1\nload 0x3000,r0\nmov.w r0,@r1\nmov.w @r1,r0\n'
          'load 0xfffffe01,r10\nmov #90,r0\nmov.b r0,@r10\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n')
    s += ('mov #0,r11\nload 0x4000,r3\njsr @r3\nnop\n') * 2
    s += ('load 0x2600e600,r8\n' + record() + record(11)
          + 'load 0x600d,r0\n' + record()
          + 'monitor_done:\nsleep\nbra monitor_done\nnop\n')
    kernel = ('mov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
              'mov.b @r4,r0\nmov.b @r5,r0\n'
              + 'mov.b @r10,r6\nextu.b r6,r6\nadd r6,r11\n' * 32
              + 'mov.b @r4,r0\nmov.b @r5,r0\nextu.b r0,r0\nrts\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for at, source in [(0x200, s), (0x4000, kernel)]:
        code = assembler.assemble(source, base=at)
        assert len(code) < (0x3E00 if at == 0x200 else 0x200)
        rom[at:at+len(code)] = code
    lines = ['# Original external-UBC/internal-bus BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return ('\n'.join(lines) + '\n').encode()


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for name, data in [('refresh.hex', build()), ('monitor.hex', monitor())]:
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
