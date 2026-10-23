#!/usr/bin/env python3
"""Original MIT VDP1 progress/erase and VDP2 monitor diagnostics.

ST-013-R3 §§2/4.2/4.4–4.8; ST-058-R2 §§2.1/2.4/2.5/3.3/4.5.
No SDK or firmware; every setup, observation and result store is SH-2 code.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_clock import put16, read
from build_hle_system import record

HERE = Path(__file__).resolve().parent


def blank(label):
    return (f'load 0x25f80004,r1\nin_{label}:\nmov.w @r1,r0\ntst #8,r0\nbt in_{label}\n'
            f'out_{label}:\nmov.w @r1,r0\ntst #8,r0\nbf out_{label}\n')


def draw_source():
    # TVSTAT.VBLANK is forced high when DISP=0. Enable display before
    # waiting on real raster transitions; VDP1 framebuffer reads are separate.
    s = 'load 0x0600d800,r8\n' + put16(0x25D00002, 2) + put16(0x25F80000, 0x8000)
    for at, value in [(0, 9), (20, 511), (22, 255),
                      (32, 6), (36, 0x8C0), (38, 0x801F), (48, 4095), (64, 0x8000)]:
        s += put16(0x25C00000 + at, value)
    s += put16(0x25D00004, 1)
    s += ('load 0x25d00014,r1\nwait_line:\nmov.w @r1,r0\n'
          'mov #4,r2\ncmp/eq r2,r0\nbf wait_line\n')
    s += read(0x25C80000, 2) + read(0x25C803FE, 2)
    s += put16(0x25D0000C, 0) + 'nop\n' * 32
    s += read(0x25D00010, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += put16(0x25D00002, 3) + blank('partial')
    s += read(0x25D00012, 2) + read(0x25D00010, 2)
    # Publish three authored words. The erase budget, not a precomputed
    # result or an injected host framebuffer, determines the last two.
    for word in [0, 58811, 58812]:
        s += put16(0x25C80000 + word * 2, 0xFFFF)
    # Ch.4 capture timing: write the intended erase before the publishing
    # change, then enable VBE. Keep all original partial-budget goldens.
    for at, value in [(6, 0x1234), (8, 0), (10, 64 << 9 | 255)]:
        s += put16(0x25D00000 + at, value)
    s += put16(0x25D00002, 3) + blank('publish')
    for at, value in [(0, 8), (2, 3)]:
        s += put16(0x25D00000 + at, value)
    s += blank('erase')
    for word in [0, 58811, 58812]:
        s += read(0x25C80000 + word * 2, 2)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def command_source():
    # PTMR starts a clip command. The following guest store changes CTRL
    # after its first word has been read but before the full table completes.
    # Retained CTRL=9 allows the line to draw; a late atomic snapshot sees END
    # and leaves both independently selected framebuffer samples black.
    s = 'load 0x0600e100,r8\n' + put16(0x25D00002, 2)
    for at, value in [(0, 9), (20, 511), (22, 255), (32, 6),
                      (36, 0xC0), (38, 0x801F), (48, 7), (64, 0x8000)]:
        s += put16(0x25C00000 + at, value)
    s += ('load 0x25d00004,r4\nload 0x25c00000,r5\nload 0x8000,r6\n'
          'mov #1,r0\nmov.w r0,@r4\nmov.w r6,@r5\n'
          'load 0x25d00010,r1\nwait_draw:\nmov.w @r1,r0\n'
          'tst #2,r0\nbt wait_draw\n')
    for address in [0x25C80000, 0x25C8000E, 0x25D00014, 0x25D00010]:
        s += read(address, 2)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def contention_source():
    s = 'load 0x0600e200,r8\n' + put16(0x25D00002, 2)
    for at, value in [(0, 9), (20, 511), (22, 255), (32, 6),
                      (36, 0x8C0), (38, 0x801F), (48, 4095), (64, 0x8000)]:
        s += put16(0x25C00000 + at, value)
    s += ('load 0x25c81000,r4\nload 0x25d00010,r5\n'
          'load 0xfffffe12,r10\nload 0xfffffe13,r11\n')
    timer = ('mov.b @r10,r0\nextu.b r0,r0\nshll8 r0\n'
             'mov.b @r11,r1\nextu.b r1,r1\nor r1,r0\n')
    s += timer + 'mov r0,r9\n' + put16(0x25D00004, 1)
    s += ('poll_draw:\nmov.w @r4,r2\nmov.w @r5,r0\n'
          'tst #2,r0\nbt poll_draw\n')
    s += timer + 'sub r9,r0\nextu.w r0,r0\n' + record()
    for address in [0x25C80000, 0x25C803FE, 0x25D00010]:
        s += read(address, 2)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def effects_source():
    # All four Gouraud vertices belong to each flat polygon. Sixteen
    # polygons write independent eight-dot rows, so blend/shadow never
    # consume an output pixel from an earlier polygon. The FRT measures
    # table reads and the documented six-clock framebuffer calculations.
    s = 'load 0x0600e300,r8\n' + put16(0x25D00002, 2)
    s += 'load 0x100,r1\nmov.w @r1,r12\n'
    s += 'load 0x102,r1\nmov.w @r1,r13\n'
    s += put16(0x25C00000, 9) + put16(0x25C00014, 511) + put16(0x25C00016, 255)
    for row in range(16):
        at = 0x25C00020 + row * 32
        for offset, value in [(0, 4), (12, 0), (14, row),
                              (16, 7), (18, row), (20, 7), (22, row),
                              (24, 0), (26, row), (28, 0x100)]:
            s += put16(at + offset, value)
        s += f'load {at+4:#x},r1\nmov.w r12,@r1\n'
        s += f'load {at+6:#x},r1\nmov.w r13,@r1\n'
        for x in range(8):
            s += put16(0x25C80000 + row * 1024 + x * 2, 0xFC00)
    s += put16(0x25C00220, 0x8000)
    for i in range(4):
        s += put16(0x25C00800 + i * 2, 0x4210)
    s += 'load 0xfffffe12,r10\nload 0xfffffe13,r11\n'
    timer = ('mov.b @r10,r0\nextu.b r0,r0\nshll8 r0\n'
             'mov.b @r11,r1\nextu.b r1,r1\nor r1,r0\n')
    s += timer + 'mov r0,r9\n' + put16(0x25D00004, 1)
    s += ('load 0x25d00010,r1\npoll_effects:\nmov.w @r1,r0\n'
          'tst #2,r0\nbt poll_effects\n')
    s += timer + 'sub r9,r0\nextu.w r0,r0\n' + record()
    for address in [0x25C80000, 0x25C8000E, 0x25C83C00, 0x25D00010]:
        s += read(address, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += read(0x25D00014, 2)
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def lookup_source():
    # The first sprite expands eight source dots to 504x4 raster dots. The
    # guest changes all palette entries during drawing. Only the next command
    # may see that change; framebuffer output never supplies the goldens.
    s = 'load 0x0600e400,r8\n' + put16(0x25D00002, 2)
    for at, value in [(0, 9), (20, 511), (22, 255),
                      (32, 1), (36, 0xC8), (38, 0x200), (40, 0x100), (42, 0x101),
                      (52, 503), (54, 3),
                      (64, 0), (68, 0xC8), (70, 0x200), (72, 0x102), (74, 0x102),
                      (78, 8), (96, 0x8000), (0x800, 0x1111), (0x802, 0x1111),
                      (0x810, 0x0123), (0x812, 0x4567), (0x814, 0x89AB), (0x816, 0xCDEF)]:
        s += put16(0x25C00000 + at, value)
    def palette(label, operand):
        return (f'load {operand:#x},r1\nmov.w @r1,r0\n'
                'load 0x25c01000,r4\nmov #16,r5\n'
                f'{label}:\nmov.w r0,@r4\nadd #2,r4\nadd #1,r0\n'
                f'dt r5\nbf {label}\n')
    s += palette('initial_palette', 0x100)
    s += put16(0x25D00004, 1) + 'nop\n' * 64
    s += palette('changed_palette', 0x102)
    s += ('load 0x25d00010,r1\npoll_lookup:\nmov.w @r1,r0\n'
          'tst #2,r0\nbt poll_lookup\n')
    for address in [0x25C80000, 0x25C803EE, 0x25C80C00, 0x25C80FEE]:
        s += read(address, 2)
    for y in range(2):
        for x in range(8):
            s += read(0x25C82000 + y * 1024 + x * 2, 2)
    for address in [0x25D00010, 0x25D00014]:
        s += read(address, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def character_source():
    # Four nibbles share a physical texture word. A very wide first sprite
    # keeps that word selected while the guest changes it. The later command
    # must reload the changed character data. All writes/polls are SH-2 code.
    s = 'load 0x0600e500,r8\n' + put16(0x25D00002, 2)
    # Keep this relative rewrite window at eight clocks per IF longword:
    # two word phases, each T1 + two Tw + T2. No timing snapshot is changed.
    s += 'load 0xffffffe8,r1\nload 0xa55aaafe,r0\nmov.l r0,@r1\n'
    for at, value in [(0, 9), (20, 511), (22, 255),
                      (32, 1), (36, 0xC8), (38, 0x200), (40, 0x100), (42, 0x101),
                      (52, 4095), (54, 0),
                      (64, 0), (68, 0xC8), (70, 0x200), (72, 0x100), (74, 0x101),
                      (78, 2), (96, 0x8000), (0x800, 0x1111), (0x802, 0x3333)]:
        s += put16(0x25C00000 + at, value)
    for i in range(16):
        s += put16(0x25C01000 + i * 2, 0x8010 + i)
    s += put16(0x25D00004, 1) + 'nop\n' * 64
    s += ('load 0x100,r1\nmov.w @r1,r0\nload 0x25c00800,r1\nmov.w r0,@r1\n')
    s += put16(0x25C00802, 0x4444)
    s += ('load 0x25d00010,r1\npoll_character:\nmov.w @r1,r0\n'
          'tst #2,r0\nbt poll_character\n')
    for x in [0, 255, 511]:
        s += read(0x25C80000 + x * 2, 2)
    for x in range(8):
        s += read(0x25C80800 + x * 2, 2)
    for address in [0x25D00010, 0x25D00014]:
        s += read(address, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def preclip_source():
    # A normal sprite begins outside the clip with two end codes. Pre-clipping
    # starts at its visible endpoint, reversing UVs. A second command proves
    # that separated lines finish without sampling or drawing their texture.
    s = 'load 0x0600e600,r8\n' + put16(0x25D00002, 2)
    for at, value in [(0, 9), (20, 511), (22, 255),
                      (32, 0), (38, 0x8000), (40, 0x100), (42, 0x101),
                      (44, 0xFFFC), (64, 0), (72, 0x100), (74, 0x101),
                      (76, 0xFFF0), (96, 0x8000), (0x800, 0xFF12), (0x802, 0x3456)]:
        s += put16(0x25C00000 + at, value)
    s += ('load 0x100,r1\nmov.w @r1,r0\nload 0x25c00024,r1\nmov.w r0,@r1\n')
    s += put16(0x25D00004, 1)
    s += ('load 0x25d00010,r1\npreclip_done:\nmov.w @r1,r0\n'
          'tst #2,r0\nbt preclip_done\n')
    for x in range(5):
        s += read(0x25C80000 + x * 2, 2)
    for at in [0x25D00010, 0x25D00014]:
        s += read(at, 2)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x2000,r2\nand r2,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def monitor_source():
    s = 'load 0x0600dc00,r8\n'
    # Set all control/data while display is disabled. Odd pixels come from
    # NBG1 only in exclusive high-resolution mode, following §2.1 p.13.
    for off, value in [(0xE, 0), (0x10, 0x4444), (0x12, 0xFFFF),
                       (0x14, 0xFFFF), (0x16, 0xFFFF), (0x18, 0x5555),
                       (0x1A, 0xFFFF), (0x1C, 0xFFFF), (0x1E, 0xFFFF),
                       (0x20, 0x303), (0x28, 0x1616), (0x3C,0x20), (0xF8, 0x101),
                       (0x7C, 1), (0x8C, 1), (0xAC, 3), (0xAE, 0xF800)]:
        s += put16(0x25F80000 + off, value)
    # 256-color 512x512 bitmaps fit separately in A and B. Two 16-bit
    # 512x512 surfaces would overlap and cannot reserve independent fetches.
    for index,color in enumerate([0x801F,0x83E0,0xFC00,0xFFFF],1):
        s += put16(0x25F00000+index*2,color)
    for base in [0x25E00000,0x25E40000]:
        for y in [0,479]:
            for x,word in enumerate([0x0102,0x0304,0x0102,0x0304]):
                s += put16(base+y*512+x*2,word)
    s += 'load 0x8006,r9\nmov r9,r0\nmov #2,r2\nand r2,r0\ntst r0,r0\nbt ordinary\n'
    for off, value in [(0x78, 2), (0x88, 2), (0x80, 1)]:
        s += put16(0x25F80000 + off, value)
    s += 'bra configured\nnop\nordinary:\n'
    for off, value in [(0x78, 1), (0x88, 1), (0x80, 0)]:
        s += put16(0x25F80000 + off, value)
    s += 'configured:\nload 0x25f80000,r1\nmov.w r9,@r1\n'
    s += blank('first')
    s += 'load 0x25f80004,r1\nwait_blank:\nmov.w @r1,r0\ntst #8,r0\nbt wait_blank\n'
    s += 'mov #8,r2\nand r2,r0\n' + record()
    s += read(0x25F80002, 2) + read(0x25F8000A, 2)
    s += 'mov r9,r0\n' + record() + 'load 0x600d,r0\n' + record()
    s += 'park:\nsleep\nbra park\nnop\n'
    return s


def grants_source():
    s = 'load 0x0600de00,r8\n'
    for at,value in [(0x20,1),(0x28,0x32),(0xF8,1)]:
        s += put16(0x25F80000+at,value)
    s += put16(0x25F80000,0x8000)
    for n,(split,a0,a1,b0,b1,mode,address) in enumerate([
        (0,0xFFFFFFFF,0,0,0,0x8000,0x25E00000),
        (0x300,0,0,0,0,0x8000,0x25E00000),
        (0x300,0xFEEE0000,0xFEEE0000,0,0,0x8000,0x25E00000),
        (0x300,0xFEEE0000,0xF0000000,0,0,0x8000,0x25E20000),
        (0x300,0,0,0xFFFFFFFF,0xFFFFFFFF,0x8000,0x25E40000),
        (0,0x0000000E,0,0,0,0x8002,0x25E00000),
    ]):
        for at,value in [(0xE,split),(0,mode)]:
            s += put16(0x25F80000+at,value)
        for bank,pattern in enumerate([a0,a1,b0,b1]):
            s += put16(0x25F80010+bank*4,pattern>>16)
            s += put16(0x25F80012+bank*4,pattern&65535)
        s += blank(f'grant{n}')
        s += (f'load {address:#x},r4\nload 0xfffffe12,r1\nload 0xfffffe13,r2\n'
              'mov.b @r1,r0\nmov.b @r2,r5\nextu.b r5,r5\n')
        s += 'mov.w @r4,r0\n'*8
        s += ('mov.b @r1,r0\nmov.b @r2,r0\nextu.b r0,r0\n'
              'sub r5,r0\nextu.b r0,r0\n') + record()
    s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
    return s


def encode(name, source, operands=()):
    code = assembler.assemble(source, base=0x200)
    rom = bytearray(524288)
    rom[:8] = struct.pack('>II', 0x200, 0x060FF000)
    rom[0x200:0x200 + len(code)] = code
    for address, value in operands:
        struct.pack_into('>H', rom, address, value)
    lines = [f'# Original MIT {name} synthetic BIOS. Adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at + 16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at + 16])))
    return {name + '.s': source.encode(), name + '.hex': ('\n'.join(lines) + '\n').encode()}, hashlib.sha256(rom).hexdigest()


def build():
    files, draw_hash = encode('vdp1-progress', draw_source())
    monitor, monitor_hash = encode('exclusive-monitor', monitor_source())
    files.update(monitor)
    grants, grants_hash = encode('vram-grants', grants_source())
    files.update(grants)
    commands, command_hash = encode('vdp1-command-fetch', command_source())
    files.update(commands)
    contention, contention_hash = encode('vdp1-memory-ownership', contention_source())
    files.update(contention)
    effects, effects_hash = encode('vdp1-effects-timing', effects_source(), [(0x100, 0xC4), (0x102, 0x8010)])
    files.update(effects)
    lookup, lookup_hash = encode('vdp1-lookup-fetch', lookup_source(), [(0x100, 0x8000), (0x102, 0x83E0)])
    files.update(lookup)
    character, character_hash = encode('vdp1-character-fetch', character_source(), [(0x100, 0x2222)])
    files.update(character)
    preclip, preclip_hash = encode('vdp1-preclip', preclip_source(), [(0x100, 0)])
    files.update(preclip)
    files['vdp1-progress.tsv'] = ''.join(f'{v:08X}\n' for v in
        [0x801F, 0, 0, 0, 4, 0, 0x1234, 0x1234, 0xFFFF, 0x600D]).encode()
    return files, [draw_hash, monitor_hash, grants_hash, command_hash, contention_hash, effects_hash, lookup_hash, character_hash, preclip_hash]


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    files, hashes = build()
    for name, data in files.items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
    print('Expanded BIOS SHA-256: ' + ', '.join(hashes))
