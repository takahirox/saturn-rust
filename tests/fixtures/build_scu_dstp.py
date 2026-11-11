#!/usr/bin/env python3
"""Original MIT initialization/DSTP diagnostics; no firmware or copied emulator code.

Final SCU compatibility policy: +60 longword read=0, writes ignored. The cache
negative control loads +6C, generating +60 before failing honestly at +64.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record
from build_smpc import byte, issue, idle
from build_clock import read
from build_dsp_dma import data as dsp_data, start as dsp_start, wait as dsp_wait

HERE = Path(__file__).resolve().parent


def build():
    result = {}
    # Independent initialization guest, not extracted from private firmware.
    # System Program SYS_SETSCUIM/CHGSCUIM and VDP2 §§3.1/16.
    s = 'load 0x2600d000,r8\nload 0x25fe00a4,r1\n'
    s += 'mov #4,r2\npending:\nmov.l @r1,r0\ntst #4,r0\nbt pending\nand r2,r0\n' + record()
    s += 'mov #-1,r0\nmov.l r0,@r1\nmov.l @r1,r0\nand r2,r0\n' + record()
    s += 'mov #-5,r0\nmov.l r0,@r1\nmov.l @r1,r0\nand r2,r0\n' + record()
    s += 'again:\nmov.l @r1,r0\ntst #4,r0\nbt again\nand r2,r0\n' + record()
    s += 'mov #0,r0\nmov.l r0,@r1\nmov.l @r1,r0\nand r2,r0\n' + record()
    for offset in [6, 0xC, 0xFE]:
        s += f'load {0x25f80000+offset:#x},r1\nload 0x7fff,r0\nmov.w r0,@r1\nmov.w @r1,r0\n' + record()
    for offset in [4, 8, 0xA]:
        s += f'load {0x25f80000+offset:#x},r1\nmov #0,r0\nmov.w r0,@r1\n'
    # Control arrives during status acquisition, with SF already claimed.
    # It must run after the current status, without an orphaned continuation.
    s += issue(0x19) + idle('enable_done') + byte(0x100063, 1)
    for i, value in enumerate([1, 8, 0xf0]):
        s += byte(0x100001+i*2, value)
    s += byte(0x10001f, 0x10) + byte(0x2010001f, 0x1a) + idle('control_done')
    s += read(0x10005f, 1)
    s += byte(0x100063, 1)
    for i, value in enumerate([1, 0, 0xf0]):
        s += byte(0x100001+i*2, value)
    s += byte(0x10001f, 0x10) + idle('reset_status')
    # STE survives cold reset after the first run; isolate the reset-disable bit.
    s += 'load 0x100021,r1\nmov.b @r1,r0\nmov #64,r2\nand r2,r0\n' + record()
    s += byte(0x100063, 1)
    for i, value in enumerate([0x20, 0x24, 0x11, 1, 0, 0, 0]):
        s += byte(0x100001+i*2, value)
    s += byte(0x10001f, 0x16) + idle('time_done') + byte(0x100063, 1)
    for i, value in enumerate([1, 0, 0xf0]):
        s += byte(0x100001+i*2, value)
    s += byte(0x10001f, 0x10) + idle('time_status')
    for address in [0x100021, 0x100023, 0x100025]:
        s += read(address, 1)
    s += store(0x26008000, 0x12345678) + store(0x26008004, 0x9abcdef0)
    s += store(0x25fe00a4, 0)
    for offset, value in [(0, 0x06008000), (4, 0x06009000), (8, 4096), (12, 0x102), (20, 7), (16, 0x101)]:
        s += store(0x25fe0000+offset, value)
    # CPU arbitration can defer the first status read until DMA completes.
    # Native passive-observer vectors separately require the active bits.
    s += 'load 0x25fe007c,r1\nmov #16,r2\n'
    s += 'dma_wait:\nmov.l @r1,r0\ntst #16,r0\nbf dma_wait\nand r2,r0\n' + record()
    s += read(0x26009000) + read(0x26009004)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x800,r2\nand r2,r0\n' + record()
    # Independently composed parallel fields: inactive P selector, live
    # ADD/writeback/D1 and RX/Y loads. The final product proves RX was loaded.
    alias1 = (4 << 26) | (1 << 23) | (2 << 17) | 0x3209
    alias5 = (4 << 26) | (5 << 23) | (6 << 17) | 0x3209
    s += dsp_data(0, [3]) + dsp_start([0x94000005, 0x00020000, alias1, alias5,
                                    0x01000000, 0x10043209, 0xf0000000])
    s += dsp_wait('dsp_done') + store(0x25fe0088, 128)
    for _ in range(3):
        s += read(0x25fe008c)
    # Original 68k clears MADRS plus its unused trailing gap, then supplies
    # a mailbox acknowledgement. No BIOS/sound-driver code is copied.
    sound = bytes.fromhex('46fc2700 7000 207c00100780 721f 20c0 51c9fffc 207c00100000 323c00ff 20c0 51c9fffc 23fc1234567800000700 4e722700')
    s += store(0x25a00000, 0x10000) + store(0x25a00004, 0x100) + store(0x25a00700, 0)
    for at in range(0, len(sound), 4):
        s += store(0x25a00100+at, int.from_bytes(sound[at:at+4].ljust(4, b'\0'), 'big'))
    s += 'load 0x25b0077e,r1\nload 0x5ab8,r0\nmov.w r0,@r1\n'
    s += issue(6) + idle('sound_on')
    s += 'load 0x25a00700,r1\nload 0x12345678,r2\nack:\nmov.l @r1,r0\ncmp/eq r2,r0\nbf ack\n' + record()
    for at in [0x25b0077e, 0x25b007be, 0x25b007c0]:
        s += read(at, 2)
    s += 'load 0x600d,r0\n' + record() + 'done:\nsleep\nbra done\nnop\n'
    image = bytearray(524288)
    struct.pack_into('>II', image, 0, 0x20000200, 0x260ff000)
    data = assembler.assemble(s, base=0x200)
    image[0x200:0x200+len(data)] = data
    lines = ['# Original MIT initialization guest; adjacent LICENSE.']
    for at in range(0, len(image), 16):
        if any(image[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', image[at:at+16])))
    result['bios-startup.hex'] = ('\n'.join(lines)+'\n').encode()
    result['bios-startup.asm'] = s.encode()
    result['bios-startup.tsv'] = ''.join(f'{v:08X}\n' for v in [4, 4, 0, 4, 0, 0, 0, 0, 0x1a, 0x40, 0xc0, 0x20, 0x24, 0, 0x12345678, 0x9abcdef0, 0x800, 5, 10, 19, 0x12345678, 0x5ab8, 0, 0, 0x600d]).encode()
    for name in ['access', 'refill', 'read', 'write']:
        sections = {}
        s = 'load 0x2600d000,r8\n'
        expected = []
        if name == 'access':
            # P1 execution prevents unrelated instruction refills. CE=0 covers
            # ordinary P0 alias; CE=1/CD=1 tests explicit data-fill disable.
            for ccr in [0, 0x15]:
                s += f'load 0xfffffe92,r1\nmov #{ccr},r0\nmov.b r0,@r1\n'
                for address in [0x05fe0060, 0x25fe0060]:
                    for value in [0, 1, 0xffffffff, 0xa55a1234]:
                        s += store(address, value)
                        s += f'load {address:#x},r1\nmov.l @r1,r0\n' + record()
                        expected.append(0)
            # Each DMA completion must still deliver the real handler and data.
            s += 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\nmov #0,r0\nldc r0,sr\n' + store(0x26008000, 0x12345678)
            for channel in range(3):
                s += 'mov #0,r10\n'
                s += store(0x25fe00a0, 0xbfff & ~(1 << (11-channel)))
                for off, value in [(0, 0x06008000), (4, 0x06009000+channel*4),
                                   (8, 4), (12, 0x102), (20, 7), (16, 0x101)]:
                    s += store(0x25fe0000+channel*32+off, value)
                s += store(0x25fe0060, 1)
                s += (f'wait_{channel}:\nmov r10,r0\ntst r0,r0\nbt wait_{channel}\n'
                      f'load {0x26009000+channel*4:#x},r1\nmov.l @r1,r0\n') + record() + record(10)
                expected += [0x12345678, 1]
            s += 'load 0x600d,r0\n' + record()
            expected.append(0x600d)
        elif name == 'refill':
            s += 'load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\nload 0x05fe006c,r1\nmov.l @r1,r0\n'
        elif name == 'read':
            s += 'load 0x25fe0060,r1\nmov.l @r1,r0\n' + record()
            expected.append(0)
        else:
            s += ('load 0x25fe0060,r1\nload 0xa55a1234,r0\nmov #100,r2\n'
                  'repeat:\nmov.l r0,@r1\ndt r2\nbf repeat\n')
            s += 'load 0x600d,r0\n' + record()
            expected.append(0x600d)
        s += 'done:\nsleep\nbra done\nnop\n'
        sections[0x200] = s
        sections[0x1800] = 'add #1,r10\nrte\nnop\n'
        rom = bytearray(524288)
        struct.pack_into('>II', rom, 0, 0x20000200, 0x260ff000)
        for vector in [0x49, 0x4a, 0x4b]:
            struct.pack_into('>I', rom, vector*4, 0x20001800)
        for at, source in sections.items():
            data = assembler.assemble(source, base=at)
            assert at != 0x200 or len(data) < 0x1600
            rom[at:at+len(data)] = data
        lines = ['# Original MIT legacy DSTP firmware; adjacent LICENSE.']
        for at in range(0, len(rom), 16):
            if any(rom[at:at+16]):
                lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
        result[f'scu-dstp-{name}.hex'] = ('\n'.join(lines)+'\n').encode()
        result[f'scu-dstp-{name}.asm'] = ''.join(f'; physical section {at:08X}\n{source}' for at, source in sections.items()).encode()
        if expected:
            result[f'scu-dstp-{name}.tsv'] = ''.join(f'{v:08X}\n' for v in expected).encode()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
