# SPDX-License-Identifier: MIT
"""Generate the original workload used to collect the Wasm branch profile."""
import argparse
import importlib.util
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('output', type=Path, help='path for the generated HEX image')
args = parser.parse_args()
sys.path.insert(0, str(root / 'tests/fixtures'))
spec = importlib.util.spec_from_file_location('runtime_scene', root / 'examples/build-runtime-scene.py')
assert spec is not None and spec.loader is not None
scene = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scene)

original, _, data = scene.build(normal=True, sound_busy=True)
prelude = original.split('normal_park:', 1)[0]
assert 'normal_park' not in prelude and len(prelude) < len(original)
for address, value in [(0, 9), (20, 511), (22, 255), (32, 0x1000),
                       (34, 4), (36, 0xC0), (38, 0x100), (40, 0x2000), (42, 0x840)]:
    prelude += scene.put16(0x25C00000 + address, value)
prelude += scene.put16(0x25D00002, 2) + scene.put16(0x25D00004, 1)
source = '''; SPDX-License-Identifier: MIT
; Original performance fixture. Reuse the project's authored video and
; sound setup, then exercise timed WRAM, Sound and video writes with both CPUs active.
load 0x06002000,r8
mov.l @r8,r0
tst #255,r0
bf.l slave_poll_start
mov #1,r0
mov.l r0,@r8
''' + prelude + '''
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #2,r0
mov.b r0,@r1
mov #1,r10
master_round:
load 0x06004000,r4
load 0x06006000,r5
load 0x00202000,r6
mov #64,r7
master_mix:
mov.l @r4+,r0
mov.l @r5,r9
add r10,r0
add r9,r0
xor r10,r9
mov.l r0,@r5
mov.l r9,@r6
add #4,r5
add #4,r6
dt r7
bf master_mix
; Independent scratch ranges in Sound RAM and idle VDP1 command/texture RAM.
; The production arbitration and Worker synchronization still see all beats.
load 0x25a60000,r11
mov #8,r7
master_sound_words:
mov.w r0,@r11
add #2,r11
dt r7
bf master_sound_words
load 0x25c22000,r12
mov #12,r7
master_video_words:
mov.l r9,@r12
add #4,r12
dt r7
bf master_video_words
load 0x25d00010,r1
mov.w @r1,r0
tst #2,r0
bf start_again
add #3,r10
start_again:
add #1,r10
bra master_round
nop
slave_poll_start:
load 0x060fe000,r15
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0xfffffe11,r1
slave_poll:
mov.b @r1,r0
tst #128,r0
bt slave_poll
bra slave_poll
nop
'''
code = scene.assembler.assemble(source, base=0x200)
assert 0x200 + len(code) < 0x2000
data[0x200:0x2000] = bytes(0x2000 - 0x200)
data[0x200:0x200 + len(code)] = code
lines = ['# SPDX-License-Identifier: MIT', '# Original mixed dual-CPU performance profile.']
for at in range(0, len(data), 16):
    if any(data[at:at + 16]):
        words = ' '.join(f'{w:04X}' for w in struct.unpack_from('>8H', data, at))
        lines.append(f'{at:08X} {words}')
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text('\n'.join(lines) + '\n')
