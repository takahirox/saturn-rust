#!/usr/bin/env python3
"""Original MIT rotation image diagnostic; adjacent LICENSE. No SDK/firmware.

ST-058-R2 §§3.3/4.4–4.9/6.1–6.3; source data at100/102/104
controls initial word, H-counter threshold and write/omit respectively;
106 controls a guest delay loop. The cached poll/store block is aligned explicitly.
"""
import argparse
import hashlib
from pathlib import Path
from build_clock import put16
from build_scu import assembler, store
from build_vdp2_fetch import line
from build_video_transactions import encode, blank

HERE = Path(__file__).resolve().parent


def build():
    files = {}
    for mode in ['tile', 'bitmap', 'rbg1']:
        dual = mode == 'rbg1'
        image = 0x25e40000 if dual else 0x25e20000
        s = '; Original MIT rotation image deadline diagnostic.\n'
        # Repeated x=y=0. RBG0 tile/bitmap uses A; RBG1 tile uses B0/B1.
        for base in [0x25e70000, 0x25e70080]:
            for off, value in [(0x1c,0x10000),(0x2c,0x10000),(0x4c,0x10000),(0x50,0x10000)]:
                s += store(base+off,value)
        for at,value in [(0xe,0x30e if dual else 0x10e),(0x20,0x30 if dual else 0x10),
                         (0x38,0x8004),(0x30,0x8008),(0x3e,0x10 if dual else 0),
                         (0x60,0x3030),(0x2a,0x200 if mode == 'bitmap' else 0),
                         (0xfc,2),(0xf8,3),(0xbc,3),(0xbe,0x8000)]:
            s += put16(0x25f80000+at,value)
        if mode == 'bitmap':
            s += put16(0x25f8003e,1)
        for index,value in [(1,0x801f),(2,0x83e0),(3,0xfc00)]:
            s += put16(0x25f00000+index*2,value)
        s += put16(0x25e00000,0) + put16(0x25e60000,0)
        s += 'load 0x100,r1\nmov.w @r1,r0\n' + f'load {image:#x},r1\nmov.w r0,@r1\n'
        s += 'load 0xfffffe92,r1\nmov #1,r0\nmov.b r0,@r1\n'
        s += put16(0x25f80000,0x8000) + blank('rotation_start') + line(15)
        s += 'load 0x106,r1\nmov.w @r1,r2\ndelay:\ndt r2\nbf delay\n'
        # Legal write: physical grant begins during horizontal blank. An
        # already granted beat may finish after active display begins.
        s += (f'load {image:#x},r5\nload 0x2222,r6\n'
              'load 0x102,r1\nmov.w @r1,r3\nload 0x104,r1\nmov.w @r1,r0\n'
              'load 0x25f80008,r4\nload 0x25f80002,r1\ntst #1,r0\nbt omit_wait\n')
        # Keep the complete poll/store path within one warm 16-byte I-cache
        # line. Instruction fetch misses must not postpone the store's grant.
        pc = 0x200 + len(assembler.assemble(s+'omit_wait:\nnop\n',base=0x200))-2
        s += 'nop\n' * ((-pc % 16)//2)
        # One cached execution state separates the counter test from the
        # store: HCNT normal-mode bit0 is fixed, so its latch quantizes a
        # four-clock interval. The extra video-setup word phases can move
        # this loop within that interval. Publish at/after first-dot acquisition
        # while remaining before the second dot, without changing the oracle.
        s += ('timed_wait:\nmov.w @r1,r0\nmov.w @r4,r0\n'
              'cmp/hs r3,r0\nbf timed_wait\nnop\nmov.w r6,@r5\n'
              'mov r0,r8\nbra record\nnop\n'
              'omit_wait:\nmov.w @r1,r0\nmov.w @r4,r0\n'
              'cmp/hs r3,r0\nbf omit_wait\nmov r0,r8\nrecord:\n'
              'load 0x2600dc00,r1\nmov.l r8,@r1\n'
              'mov.w @r5,r0\nload 0x2600dc04,r1\nmov.l r0,@r1\n'
              'load 0x100,r1\nmov.w @r1,r0\nmov.w @r5,r2\nadd r2,r0\n'
              'load 0x2600dc08,r1\nmov.l r0,@r1\n')
        s += line(20)
        s += ('load 0x2600dc0c,r1\nload 0x600d,r0\nmov.l r0,@r1\n'
              'park:\nsleep\nbra park\nnop\n')
        output,digest = encode('rotation-fetch-'+mode,s,[(0x100,0x1111),(0x102,842),(0x104,1),(0x106,7 if mode == 'bitmap' else 3)])
        files.update(output)
        # Independently authored default guest records: latched H=844,
        # published word=2222, SH-2 sum1111+2222, completion marker.
        files['rotation-fetch-'+mode+'.tsv'] = b'0000034C\n00002222\n00003333\n0000600D\n'
        files['rotation-fetch-'+mode+'.sha256'] = (digest+'\n').encode()
    return files


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
