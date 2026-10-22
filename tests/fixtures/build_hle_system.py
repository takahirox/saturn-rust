#!/usr/bin/env python3
"""Original system-service Saturn guest and golden ABI records. MIT: LICENSE.

ST-162-R1 System Program §2.2, SH-2/SHC calling convention; SCU ST-210 #7.
The disc calls actual HLE ROM entry pointers. No host guest-result writes,
SDK code, firmware, core dependency or output captured from an emulator.
"""
import argparse
import hashlib
from pathlib import Path
from build_scu import assembler, store

HERE = Path(__file__).resolve().parent


def call(pointer):
    return f'load {pointer:#x},r1\nmov.l @r1,r1\njsr @r1\nnop\n'


def record(register=0):
    return f'mov.l r{register},@r8\nadd #4,r8\n'


def wait(address, label):
    return f'load {address | 0x20000000:#x},r1\n{label}:\nmov.l @r1,r0\ntst r0,r0\nbt {label}\n'


def smpc(command):
    return (f'load 0x20100063,r1\nmov #1,r0\nmov.b r0,@r1\n'
            f'load 0x2010001f,r1\nmov #{command},r0\nmov.b r0,@r1\n'
            f'load 0x20100063,r1\nwait_smpc{command}:\nmov.b @r1,r0\ntst #1,r0\nbf wait_smpc{command}\n')


# Independently authored expected records; not derived by executing the guest.
EXPECTED = [
    ('master VBR', 0x06000000), ('initial mask', 0xBFFF),
    ('cache control', 3),
    ('empty callback', 0x4B10), ('set SINT', 0x06009000),
    ('trap count', 1), ('restored SINT', 0x4B00),
    ('alternate VBR handler', 0x06009000), ('original VBR unchanged', 0x4B00),
    ('mask set', 0xB7FF), ('mask change', 0xBBFF),
    ('A bus mask', 0x3FFF), ('A bus acknowledge', 1),
]
for number in [0, 127, 128, 255]:
    EXPECTED += [(f'sem{number} first', 1), (f'sem{number} occupied', 0),
                 (f'sem{number} cleared', 1)]
EXPECTED += [('shared master first', 1), ('shared master after slave release', 1),
             ('registered callback', 0x06009200), ('restored SCU wrapper', 0x8160)]
EXPECTED += [(f'R{i} after nested IRQ', 42+i) for i in range(8)]
EXPECTED += [('MACH after IRQ', 0x345), ('MACL after IRQ', 0x678),
             ('PR after IRQ', 0x06009F00), ('SR after IRQ', 0),
             ('SP after IRQ', 0x060FF000), ('mask after IRQ', 0xB3FF),
             ('DMA0 data', 0x13579BDF), ('DMA1 data', 0x13579BDF),
             ('done', 0x600D)]


