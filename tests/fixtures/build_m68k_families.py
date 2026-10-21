#!/usr/bin/env python3
"""Reproduce original sound-bus architectural guest and authored expectations.

MIT: adjacent LICENSE. No SDK, firmware or third-party CPU implementation.
"""
import argparse
from pathlib import Path
from build_sound import build
from m68k_families import CASES

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    text, digest = build(families=True)
    text = text.replace('build_sound.py from sound.s','build_m68k_families.py / m68k_families.py from sound.s')
    expected = ''.join(f'{name}\t{value:08X}\t{flags:02X}\n' if flags is not None else f'{name}\t{value:08X}\t-\n' for name,_,value,flags in CASES)
    for name, data in [('m68k-families.hex',text),('m68k-families.tsv',expected)]:
        path = Path(__file__).with_name(name)
        if args.check:
            assert path.read_text() == data, f'Rebuild {name}'
        else:
            path.write_text(data)
    print(f'{"Verified" if args.check else "Built"} {len(CASES)} sound-bus vectors; expanded ROM SHA-256 {digest}')
