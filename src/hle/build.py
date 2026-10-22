#!/usr/bin/env python3
"""Build original independent reset and system-service ROMs; MIT: LICENSE."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
HERE=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('assembler',HERE.parents[1]/'homebrew/star-collector/build.py')
a=importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)

def build():
    return a.assemble((HERE/'boot.s').read_text(),base=0x100)

def system():
    sections = {}
    for line in (HERE/'system.s').read_text().splitlines():
        if line.startswith('.section '):
            address = int(line.split()[1], 0)
            sections[address] = []
        else:
            if sections:
                sections[address].append(line)
    # Fixed ABI targets make the source and independently written call fixtures
    # reviewable. These are original routines, not an SDK or firmware dump.
    for cause in range(32):
        address = 0x8000 + cause * 32
        sections[address] = [f'mov.l r0,@-r15\nmov #{cause},r0\nbra 0x9000\nnop']
    image = bytearray(0x5200)
    end = 0x4000
    for address, lines in sorted(sections.items()):
        code = a.assemble('\n'.join(lines), base=address)
        assert address >= end and address + len(code) <= 0x9200
        image[address-0x4000:address-0x4000+len(code)] = code
        end = address + len(code)
    return bytes(image)


def backup():
    sections = {}
    for line in (HERE/'backup.s').read_text().splitlines():
        if line.startswith('.section '):
            address = int(line.split()[1], 0)
            sections[address] = []
        elif sections:
            sections[address].append(line)
    image = bytearray(0x4000)
    entries = [0x10100,0x10200,0x10400,0x10600,0x10A00,
               0x10C00,0x10E00,0x11100,0x11300,0x11600]
    for i, entry in enumerate(entries):
        image[(i+1)*4:(i+2)*4] = entry.to_bytes(4,'big')
    end = 0x1002C
    for address, lines in sorted(sections.items()):
        code = a.assemble('\n'.join(lines), base=address)
        assert address >= end and address+len(code) <= 0x14000, hex(address)
        image[address-0x10000:address-0x10000+len(code)] = code
        end = address+len(code)
    return bytes(image)


def player():
    """Original guest CD player and original 5x7 glyph assets, all MIT."""
    sections = {}
    for line in (HERE/'player.s').read_text().splitlines():
        if line.startswith('.section '):
            address = int(line.split()[1], 0)
            sections[address] = []
        elif sections:
            sections[address].append(line)
    image = bytearray(0x5500)
    end = 0x18000
    for address, lines in sorted(sections.items()):
        code = a.assemble('\n'.join(lines), base=address)
        assert address >= end and address + len(code) <= 0x1D000
        image[address-0x18000:address-0x18000+len(code)] = code
        end = address+len(code)
    # Each source row is an original five-bit bitmap, not a system font.
    glyphs = {
        ' ': [0]*7,
        'A': [14,17,17,31,17,17,17], 'B': [30,17,17,30,17,17,30],
        'C': [14,17,16,16,16,17,14], 'D': [30,17,17,17,17,17,30],
        'E': [31,16,16,30,16,16,31], 'I': [14,4,4,4,4,4,14],
        'L': [16,16,16,16,16,16,31], 'N': [17,25,25,21,19,19,17],
        'P': [30,17,17,30,16,16,16], 'R': [30,17,17,30,20,18,17],
        'S': [15,16,16,14,1,1,30], 'T': [31,4,4,4,4,4,4],
        'U': [17,17,17,17,17,17,14], 'Y': [17,17,10,4,4,4,4],
    }
    for offset, text in [(0x5000,'DISC PLAYER'), (0x5020,'A PLAY  B PAUSE'),
                         (0x5040,'START PLAY PAUSE')]:
        assert set(text) <= glyphs.keys()
        image[offset:offset+len(text)+1] = text.encode('ascii')+b'\0'
    for char, rows in glyphs.items():
        offset = 0x5100 + ord(char)*7
        image[offset:offset+7] = bytes(rows)
    return bytes(image)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in [('boot',build()),('system',system()),('backup',backup()),('player',player())]:
        target=HERE/f'{name}.bin'
        if args.check: assert target.read_bytes()==data,f'Rebuild HLE {name}'
        else: target.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} HLE {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
