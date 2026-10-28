#!/usr/bin/env python3
"""Original MIT MVOL guest/media; adjacent LICENSE. No SDK or firmware.

68k computes bipolar PCM, uploads an executing half-gain MIXS0 effects program,
programs MVOL by byte, then keeps counting. SH-2 uploads it, starts runtime CD-DA,
waits, mutes by byte, waits, restores by byte and keeps counting. Test variants
change only source operands/routing/volume instructions. Expectations are authored
in tests/scsp_mvol.rs and scripts/scsp-mvol.mjs, independently of this encoder.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_cd_drive import command, store
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x1100)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    words = [0x41f9, 0, 0x1000, 0x7010, 0xe148, 0x30c0, 0x4440,
             0x30c0, 0xe348, 0x30c0, 0x4440, 0x30c0]
    operands = {'wave': 0x4107}

    def write(address, value, name=None, byte=False):
        if name:
            operands[name] = 0x4100 + len(words) * 2 + 2
        words.extend([0x13fc if byte else 0x33fc, value, address >> 16, address & 65535])

    # Initial gain is zero. The final byte write must retain MEM4MB.
    write(0x100400, 0x200)
    for offset, value in [(2,0x1000),(4,0),(6,4),(12,0x100),(16,0),(20,7)]:
        write(0x100000+offset,value)
    write(0x100016,0xe8ff,'pcm')  # dry pan8 plus wet left
    write(0x100216,0xff,'ext_left')
    write(0x100236,0xef,'ext_right')
    write(0x100700,0x4000,'coef')
    # ST-077 figs4.18/4.30: wait through both level intervals; slot0
    # publishes at OP7, 24 memory steps after OP1. Preserve the wet oracle.
    program = [[0,0,2,0]]*24 + [[0,0xa800,2,0],[0,0,0x1002,0]]
    for i, step in enumerate(program):
        for j, word in enumerate(step):
            write(0x100800+i*8+j*2,word)
    write(0x70000,0x5a5a)  # absent-chip sentinel if MEM4MB is lost
    write(0x100000,0x1820)
    write(0x100401,13,'sound_volume',byte=True)
    words.extend([0x33f9,0x0007,0,0,0x808])  # retained geometry after byte write
    words.extend([0x23fc,0,0x600d,0,0x800])
    # Independent CPU progress mailbox, including throughout MVOL=0.
    words.extend([0x52b9,0,0x804,0x60f8])
    struct.pack_into('>'+'H'*len(words),sound,0x100,*words)
    assert 0x100+len(words)*2 < 0x800
    source = store(0x25b00400,0x200)
    source += ('load 0x25b00401,r1\nmov #15,r0\nmov.b r0,@r1\n'
               'load 0x4000,r1\nload 0x25a00000,r2\nload 0x440,r3\ncopy:\n'
               'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    source += smpc(6) + wait(0x05a00800,'ready')
    source += command([0x1080,151,0x80,32],'play',0x06009000)
    # Log the sound-CPU progress at both edges. The CD source contains a
    # non-repeating frame index; restored audio must expose elapsed CD playback.
    for phase, volume in [(1,None),(2,0),(3,13)]:
        if volume is not None:
            source += 'load 0x25b00401,r1\n'
            operands['mute' if phase==2 else 'restore'] = 0x200 + len(assembler.assemble(source,base=0x200))
            source += f'mov #{volume},r0\nmov.b r0,@r1\n'
        source += f'load 0x06009100,r1\nmov #{phase},r0\nmov.l r0,@r1\n'
        source += f'load 15000,r3\ndwell{phase}:\nload 0x06009104,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\ndt r3\nbf dwell{phase}\n'
        source += f'load 0x25a00804,r1\nmov.l @r1,r0\nload {0x06009110+phase*4:#x},r1\nmov.l r0,@r1\n'
    source += 'load 0x25a70000,r1\nmov.w @r1,r0\nextu.w r0,r0\nload 0x06009120,r1\nmov.l r0,@r1\n'
    source += 'load 0x06009100,r1\nmov #4,r0\nmov.l r0,@r1\npark:\nsleep\nbra park\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x06100000)
    code = assembler.assemble(source,base=0x200)
    assert len(code) < 0x3000
    rom[0x200:0x200+len(code)] = code
    rom[0x4000:0x5100] = sound
    lines = ['# Original MIT MVOL SH-2/68k/DSP guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    # Original mixed data/audio media, one data sector followed by 32 audio
    # sectors. Right-channel frame index makes mute/resume progress observable.
    audio = b''.join(struct.pack('<hh',1024,-1000-i) for i in range(32*588))
    cue = 'FILE "mvol-data.bin" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "mvol-audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n'
    return {'scsp-mvol.hex':('\n'.join(lines)+'\n').encode(),
            'scsp-mvol.s':source.encode(),
            'scsp-mvol.tsv':''.join(f'{name}\t{at:08X}\n' for name,at in operands.items()).encode(),
            'mvol-data.bin':bytes(2048), 'mvol-audio.bin':audio, 'mvol.cue':cue.encode()}, rom


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    outputs,rom=build()
    for name,data in outputs.items():
        path=HERE/name
        if args.check:
            assert path.read_bytes()==data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
    print(f'Expanded ROM SHA-256 {hashlib.sha256(rom).hexdigest()}')
