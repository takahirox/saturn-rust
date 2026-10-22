#!/usr/bin/env python3
"""Original MIT separately clocked PCM read diagnostic; adjacent LICENSE.

ST-077 §3.2 pp.20–21/fig3.2 and §2.2: two DRAM reads followed by first-order
interpolation. A real 68k toggles the same waveform word, while a one-sample
half-rate loop reads that word twice. An intervening write yields the arithmetic
midpoint; an atomic source snapshot can only produce the two endpoints.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import smpc, wait, record

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x1100)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    words = []

    def write(address, value):
        words.extend([0x33fc, value, address >> 16, address & 65535])

    for address, value in [(0x100400, 0x20f), (0x1000, 0x2000),
                           (0x100002, 0x1000), (0x100004, 0), (0x100006, 1),
                           (0x10000c, 0x100), (0x100010, 0x7800),
                           (0x100016, 0xe000)]:
        write(address, value)
    keyon = 0x100 + len(words) * 2
    write(0x100000, 0x1820)
    words.extend([0x303c, 1023])  # MOVE.W #1023,D0
    loop = 0x100 + len(words) * 2
    write(0x1000, 0x2000)
    write(0x1000, 0x6000)
    dbra = 0x100 + len(words) * 2
    words.extend([0x51c8, (loop - (dbra + 2)) & 65535])
    words.extend([0x23fc, 0, 0x600d, 0, 0x804, 0x4e72, 0x2700, 0x60fa])
    struct.pack_into('>' + 'H' * len(words), sound, 0x100, *words)
    host = ('load 0x4000,r1\nload 0x25a00000,r2\nload 0x440,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    host += smpc(6) + wait(0x05a00804, 'wait_sound')
    host += 'load 0x2600ed00,r8\nload 0x25a00804,r1\nmov.l @r1,r0\n' + record()
    host += 'finished:\nsleep\nbra finished\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060ff000)
    code = assembler.assemble(host, base=0x200)
    assert len(code) < 0x3e00
    rom[0x200:0x200 + len(code)] = code
    rom[0x4000:0x4000 + len(sound)] = sound
    lines = ['# Original separately clocked SCSP PCM BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at + 16])))
    return {'scsp-pcm.hex': '\n'.join(lines) + '\n',
            'scsp-pcm.s': '; Original MIT: adjacent LICENSE.\n' + host,
            'scsp-pcm.tsv': f'{0x4000+keyon+2:08X}\n'}, hashlib.sha256(rom).hexdigest()


def build_arbitration():
    def word(address, value):
        return f'load {address:#x},r1\nload {value:#x},r0\nmov.w r0,@r1\n'

    host = word(0x25b00400, 0x20f)
    host += ('load 0x25a01000,r1\nload 0x1230,r0\nmov #64,r2\nfill:\n'
             'mov.w r0,@r1\nadd #2,r1\nadd #8,r0\ndt r2\nbf fill\n')
    host += 'load 0x25b00000,r1\nmov #32,r2\nslots:\n'
    for value in [0x820,0x1000,0,1,0,0,0,0,0,0,0,0]:
        host += f'load {value:#x},r0\nmov.w r0,@r1\nadd #2,r1\n'
    host += 'add #8,r1\ndt r2\nbf slots\n'
    host += word(0x25b003e0,0x1820)  # KEYONEX for all32; AR0, EG3FF
    host += 'load 0x2600ee00,r8\n'
    for mode in range(3):
        if mode == 1:
            host += ('load 0x25b0000c,r1\nload 0x100,r0\nmov #32,r2\ndirect:\n'
                     'mov.w r0,@r1\nadd #32,r1\ndt r2\nbf direct\n')
        if mode == 2:
            host += ('load 0x25b00000,r1\nload 0x8a0,r0\nmov #32,r2\nnoise:\n'
                     'mov.w r0,@r1\nadd #32,r1\ndt r2\nbf noise\n')
        for repeat in range(2):
            host += word(0x25b00412,0x1000)+word(0x25b00414,0x700)
            host += ('load 0xfffffe12,r4\nload 0xfffffe13,r5\n'
                     'load 0x25b00416,r6\nload 0x1080,r7\nload 0x1000,r2\n'
                     'mov #0,r0\nmov.b r0,@r4\nmov.b r0,@r5\nmov.w r7,@r6\n')
            host += f'poll{mode}_{repeat}:\nmov.w @r6,r0\ntst r2,r0\nbf poll{mode}_{repeat}\n'
            host += ('mov.b @r4,r0\nextu.b r0,r0\nshll8 r0\nmov r0,r9\n'
                     'mov.b @r5,r0\nextu.b r0,r0\nor r9,r0\n') + record()
    for address in [0x25b00700,0x25b0077e]:
        host += f'load {address:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\n'+record()
    host += 'load 0x600d,r0\n'+record()+'finished:\nsleep\nbra finished\nnop\n'
    code = assembler.assemble(host,base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x060ff000)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original PCM/EG/noise versus internal DMA guest. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'scsp-pcm-arbitration.hex':'\n'.join(lines)+'\n',
            'scsp-pcm-arbitration.s':'; Original MIT: adjacent LICENSE.\n'+host},hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    arbitration, arbitration_digest = build_arbitration()
    files.update(arbitration)
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} SCSP separate PCM reads; expanded ROM SHA-256 {digest}')

    print(f'PCM/EG/noise DMA arbitration; expanded ROM SHA-256 {arbitration_digest}')
