#!/usr/bin/env python3
"""Original backup-library guest disc and independent ABI vectors. MIT: LICENSE.

ST-162-R1 backup functions 1-11. No firmware or SDK routines. Directory/data,
lookup, allocation, copying, checksum and calendar arithmetic run on guest SH-2.
"""
import argparse
import datetime
import hashlib
from pathlib import Path
from build_scu import assembler, store

HERE = Path(__file__).resolve().parent
LIB = 0x06030000
CONF = 0x06020000
DIR = 0x06020200
DATA = 0x06021000
OUT = 0x06022000
STAT = 0x06023000
TRACE = 0x06024000
DATE = 0x06025000
RECORDS = []


def call(function, args):
    s = ''.join(f'load {n:#x},r{4+i}\n' for i,n in enumerate(args))
    s += (f'load 0x06000354,r1\nmov.l @r1,r1\nadd #{(function+1)*4},r1\n'
          'mov.l @r1,r1\njsr @r1\nnop\n')
    return s


def result(name, expected):
    RECORDS.append((name, expected & 0xFFFFFFFF))
    return 'mov.l r0,@r8\nadd #4,r8\n'


def words(base, values, name):
    s = ''
    for i,v in enumerate(values):
        s += f'load {base+4*i:#x},r1\nmov.l @r1,r0\n'
        s += result(f'{name} {i}',v)
    return s


def bytes_at(base, values):
    return ''.join(f'load {base+i:#x},r1\nload {v},r0\nmov.b r0,@r1\n' for i,v in enumerate(values))


def status(name, size, values):
    return call(2,[0,size,STAT])+result(name,0)+words(STAT,values,name)


def directory(letter, size):
    return bytes_at(DIR,b'FILE_'+letter+b'\0'+b'\0'*5 + b'ORIGINAL\0\0\0'+b'\1') + store(DIR+24,0x12345678)+store(DIR+28,size)


