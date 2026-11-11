#!/usr/bin/env python3
"""Original MIT reset-vector initialization + no-media CD diagnostic.

No firmware/SDK bytes; adjacent LICENSE. Literal expected records are authored
from the contracts in docs/bios-startup.md, never from emulator output.
"""
import argparse
import struct
from pathlib import Path
from build_scu import store
from build_clock import put16, put8, read
from build_hle_system import record
from build_video_transactions import encode
from build_smpc import idle
from build_smpc_transactions import WINDOW_CALL, command_window

HERE = Path(__file__).resolve().parent


def masked(address, mask):
    return f'load {address:#x},r1\nmov.l @r1,r0\nload {mask:#x},r2\nand r2,r0\n' + record()


def poll(address, mask, tag, width='w', nonzero=True):
    return (f'load {address:#x},r1\n{tag}:\nmov.{width} @r1,r0\n'
            f'load {mask:#x},r2\ntst r2,r0\n' +
            f'{"bt" if nonzero else "bf"} {tag}\n')


def smpc(command, tag):
    return put8(0x20100063, 1) + put8(0x2010001f, command) + idle(tag)


def status(tag, overlap=False):
    s = put8(0x20100063, 1)
    for i, value in enumerate([1, 8 if overlap else 0, 0xf0]):
        s += put8(0x20100001 + 2*i, value)
    s += put8(0x2010001f, 0x10)
    if overlap:
        s += put8(0x2010001f, 0x1a) + store(0x26020ff4, 1)
    return s + idle(tag)


def sound_program():
    # Real MC68000 clear loops. D2 counts every completed CLR.W; acknowledgement
    # follows the final bus cycle. An exception or omitted SNDON cannot finish.
    words = [0x7400, 0x41f9, 0x0010, 0, 0x303c, 511,
             0x4258, 0x5282, 0x51c8, 0xfffa,  # CLR.W (A0)+; ADDQ.L #1,D2; DBRA
             0x41f9, 0x0010, 0x07c0, 0x701f,
             0x4258, 0x5282, 0x51c8, 0xfffa,
             0x23c2, 0, 0x800,  # 544 completed writes from the 68k itself
             0x23fc, 0, 0x600d, 0, 0x804,
             0x4e72, 0x2700, 0x60fa]
    image = bytearray(0x200)
    struct.pack_into('>II', image, 0, 0x7ff00, 0x100)
    struct.pack_into('>'+'H'*len(words), image, 0x100, *words)
    return image


