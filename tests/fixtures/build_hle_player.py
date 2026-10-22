#!/usr/bin/env python3
"""Original licensed SYS_EXECDMP call disc; MIT: adjacent LICENSE.

An application deliberately leaves different hardware states before invoking
the non-returning guest service. The CUE's original audio is generated here.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import call
from build_cd_drive import command

HERE = Path(__file__).resolve().parent


def build():
    s = store(0x06000C00, 0x13579BDF)
    s += 'load 0x20180001,r1\nmov #42,r0\nmov.b r0,@r1\n'
    # Enable/prime cache, choose graphic B, and leave an active CD transfer.
    s += 'load 0xfffffe92,r1\nmov #19,r0\nmov.b r0,@r1\n'
    s += 'mov #1,r4\n' + call(0x06000320)
    s += command([0x0200, 0, 0, 0], 'previous_toc', 0x06009000)
    # Leave INTBACK waiting for CONTINUE; the player must break that handshake.
    for address, value in [(0x20100001,1), (0x20100003,8), (0x20100005,0xF0)]:
        s += f'load {address:#x},r1\nload {value},r0\nmov.b r0,@r1\n'
    s += ('load 0x20100063,r1\nmov #1,r0\nmov.b r0,@r1\n'
          'load 0x2010001f,r1\nmov #16,r0\nmov.b r0,@r1\n'
          'load 0x20100061,r1\nwait_continue:\nmov.b @r1,r0\n'
          'tst #32,r0\nbt wait_continue\n')
    s += call(0x0600026C)
    s += store(0x06009010, 0xBAD) + 'sleep\n'  # Must never return here.
    code = assembler.assemble(s)
    iso = assembler.build(code, (HERE/'LICENSE').read_bytes(), 'HLE DISC PLAYER', 'DPLAYER V1.000')
    # Two distinct original one-second stereo signals. Guest startup does not
    # advance a paused track; native/Wasm controls use emulated field deadlines.
    audio = b''.join(struct.pack('<hh',left,right) * 44100
                     for left,right in [(4096,-8192), (-12288,16384)])
    cue = ('FILE "hle-player.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\n'
           'FILE "hle-player-audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n'
           'TRACK 03 AUDIO\nINDEX 01 00:01:00\n')
    return {'hle-player.iso':iso, 'hle-player.cue':cue.encode(), 'hle-player-audio.bin':audio}


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data, f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
