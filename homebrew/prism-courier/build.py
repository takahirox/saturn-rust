#!/usr/bin/env python3
"""Reproducible original Saturn disc; MIT: adjacent LICENSE.

Uses only this repository's original assembler/ISO writer and independently
authored sound guest. No Sega SDK, executable, firmware or third-party artwork.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUTPUT = ROOT / 'web/prism-courier.iso'


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


assembler = module('assembler', ROOT / 'homebrew/star-collector/build.py')
sound = module('sound', ROOT / 'tests/fixtures/build_sound.py')


def build():
    image = bytearray(0x7400)
    code = assembler.assemble((HERE/'game.s').read_text())
    assert len(code) < 0x2000
    image[:len(code)] = code
    handler = assembler.assemble((HERE/'draw-end.s').read_text(),base=0x06006800)
    assert len(handler) < 0x800
    image[0x2800:0x2800+len(handler)] = handler
    commands = [[0] * 16 for _ in range(8)]
    commands[0][0] = 9
    commands[0][10:12] = [319, 223]
    commands[1][0] = 10
    for i, (x0,y0,x1,y1,color) in enumerate([
        (148,40,164,90,0x801F), (148,134,164,184,0x801F),
        (280,96,296,128,0x83E0), (88,16,232,32,0x8421)
    ]):
        at = i+2 if i<3 else 6
        commands[at][0:4] = [4,0,0xC0,color]
        commands[at][6:14] = [x0,y0,x1,y0,x1,y1,x0,y1]
    commands[5][0:8] = [0,0,0xA8,0,0x200,0x108,24,112]
    commands[7][0] = 0x8000
    for i, cmd in enumerate(commands):
        struct.pack_into('>16H',image,0x2000+i*32,*cmd)
    for y in range(8):
        for x in range(8):
            color = 0xFFE0 if abs(x-3)+abs(y-3)<5 else 0
            struct.pack_into('>H',image,0x2400+(y*8+x)*2,color)
    audio = sound.sound_image()
    image[0x3000:0x3000+len(audio)] = audio
    return assembler.build(bytes(image),(HERE/'LICENSE').read_bytes(),
                           'PRISM COURIER','PRISM-001 V1.000')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args = parser.parse_args()
    image = build()
    if args.check:
        assert OUTPUT.read_bytes() == image, 'Rebuild Prism Courier ISO'
    else:
        OUTPUT.write_bytes(image)
    print(f'{"Verified" if args.check else "Built"} {OUTPUT.name}; SHA-256 {hashlib.sha256(image).hexdigest()}')
