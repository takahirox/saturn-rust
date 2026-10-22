#!/usr/bin/env python3
"""Original MIT dual-SH-2 TAS/bus-order diagnostic (H §7; P TAS.B).

Both guest CPUs increment one uncached counter using a shared TAS lock. Real
waits separate the read/write phases. No host code computes or writes results.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler, store
from build_hle_system import smpc

HERE = Path(__file__).resolve().parent


def build():
    source = ('load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf slave\n'
              'load 0x2600ed00,r8\nmov #1,r10\n') + smpc(2) + 'bra common\nnop\n'
    source += 'slave:\nload 0x060fe000,r15\nload 0x2600ed10,r8\nmov #2,r10\ncommon:\n'
    # H §7.5.8: all six supported MRS encodings, word data ignored.
    # Odd A0 proves table4.6's explicit exemption from address errors.
    source += store(0xFFFFFFE0, 0xA55A0001) + 'mov #0,r9\n'
    for size, addresses in [(0, [0xFFFF8426, 0xFFFF8447, 0xFFFF8466]),
                            (0x40, [0xFFFF8848, 0xFFFF8889, 0xFFFF88C8])]:
        source += store(0xFFFFFFEC, 0xA55A0000 | size)
        for address in addresses:
            source += f'load {address:#x},r1\nmov #-1,r0\nmov.w r0,@r1\nadd #1,r9\n'
    source += 'mov r8,r1\nadd #8,r1\nmov.l r9,@r1\n'
    source += store(0xFFFFFFE8, 0xA55A0080)
    source += ('load 0xfffffe92,r1\nmov #17,r0\nmov.b r0,@r1\n'
               'load 0x2600f000,r4\nload 0x2600f004,r5\nload 0x2600f008,r13\nmov #0,r11\n'
               'mov #32,r6\nmov #0,r7\n'
               'again:\ntas.b @r4\nbf again\nmov.l r10,@r13\n' + 'nop\n'*32 +
               'mov.l @r13,r0\ncmp/eq r10,r0\nbt owner_ok\nadd #1,r11\nowner_ok:\n'
               'mov.l @r5,r0\nadd #1,r0\nmov.l r0,@r5\n'
               'mov #0,r0\nmov.b r0,@r4\nadd #1,r7\ndt r6\nbf again\n'
               'mov r8,r1\nadd #12,r1\nmov.l r11,@r1\nmov.l r7,@r8\nload 0x600d,r0\nadd #4,r8\nmov.l r0,@r8\n'
               'load 0xffffffe0,r1\nmov.l @r1,r0\nload 0x8000,r2\ntst r2,r0\nbf peer_wait_setup\n'
               'load 0x2000,r3\njsr @r3\nnop\nbra done\nnop\n'
               'peer_wait_setup:\nload 0x2600ed30,r1\n'
               'peer_wait:\nmov.l @r5,r0\nmov.l @r1,r0\ntst r0,r0\nbt peer_wait\n'
               'done:\nsleep\nbra done\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>IIII', rom, 0, 0x200, 0x060FF000, 0x200, 0x060FE000)
    dma = 'load 0xfffffee0,r1\nmov #1,r0\nmov.w r0,@r1\n' + store(0x25FE00A0, 0xB1FF) + 'mov #0,r0\nldc r0,sr\n'
    for address, value in [(0x0600F100,0x11223344),(0x0600F104,0xAABBCCDD),
                           (0x0600F180,8),(0x0600F184,0x0600F200),(0x0600F188,0x8600F100)]:
        dma += store(address,value)
    sections = {0x200: source}
    for channel, destination in enumerate([0x0600F200,0x05A01000,0x05E01000]):
        for offset, value in [(0,0x0600F100),(4,0x0600F180 if channel == 0 else destination),
                              (8,8),(12,0x102 if channel == 0 else 0x101),
                              (20,0x01000107 if channel == 0 else 7),(16,0x101)]:
            dma += store(0x25FE0000+channel*32+offset,value)
        mailbox = 0x2600ED20+channel*4
        dma += (f'load {mailbox:#x},r1\nwait_dma{channel}:\nmov.l @r1,r0\n'
                f'mov #1,r2\ncmp/eq r2,r0\nbf wait_dma{channel}\n')
        for offset in [0,4]:
            dma += (f'load {destination+offset:#x},r1\nmov.l @r1,r0\n'
                    f'load {0x2600ED40+channel*8+offset:#x},r1\nmov.l r0,@r1\n')
        handler = 0x3000 + channel*0x100
        sections[handler] = ('mov.l r0,@-r15\nmov.l r1,@-r15\n'
                             f'load {mailbox:#x},r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'
                             'mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n')
        struct.pack_into('>I',rom,(0x4B-channel)*4,handler)
    dma += store(0x2600ED30,0x600D) + 'rts\nnop\n'
    sections[0x2000] = dma
    for address, text in sections.items():
        code = assembler.assemble(text, base=address)
        assert len(code) < (0x1000 if address == 0x2000 else 0x800)
        rom[address:address+len(code)] = code
    lines = ['# Original dual-SH-2 bus/TAS BIOS; MIT: adjacent LICENSE.']
    for at in range(0, len(rom), 16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} ' + ' '.join(f'{w:04X}' for w in struct.unpack('>8H', rom[at:at+16])))
    return {'bus-transactions.hex': ('\n'.join(lines)+'\n').encode()}


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for name, content in build().items():
        path = HERE/name
        if args.check:
            assert path.read_bytes() == content, f'Rebuild {name}'
        else:
            path.write_bytes(content)
        print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(content).hexdigest()}')
