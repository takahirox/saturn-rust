#!/usr/bin/env python3
"""Original MIT interleaved/XA filesystem diagnostic and disc.

ISO9660 §§6.4.3/9.1, CD-ROM XA directory system use, ECMA-130 annex A,
ST-162 §§6.2/6.8/8.2.8. All payload, metadata, parity and guest code is original.
No SDK, emulator core, firmware, third-party media or captured golden output.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_cd_drive import command, store

HERE = Path(__file__).resolve().parent


def crc(data):
    value = 0
    for byte in data:
        value ^= byte
        for _ in range(8):
            value = (value >> 1) ^ (0xD8018001 if value & 1 else 0)
    return value


def raw_xa(payload, fad, number):
    sector = bytearray(2352)
    sector[1:11] = b'\xff' * 10
    sector[16:24] = bytes([number, 0, 8, 0]) * 2
    sector[24:2072] = payload
    struct.pack_into('<I', sector, 2072, crc(sector[16:2072]))
    # ECMA-130 annex A P/Q interleaves, with a zero header for XA Form 1.
    def times_x(v): return ((v << 1) ^ (0x11D if v & 128 else 0)) & 255
    inverse = {v ^ times_x(v): v for v in range(256)}
    for rows, columns, step, stride, output in [(86, 24, 2, 86, 2076), (52, 43, 86, 88, 2248)]:
        for row in range(rows):
            a = b = 0
            for column in range(columns):
                index = ((row // 2) * step + row % 2 + column * stride) % (rows * columns)
                symbol = sector[12 + index]
                a = times_x(a ^ symbol)
                b ^= symbol
            a = inverse[times_x(a) ^ b]
            sector[output + row] = a
            sector[output + rows + row] = a ^ b
    def bcd(v): return (v // 10) * 16 + v % 10
    sector[12:16] = bytes([bcd(fad // 4500), bcd(fad // 75 % 60), bcd(fad % 75), 2])
    return sector


def disc():
    iso = bytearray(64 * 2048)
    iso[16*2048:16*2048+7] = b'\x01CD001\x01'
    iso[17*2048:17*2048+7] = b'\xffCD001\x01'
    iso[16*2048+80:16*2048+88] = assembler.both32(64)
    iso[16*2048+128:16*2048+132] = assembler.both16(2048)
    root = assembler.record(b'\0', 20, 2048, True)
    iso[16*2048+156:16*2048+190] = root
    file = bytearray(assembler.record(b'STREAM.BIN;1', 21, 6144))
    file[1] = 1  # Extended-attribute sector precedes the first file unit.
    file[26:28] = bytes([2, 2])
    xa = bytes([0, 0, 0, 0, 0x28, 0, ord('X'), ord('A'), 7, 0, 0, 0, 0, 0])
    file[0] += len(xa)
    file += xa
    license = (HERE / 'LICENSE').read_bytes()
    directory = root + assembler.record(b'\1', 20, 2048, True) + file
    directory += assembler.record(b'LICENSE;1', 30, len(license))
    iso[20*2048:20*2048+len(directory)] = directory
    iso[21*2048:22*2048] = b'\xa5' * 2048  # Attributes must never be returned as payload.
    for lba, value in [(22, 0x11), (23, 0x22), (24, 0xEE), (25, 0xEE), (26, 0x33)]:
        iso[lba*2048:(lba+1)*2048] = bytes([value]) * 2048
    iso[30*2048:30*2048+len(license)] = license
    return b''.join(raw_xa(iso[lba*2048:(lba+1)*2048], 150+lba,
                           7 if lba in [22, 23, 26] else 9 if lba in [24, 25] else 0)
                    for lba in range(64))


def build():
    def cmd(words, name, offset=0): return command(words, name, 0x0600A000+offset)
    def save(reg, offset): return f'load {0x2600B000+offset:#x},r2\nmov.l r{reg},@r2\n'
    def eof(name):
        return f'load 0x25890008,r1\nload 0x200,r2\n{name}:\nmov.w @r1,r0\ntst r2,r0\nbt {name}\n'
    s = cmd([0x7000, 0, 0x00FF, 0xFFFF], 'root')
    s += cmd([0x7300, 0, 0, 2], 'info', 0x10)
    s += 'load 0x25818000,r1\n'
    for offset in [0, 4, 8]:
        s += 'mov.l @r1,r0\n' + save(0, offset)
    s += cmd([0x0600, 0, 0, 0], 'end_info')
    # A unique real instruction selects the first logical sector, independently
    # mutated to MOV #1,R6 in native and browser tests before reset.
    s += 'mov #0,r6\n' + store(0x25890008, 0xFDFE)
    s += store(0x25890018, 0x7400) + 'load 0x2589001c,r1\nmov.w r6,@r1\n'
    s += store(0x25890020, 0x0300) + store(0x25890024, 2)
    s += eof('first_eof')
    s += cmd([0x5100, 0, 0x0300, 0], 'first_count') + save(0, 12)
    s += 'mov r0,r7\n'
    s += cmd([0x6100, 0, 0x0300, 0xFFFF], 'get_first')
    s += ('load 0x25818000,r1\nmov #0,r4\nshll8 r7\nshll2 r7\n'
          'sum:\nmov.w @r1,r0\nextu.w r0,r0\nadd r0,r4\ndt r7\nbf sum\n') + save(4, 16)
    s += cmd([0x0600, 0, 0, 0], 'end_first')
    s += store(0x25890008, 0xFDFF)
    s += cmd([0x7400, 2, 0x0300, 2], 'read_last') + eof('last_eof')
    s += cmd([0x5100, 0, 0x0300, 0], 'last_count') + save(0, 20)
    s += cmd([0x6100, 0, 0x0300, 1], 'last_get')
    s += 'load 0x25818000,r1\nmov.l @r1,r0\n' + save(0, 24)
    s += cmd([0x0600, 0, 0, 0], 'last_end')
    # Observable filesystem-owned filter and connection configuration.
    s += cmd([0x4500, 0, 0x0300, 0], 'filter_mode', 0x20)
    s += cmd([0x4700, 0, 0x0300, 0], 'connectors', 0x30)
    s += cmd([0x3100, 0, 0, 0], 'device', 0x40)
    s += 'load 0x600d,r0\n' + save(0, 28) + 'sleep\n'
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    code = assembler.assemble(s, base=0x200)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original MIT XA/interleaved filesystem BIOS; adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{v:04X}' for v in struct.unpack('>8H', rom[at:at+16])))
    expected = [172, 6144, 0x02020728, 3, 0x1111*1024+0x2222*1024+0x3333*1024, 1, 0x33333333, 0x600D]
    return {'cd-filesystem.hex': ('\n'.join(lines)+'\n').encode(),
            'cd-filesystem.tsv': ''.join(f'{v:08X}\n' for v in expected).encode(),
            'cd-filesystem.bin': disc(),
            'cd-filesystem.cue': b'FILE "cd-filesystem.bin" BINARY\nTRACK 01 MODE2/2352\nINDEX 01 00:00:00\n'}


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
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
