#!/usr/bin/env python3
"""Build the original MIT DMA throughput guest using the project assembler."""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[1]


def build():
    spec = importlib.util.spec_from_file_location(
        'runtime_dmac_assembler', ROOT / 'homebrew/star-collector/build.py')
    assembler = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(assembler)
    source = Path(__file__).with_name('runtime-dmac-stream.s').read_text()
    data = bytearray(524288)
    struct.pack_into('>IIII', data, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    code = assembler.assemble(source, base=0x200)
    assert 0x200 + len(code) < 0x2000
    data[0x200:0x200 + len(code)] = code
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    args = parser.parse_args()
    data = build()
    args.binary.parent.mkdir(parents=True, exist_ok=True)
    args.binary.write_bytes(data)
    print(f'Built original DMA stream; SHA-256 {hashlib.sha256(data).hexdigest()}')


if __name__ == '__main__':
    main()