def main():
    # Cache-enabled callers may have already read SYS_GETSCUIM. Service writes
    # must update that CPU's resident line, while shared semaphores stay P1.
    s = 'load 0xfffffe92,r1\nmov #19,r0\nmov.b r0,@r1\n'
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n'
    s += 'load 0x0600e000,r8\nstc vbr,r0\n' + record()
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0xfffffe92,r1\nmov.b @r1,r0\n' + record()
    s += 'mov #75,r4\n' + call(0x06000304) + record()
    s += 'mov #112,r4\nload 0x06009000,r5\n' + call(0x06000310)
    s += 'mov #112,r4\n' + call(0x06000314) + record()
    s += 'trapa #112\nload 0x0600e400,r1\nmov.l @r1,r0\n' + record()
    s += 'mov #112,r4\nmov #0,r5\n' + call(0x06000310)
    s += 'mov #112,r4\n' + call(0x06000314) + record()
    s += 'load 0x06000400,r0\nldc r0,vbr\n'
    s += 'mov #113,r4\nload 0x06009000,r5\n' + call(0x06000310)
    s += 'mov #113,r4\n' + call(0x06000314) + record()
    s += 'load 0x06000000,r0\nldc r0,vbr\nmov #113,r4\n' + call(0x06000314) + record()
    s += 'load 0xb7ff,r4\n' + call(0x06000340)
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0xfffffbff,r4\nload 0x800,r5\n' + call(0x06000344)
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x3fff,r4\n' + call(0x06000340)
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x25fe00a8,r1\nmov.l @r1,r0\n' + record()
    s += 'load 0xbfff,r4\n' + call(0x06000340)
    for n in [0, 127, 128, 255]:
        # Each successful second acquisition is released before moving on.
        for pointer in [0x06000330, 0x06000330, 0x06000334, 0x06000330, 0x06000334]:
            s += f'load {n},r4\n' + call(pointer)
            if pointer == 0x06000330:
                s += record()
    s += 'mov #9,r4\n' + call(0x06000330) + record()
    s += store(0x06000250, 0x06008000) + smpc(2)
    s += wait(0x0600E504, 'wait_slave_first')
    s += 'mov #9,r4\n' + call(0x06000334)
    s += store(0x0600E508, 1) + wait(0x0600E518, 'wait_slave_done')
    s += 'mov #9,r4\n' + call(0x06000330) + record()
    s += 'mov #9,r4\n' + call(0x06000334) + smpc(3)
    s += 'mov #75,r4\nload 0x06009200,r5\n' + call(0x06000300)
    s += 'mov #74,r4\nload 0x06009400,r5\n' + call(0x06000300)
    s += 'mov #75,r4\n' + call(0x06000304) + record()
    s += 'mov #75,r4\nload 0x06009000,r5\n' + call(0x06000310)
    s += 'mov #75,r4\nmov #0,r5\n' + call(0x06000310)
    s += 'mov #75,r4\n' + call(0x06000314) + record()
    s += 'load 0x0600f000,r1\nload 0x00f0ffff,r0\nmov #32,r2\nfill_priority:\nmov.l r0,@r1\nadd #4,r1\ndt r2\nbf fill_priority\n'
    s += store(0x0600F02C, 0x0020F800)
    s += 'load 0x0600f000,r4\n' + call(0x06000280)
    # Destroy the source table: callback must still observe the copied value.
    s += store(0x0600F02C, 0x00F0FFFF)
    s += store(0x0600D000, 0x13579BDF)
    s += 'load 0xb3ff,r4\n' + call(0x06000340)
    for off, val in [(0,0x0600D000),(4,0x0600D100),(8,4),(12,0x102),(20,7),(16,0x101)]:
        s += store(0x25FE0000+off,val)
    s += 'mov #100,r2\nwait_pending:\ndt r2\nbf wait_pending\n'
    s += 'load 0x345,r0\nlds r0,mach\nload 0x678,r0\nlds r0,macl\nload 0x06009f00,r0\nlds r0,pr\n'
    s += ''.join(f'mov #{42+i},r{i}\n' for i in range(1,8))
    s += 'mov #0,r0\nldc r0,sr\nmov #42,r0\n'
    s += ''.join(record(i) for i in range(8))
    for reg in ['mach','macl','pr']:
        s += f'sts {reg},r0\n' + record()
    s += 'stc sr,r0\n' + record() + record(15)
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    for address in [0x0600D100,0x0600D104]:
        s += f'load {address:#x},r1\nmov.l @r1,r0\n' + record()
    s += 'load 0x600d,r0\n' + record() + 'done_sleep:\nsleep\nbra done_sleep\nnop\n'
    return s


def slave():
    s = 'mov #9,r4\n' + call(0x06000330)
    s += 'load 0x0600e500,r1\nmov.l r0,@r1\n' + store(0x0600E504,1)
    s += wait(0x0600E508,'slave_wait_release')
    s += 'mov #9,r4\n' + call(0x06000330)
    s += 'load 0x0600e50c,r1\nmov.l r0,@r1\nmov #9,r4\n' + call(0x06000334)
    s += 'mov #113,r4\nload 0x06009000,r5\n' + call(0x06000310)
    s += 'mov #113,r4\n' + call(0x06000314)
    s += 'load 0x0600e510,r1\nmov.l r0,@r1\n'
    s += 'mov #113,r4\nmov #0,r5\n' + call(0x06000310)
    s += 'mov #113,r4\n' + call(0x06000314)
    s += 'load 0x0600e514,r1\nmov.l r0,@r1\n' + store(0x0600E518,0x600D)
    return s + 'slave_sleep:\nsleep\nbra slave_sleep\nnop\n'


