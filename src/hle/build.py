#!/usr/bin/env python3
"""Original independent reset trampoline; MIT: adjacent LICENSE."""
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

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    data=build();target=HERE/'boot.bin'
    if args.check: assert target.read_bytes()==data,'Rebuild HLE trampoline'
    else: target.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} HLE reset trampoline; SHA-256 {hashlib.sha256(data).hexdigest()}')
