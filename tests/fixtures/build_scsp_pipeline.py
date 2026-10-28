#!/usr/bin/env python3
"""Original MIT SCSP pipeline diagnostic (adjacent LICENSE).

ST-077 figs4.18/4.30/4.31: OP1->OP7 six intervals, OP3->OP7 four.
Both CPUs execute the configuration/upload; only the DSP computes probe data.
Expected records are authored arithmetic, never captured emulator output.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler
from build_hle_system import smpc, wait

HERE = Path(__file__).resolve().parent


def build():
    sound = bytearray(0x1800)
    struct.pack_into('>II', sound, 0, 0x7ff00, 0x100)
    code, source, metadata = [], [], []

    def write(address, value):
        operand = 0x4000 + 0x100 + len(code)*2 + 2
        source.append(f'; MOVE.W #{value:04X},${address:06X}')
        code.extend([0x33fc, value, address >> 16, address & 65535])
        return operand

    write(0x100400, 0x20f)
    write(0x100700, 0x4000)  # coefficient .5
    write(0x100e02, 0x600d)  # DSP marker input, half=3006
    metadata += [write(0x1000, 0x2000), write(0x1002, 0x4000)]
    for slot, address, send in [(0,0x1000,7), (31,0x1002,15)]:
        for offset,value in [(2,address),(6,1),(12,0x100),(16,0x7800)]:
            write(0x100000+slot*32+offset,value)
        at = write(0x100000+slot*32+20,send)
        if slot == 31: metadata.append(at)
        at = write(0x100000+slot*32,0x820)
        if slot == 31: metadata.append(at)
    write(0x100016, 0)       # no direct/early output
    write(0x100036, 0xef)    # EF1 -> right
    write(0x100076, 0xff)    # EF3 -> left
    for index in range(5):
        write(0x100780+index*2,0x480+index)  # TABLE RAM records900..908
    program = []
    # Read immediately before and at/after the post-level boundary. Stores
    # run one instruction later; TABLE/NOFL also publishes each result to RAM.
    for read, store, mix, effect, record in [(22,23,0,0,0),(24,25,0,1,1),
                                             (18,19,1,2,2),(20,21,1,3,3)]:
        program += [(read,[0,0xa800+mix*64,2,0]),
                    (store,[0,0,0xd002+effect*256,0x8000+record*4])]
    program += [(100,[0,0xa000,2,0]),(101,[0,0,0xc002,0x8010])]
    for step, words in program:
        for part, value in enumerate(words):
            write(0x100800+step*8+part*2,value)
    write(0x100000,0x1820)  # actual KEYONEX starts both
    # Let every slot run for >2 samples before reading DSP-computed RAM.
    code.extend([0x303c,255])
    loop = 0x100+len(code)*2
    code.append(0x4e71)
    dbra=0x100+len(code)*2
    code.extend([0x51c8,(loop-(dbra+2))&65535])
    source += ['; MOVE.W #255,D0; wait: NOP; DBRA D0,wait',
               '; Copy DSP-produced RAM records to the SH-2 mailbox; STOP #2700']
    for at in range(0,10,2):
        code.extend([0x3039,0,0x900+at,0x33c0,0,0x800+at])
    code.extend([0x23fc,0,0x600d,0,0x80c,0x4e72,0x2700,0x60fa])
    assert 0x100+len(code)*2 < 0x800
    struct.pack_into('>'+'H'*len(code),sound,0x100,*code)
    host = ('load 0x4000,r1\nload 0x25a00000,r2\nload 0x600,r3\ncopy:\n'
            'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf copy\n')
    host += smpc(6)+wait(0x05a0080c,'wait_sound')
    host += ('load 0x25a00800,r1\nload 0x2600ef00,r2\nmov #4,r3\nrecords:\n'
             'mov.l @r1,r0\nmov.l r0,@r2\nadd #4,r1\nadd #4,r2\ndt r3\nbf records\n'
             'finished:\nsleep\nbra finished\nnop\n')
    rom=bytearray(524288)
    struct.pack_into('>II',rom,0,0x200,0x060ff000)
    sh2=assembler.assemble(host,base=0x200)
    assert len(sh2)<0x3e00
    rom[0x200:0x200+len(sh2)]=sh2
    rom[0x4000:0x4000+len(sound)]=sound
    lines=['# Original MIT SCSP pipeline guest; adjacent LICENSE.']
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]):
            lines.append(f'{at:04X} '+' '.join(f'{v:04X}' for v in struct.unpack('>8H',rom[at:at+16])))
    return {'scsp-pipeline.hex':'\n'.join(lines)+'\n',
            'scsp-pipeline.s':'; Original MIT, adjacent LICENSE. SH-2 source:\n'+host+'\n; 68k literal encodings in builder, operations:\n'+'\n'.join(source)+'\n',
            'scsp-pipeline.tsv':'\t'.join(f'{v:08X}' for v in metadata)+'\n'},hashlib.sha256(rom).hexdigest()


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    files,digest=build()
    for name,content in files.items():
        path=HERE/name
        if args.check:
            assert path.read_text()==content,f'Rebuild {name}'
        else:
            path.write_text(content)
    print(f'{"Verified" if args.check else "Built"} SCSP pipeline; expanded ROM SHA-256 {digest}')
