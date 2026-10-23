#!/usr/bin/env python3
"""Original MIT sector-addressed bootstrap/CD guest; adjacent LICENSE.

Only the small seed/source/manifest are committed. --output DIR creates sparse
270000-sector ISO and original audio/CUE, then checks their complete hashes.
No SDK, firmware, commercial media or emulator decoding routines are used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
from build_scu import assembler as a


def store(address, value, width='w'):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.{width} r0,@r1\n'


HERE = Path(__file__).resolve().parent
SECTOR = 2048
COUNT = 270000
FIRST = 9003
LATE = 260000
AUDIO_FAD = 270450
SIZE = 0x1400


def command(words):
    s = ''
    for register, value in zip(range(4, 8), words):
        s += f'load {value:#x},r{register}\n'
    return s + 'bsr command\nnop\n'


def build():
    s = 'mov #-16,r0\nldc r0,sr\nload 0x06000000,r0\nldc r0,vbr\n'
    # Backup bytes and selected boot mode must survive both reset paths.
    s += 'load 0x20180000,r1\nmov.w @r1,r0\nadd #1,r0\nmov.w r0,@r1\n'
    s += store(0x0600012C, 0x06005000, 'l') + store(0x06000140, 0x06005200, 'l')
    s += store(0x25FE00A0, 0x37FF, 'l') + store(0x25FE00A8, 1, 'l')
    s += store(0x2589000C, 4) + 'mov #0,r0\nldc r0,sr\n'
    s += command([0x3000, 0, 0, 0])
    s += 'read_command:\n' + command([0x1080 | ((LATE + 150) >> 16), (LATE + 150) & 65535, 0x80, 1])
    s += 'sector_wait:\n' + command([0x5100, 0, 0, 0])
    s += 'mov #1,r2\ncmp/eq r2,r0\nbf sector_wait\n'
    s += command([0x6300, 0, 0, 1])
    for off, value in [(0, 0x05818000), (4, 0x06011000), (8, 2048), (12, 2), (20, 7), (16, 0x101)]:
        s += store(0x25FE0000 + off, value, 'l')
    s += 'load 0x06010004,r1\ndma_wait:\nmov.l @r1,r0\ntst r0,r0\nbt dma_wait\n'
    s += command([0x0600, 0, 0, 0])
    s += 'load 0x06011000,r2\nload 512,r3\nload 7,r4\nsum_data:\nmov.l @r2+,r0\nadd r0,r4\ndt r3\nbf sum_data\nload 0x06010000,r1\nmov.l r4,@r1\n'
    s += store(0x25B00400, 0x020F) + store(0x25B00216, 0xFF) + store(0x25B00236, 0xEF)
    # ISO has no audio; the CUE version starts the source-authored second track.
    s += command([0x1080 | (AUDIO_FAD >> 16), AUDIO_FAD & 65535, 0x0F80, 300])
    s += store(0x0601000C, 0x600D, 'l')
    s += store(0x20100079, 0x60, 'b') + store(0x2010007D, 1, 'b') + store(0x20100075, 0x40, 'b')
    s += 'restart_wait:\nload 0x20100075,r1\nmov.b @r1,r0\ntst #8,r0\nbf restart_wait\n'
    s += store(0x20100063, 1, 'b') + store(0x2010001F, 0x0D, 'b')
    s += 'park:\nbra park\nnop\n'
    s += 'command:\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\nload 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\nadd #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\nload 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\nload 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n'
    program = bytearray(SIZE)
    main = a.assemble(s, base=0x06004000)
    assert len(main) < 0x1000
    program[:len(main)] = main
    push = 'mov.l r0,@-r15\nmov.l r1,@-r15\n'
    pop = 'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    dma = push + store(0x06010004, 1, 'l') + pop
    irq = push + store(0x25890008, 0xFFFB) + store(0x25FE00A8, 1, 'l')
    irq += 'load 0x06010008,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n' + pop
    for off, source in [(0x1000, dma), (0x1200, irq)]:
        code = a.assemble(source, base=0x06004000 + off)
        assert len(code) < 0x200
        program[off:off + len(code)] = code
    image = bytearray(a.build(bytes(program), (HERE/'LICENSE').read_bytes(), 'LATE SECTOR LAB', 'LATE-001 V1.000')[:32*SECTOR])
    image[24*SECTOR:24*SECTOR+SIZE] = program
    image[20*SECTOR:22*SECTOR] = bytes(2*SECTOR)
    pvd = 16*SECTOR
    image[pvd+80:pvd+88] = a.both32(COUNT)
    root = a.record(b'\0', 20, 4096, True)
    image[pvd+156:pvd+190] = root
    image[20*SECTOR:20*SECTOR+34] = root
    image[20*SECTOR+34:20*SECTOR+68] = a.record(b'\1', 20, 4096, True)
    record = bytearray(a.record(b'LATE.BIN;1', 9000, SIZE))
    record[1] = 2  # ECMA-119: whole attribute unit, then gap, then data
    record[26:28] = bytes([2, 1])
    image[21*SECTOR:21*SECTOR+len(record)] = record
    for i in range(512):
        struct.pack_into('>I', image, 28*SECTOR + i*4, (i+1)*3)
    cue = 'FILE "large-media.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nPOSTGAP 00:02:00\nFILE "large-media-audio.bin" BINARY\nTRACK 02 AUDIO\nPREGAP 00:02:00\nINDEX 01 00:00:00\n'
    sources = s + '\n; DMA handler at 0x06005000\n' + dma + '\n; CD IRQ handler at 0x06005200\n' + irq
    return bytes(image), sources.encode(), cue.encode()


def raw_vectors(seed, xa):
    from build_cd_filesystem import crc, raw_xa
    result = bytearray()
    for lba in [0, 1, 16, 17, 20, 21, FIRST, FIRST+1, FIRST+3]:
        if lba < 22:
            payload = seed[lba*SECTOR:(lba+1)*SECTOR]
        else:
            i = [FIRST, FIRST+1, FIRST+3].index(lba)
            payload = seed[24*SECTOR+i*SECTOR:24*SECTOR+min(SIZE,(i+1)*SECTOR)]
            payload += bytes(SECTOR-len(payload))
        if xa:
            result += raw_xa(payload, lba+150, 0)
            continue
        raw = bytearray(2352)
        raw[1:11] = b'\xff'*10
        fad = lba+150
        def bcd(n): return n//10*16+n%10
        raw[12:16] = bytes([bcd(fad//4500), bcd(fad//75%60), bcd(fad%75), 1])
        raw[16:2064] = payload
        struct.pack_into('<I', raw, 2064, crc(raw[:2064]))
        def mul(left, right):
            out = 0
            while right:
                if right & 1: out ^= left
                right >>= 1
                left <<= 1
                if left & 256: left ^= 0x11D
            return out
        powers = [1]
        for _ in range(255): powers.append(mul(powers[-1], 2))
        inv3 = next(i for i in range(256) if mul(i, 3) == 1)
        # Independent parity-check matrix, H*V=0 (ECMA-130 annex A).
        for rows, columns, step, stride, output in [(86,24,2,86,2076),(52,43,86,88,2248)]:
            for row in range(rows):
                total = weighted = 0
                for col in range(columns):
                    symbol = raw[12+((row//2)*step+row%2+col*stride)%(rows*columns)]
                    total ^= symbol
                    weighted ^= mul(symbol, powers[columns+1-col])
                parity = mul(weighted^total, inv3)
                raw[output+row] = parity
                raw[output+rows+row] = parity^total
        result += raw
    return bytes(result)


def generate(directory, seed, cue):
    directory.mkdir(parents=True, exist_ok=True)
    iso = directory/'large-media.iso'
    with iso.open('wb') as f:
        f.truncate(COUNT*SECTOR)
        f.write(seed[:22*SECTOR])
        for i in range((SIZE+SECTOR-1)//SECTOR):
            f.seek((FIRST+(i//2)*3+i%2)*SECTOR)
            f.write(seed[24*SECTOR+i*SECTOR:24*SECTOR+min(SIZE,(i+1)*SECTOR)])
        f.seek(LATE*SECTOR)
        f.write(seed[28*SECTOR:29*SECTOR])
    audio = directory/'large-media-audio.bin'
    with audio.open('wb') as f:
        # Constant but nonzero stereo is independently recognizable after routing.
        f.write(struct.pack('<hh', 4096, -2048)*(300*588))
    (directory/'large-media.cue').write_bytes(cue)
    hashes = {}
    for path in [iso, audio, directory/'large-media.cue']:
        digest = hashlib.sha256()
        with path.open('rb') as source:
            while data := source.read(1024*1024):
                digest.update(data)
        hashes[path.name] = digest.hexdigest()
    return hashes


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    p.add_argument('--output', type=Path)
    args = p.parse_args()
    seed, source, cue = build()
    for name, data in [('large-media.seed', seed), ('large-media.s', source), ('large-media.cue', cue),
                       ('large-media-mode1.raw', raw_vectors(seed, False)),
                       ('large-media-xa.raw', raw_vectors(seed, True))]:
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{name}: {hashlib.sha256(data).hexdigest()}')
    if args.output:
        hashes = generate(args.output, seed, cue)
        manifest = dict(sectors=COUNT, bytes=COUNT*SECTOR, first=FIRST, late=LATE,
                        audio_fad=AUDIO_FAD, leadout=270750, sum=393991, hashes=hashes)
        target = HERE/'large-media.json'
        text = json.dumps(manifest, indent=2, sort_keys=True)+'\n'
        if args.check:
            assert target.read_text() == text, 'Rebuild large-media.json'
        else:
            target.write_text(text)
        print(text)
