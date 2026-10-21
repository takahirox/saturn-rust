#!/usr/bin/env python3
"""Build original SH-2 + MC68EC000 sound fixture. MIT: adjacent LICENSE.

Only literal architectural instruction encodings are emitted; no SDK/core/assets.
The sound CPU computes the PCM waveform, programs its own SCSP, takes an
SCSP timer interrupt, returns with RTE, and stops until another interrupt.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct
HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('assembler', HERE.parents[1] / 'homebrew/star-collector/build.py')
assembler = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assembler)


def sound_image(effects=False, synthesis=False):
    image = bytearray(0x4400)
    struct.pack_into('>II', image, 0, 0x7FF00, 0x100)
    struct.pack_into('>I', image, 28 * 4, 0x300)  # level-four autovector
    words = []
    def emit(*values): words.extend(values)
    def movw(value, address): emit(0x33FC, value & 65535, address >> 16, address & 65535)
    # LEA $4000,A0; MOVEQ #16,D0. Create bipolar samples via 68k arithmetic.
    emit(0x41F9, 0, 0x4000, 0x7010, 0xE148, 0x30C0, 0x4440, 0x30C0,
         0xE348, 0x30C0, 0x4440, 0x30C0)
    movw(0x20F, 0x100400)  # 4Mbit sound RAM, full master volume
    if synthesis:
        emit(0x4EB9, 0, 0xC00)  # JSR original slot-synthesis program
    else:
        for offset, value in [(2, 0x4000), (4, 0), (6, 4), (12, 0x100), (16, 0), (22, 0xE800), (0, 0x1820)]:
            movw(value, 0x100000 + offset)  # PCM16, loop, direct mode, pan 8
    if effects:
        emit(0x4EB9, 0, 0x900)  # JSR original effects setup (Hardware Lab only).
    movw(0x40, 0x10041E)  # timer A enabled for sound CPU
    movw(0x40, 0x100428)  # timer A level-four interrupt
    movw(0xFE, 0x100418)  # expires after one sample tick
    # MOVE.L #$600D,$800; MOVE #$2000,SR; STOP #$2000; BRA.S back to STOP.
    emit(0x23FC, 0, 0x600D, 0, 0x800, 0x46FC, 0x2000, 0x4E72, 0x2000, 0x60FA)
    struct.pack_into('>'+'H'*len(words), image, 0x100, *words)
    # ADDQ.L #1,$804; clear timer A pending; RTE.
    handler = [0x52B9, 0, 0x804, 0x33FC, 0x40, 0x10, 0x422, 0x4E73]
    struct.pack_into('>'+'H'*len(handler), image, 0x300, *handler)
    if effects:
        words = []
        # ST-077 fig.5.1 / ST-228 Reference 2: independently authored 128-step
        # program, four active multiply/output pairs. .5*MIXS0 -> EFREG0;
        # .5*MEMS0 -> EFREG1; .5*EXTS0 -> EFREG2; .25*EXTS1 -> EFREG3.
        movw(0x4000, 0x100700)  # signed Q12 .5, left aligned by three bits
        movw(0x2000, 0x100702)  # .25
        movw(0, 0x100E00)
        movw(0x1200, 0x100E02)  # MEMS0=0x120000 -> EFREG1=0x0900
        program = [
            [0, 0xA800, 0x0002, 0], [0, 0, 0x1002, 0],
            [0, 0xA000, 0x0002, 0], [0, 0, 0x1102, 0],
            [0, 0xAC00, 0x0002, 0], [0, 0, 0x1202, 0],
            [0, 0xAC40, 0x0002, 0x0200], [0, 0, 0x1302, 0],
        ]
        for i, step in enumerate(program):
            for j, word in enumerate(step):
                movw(word, 0x100800 + i*8 + j*2)
        # Direct voice keeps its original pan; wet voice left only. The known
        # constant on EFREG1 is diagnostic-only, never an audio source.
        movw(7, 0x100014)  # ISEL0, full IMXL
        movw(0xE8FF, 0x100016)  # direct PCM plus EFREG0 left
        movw(0x00FF, 0x100056)  # EFREG2 CD left
        movw(0x00EF, 0x100076)  # EFREG3 CD right
        emit(0x4E75)  # RTS
        struct.pack_into('>'+'H'*len(words), image, 0x900, *words)
        assert 0x900+len(words)*2 < 0x4000
    if synthesis:
        words = []
        # Original guard waveform: two 64-sample ramps. This is ordinary guest
        # RAM, so FM can address beyond LEA without a per-guest host callback.
        for i in range(128):
            movw((i & 63) * 256, 0x4100 + i*2)
        for i in range(4):
            movw(8192, 0x4300+i*2)
            movw(4096, 0x4320+i*2)
        # Slot0: silent phase modulator. Slot5: FM ramp -> left.
        # Slot6: constant with 172.3Hz square ALFO -> right. EGHOLD with AR0
        # exercises specified held gain without inventing envelope rate timing.
        for slot, values in [
            (0, [(2,0x4300),(6,4),(12,0x100),(0,0x0820)]),
            (5, [(2,0x4100),(6,64),(8,0x20),(12,16),
                 (14,0x5000|(0x3B<<6)|0x3B),(18,0x8000),(22,0xFF00),(0,0x0820)]),
            (6, [(2,0x4320),(6,4),(8,0x20),(12,32),
                 (18,(31<<10)|(1<<3)|7),(22,0xEF00),(0,0x0820)]),
        ]:
            for offset, value in values:
                movw(value, 0x100000+slot*32+offset)
        movw(0x1820,0x100000) # KYONEX starts all three slots on the same sample.
        emit(0x4E75)
        struct.pack_into('>'+'H'*len(words), image, 0xC00, *words)
        assert 0xC00+len(words)*2 < 0x4000
    return image


def build(synthesis=False):
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x06100000)
    host = assembler.assemble((HERE/'sound.s').read_text(), base=0x200)
    assert len(host) < 0x1E00
    rom[0x200:0x200+len(host)] = host
    sound = sound_image(synthesis=synthesis)
    rom[0x2000:0x2000+len(sound)] = sound
    text = ['# Original dual-architecture sound BIOS fixture. MIT: adjacent LICENSE.',
            '# Generated by build_sound.py from sound.s and original MC68EC000 words.']
    for address in range(0, len(rom), 16):
        chunk = rom[address:address+16]
        if any(chunk):
            text.append(f'{address:04X} '+' '.join(f'{word:04X}' for word in struct.unpack('>8H', chunk)))
    return '\n'.join(text)+'\n', hashlib.sha256(rom).hexdigest()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text, digest = build()
    path = HERE/'sound.hex'
    if args.check:
        assert path.read_text() == text, 'Rebuild sound.hex'
    else:
        path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} sound.hex; expanded ROM SHA-256 {digest}')
