#!/usr/bin/env python3
"""Original MIT SH-2 mosaic guests, adjacent LICENSE. Sega VDP2 §§2.2/4.11.

Word100=TVMD,102=source seed,104=MZCTL,106=RBG0 A/B (mode 0 for RBG1).
Guest creates all source rows, palette, names, matrices and progress records.
No SDK, personal firmware, captured emulator results or host pixel injection.
"""
import argparse
import hashlib
from pathlib import Path
from build_clock import put16, read
from build_scu import store
from build_hle_system import record
from build_video_transactions import encode, blank

HERE = Path(__file__).resolve().parent


def build():
    files = {}
    for mode in ['normal', 'rotation', 'rbg1']:
        normal = mode == 'normal'
        dual = mode == 'rbg1'
        s = '; Original MIT mosaic density and size diagnostic.\n'
        # No PLL change is needed for standard 320-dot mode.
        for at, value in [(0xE,0 if normal else 0x30e if dual else 0x10c),
                          (0x10,0x4444),(0x12,0xeeee),(0x18,0x5555),(0x1a,0xeeee),
                          (0x28,0x1212 if normal else 0x20),
                          (0x2a,0x2000 if dual else 0x3200),
                          (0x3c,0x20 if normal else 0),
                          (0x3e,0x10 if dual else 0x11),
                          (0x60,0x1818 if dual else 0),
                          (0xf8,0x102 if normal else 3),(0xfc,2),
                          (0x78,1),(0x7c,1),(0x88,1),(0x8c,1),
                          (0xbc,3),(0xbe,0x8000),(0xac,3),(0xae,0xf800)]:
            s += put16(0x25f80000+at,value)
        if not normal:
            for base in [0x25e70000,0x25e70080]:
                for off in [0x10,0x14,0x1c,0x2c,0x4c,0x50]:
                    s += store(base+off,0x10000)
        # Palette index n is literal RGB555 n. Color zero is transparent.
        s += ('load 0x25f00000,r3\nload 256,r2\nmov #0,r4\n'
              'palette:\nmov.w r4,@r3\nadd #2,r3\nadd #1,r4\ndt r2\nbf palette\n')
        if dual:
            # Four columns x eight rows of 8x8, 11-bit palette characters.
            for name_base,char_base in [(0x25e00000,0x1000),(0x25e60000,0x2000)]:
                s += f'load {name_base:#x},r3\nload {char_base:#x},r4\nmov #8,r5\n'
                label = 'b' if name_base == 0x25e60000 else 'a'
                s += (f'names_{label}:\nmov #4,r2\nname_{label}:\nmov #0,r0\nmov.w r0,@r3\n'
                      f'add #2,r3\nmov.w r4,@r3\nadd #2,r3\nadd #4,r4\ndt r2\nbf name_{label}\n'
                      f'load 240,r0\nadd r0,r3\ndt r5\nbf names_{label}\n')
        # Each source row differs. Normal NBG0 contains 16 colored columns;
        # NBG1 has 32, exposing an independently double-density companion.
        s += 'load 0x102,r1\nmov.w @r1,r6\nmov #0,r9\n'
        for surface,base in enumerate([0x25e00000,0x25e40000] if normal else [0x25e20000,0x25e40000] if dual else [0x25e20000]):
            s += f'load {base:#x},r3\nmov #0,r5\n'
            s += f'row_{surface}:\nmov #0,r2\n'
            s += f'dot_{surface}:\nmov r5,r0\nshll r0\nadd r5,r0\nadd r2,r0\nadd r6,r0\n'
            if normal and surface == 0:
                s += f'mov #16,r1\ncmp/hs r1,r2\nbf colored_{surface}\nmov #0,r0\ncolored_{surface}:\n'
            if not normal and not dual:
                s += 'load 0x8000,r1\nor r1,r0\n'
            s += 'add r0,r9\n' + ('mov.b r0,@r3\nadd #1,r3\n' if normal else 'mov.w r0,@r3\nadd #2,r3\n')
            if dual:
                # Walk character row fragments, then advance the cell row.
                s += f'mov r2,r0\nmov #7,r1\nand r1,r0\nmov #7,r1\ncmp/eq r1,r0\nbf same_{surface}\nload 112,r0\nadd r0,r3\nsame_{surface}:\n'
            s += f'add #1,r2\nmov #32,r1\ncmp/eq r1,r2\nbf dot_{surface}\n'
            if dual:
                s += f'load 0xfffffe10,r0\nadd r0,r3\nmov r5,r0\nmov #7,r1\nand r1,r0\nmov #7,r1\ncmp/eq r1,r0\nbf next_{surface}\nload 384,r0\nadd r0,r3\nnext_{surface}:\n'
            else:
                s += f'load {480 if normal else 960},r0\nadd r0,r3\n'
            s += f'add #1,r5\nmov #64,r1\ncmp/eq r1,r5\nbf row_{surface}\n'
        for operand,at in [(0x104,0x22),(0x106,0xb0),(0x100,0)]:
            if at == 0:
                s += put16(0x25f80020,3 if normal else 0x30 if dual else 0x10)
            s += f'load {operand:#x},r1\nmov.w @r1,r0\nload {0x25f80000+at:#x},r1\nmov.w r0,@r1\n'
        s += 'load 0x2600dc00,r8\nmov r9,r0\n' + record()
        for at,width in [(0x25f80000,2),(0x25f80022,2),(0x25f80020,2),(0x25f800b0,2)]:
            s += read(at,width)
        # Observe both field transitions in guest code, not a host success write.
        s += blank('first') + read(0x25f80004,2)
        s += blank('second') + read(0x25f80004,2)
        s += 'load 0x600d,r0\n' + record() + 'park:\nsleep\nbra park\nnop\n'
        outputs,digest = encode('mosaic-'+mode,s,[(0x100,0x80c0),(0x102,1),(0x104,1 if normal or dual else 0x10),(0x106,0)])
        files.update(outputs)
        files['mosaic-'+mode+'.sha256'] = (digest+'\n').encode()
    return files


if __name__ == '__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