def main():
    RECORDS.clear()
    s = f'load {TRACE:#x},r8\nload {LIB:#x},r4\nload 0x06040000,r5\nload {CONF:#x},r6\n'
    s += 'load 0x06000358,r1\nmov.l @r1,r1\njsr @r1\nnop\n'
    s += words(CONF,[0x10001,0,0],'configuration')
    for args,expect in [([0,0],0),([1,0],1),([0,1],9)]:
        s += call(0,args)+result(f'partition {args}',expect)
    s += call(2,[0,257,STAT])+result('unformatted stat',2)
    s += call(4,[0,DIR,OUT])+result('unformatted read',2)
    s += call(1,[0])+result('format',0)
    s += status('empty',257,[32768,512,64,32640,510,102])
    s += f'load {DATA:#x},r1\nload 257,r2\nmov #0,r3\nmov #90,r4\nfill_data:\nmov r3,r0\nxor r4,r0\nmov.b r0,@r1\nadd #1,r1\nadd #1,r3\ndt r2\nbf fill_data\n'
    s += directory(b'A',257)
    s += call(3,[0,DIR,DATA,1])+result('write A',0)
    s += call(3,[0,DIR,DATA,1])+result('duplicate A',6)
    s += call(4,[0,DIR,OUT])+result('read A',0)
    for off,expected in [(0,90),(255,165),(256,90)]:
        s += f'load {OUT+off:#x},r1\nmov.b @r1,r0\nextu.b r0,r0\n'+result(f'A byte {off}',expected)
    s += call(7,[0,DIR,DATA])+result('verify A',0)
    s += bytes_at(DATA,[0x33])+call(7,[0,DIR,DATA])+result('mismatch',7)+bytes_at(DATA,[90])
    s += directory(b'B',1)+call(3,[0,DIR,DATA,1])+result('write B',0)
    s += directory(b'C',0)+call(3,[0,DIR,DATA,1])+result('write zero',0)
    s += status('three files',257,[32768,512,64,32192,503,100])
    s += bytes_at(DIR,b'FILE_\0')+store(OUT+36,0xAABBCCDD)
    s += call(6,[0,DIR,1,OUT])+result('short directory',-3)
    s += words(OUT+36,[0xAABBCCDD],'directory guard')
    s += bytes_at(DIR,b'FILE_B\0')+call(6,[0,DIR,4,OUT])+result('prefix B',1)
    s += words(OUT,[0x46494C45,0x5F420000,0,0x4F524947,0x494E414C,1,0x12345678,1,0x00010000],'B directory')
    s += bytes_at(DIR,b'ABSENT\0')+call(4,[0,DIR,OUT])+result('missing read',5)
    s += call(5,[0,DIR])+result('missing delete',5)
    s += directory(b'A',65)+call(3,[0,DIR,DATA,0])+result('overwrite A',0)
    s += bytes_at(DIR,b'FILE_B\0')+call(4,[0,DIR,OUT])+result('B after compact',0)
    s += f'load {OUT:#x},r1\nmov.b @r1,r0\nextu.b r0,r0\n'+result('B data after compact',90)
    s += status('overwrite free',0,[32768,512,64,32384,506,506])
    s += directory(b'A',32600)+call(3,[0,DIR,DATA,0])+result('overwrite out of space',4)
    s += store(DIR+28,65)+call(7,[0,DIR,DATA])+result('old data retained',0)
    s += bytes_at(DIR,b'FILE_B\0')+call(5,[0,DIR])+result('delete middle',0)
    s += call(4,[0,DIR,OUT])+result('B deleted',5)
    s += bytes_at(DIR,b'FILE_\0')+call(6,[0,DIR,4,OUT])+result('two directories',2)
    for letter in [b'A',b'C']:
        s += bytes_at(DIR,b'FILE_'+letter+b'\0')+call(5,[0,DIR])+result(f'delete {letter!r}',0)
    s += status('all deleted',257,[32768,512,64,32640,510,102])
    s += directory(b'A',32600)+call(3,[0,DIR,0x06060000,0])+result('full capacity write',0)
    s += status('full',0,[32768,512,64,0,0,0])
    s += call(7,[0,DIR,0x06060000])+result('verify full allocation',0)
    s += directory(b'B',0)+call(3,[0,DIR,DATA,0])+result('full capacity failure',4)
    s += directory(b'A',0xFFFFFFFF)+call(3,[0,DIR,DATA,0])+result('size overflow failure',4)
    s += call(5,[0,DIR])+result('delete full allocation',0)
    s += status('full freed',257,[32768,512,64,32640,510,102])
    s += call(1,[1])+result('absent format',1)
    s += bytes_at(DIR,b'\0')+call(3,[0,DIR,DATA,0])+result('empty name failure',9)
    s += bytes_at(DIR,b'ABCDEFGHIJKL')+call(3,[0,DIR,DATA,0])+result('unterminated name failure',9)
    s += directory(b'A',257)+call(3,[0,DIR,DATA,0])+result('persistent A',0)
    for date in [(1980,1,1,0,0),(1980,2,29,23,59),(1999,12,31,23,59),
                 (2000,2,29,12,34),(2026,10,21,9,45),(2100,3,1,0,0),
                 (2200,3,1,0,0),(2235,12,31,23,59)]:
        dt=datetime.datetime(*date)
        minutes=int((dt-datetime.datetime(1980,1,1)).total_seconds())//60
        raw=[date[0]-1980,*date[1:],(dt.weekday()+1)%7]
        s += bytes_at(DATE,raw)+call(9,[DATE])+result(f'packed date {date}',minutes)
        s += call(8,[minutes,DATE+16])
        for i,b in enumerate(raw):
            s += f'load {DATE+16+i:#x},r1\nmov.b @r1,r0\nextu.b r0,r0\n'+result(f'expanded date {date} {i}',b)
    # Public functions must preserve all SHC callee-save registers.
    s += ''.join(f'load {0x13570000+i:#x},r{i}\n' for i in range(9,15))
    s += call(2,[0,257,STAT])+result('callee-save call',0)
    for i in range(9,15):
        s += f'mov r{i},r0\n'+result(f'callee-save R{i}',0x13570000+i)
    s += 'load 0x600d,r0\n'+result('done',0x600D)
    s += 'finished:\nsleep\nbra finished\nnop\n'
    return s


def probe():
    # Entry selected through the original IP AIP by tests. Never formats/writes
    # backup; observes persisted/imported media using the same public ABI.
    s=f'load {LIB:#x},r4\nload 0x06040000,r5\nload {CONF:#x},r6\nload 0x06000358,r1\nmov.l @r1,r1\njsr @r1\nnop\n'
    s += f'load {TRACE:#x},r8\n'+bytes_at(DIR,b'FILE_A\0')
    for function,args in [(2,[0,257,STAT]),(4,[0,DIR,OUT]),(7,[0,DIR,DATA])]:
        s += call(function,args)+'mov.l r0,@r8\nadd #4,r8\n'
    s += 'load 0x600d,r0\nmov.l r0,@r8\nprobe_done:\nsleep\nbra probe_done\nnop\n'
    return s


def init():
    return (f'load {TRACE:#x},r8\nload {LIB:#x},r4\nload 0x06040000,r5\nload {CONF:#x},r6\n'
            'load 0x06000358,r1\nmov.l @r1,r1\njsr @r1\nnop\n')


