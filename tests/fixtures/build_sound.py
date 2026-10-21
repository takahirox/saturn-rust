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


def sound_image(effects=False, synthesis=False, peripherals=False, envelope=False, families=False):
    image = bytearray(0x4400)
    struct.pack_into('>II', image, 0, 0x7FF00, 0x100)
    struct.pack_into('>I', image, 28 * 4, 0x300)  # level-four autovector
    words = []
    def emit(*values): words.extend(values)
    def movw(value, address): emit(0x33FC, value & 65535, address >> 16, address & 65535)
    if families:
        movw(0x20F, 0x100400)  # Select 4-Mbit geometry before any stack access.
    # LEA $4000,A0; MOVEQ #16,D0. Create bipolar samples via 68k arithmetic.
    emit(0x41F9, 0, 0x4000, 0x7010, 0xE148, 0x30C0, 0x4440, 0x30C0,
         0xE348, 0x30C0, 0x4440, 0x30C0)
    if families:
        emit(0x4EB9,0,0x1000)
    else:
        movw(0x20F, 0x100400)  # 4Mbit sound RAM, full master volume
    if synthesis or envelope:
        emit(0x4EB9, 0, 0xC00)  # JSR original slot-synthesis program
    else:
        for offset, value in [(2, 0x4000), (4, 0), (6, 4), (12, 0x100), (16, 0), (22, 0xE800), (0, 0x1820)]:
            movw(value, 0x100000 + offset)  # PCM16, loop, direct mode, pan 8
    if peripherals:
        emit(0x4EB9, 0, 0xC00)  # JSR original DMA/MIDI routine
    if effects:
        emit(0x4EB9, 0, 0x900)  # JSR original effects setup (Hardware Lab only).
    movw(0x250 if peripherals else 0x40, 0x10041E)  # timer A enabled for sound CPU
    movw(0x40, 0x100428)  # timer A level-four interrupt
    movw(0x100 if envelope else 0xFE, 0x100418)  # EG fixture waits 512 samples
    if peripherals or envelope:
        movw(0x40,0x100422) # clear the power-on timer request before unmasking
    # MOVE.L #$600D,$800; MOVE #$2000,SR; STOP #$2000; BRA.S back to STOP.
    emit(0x23FC, 0, 0x600D, 0, 0x800, 0x46FC, 0x2000, 0x4E72, 0x2000, 0x60FA)
    struct.pack_into('>'+'H'*len(words), image, 0x100, *words)
    # ADDQ.L #1,$804; clear timer A pending; RTE.
    handler = [0x52B9, 0, 0x804, 0x33FC, 0x40, 0x10, 0x422, 0x4E73]
    if envelope:
        # Actual timed 68k KEY_OFF, then freeze timer A at its terminal count.
        handler = handler[:-1] + [0x33FC, 0x1020, 0x10, 0xC0, 0x4E73]
    struct.pack_into('>'+'H'*len(handler), image, 0x300, *handler)
    if peripherals:
        # Independent handlers: MIDI at level3, DMA at level1, timer at level4.
        for level, address, flag, mailbox in [(3,0x500,0x200,0x818),(1,0x600,0x10,0x81C)]:
            struct.pack_into('>I',image,(24+level)*4,address)
            handler=[0x52B9,0,mailbox,0x33FC,flag,0x10,0x422,0x4E73]
            struct.pack_into('>'+'H'*len(handler),image,address,*handler)
        # Additional original 68000 family vectors execute on the real sound
        # RAM bus (M68000PRM instruction entries / MC68000UM §6.3.9).
        words=[]
        def result(register,address):emit(0x23C0|register,0,address)
        emit(0x7049,0x7250,0x44FC,4,0xC101);result(0,0x830) # ABCD 49+50=99
        emit(0x4800);result(0,0x834) # NBCD 99 -> 01 (X from ABCD is zero)
        emit(0x7000,0x7201,0x44FC,4,0x8101);result(0,0x838) # SBCD 00-01=99
        emit(0x41F9,0,0x4400,0x203C,0x1234,0x5678,0x01C8,1,0x0348,1)
        result(1,0x83C) # MOVEP.L uses four odd byte lanes, not a contiguous long
        emit(0x41F9,0,0x4500,0x203C,0x1122,0x3344,0x223C,0x5566,0x7788)
        emit(0x48E0,0xC080,0x7000,0x7200,0x4CD8,0x0103)
        result(0,0x840);result(1,0x844);result(8,0x848)
        emit(0x203C,0,0xFFFF,0x7202,0xC1C1);result(0,0x88C) # MULS -1*2
        emit(0x70F9,0x7202,0x81C1);result(0,0x890) # DIVS -7/2 -> rem -1, quotient -3
        emit(0x203C,0,0xFF,0x7200,0x44FC,0x14,0xD101);result(0,0x894)
        emit(0x7000,0x44FC,0x14,0x9101);result(0,0x898)
        # MOVEM's first load completes before its next long hits the disconnected
        # sound-bus region. The handler records the actual seven-word fault frame.
        movw(0xCAFE,0x7FFFC);movw(0xBABE,0x7FFFE)
        emit(0x41F9,0x0007,0xFFFC,0x223C,0xDEAD,0xBEEF,0x4CD8,3)
        resume = 0x900 + len(words)*2
        result(0,0x850);result(1,0x854);result(8,0x858)
        emit(0x4E40,0x4E75) # TRAP #0/RTE then RTS to the I/O routine
        assert 0x900+len(words)*2<0xC00
        struct.pack_into('>'+'H'*len(words),image,0x900,*words)
        struct.pack_into('>I',image,2*4,0x700)
        struct.pack_into('>I',image,32*4,0x820)
        # MOVE.W (SP),$860; MOVE.L 2(SP),$864; MOVE.W 6(SP),$868;
        # LEA 8(SP),SP drops the extended fault prefix before normal RTE.
        fault=[0x33D7,0,0x860,0x23EF,2,0,0x864,0x33EF,6,0,0x868,
               0x23EF,10,0,0x89C,0x33EF,8,0,0x8A0,
               0x2F7C,0,resume,10,0x4FEF,8,0x4E73]
        struct.pack_into('>'+'H'*len(fault),image,0x700,*fault)
        trap=[0x52B9,0,0x884,0x4E73]
        struct.pack_into('>'+'H'*len(trap),image,0x820,*trap)
        words=[]
        emit(0x4EB9,0,0x900) # Execute architectural family/fault vectors first.
        movw(0xA55A,0x4300);movw(0x1234,0x4302)
        movw(0x4300,0x100412);movw(0x3E2,0x100414);movw(0x1004,0x100416)
        # ST-077 table4.40: DEXE=0 must not clear busy before the copy ends.
        movw(4,0x100416)
        emit(0x3039,0x10,0x416,0x0240,0x1000,0x66F4) # poll DEXE
        for source,destination in [(0x1003E2,0x810),(0x1003E4,0x812)]:
            emit(0x33F9,source>>16,source&65535,0,destination)
        for byte in [0x91,0x40,0x7F]:movw(byte,0x100406)
        emit(0x3039,0x10,0x404,0x0240,0x0800,0x67F4) # poll MOEMP
        emit(0x33F9,0x10,0x420,0,0x814) # record DMA + MIDI pending
        movw(0x90,0x100424);movw(0x80,0x100426)
        emit(0x4E75)
        struct.pack_into('>'+'H'*len(words),image,0xC00,*words)
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
    if synthesis or envelope:
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
        if envelope:
            # Slot6: ordinary timed decay, sustain, and release.
            movw(0x06DF,0x1000C8) # AR31,D1R27,D2R0; no EGHOLD
            movw(0x3C9F,0x1000CA) # KRS off,DL4,RR31
            movw(0x8000,0x1000D2) # LFO held; EG is the only changing gain
        movw(0x1820,0x100000) # KYONEX starts all three slots on the same sample.
        emit(0x4E75)
        struct.pack_into('>'+'H'*len(words), image, 0xC00, *words)
        assert 0xC00+len(words)*2 < 0x4000
    if families:
        from m68k_families import install
        install(image)
    return image


def build(synthesis=False, peripherals=False, envelope=False, families=False):
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x06100000)
    host = assembler.assemble((HERE/'sound.s').read_text(), base=0x200)
    assert len(host) < 0x1E00
    rom[0x200:0x200+len(host)] = host
    sound = sound_image(synthesis=synthesis, peripherals=peripherals, envelope=envelope, families=families)
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
