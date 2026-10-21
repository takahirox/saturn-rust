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

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in [('boot',build()),('system',system()),('backup',backup())]:
        target=HERE/f'{name}.bin'
        if args.check: assert target.read_bytes()==data,f'Rebuild HLE {name}'
        else: target.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} HLE {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
