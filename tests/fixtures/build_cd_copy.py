#!/usr/bin/env python3
"""Original scheduled CD selector/host-I/O SH-2 diagnostic. MIT: LICENSE.

ST-162 §§3.2–3.4 and 8.2.7, command classes and COPY_OK/NG/BUSY.
The guest itself issues commands, polls HIRQ and computes its records.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_cd_drive import command, store

HERE = Path(__file__).resolve().parent
BASE = 0x0600A000


def build():
    s = ''
    index = 0
    labels = []

    def cmd(words):
        nonlocal s, index
        at = BASE + index * 16
        s += command(words, f'cmd{index}', at)
        labels.append(f'{index:02d} ' + ' '.join(f'{v:04X}' for v in words))
        index += 1
        return at

    def wait_copy(name):
        nonlocal s
        s += (f'load 0x25890008,r1\n{name}:\nmov.w @r1,r0\n'
              f'load 0x100,r2\ntst r2,r0\nbt {name}\n')

    def payload(address):
        nonlocal s
        s += (f'load {address:#x},r1\nmov.w @r1,r0\n'
              'load 0x25818000,r1\nmov.w r0,@r1\n')

    def get_word(mailbox):
        nonlocal s
        s += ('load 0x25818000,r1\nmov.w @r1,r0\nextu.w r0,r0\n'
              f'load {mailbox:#x},r1\nmov.l r0,@r1\n')

    cmd([0x0401,0,0,0])                                # 00 reset
    cmd([0x6400,0,0,32])                               # 01 source reservation
    payload(0x100)
    cmd([0x0600,0,0,0])                                # 02 publish32
    s += store(0x25890008,0xFEFF)
    cmd([0x6501,0,0,32])                               # 03 start copy
    cmd([0x6700,0,0,0])                                # 04 BUSY
    cmd([0x6400,0,0x0300,2])                            # 05 independent PUT
    payload(0x102)
    cmd([0x60FF,0x0300,0,0])                            # 06 next PUT length
    wait_copy('copy_complete')
    cmd([0x6700,0,0,0])                                # 07 OK during PUT
    cmd([0x5100,0,0x0100,0])                            # 08 copied32
    cmd([0x0600,0,0,0])                                # 09 commit original 2048 PUT
    cmd([0x6000,0,0,0])                                # 10 lengths2048
    cmd([0x6100,0,0x0100,1])                            # 11 actual copied bytes
    get_word(0x0600A800)
    cmd([0x0600,0,0,0])                                # 12 endGET
    cmd([0x6100,0,0x0300,1])                            # 13 retained PUT bytes
    get_word(0x0600A804)
    cmd([0x0600,0,0,0])                                # 14 endGET
    cmd([0x6400,0,0x0200,134])                          # 15 fill remaining pool
    cmd([0x0600,0,0,0])                                # 16 total200
    cmd([0x6504,0,0x0100,1])                            # 17 full copy WAIT
    s += store(0x25890008,0xFEFF)
    cmd([0x6604,0,0x0100,32])                           # 18 full move accepted
    cmd([0x6700,0,0,0])                                # 19 BUSY
    wait_copy('move_complete')
    cmd([0x6700,0,0,0])                                # 20 OK
    cmd([0x5100,0,0x0100,0])                            # 21 source empty
    cmd([0x5100,0,0x0400,0])                            # 22 target32
    cmd([0x5000,0,0,0])                                # 23 no allocation by move
    cmd([0x6100,0,0x0400,1])                            # 24 actual moved bytes
    get_word(0x0600A808)
    cmd([0x0600,0,0,0])                                # 25 endGET
    cmd([0x483F,0,0,0])                                # 26 release pool/reset routing
    cmd([0x6400,0,0,32])                               # 27 another source
    cmd([0x0600,0,0,0])                                # 28 publish
    s += store(0x25890008,0xFEFF)
    cmd([0x6501,0,0,32])                               # 29 copy
    cmd([0x6700,0,0,0])                                # 30 BUSY
    cmd([0x4601,0xFF00,0x0100,0])                       # 31 detach target
    wait_copy('copy_error')
    cmd([0x6700,0,0,0])                                # 32 NG
    cmd([0x5100,0,0,0])                                # 33 source retained
    cmd([0x4601,0x0100,0x0100,0])                       # 34 reconnect
    s += store(0x25890008,0xFEFF)
    cmd([0x6501,0,0,32])                               # 35 new job
    cmd([0x0401,0,0,0])                                # 36 cancel through soft reset
    cmd([0x6700,0,0,0])                                # 37 reset error
    cmd([0x5100,0,0x0100,0])                            # 38 no late publication
    # 32 + 168 makes a full pool. A copy first accepted with free space can
    # fail if an independent host reservation consumes it before completion.
    cmd([0x6400,0,0,32])                               # 39 source
    cmd([0x0600,0,0,0])                                # 40 publish
    s += store(0x25890008,0xFEFF)
    cmd([0x6501,0,0,32])                               # 41 copy
    cmd([0x6400,0,0x0200,168])                          # 42 concurrent reservation
    wait_copy('capacity_error')
    cmd([0x6700,0,0,0])                                # 43 NG while PUT active
    cmd([0x0600,0,0,0])                                # 44 publish reserved168
    cmd([0x5000,0,0,0])                                # 45 free0
    s += ('load 0x0600A80C,r1\nload 0x600d,r0\nmov.l r0,@r1\n'
          'done:\nsleep\nbra done\nnop\n')
    rom = bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x060FF000)
    struct.pack_into('>HH',rom,0x100,0x1357,0x5678)
    code = assembler.assemble(s,base=0x200)
    assert len(code)<0x9000
    rom[0x200:0x200+len(code)] = code
    lines = ['# Original CD selector concurrency guest. MIT: adjacent LICENSE.']
    for at in range(0,len(rom),16):
        part = rom[at:at+16]
        if any(part): lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',part)))
    return '\n'.join(lines)+'\n', hashlib.sha256(rom).hexdigest(), labels


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    text,digest,_=build();target=HERE/'cd-copy.hex'
    if args.check: assert target.read_text()==text,'Rebuild cd-copy.hex'
    else: target.write_text(text)
    print(f'{"Verified" if args.check else "Built"} cd-copy.hex; expanded ROM SHA-256 {digest}')