def cartridge_main():
    RECORDS.clear()
    s = init() + words(CONF, [0x10001, 0x20001, 0], 'configuration')
    s += call(0, [1, 0]) + result('partition 0', 0)
    s += call(0, [1, 1]) + result('invalid partition', 9)
    s += call(0, [2, 0]) + result('absent device', 1)
    s += call(2, [1, 257, STAT]) + result('unformatted cartridge', 2)
    for device in [0, 1]:
        s += call(1, [device]) + result(f'format {device}', 0)
    s += bytes_at(DATA, [0xDE, 0xCA, 0xFB, 0xAD])
    s += directory(b'A', 4) + call(3, [0, DIR, DATA, 1]) + result('write internal A', 0)
    s += bytes_at(DATA, [0x12, 0x34, 0x56, 0x78])
    s += directory(b'A', 513) + call(3, [1, DIR, DATA, 1]) + result('write external A', 0)
    s += call(3, [1, DIR, DATA, 1]) + result('external duplicate', 6)
    s += call(7, [1, DIR, DATA]) + result('verify external A', 0)
    s += call(4, [1, DIR, OUT]) + result('read external A', 0)
    s += words(OUT, [0x12345678], 'external payload')
    s += directory(b'B', 1) + call(3, [1, DIR, DATA, 1]) + result('write external B', 0)
    s += bytes_at(DIR, b'FILE_A\0') + call(5, [1, DIR]) + result('delete external A', 0)
    s += call(4, [1, DIR, OUT]) + result('external A missing', 5)
    s += bytes_at(DIR, b'FILE_\0') + call(6, [1, DIR, 4, OUT]) + result('directory after compact', 1)
    s += words(OUT, [0x46494C45, 0x5F420000], 'external B name')
    s += directory(b'B', 523225) + call(3, [1, DIR, DATA, 0]) + result('oversized write', 4)
    s += store(DIR+28, 513) + call(3, [1, DIR, DATA, 0]) + result('replace compacted B', 0)
    s += directory(b'X', 523224) + call(3, [1, DIR, DATA, 0]) + result('insufficient free blocks', 4)
    s += call(2, [1, 513, STAT]) + result('cartridge status', 0)
    s += words(STAT, [524288, 1024, 512, 522240, 1020, 510], 'cartridge capacity')
    s += call(2, [1, 0xffffffff, STAT]) + result('oversized status', 0)
    s += words(STAT+20, [0], 'oversized status slots')
    s += directory(b'A', 4) + call(4, [0, DIR, OUT]) + result('internal A preserved', 0)
    s += words(OUT, [0xDECAFBAD], 'internal payload')
    return s + 'load 0x600d,r0\n' + result('done', 0x600D) + 'sleep\n'


def cartridge_probe():
    s = init()
    for device, letter in [(0, b'A'), (1, b'B')]:
        s += bytes_at(DIR, b'FILE_'+letter+b'\0')
        s += call(2, [device, 513, STAT]) + 'mov.l r0,@r8\nadd #4,r8\n'
        s += call(4, [device, DIR, OUT]) + 'mov.l r0,@r8\nadd #4,r8\n'
        s += f'load {OUT:#x},r1\nmov.l @r1,r0\nmov.l r0,@r8\nadd #4,r8\n'
    for offset in range(0, 40, 4):
        s += store(OUT + offset, 0xAABBCCDD)
    s += call(6, [1, DIR, 1, OUT]) + 'mov.l r0,@r8\nadd #4,r8\n'
    for offset in [0, 36]:
        s += f'load {OUT+offset:#x},r1\nmov.l @r1,r0\nmov.l r0,@r8\nadd #4,r8\n'
    return s + 'load 0x600d,r0\nmov.l r0,@r8\nsleep\n'


def fixtures(main_source, probe_source, basename, title, product):
    # Keep the ISO writer's bounded 0xB000 first-read contract: place the
    # read-only probe at 0600E000 and select it through the original IP AIP.
    a = assembler
    code = a.assemble(main_source)
    probe_code = a.assemble(probe_source, base=0x0600E000)
    assert len(code) < 0xA000
    binary = bytearray(0xA000 + len(probe_code))
    binary[:len(code)] = code
    binary[0xA000:] = probe_code
    yield basename + '.iso', a.build(bytes(binary), (HERE/'LICENSE').read_bytes(), title, product)
    yield basename + '.tsv', ''.join(f'{name}\t{value:08x}\n' for name, value in RECORDS).encode()


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    for main_fn, probe_fn, name, title, product in [
        (main, probe, 'hle-backup', 'BACKUP SERVICES', 'BUP-001 V1.000'),
        (cartridge_main, cartridge_probe, 'hle-cartridge-backup', 'CARTRIDGE BACKUP', 'BUP-002 V1.000'),
    ]:
        for filename, data in fixtures(main_fn(), probe_fn(), name, title, product):
            path = HERE / filename
            if args.check:
                assert path.read_bytes() == data, f'Rebuild {filename}'
            else:
                path.write_bytes(data)
            print(f'{"Verified" if args.check else "Built"} {filename}; SHA-256 {hashlib.sha256(data).hexdigest()}')
