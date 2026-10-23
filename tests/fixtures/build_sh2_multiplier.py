#!/usr/bin/env python3
"""Original MIT SH-2 multiplier/FRT guest. See docs/sh2-multiplier.md.

Literal expectations are authored from primary pipeline diagrams, never from
production timing helpers or collected emulator traces. Every kernel begins at
4n; its two warm calls execute identical bytes on each CPU.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import record, smpc
from build_sh2_timing import kernel

HERE = Path(__file__).resolve().parent
# name, guest body, clocks, final MACL. FRC final latch = floor((6+clocks)/8).
# A word multiply at 4n has no IF split, its adjacent STS at 4n+2
# adds one mm wait and one IF split: 1+3 = 4 clocks per pair.
KERNELS = [
    ('muls_sts', 'muls.w r1,r2\nsts macl,r6\n'*16, 64, 21),
    ('mulu_sts', 'mulu.w r1,r2\nsts macl,r6\n'*16, 64, 21),
    # With two independent NOPs the STS is still at 4n+2 but no longer waits.
    ('muls_separated', 'muls.w r1,r2\nnop\nnop\nsts macl,r6\n'*8, 40, 21),
    ('mulu_separated', 'mulu.w r1,r2\nnop\nnop\nsts macl,r6\n'*8, 40, 21),
    # Omitting the producer leaves zero MACL, and only the STS IF split.
    ('omitted', 'nop\nsts macl,r6\n'*16, 48, 0),
    # Long multiply: 2 EX states + one IF split. Ready at EX+8.
    # Adjacent STS: three mm wait clocks, one IF split, one EX state.
    ('mul_sts', 'mul.l r1,r2\nsts macl,r6\n'*8, 64, 21),
    ('dmuls_sts', 'dmuls.l r1,r2\nsts macl,r6\n'*8, 64, 21),
    ('dmulu_sts', 'dmulu.l r1,r2\nsts macl,r6\n'*8, 64, 21),
    ('dmuls_separated', 'dmuls.l r1,r2\nnop\nnop\nnop\nnop\nsts macl,r6\n'*4, 36, 21),
    # CLRMAC uses the same multiplier MA as LDS register (fig7.88).
    ('clrmac', 'muls.w r1,r2\nclrmac\n'*16+'sts macl,r6\nnop\n', 66, 0),
    ('lds_macl', 'muls.w r1,r2\nlds r1,macl\n'*16+'sts macl,r6\nnop\n', 66, 3),
    ('mac_word', 'mac.w @r12+,@r13+\n'*16+'sts macl,r6\nnop\n', 50, 336),
    ('mac_long', 'mac.l @r12+,@r13+\n'*16+'sts macl,r6\nnop\n', 68, 336),
    ('mac_separated', ('mac.l @r12+,@r13+\n'+'nop\n'*4)*8+'sts macl,r6\nnop\n', 58, 168),
    ('lds_mach', 'dmuls.l r1,r2\nlds r1,mach\n'*8+'sts mach,r6\nnop\n', 66, 3),
]


def build():
    s = ('load 0xffffffe0,r1\nmov.l @r1,r9\nload 0x8000,r2\ntst r2,r9\nbf slave\n'
         'mov #0,r11\ntst r11,r11\nbt clock_ready\n') + smpc(14) + 'clock_ready:\nload 0x2600e900,r8\n' + smpc(2) + 'bra common\nnop\n'
    s += 'slave:\nload 0x060fe000,r15\nload 0x2600eb00,r8\ncommon:\n'
    s += store(0xFFFFFFE8, 0xA55A0000)
    s += ('load 0xfffffe92,r1\nmov #25,r0\nmov.b r0,@r1\n'
          'load 0xfffffe12,r4\nload 0xfffffe13,r5\nload 0xfffffe91,r7\n')
    # Original operands live in CPU-local cache RAM; ordinary stores initialize
    # both word lanes and distinct longword arrays before either timed call.
    for base, value in [(0xC0000400,0x00030003),(0xC0000500,0x00070007),
                        (0xC0000600,3),(0xC0000700,7)]:
        s += f'load {base:#x},r12\nload {value:#x},r0\n'
        s += 'mov.l r0,@r12\nadd #4,r12\n'*32
    for i, (name, _, _, _) in enumerate(KERNELS):
        for _ in range(2):
            s += (f'load {0xC0000400 if name=="mac_word" else 0xC0000600:#x},r12\n'
                  f'load {0xC0000500 if name=="mac_word" else 0xC0000700:#x},r13\n')
            s += ('clrmac\nmov #3,r1\nmov #7,r2\nmov #0,r6\n'
                  f'load {0x4000+i*0x400:#x},r3\njsr @r3\nnop\n')
        s += record() + record(6)
    s += 'load 0x600d,r0\n'+record()+'done:\nsleep\nbra done\nnop\n'
    sections = {0x200:s}
    for i, (_, body, _, _) in enumerate(KERNELS):
        sections[0x4000+i*0x400] = kernel(body)
    rom = bytearray(524288)
    struct.pack_into('>IIII',rom,0,0x200,0x060FF000,0x200,0x060FE000)
    for address, source in sections.items():
        code=assembler.assemble(source,base=address)
        assert len(code)<(0x3e00 if address==0x200 else 0x400)
        rom[address:address+len(code)]=code
    lines=['# Original clocked SH-2 multiplier BIOS; MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    expected=[v for _,_,clocks,result in KERNELS for v in [(6+clocks)//8,result]]+[0x600D]
    source='; Original MIT SH-2 multiplier guest; see adjacent LICENSE.\n'+'\n'.join(f'; section {at:08X}\n{body}' for at,body in sections.items())
    return {'sh2-multiplier.hex': ('\n'.join(lines)+'\n').encode(),
            'sh2-multiplier.tsv': ''.join(f'{v:08X}\n' for v in expected).encode(),
            'sh2-multiplier.s': source.encode()}

if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for name,content in build().items():
        path=HERE/name
        if args.check:
            assert path.read_bytes()==content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
