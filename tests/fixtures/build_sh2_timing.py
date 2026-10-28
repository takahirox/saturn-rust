#!/usr/bin/env python3
"""Original MIT SH-2 execution-state diagnostic, Programming Manual table7.2.

Both real CPUs warm each kernel, reset their own FRT divider using module stop,
then measure cached instruction sequences with phi/8 FRC. Multiplier-register
transfers include alignment-dependent IF/MA splits (P §7.4).
Expectations are authored from states, not collected from an emulator.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc

HERE = Path(__file__).resolve().parent
KERNELS = [
    # name, body, states, computed result
    ('add', 'add #1,r6\n' * 32, 32, 32),
    ('ldc', 'ldc.l @r12+,gbr\n' * 32, 96, 0),
    ('taken', ''.join(f'clrt\nbf taken{i}\nadd #99,r6\ntaken{i}:\nadd #1,r6\n' for i in range(8)), 40, 8),
    ('untaken', ''.join(f'sett\nbf untaken{i}\nadd #1,r6\nuntaken{i}:\n' for i in range(8)), 24, 8),
    ('delayed', ''.join(f'sett\nbt/s delayed{i}\nadd #1,r6\ndelayed{i}:\n' for i in range(8)), 32, 8),
    ('stc', 'stc.l gbr,@-r12\n' * 8, 16, 0),
    # §7.5 WB/EX interlock, independent successor, repeated-load bypass,
    # and one intervening instruction. Expectations count explicit slots.
    ('load_use', 'mov.l @r12,r6\nadd #1,r6\n' * 16, 48, 1),
    ('load_independent', 'mov.l @r12,r6\nadd #1,r9\n' * 16, 32, 0),
    ('load_repeat', 'mov.l @r12,r6\n' * 32, 48, 0),
    ('load_separated', 'mov.l @r12,r6\nnop\nadd #1,r6\n' * 8, 28, 1),
    # H §7.11.2: determine-cycle state plus internal peripheral read.
    ('peripheral_read', 'mov.b @r10,r6\n' * 32, 80, 0xffffffff),
    # P fig7.12: sixteen word-offset MAs add sixteen IF splits.
    ('peripheral_write', 'mov.b r6,@r10\n' * 32, 48, 0),
    # §7.7.6 figs7.86/7.90: system transfers with WB use the same
    # one-successor interlock. PR is an implicit source of STS PR.
    ('sts_macl', 'sts macl,r6\nadd #1,r6\n' * 16, 48, 1),
    ('sts_mach', 'sts mach,r6\nadd #1,r6\n' * 16, 48, 1),
    # P §§7.4.2–3: four STS at 4n+2 add four IF/MA split clocks.
    ('sts_separated', 'sts macl,r6\nnop\nadd #1,r6\n' * 8, 28, 1),
    ('pr_use', 'lds.l @r12+,pr\nsts pr,r6\n' * 16, 48, 0),
    ('pr_separated', 'lds.l @r12+,pr\nnop\nsts pr,r6\n' * 8, 24, 0),
]


def kernel(body, preserve_pr=False):
    # The caller preloads FRC high/low into R4/R5 and SBYCR into R7.
    # The resume write completes while the FRT is stopped, starting phase zero.
    # EX/MA lead-in makes high latch(3), low read(2), body(N),
    # final latch(3): P fig7.11 and H §7.11.2 give floor((8+N)/8)
    # at the final high-byte read. The STS-separated
    # case includes four alignment splits and is not a multiple of eight.
    # With preserve_pr the resume store is aligned, so the first timer load
    # shares its MA clock: the wrapper is seven clocks, rather than eight.
    # See docs/sh2-store-pipeline.md for the independent stage enumeration.
    save = 'sts pr,r14\n' if preserve_pr else ''
    restore = 'lds r14,pr\n' if preserve_pr else ''
    return (save + 'mov #2,r0\nmov.b r0,@r7\nmov #0,r0\nmov.b r0,@r7\n'
            'mov.b @r4,r0\nmov.b @r5,r0\n' + body +
            'mov.b @r4,r0\nmov.b @r5,r0\nextu.b r0,r0\n' + restore + 'rts\nnop\n')


def kernel_address(index):
    # Preserve the original zero words at 7000..707C used by LDC/LDS.L.
    return 0x4000 + index * 0x400 if index < 12 else 0x10000 + (index - 12) * 0x400


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
         'load 0x2600e600,r8\n') + smpc(2) + 'bra common\nnop\n'
    s += 'slave:\nload 0x060fe000,r15\nload 0x2600e700,r8\ncommon:\n'
    s += store(0xFFFFFFE8, 0xA55A0000)
    # Two-way cache reserves ways0/1 for the STC.L kernel's data array.
    s += ('load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n'
          'load 0xfffffe01,r10\n')
    for i, (name, _, _, _) in enumerate(KERNELS):
        address = kernel_address(i)
        # Two identical calls; only the second warmed-cache result is recorded.
        for _ in range(2):
            s += 'clrmac\nmov #-1,r0\nmov.b r0,@r10\n'
            s += (f'load {address:#x},r3\nmov #0,r6\n'
                  f'load {0xC0000400 if name == "stc" else 0x7000:#x},r12\n'
                  'jsr @r3\nnop\n')
        s += record() + record(6)
    s += 'load 0x600d,r0\n' + record() + 'done:\nsleep\nbra done\nnop\n'
    sections = {0x200: s}
    for i, (name, body, _, _) in enumerate(KERNELS):
        sections[kernel_address(i)] = kernel(body, preserve_pr=name.startswith('pr_'))
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    for address, source in sections.items():
        code = assembler.assemble(source, base=address)
        assert len(code) < (0x3E00 if address == 0x200 else 0x400)
        rom[address:address+len(code)] = code
    # LDC.L consumes original zero words at 7000..707C. STC.L uses cache RAM.
    lines = ['# Original SH-2 execution-state BIOS; MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    expected = [v for name, _, clocks, result in KERNELS
                for v in [(clocks + (7 if name.startswith("pr_") else 8)) // 8, result]] + [0x600D]
    return {'sh2-timing.hex': ('\n'.join(lines)+'\n').encode(),
            'sh2-timing.tsv': ''.join(f'{v:08X}\n' for v in expected).encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for name, content in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
