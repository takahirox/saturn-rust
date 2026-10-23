#!/usr/bin/env python3
"""Original MIT SCU byte-fringe guest. ST-097 §2.1 fig.2.1; H §6.3.4.

External UBC address comparisons distinguish individual fringe bytes from an
incorrect word cycle, even when both copies produce identical RAM contents.
SH-2 instructions initialize data, configure DMA/UBC, and execute both handlers.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record

HERE = Path(__file__).resolve().parent


def build():
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x1d0, 0x060ff000)
    struct.pack_into('>I', rom, 12 * 4, 0x60000)
    for channel in range(3):
        struct.pack_into('>I', rom, (0x4b-channel)*4, 0x60100)
    s = 'load 0x2600d000,r8\nload 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n' + store(0xffffffe4,0xa55a00bc)
    # Manual framebuffer change, no FCT: VBlank must not erase a DMA
    # source while this long diagnostic crosses multiple fields.
    setup = 'load 0x25d00002,r1\nmov #2,r0\nmov.w r0,@r1\nload 0x200,r1\njmp @r1\nnop\n'
    # Source-authored data, also used by operand-mutation controls.
    for source_base in [0x06008000,0x05a08000,0x060ffff8,0x05cbfff8]:
        s += store(source_base | 0x20000000, 0x12345678) + store((source_base+4) | 0x20000000, 0x9abcdef0)
    expected = []
    for channel in range(3):
        for indirect in [False, True]:
            for source_base in [0x06008000,0x05a08000,0x060ffff8,0x05cbfff8]:
                for destination_base in [0x06009000,0x05a09000]:
                    for case, (offset, count, watched) in enumerate([(2, 6, 3), (0, 2, 1), (0, 6, 5), (1, 3, 3), (0, 4, 2)]):
                        tag = f'{channel}_{int(indirect)}_{source_base:x}_{destination_base:x}_{case}'
                        source, destination = source_base+offset, destination_base+offset
                        s += 'load 0xf0,r0\nldc r0,sr\nmov #0,r10\nmov #0,r11\n'
                        for address, value in [(destination_base | 0x20000000, 0xeeeeeeee), ((destination_base+4) | 0x20000000, 0xeeeeeeee),
                                               (0xffffff40, source_base+watched),
                                               (0xffffff60, destination_base+watched)]:
                            s += store(address, value)
                        s += ('load 0xffffff48,r1\nload 0xa4,r0\nmov.w r0,@r1\n'
                              'load 0xffffff68,r1\nload 0xa8,r0\nmov.w r0,@r1\n'
                              'load 0xffffff78,r1\nload 0x3000,r0\nmov.w r0,@r1\nmov.w @r1,r0\n')
                        s += store(0x25fe00a0, 0xbfff & ~(1 << (11-channel)))
                        if indirect:
                            s += store(0x2600a000, count) + store(0x2600a004, destination) + store(0x2600a008, source | 0x80000000)
                        for off, value in [(0, source), (4, 0x0600a000 if indirect else destination),
                                           (8, count), (12, 0x101 if destination_base < 0x06000000 else 0x102),
                                           (20, 0x01010107 if indirect else 0x00010107), (16, 0x101)]:
                            s += store(0x25fe0000+channel*32+off, value)
                        # Hold CPU interrupts until both DMA comparisons have occurred.
                        s += (f'load 0x25fe00a4,r1\nload {1 << (11-channel):#x},r2\n'
                              f'complete_{tag}:\nmov.l @r1,r0\ntst r2,r0\nbt complete_{tag}\n'
                              'load 0xffffff78,r1\nmov.w @r1,r0\nextu.w r0,r0\nload 0x4040,r2\nand r2,r0\n') + record()
                        s += ('mov #0,r0\nldc r0,sr\n'
                              f'irq_{tag}:\nmov r11,r0\ntst r0,r0\nbt irq_{tag}\n') + record(10) + record(11)
                        for address in [destination_base | 0x20000000, (destination_base+4) | 0x20000000]:
                            s += f'load {address:#x},r1\nmov.l @r1,r0\n' + record()
                        # Literal golden bytes, independent of emulated execution.
                        result = bytearray(b'\xee'*8)
                        result[offset:offset+count] = bytes.fromhex('123456789abcdef0')[offset:offset+count]
                        expected += [0x4040, 1, 1, *struct.unpack('>II', result)]
    s += 'load 0x600d,r0\n' + record() + 'done:\nsleep\nbra done\nnop\n'
    handlers = {
        0x60000: ('mov.l r0,@-r15\nmov.l r1,@-r15\nadd #1,r10\n'
                 'load 0xffffff48,r1\nmov #0,r0\nmov.w r0,@r1\n'
                 'load 0xffffff68,r1\nmov.w r0,@r1\n'
                 'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'),
        0x60100: 'add #1,r11\nrte\nnop\n',
    }
    for at, text in [(0x1d0, setup), (0x200, s), *handlers.items()]:
        code = assembler.assemble(text, base=at)
        assert len(code) < (0x5fe00 if at == 0x200 else 0x100)
        rom[at:at+len(code)] = code
    lines = ['# Original SCU byte-fringe/UBC BIOS; MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return {'scu-fringe.hex': ('\n'.join(lines)+'\n').encode(),
            'scu-fringe.tsv': ''.join(f'{v:08X}\n' for v in [*expected, 0x600d]).encode()}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, content in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
