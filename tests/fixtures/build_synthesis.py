#!/usr/bin/env python3
"""Reproduce the original executing SCSP slot-synthesis ROM. MIT: LICENSE.

SH-2 uploads/starts the independent 68k, which writes the waveforms, programs
three SCSP slots and handles timer IRQ. No firmware, SDK objects or cores.
"""
import argparse
from pathlib import Path
from build_sound import build

if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    text,digest=build(synthesis=True)
    text=text.replace('build_sound.py from sound.s','build_synthesis.py / build_sound.py from sound.s')
    path=Path(__file__).with_name('synthesis.hex')
    if args.check:
        assert path.read_text()==text,'Rebuild synthesis.hex'
    else:
        path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} synthesis.hex; expanded ROM SHA-256 {digest}')
