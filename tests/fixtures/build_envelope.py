#!/usr/bin/env python3
"""Reproduce the original executing SCSP slot-envelope ROM. MIT: LICENSE.

SH-2 uploads/starts the independent 68k, which writes the waveforms, programs
three SCSP slots and keys off after a timed interrupt. No firmware, SDK objects or cores.
"""
import argparse
from pathlib import Path
from build_sound import build

if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    text,digest=build(envelope=True)
    text=text.replace('build_sound.py from sound.s','build_envelope.py / build_sound.py from sound.s')
    path=Path(__file__).with_name('envelope.hex')
    if args.check:
        assert path.read_text()==text,'Rebuild envelope.hex'
    else:
        path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} envelope.hex; expanded ROM SHA-256 {digest}')
