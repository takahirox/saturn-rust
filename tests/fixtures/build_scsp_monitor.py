#!/usr/bin/env python3
"""Original MIT SH-2/MC68EC000 SCSP slot-monitor diagnostic; LICENSE.

ST-077 §4.1 fig.4.3 and §4.2 p.89: MSLC[15:11], CA[10:7],
4K samples per CA unit. The sound CPU programs all 32 actual PCM voices,
waits on the sample interrupt flag, and records word/byte reads. No firmware,
SDK objects, emulator core, host callbacks or sampled golden results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_sound import assembler

HERE = Path(__file__).resolve().parent


def build():
    image = bytearray(0x4400)
    struct.pack_into('>II', image, 0, 0x7ff00, 0x100)
    words = []

    def emit(*values):
        words.extend(values)

    def movw(value, address):
        emit(0x33fc, value & 65535, address >> 16, address & 65535)

    movw(0x20f, 0x100400)  # MEM4MB + MVOL
    movw(0x1000, 0x10000)  # attack waveform sample, ordinary shared sound RAM
    for block in range(1, 16):
        movw(0x1000 + block * 256, 0x10000 + block * 8192)
    for slot in range(32):
        block = slot % 15 + 1
        base = 0x100000 + slot * 32
        # SA=10000, short forward loop at block*4096. OCT7 advances
        # 128 samples per output. Even the last loop settles in 481 samples.
        for offset, value in [(2, 0), (4, block * 4096), (6, block * 4096 + 1),
                              (12, 0x100), (16, 0x3800),
                              (22, 0xff00 if slot == 0 else 0xef00 if slot == 31 else 0),
                              (0, 0x0821)]:
            movw(value, base + offset)
    movw(0x1821, 0x100000)  # shared KYONEX, all 32 KEY_ON bits are set
    movw(0x400, 0x100422)  # remove the stale sample flag before waiting
    emit(0x3e3c, 599)  # MOVE.W #599,D7: 600 actual sample edges
    wait = 0x100 + len(words) * 2
    emit(0x3039, 0x10, 0x420, 0x0240, 0x400)  # MOVE.W SCIPD,D0; ANDI.W
    branch = 0x100 + len(words) * 2
    emit(0x6700 | ((wait - branch - 2) & 255))  # BEQ.S wait
    movw(0x400, 0x100422)
    branch = 0x100 + len(words) * 2
    emit(0x51cf, (wait - branch - 2) & 65535)  # DBRA D7,wait
    emit(0x4df9, 0, 0x5000)  # LEA $5000,A6
    for slot in range(32):
        movw(slot << 11, 0x100408)
        emit(0x3cf9, 0x10, 0x408)  # MOVE.W monitor,(A6)+
        emit(0x1cf9, 0x10, 0x408)  # MOVE.B high,(A6)+
        emit(0x1cf9, 0x10, 0x409)  # MOVE.B low,(A6)+
        emit(0x13fc, 0, 0x10, 0x409)  # low-byte write preserves MSLC latch
        emit(0x3cf9, 0x10, 0x408, 0x3cfc, 0)
    # The uploading SH-2 sees only results computed/stored by this 68k.
    emit(0x23fc, 0, 1, 0, 0x804, 0x23fc, 0, 0x600d, 0, 0x800)
    emit(0x4e72, 0x2700, 0x60fa)
    assert 0x100 + len(words) * 2 < 0x4400
    struct.pack_into('>' + 'H' * len(words), image, 0x100, *words)
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x06100000)
    host = assembler.assemble((HERE / 'sound.s').read_text(), base=0x200)
    assert len(host) < 0x1e00
    rom[0x200:0x200 + len(host)] = host
    rom[0x2000:0x2000 + len(image)] = image
    text = ['# Original SH-2/MC68EC000 SCSP monitor BIOS. MIT: adjacent LICENSE.',
            '# Reproduce with build_scsp_monitor.py. Expected words derive from fig.4.3.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            text.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    vectors = []
    for slot in range(32):
        ca = (slot % 15 + 1) << 7
        vectors += [(ca << 16) | ca, ca << 16]
    return {'scsp-monitor.hex': '\n'.join(text) + '\n',
            'scsp-monitor.tsv': ''.join(f'{v:08X}\n' for v in vectors)}, hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    for name, content in files.items():
        path = HERE / name
        if args.check:
            assert path.read_text() == content, f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} SCSP monitor fixture; expanded ROM SHA-256 {digest}')