def build():
    expected = []
    s = 'load 0x26020000,r8\n'
    s += store(0x26020ff8, 1)
    # Real raster interrupt pending, preserved/cleared and later reasserted.
    s += poll(0x25fe00a4, 1, 'first_blank', 'l') + masked(0x25fe00a4, 1)
    s += store(0x25fe00a4, 0xffffffff) + masked(0x25fe00a4, 1)
    s += store(0x25fe00a4, 0) + masked(0x25fe00a4, 1)
    s += poll(0x25fe00a4, 1, 'next_blank', 'l') + masked(0x25fe00a4, 1)
    expected += [1, 1, 0, 1]
    for at in [4, 6, 8, 10, 12, 0xfe]:
        s += put16(0x25f80000+at, 0x7fff)
    for at in [6, 8, 10, 12, 0xfe]:
        s += read(0x25f80000+at, 2)
    expected += [0]*5
    # Legal status/control issue window, except the deliberate overlap itself.
    # Remove the helper's timing records; it still waits VBlank-in + >300us.
    s += put16(0x25f80000, 0x8000) + WINDOW_CALL
    s += smpc(0x19, 'reset_enabled')
    for i, value in enumerate([0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56]):
        s += put8(0x20100001 + 2*i, value)
    s += smpc(0x16, 'calendar_set') + status('overlap_done', True)
    s += read(0x20100063, 1) + read(0x2010005f, 1)
    expected += [0, 0x1a]
    s += status('readback')
    for i in range(8):
        s += read(0x20100021 + i*2, 1)
    expected += [0xc0, 0x20, 0x24, 0x4a, 0x31, 0x12, 0x34, 0x56]
    s += store(0x26020ff8, 2)
    s += read(0x25fe007c)
    expected += [0]
    s += store(0x26030000, 0x13579bdf) + store(0x26030004, 0x2468ace0)
    for off, value in [(0, 0x06030000), (4, 0x05e00000), (8, 4096), (12, 0x101), (20, 7), (16, 0x101)]:
        s += store(0x25fe0000 + off, value)
    s += read(0x25fe007c)
    s += poll(0x25fe007c, 0x10, 'dma_done', 'l', False)
    s += read(0x25e00000) + read(0x25e00004) + masked(0x25fe00a4, 0x800)
    # CPU external fetch/read waits behind the SCU burst; this snapshot is
    # after completion. Independent endpoint vectors inspect each busy phase.
    expected += [0, 0x13579bdf, 0x2468ace0, 0x800]
    # Unused X selectors 101 and 001 with RX, arithmetic and D1 effects.
    s += store(0x25fe0088, 0)
    s += 'load 0x100,r1\nmov.l @r1,r0\nload 0x25fe008c,r1\nmov.l r0,@r1\n'
    s += store(0x25fe0088, 64) + store(0x25fe008c, 6)
    s += store(0x25fe0080, 0x8000)
    for word in [0x02884000, 0x01020000, 0x18840000, 0x10043209, 0xf8000000]:
        s += store(0x25fe0084, word)
    s += store(0x25fe0080, 0x18000)
    s += poll(0x25fe00a4, 0x20, 'dsp_done', 'l')
    s += store(0x25fe0088, 128) + read(0x25fe008c)
    expected += [84]
    sound = sound_program()
    s += ('load 0x6000,r4\nload 0x25a00000,r5\nload 128,r6\ncopy_sound:\n'
          'mov.l @r4,r0\nmov.l r0,@r5\nadd #4,r4\nadd #4,r5\ndt r6\nbf copy_sound\n')
    s += 'load 0x104,r1\nmov.b @r1,r0\nload 0x2010001f,r1\nmov.b r0,@r1\n'
    s += poll(0x25a00804, 0xffff, 'sound_done', 'l')
    s += read(0x25a00800) + read(0x25a00804)
    expected += [544, 0x600d]
    s += store(0x26020ff8, 3)
    # Independent no-media startup, command response and end flags, then two
    # released periodic reports. No drive command invents media or ready data.
    s += put16(0x2589000c, 0x401) + store(0x25fe00a8, 1)
    commands = [([0x0401, 0, 0, 0], [0x07ff, 0xffff, 0xffff, 0xffff]),
                ([0x0100, 0, 0, 0], [0x07ff, 2, 0, 0]),
                ([0x0200, 0, 0, 0], [0x87ff, 0xffff, 0xffff, 0xffff]),
                ([0x0300, 0, 0, 0], [0x87ff, 0xffff, 0xffff, 0xffff]),
                ([0, 0, 0, 0], [0x07ff, 0xffff, 0xffff, 0xffff])]
    for n, (request, response) in enumerate(commands):
        s += put16(0x25890008, 0xffbe if n == 0 else 0xfffe)
        for i, value in enumerate(request):
            s += put16(0x25890018+i*4, value)
        s += poll(0x25890008, 1, f'cd{n}')
        for i in range(4):
            s += read(0x25890018+i*4, 2)
        s += 'load 0x25890008,r1\nmov.w @r1,r0\nmov #0x41,r2\nand r2,r0\n' + record()
        expected += response + [0x41]
    s += masked(0x25fe00a4, 0x10000)
    expected += [0x10000]
    for i in range(2):
        s += put16(0x25890008, 0xfbff) + poll(0x25890008, 0x400, f'periodic{i}')
        for j in range(4):
            s += read(0x25890018+j*4, 2)
        expected += [0x27ff, 0xffff, 0xffff, 0xffff]
    # The guest derives its visible back-screen color from its DSP result.
    s += 'load 0x26020060,r1\nmov.l @r1,r0\nload 0x8000,r2\nor r2,r0\nload 0x25e70000,r1\nmov.w r0,@r1\n'
    s += put16(0x25f800ac, 3) + put16(0x25f800ae, 0x8000) + put16(0x25f80000, 0x8000)
    s += 'load 0x600d,r0\n' + record() + store(0x26020ffc, 0x600d)
    s += 'park:\nsleep\nbra park\nnop\n' + command_window().replace(record(), '')
    operands = [(0x100, 0), (0x102, 7), (0x104, 0x0600)]
    operands += [(0x6000+i, int.from_bytes(sound[i:i+2], 'big')) for i in range(0, len(sound), 2)]
    files, digest = encode('bios-initialization', s, operands)
    files['bios-initialization.tsv'] = ''.join(f'{v:08X}\n' for v in expected + [0x600d]).encode()
    return files, digest


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    files, digest = build()
    for name, data in files.items():
        if args.check:
            assert (HERE/name).read_bytes() == data, f'Rebuild {name}'
        else:
            (HERE/name).write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} BIOS initialization; expanded SHA-256 {digest}')
