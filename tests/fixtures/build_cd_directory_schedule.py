#!/usr/bin/env python3
"""Original MIT scheduled-directory guest and ISO. Adjacent LICENSE applies.

ST-162 §§3.2/6.2/8.2.8; SH7604 FRT measures actual guest time. No SDK,
firmware, captured output or host MMIO writes. ROM operands at 100/102/104
select navigation FID, initial command and mid-navigation AbortFile control.
"""
import argparse
import struct
from pathlib import Path
from build_scu import assembler
from build_cd_drive import command, store

HERE = Path(__file__).resolve().parent
BASE = 0x2600C000


def disc():
    iso = bytearray(32 * 2048)
    iso[16*2048:16*2048+7] = b'\1CD001\1'
    iso[17*2048:17*2048+7] = b'\xffCD001\1'
    iso[16*2048+80:16*2048+88] = assembler.both32(32)
    iso[16*2048+128:16*2048+132] = assembler.both16(2048)
    root = assembler.record(b'\0', 20, 6144, True)
    iso[16*2048+156:16*2048+190] = root
    records = {
        20: root + assembler.record(b'\1', 20, 6144, True) + assembler.record(b'CHILD', 24, 2048, True),
        21: assembler.record(b'FIRST;1', 26, 4),
        22: assembler.record(b'LAST;1', 27, 8),
        24: assembler.record(b'\0', 24, 2048, True) + assembler.record(b'\1', 20, 6144, True) + assembler.record(b'DATA;1', 28, 12),
        26: b'one!', 27: b'two!two!', 28: b'child-data!!',
        30: (HERE / 'LICENSE').read_bytes(),
    }
    for lba, data in records.items():
        iso[lba*2048:lba*2048+len(data)] = data
    return iso


def build():
    def save(offset, reg=0):
        return f'load {BASE+offset:#x},r2\nmov.l r{reg},@r2\n'
    def cmd(words, name, offset=0x100):
        return command(words, name, BASE + offset)
    def wait(name):
        return f'load 0x25890008,r1\nload 0x200,r2\n{name}:\nmov.w @r1,r0\ntst r2,r0\nbt {name}\n'
    def frc():
        return ('load 0xfffffe12,r1\nmov.b @r1,r0\nextu.b r0,r0\nshll8 r0\nmov r0,r4\n'
                'load 0xfffffe13,r1\nmov.b @r1,r0\nextu.b r0,r0\nor r4,r0\n')
    s = store(0xFFFFFEE0, 1) + 'mov #0,r0\nldc r0,sr\n'
    # FRT /128 avoids wrap over the entire five-sector root acquisition.
    s += 'load 0xfffffe16,r1\nmov #2,r0\nmov.b r0,@r1\n'
    s += store(0x25890008, 0xFDFF) + store(0x2589000C, 0)
    s += 'load 0x25fe00a8,r1\nmov #1,r0\nmov.l r0,@r1\n'
    s += frc() + 'mov r0,r8\n'
    s += store(0x25890008, 0xFFFE)
    s += 'load 0x102,r1\nmov.w @r1,r0\nload 0x25890018,r1\nmov.w r0,@r1\n'
    for at, v in [(0x1C, 0), (0x20, 255), (0x24, 65535)]:
        s += store(0x25890000+at, v)
    s += 'load 0x25890008,r1\naccept:\nmov.w @r1,r0\ntst #1,r0\nbt accept\n'
    s += 'load 0x200,r2\nand r2,r0\n' + save(0)  # must be zero at CMOK
    s += wait('root_done') + frc() + 'sub r8,r0\nextu.w r0,r0\n' + save(4)
    # A masked CD factor cannot reach SCU. Enabling its mask latches source16
    # while SCU still masks external IRQ delivery to the executing SH-2.
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x10000,r2\nand r2,r0\n' + save(0x1C)
    s += store(0x2589000C, 0x200)
    s += 'load 0x25fe00a4,r1\nmov.l @r1,r0\nload 0x10000,r2\nand r2,r0\n' + save(8)
    s += 'load 0x25fe00a0,r1\nload 0x3fff,r0\nmov.l r0,@r1\n'
    s += f'load {BASE+12:#x},r1\nirq_done:\nmov.l @r1,r0\ntst #1,r0\nbt irq_done\n'
    s += 'load 0x25890008,r1\nmov.w @r1,r0\nload 0x200,r2\nand r2,r0\n' + save(0x18)
    s += store(0x2589000C, 0)  # subsequent completion polled independently
    s += cmd([0x7200, 0, 0, 0], 'root_scope', 0x20)
    s += store(0x25890008, 0xFDFF)
    s += store(0x25890018, 0x7000) + store(0x2589001C, 0) + store(0x25890020, 0)
    s += 'load 0x100,r1\nmov.w @r1,r0\nload 0x25890024,r1\nmov.w r0,@r1\n'
    s += 'load 0x25890008,r1\nnav_accept:\nmov.w @r1,r0\ntst #1,r0\nbt nav_accept\n'
    s += 'load 0x104,r1\nmov.w @r1,r0\ntst #1,r0\nbf do_abort\nbra navigation_wait\nnop\ndo_abort:\n'
    # 240000 reference clocks, after the first root directory sector and before
    # the second, measured by guest FRT rather than host advancement callbacks.
    s += frc() + 'mov r0,r8\nabort_delay:\n' + frc()
    s += 'sub r8,r0\nextu.w r0,r0\nload 1875,r2\ncmp/hs r2,r0\nbf abort_delay\n'
    s += cmd([0x7500, 0, 0, 0], 'abort')
    s += 'navigation_wait:\n' + wait('nav_done')
    s += cmd([0x7200, 0, 0, 0], 'nav_scope', 0x40)
    s += cmd([0x7300, 0, 0, 2], 'nav_info', 0x60)
    s += 'load 0x25818000,r1\nmov.l @r1,r0\n' + save(0x10)
    s += cmd([0x0600, 0, 0, 0], 'end_info')
    s += 'load 0x600d,r0\n' + save(0x14) + 'park:\nsleep\nbra park\nnop\n'
    handler = 'mov.l r0,@-r15\nmov.l r1,@-r15\nmov.l r2,@-r15\n'
    handler += store(0x25890008, 0xFDFF) + 'mov #1,r0\n' + save(12)
    handler += 'mov.l @r15+,r2\nmov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    struct.pack_into('>HHH', rom, 0x100, 2, 0x7000, 0)
    struct.pack_into('>I', rom, 0x50*4, 0x6000)
    for at, source in [(0x200, s), (0x6000, handler)]:
        code = assembler.assemble(source, base=at)
        rom[at:at+len(code)] = code
    lines = ['# Original MIT scheduled-directory guest; adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at+16])))
    return {'cd-directory-schedule.hex': ('\n'.join(lines)+'\n').encode(),
            'cd-directory-schedule.iso': bytes(disc())}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, data in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == data, f'Rebuild {name}'
        else:
            path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}')
