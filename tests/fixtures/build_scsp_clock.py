#!/usr/bin/env python3
"""Original MIT clocked SCSP/68k diagnostic; adjacent LICENSE.

ST-077 table2.2 (128 instructions/Fs), §3.2 (shared RAM), §5 fig5.1.
The 68k uploads a DSP program with early and late TABLE/NOFL reads of the
same sound-RAM word. It observes EFREG0 then changes that word between reads.
Only real guest instructions, MMIO and sound RAM carry synchronization/results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x1100)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    code = []

    def write(address, value):
        code.extend([0x33fc, value, address >> 16, address & 65535])

    write(0x100400, 0x20f)
    write(0x100700, 0x4000)
    write(0x100780, 0x800)
    write(0x100016, 0xe0)  # effect0 center/full
    write(0x100036, 0xe0)  # effect1 center/full
    for step, words in [
        (1, [0, 0, 0xa002, 0x8000]),
        (3, [0, 0x20, 2, 0]),
        (5, [0, 0xa000, 2, 0]),
        (6, [0, 0, 0x1002, 0]),
        (65, [0, 0, 0xa002, 0x8000]),
        (67, [0, 0x21, 2, 0]),
        (68, [0, 0xa040, 2, 0]),
        (69, [0, 0, 0x1102, 0]),
    ]:
        for part, word in enumerate(words):
            write(0x100800 + step * 8 + part * 2, word)
    write(0x100ec0, 0)
    write(0x100ec2, 0)
    write(0x1000, 0x1234)
    code.extend([0x3039, 0x10, 0xec0, 0x4a40, 0x67f6])  # poll early EF0
    code.extend([0x33c0, 0, 0x800])
    change = 0x100 + len(code) * 2
    write(0x1000, 0x2345)
    code.extend([0x3039, 0x10, 0xec2, 0x4a40, 0x67f6])  # poll late EF1
    code.extend([0x33c0, 0, 0x802, 0x23fc, 0, 0x600d, 0, 0x804,
                 0x4e72, 0x2700, 0x60fa])
    assert 0x100 + len(code) * 2 < 0x800
    struct.pack_into('>' + 'H' * len(code), sound, 0x100, *code)
    source = ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x440,r3\ncopy:\n'
              'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    source += smpc(6) + wait(0x05a00804, 'wait_dsp')
    source += ('load 0x25a00800,r1\nload 0x2600ea00,r2\nmov.l @r1,r0\nmov.l r0,@r2\n'
               'add #4,r1\nadd #4,r2\nmov.l @r1,r0\nmov.l r0,@r2\n'
               'finished:\nsleep\nbra finished\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    host = assembler.assemble(source, base=0x200)
    assert len(host) < 0x1e00
    rom[0x200:0x200 + len(host)] = host
    rom[0x2000:0x2000 + len(sound)] = sound
    lines = ['# Original clocked 68k/SCSP DSP BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
    # Word-index provenance for the independent operand/control mutations.
    metadata = f'{0x2000 + change + 2:08X}\n'
    return {'scsp-clock.hex': '\n'.join(lines) + '\n',
            'scsp-clock.s': '; Original MIT: adjacent LICENSE.\n' + source,
            'scsp-clock.tsv': metadata}, hashlib.sha256(rom).hexdigest()


def build_effects():
    """ST-228 p.31: repeated EWT stores replace, including negative and zero.

    The 68k uploads the DSP instructions and input operands, then reads its
    results after a DSP-computed marker. The DSP, rather than host code or
    the 68k, computes and publishes the effect samples routed to stereo.
    """
    sound = bytearray(0x1100)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    code = []

    def write(address, value):
        operand = 0x2000 + 0x100 + len(code) * 2 + 2
        code.extend([0x33fc, value, address >> 16, address & 65535])
        return operand

    write(0x100400, 0x20f)
    write(0x100700, 0x4000)  # signed coefficient +.5
    positive = write(0x100e02, 0x4000)
    negative = write(0x100e06, 0xe000)
    write(0x100e0a, 0x600d)  # marker operand; half = 0x3006
    write(0x100016, 0xff)  # effect0 -> left only
    write(0x100036, 0xef)  # effect1 -> right only
    for step, words in [
        (8, [0, 0xa000, 2, 0]),       # ACC = P/2
        (9, [0, 0xa040, 0x1002, 0]),  # EF0 = P/2; ACC = N/2
        (10, [0, 0xa040, 0x1002, 0]),  # EF0 = N/2
        (11, [0, 0xa000, 0x1102, 0]),  # EF1 = N/2; ACC = P/2
        (12, [0, 0xa000, 0x1102, 0]),  # EF1 = P/2
        (13, [0, 0, 0x1202, 0]),       # EF2 = P/2; ACC = 0
        (14, [0, 0xa080, 0x1202, 0]),  # EF2 = 0; ACC = marker/2
        (15, [0, 0, 0x1302, 0]),       # EF3 = marker/2
    ]:
        for part, value in enumerate(words):
            operand = write(0x100800 + step * 8 + part * 2, value)
            if step == 10 and part == 2:
                final_store = operand
    code.extend([0x3039, 0x10, 0xec6, 0x4a40, 0x67f6])
    for port, target in [(0xec0, 0x800), (0xec2, 0x802), (0xec4, 0x804)]:
        code.extend([0x3039, 0x10, port, 0x33c0, 0, target])
    code.extend([0x23fc, 0, 0x600d, 0, 0x808, 0x4e72, 0x2700, 0x60fa])
    assert 0x100 + len(code) * 2 < 0x800
    struct.pack_into('>' + 'H' * len(code), sound, 0x100, *code)
    source = ('load 0x2000,r1\nload 0x25a00000,r2\nload 0x440,r3\ncopy:\n'
              'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    source += smpc(6) + wait(0x05a00808, 'wait_effects')
    source += ('load 0x25a00800,r1\nload 0x2600eb00,r2\nmov #3,r3\nrecords:\n'
               'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf records\n'
               'finished:\nsleep\nbra finished\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    host = assembler.assemble(source, base=0x200)
    assert len(host) < 0x1e00
    rom[0x200:0x200 + len(host)] = host
    rom[0x2000:0x2000 + len(sound)] = sound
    lines = ['# Original repeated SCSP effect-store guest. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
    metadata = f'{positive:08X}\t{negative:08X}\t{final_store:08X}\n'
    return {'scsp-effects.hex': '\n'.join(lines) + '\n',
            'scsp-effects.s': '; Original MIT: adjacent LICENSE.\n' + source,
            'scsp-effects.tsv': metadata}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    effects, effects_digest = build_effects()
    files.update(effects)
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} SCSP instruction clocks; expanded ROM SHA-256 {digest}')
    print(f'SCSP effect stores; expanded ROM SHA-256 {effects_digest}')