def callback(nested=False):
    trace = 0x0600E700 if nested else 0x0600E600
    s = f'load {trace:#x},r1\nstc sr,r0\nmov.l r0,@r1\n'
    s += 'add #4,r1\nload 0x06000348,r2\nmov.l @r2,r0\nmov.l r0,@r1\n'
    if nested:
        s += store(0x0600E710,1)
    else:
        s += 'sts pr,r0\nmov.l r0,@-r15\n'
        for off,val in [(0,0x0600D000),(4,0x0600D104),(8,4),(12,0x102),(20,7),(16,0x101)]:
            s += store(0x25FE0020+off,val)
        s += wait(0x0600E710,'wait_nested')
        s += 'mov.l @r15+,r0\nlds r0,pr\n'
        s += store(0x0600E610,1)
    s += 'mov #99,r0\nlds r0,mach\nlds r0,macl\n'
    s += ''.join(f'mov #99,r{i}\n' for i in range(8))
    return s + 'rts\nnop\n'


def reset_program():
    # First entry initializes power-clear memory, passes through a real NMI,
    # contaminates system-service state, and requests timed SYSRES. Second entry
    # reads retained bytes and reset service state. No host memory writes.
    s = 'load 0x06000210,r1\nmov.l @r1,r9\nmov.l @r9,r0\ntst r0,r0\nbf.l after_reset\n'
    s += 'load 0x12345678,r0\nmov.l r0,@r9\n'
    s += 'mov #11,r4\nload 0x06009b00,r5\n' + call(0x06000310) + smpc(0x18)
    # Only the guest NMI handler writes the second power-clear word. A no-op
    # NMIREQ can never reach SYSRES or publish the final success records.
    s += wait(0x06000C04,'wait_nmi_computation')
    s += 'load 255,r4\n' + call(0x06000330)
    s += 'mov #75,r4\nload 0x06009200,r5\n' + call(0x06000300)
    s += 'load 0xb7ff,r4\n' + call(0x06000340) + smpc(0x0D)
    s += 'unexpected_return:\nsleep\nbra unexpected_return\nnop\n'
    s += 'after_reset:\nload 0x0600e800,r8\nmov.l @r9,r0\n' + record()
    s += 'add #4,r9\nmov.l @r9,r0\n' + record()
    s += 'load 0x06000348,r1\nmov.l @r1,r0\n' + record()
    s += 'mov #75,r4\n' + call(0x06000304) + record()
    s += 'load 255,r4\n' + call(0x06000330) + record()
    s += 'load 0x600d,r0\n' + record()
    return s + 'reset_done:\nsleep\nbra reset_done\nnop\n'


def build():
    sections = {0x06004000:main(),0x06008000:slave(),
                0x06009000:'mov.l r0,@-r15\nmov.l r1,@-r15\nload 0x0600e400,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\nmov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n',
                0x06009200:callback(),0x06009400:callback(True),
                0x06009600:reset_program(),
                0x06009B00:'mov.l r0,@-r15\nmov.l r1,@-r15\nload 0x06000210,r1\nmov.l @r1,r1\nmov.l @r1,r0\nadd #1,r0\nadd #4,r1\nmov.l r0,@r1\nmov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n',
                0x06009F00:'sleep\nbra 0x06009f00\nnop\n'}
    guest = bytearray(0x6000)
    end = 0x06004000
    for address,source in sorted(sections.items()):
        code = assembler.assemble(source,base=address)
        assert address >= end and address+len(code) <= 0x0600A000
        end = address+len(code)
        guest[address-0x06004000:end-0x06004000] = code
    return assembler.build(bytes(guest),(HERE/'LICENSE').read_bytes(),
                           'ORIGINAL HLE SYSTEM TEST','HLE-SYS V1.000')


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    image=build()
    golden=''.join(f'{name}\t{value:08X}\n' for name,value in EXPECTED)
    outputs={'hle-system.iso':image,'hle-system.tsv':golden.encode()}
    for name,data in outputs.items():
        target=HERE/name
        if args.check: assert target.read_bytes()==data,f'Rebuild {name}'
        else: target.write_bytes(data)
    print(f'{"Verified" if args.check else "Built"} hle-system.iso; SHA-256 {hashlib.sha256(image).hexdigest()}')
