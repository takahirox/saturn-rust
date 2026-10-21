#!/usr/bin/env python3
"""Original executing 68k SCSP DMA/MIDI diagnostic. MIT: adjacent LICENSE."""
import argparse
from pathlib import Path
from build_sound import build
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    text,digest=build(peripherals=True)
    text=text.replace('build_sound.py from sound.s','build_sound_io.py / build_sound.py from sound.s')
    path=Path(__file__).with_name('sound-io.hex')
    if args.check:assert path.read_text()==text,'Rebuild sound-io.hex'
    else:path.write_text(text)
    print(f'{"Verified" if args.check else "Built"} sound-io.hex; expanded ROM SHA-256 {digest}')
