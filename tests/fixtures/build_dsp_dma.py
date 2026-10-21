#!/usr/bin/env python3
"""Original MIT guest SCU DSP DMA programs; ST-097 §§4.3–4.5, ST-210 #27.

No SDK/firmware/emulator dependencies. SH-2 uploads register-count DMA and
loads another DSP program from work RAM using DSP program-RAM DMA.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record

HERE = Path(__file__).resolve().parent

# RA0=source/4; DMA1 D0,M1,MC0; wait T0; MOV MC0,MC2;
# CT1=0; WA0=destination/4; DMA0 M1,D0,3; ENDI (DMA continues).
TRANSFER = [0x99801400, 0xC000A104, 0xD3400002, 0,
            0x00003204, 0x00001D00, 0x9D801800, 0xC0001103, 0xF8000000]
# CT0=0; RA0=code/4; DMAH1 D0,PRG,M0; END. SH-2 waits for EX=T0=0,
# then starts the actual DMA-loaded program at zero through SCU MMIO.
BOOTSTRAP = [0x00001C00, 0x99801C00, 0xC000E400, 0xF0000000]
PAYLOAD = [0x8400002A, 0xF8000000]  # MVI #42,MC1; ENDI.


def data(address, values):
    return store(0x25FE0088, address) + ''.join(store(0x25FE008C, v) for v in values)


def start(words):
    return (store(0x25FE0080, 0x8000)
            + ''.join(store(0x25FE0084, word) for word in words)
            + store(0x25FE0080, 0x18000))


def wait(label):
    return ('load 0x25fe0080,r1\nload 0x810000,r2\n'
            f'{label}:\nmov.l @r1,r0\ntst r2,r0\nbf {label}\n')


def read(address):
    return f'load {address:#x},r1\nmov.l @r1,r0\n' + record()


def build():
    s = 'load 0x2600e400,r8\n'
    for i, value in enumerate([7, 11, 17]):
        s += store(0x26005000 + i * 4, value)
    s += store(0x26006004, 0xABCDDCBA)
    s += data(0, [3, 0x1234]) + start(TRANSFER) + wait('wait_data')
    s += read(0x26006000) + read(0x26006004)
    s += store(0x25FE0088, 128) + read(0x25FE008C)
    # Input DMA has advanced CT1 to 3, output DMA then to 3 after the reset.
    # Postincremented count selection must leave MC0 at the following 0x1234.
    for i, word in enumerate(PAYLOAD):
        s += store(0x26007000 + i * 4, word)
    s += data(0, [len(PAYLOAD)]) + start(BOOTSTRAP) + wait('wait_program')
    s += 'mov r0,r6\nload 0xff,r2\nand r2,r6\n' + record(6)
    # The loaded MVI writes at its retained CT1=3, observable at address 67.
    s += store(0x25FE0080, 0x18000) + wait('wait_loaded')
    s += 'mov r0,r6\nload 0xff,r2\nand r2,r6\n' + record(6)
    s += store(0x25FE0088, 67) + read(0x25FE008C)
    s += 'load 0x600d,r0\n' + record()
    s += 'finished:\nsleep\nbra finished\nnop\n'
    code = assembler.assemble(s, base=0x200)
    rom = bytearray(524288)
    struct.pack_into('>II', rom, 0, 0x200, 0x060FF000)
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original SCU DSP register/program/fixed DMA BIOS. MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return {'dsp-dma.hex': ('\n'.join(lines)+'\n').encode(),
            'dsp-dma.tsv': b'00000011\nABCDDCBA\n00001234\n00000004\n00000002\n0000002A\n0000600D\n'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    for name, content in build().items():
        path = HERE / name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
