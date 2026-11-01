#!/usr/bin/env python3
"""Original MIT application-level CD reader. Adjacent LICENSE applies.

The unchanged #85 synthetic ROM discovers this application as first-read FID 2.
All names, handles, byte cursors and decisions below execute as SH-2 instructions.
This is a private test ABI, not the Sega CDC/GFS binary ABI.
"""
import argparse
import hashlib
import struct
from pathlib import Path
from build_scu import assembler as a, store
from build_clock import put8, put16
from build_hle_system import smpc
from build_external_discovery import M

HERE = Path(__file__).resolve().parent
BASE = 0x06020000
OUT = 0x26070000
HA, HB = OUT+0x800, OUT+0x820
G = OUT+0xC00
DIR = 0x06060000
BUF = 0x06080004
TRACE = 0x26072000
NAMES = BASE+0x4000
_serial = 0


def call(label):
    return f'load {label},r0\njsr @r0\nnop\n'


def guard(branch, target='failed'):
    global _serial
    _serial += 1
    skip = f'guard_{_serial}'
    return f'{"bf" if branch == "bt" else "bt"} {skip}\nload {target},r0\njmp @r0\nnop\n{skip}:\n'


def save(address, reg='r0'):
    return f'load {address:#x},r1\nmov.l {reg},@r1\n'


def get(address, reg='r0'):
    return f'load {address:#x},r1\nmov.l @r1,{reg}\n'


def check(value, target='failed'):
    return f'load {value & 0xffffffff:#x},r2\ncmp/eq r2,r0\n'+guard('bf', target)


def cmd(op, b=0, c=0, d=0, raw=False):
    return (f'load {op:#x},r4\nload {b:#x},r5\nload {c:#x},r6\nload {d:#x},r7\n'
            +call('issue' if raw else 'command'))


def wait(mask):
    global _serial
    _serial += 1
    label = f'wait_{_serial}'
    return (f'load 0x25890008,r1\nload {mask},r2\nload 200000,r3\n{label}:\n'
            f'mov.w @r1,r0\ntst r2,r0\nbf {label}_done\ndt r3\nbf {label}\n'
            'load io_error,r0\njmp @r0\nnop\n'+label+'_done:\n')


def open_file(handle, name, selector):
    return f'load {handle:#x},r8\nload {NAMES+name:#x},r10\nload {selector},r11\n'+call('open')+check(0)


def read_file(handle, offset, count, slot):
    s = f'load {handle:#x},r8\n'
    if offset is not None:
        s += f'load {offset},r10\n'+call('seek')+check(0)
    s += f'load {BUF+slot*0x2000:#x},r10\nload {count},r11\n'+call('read')
    return s+save(OUT+0x40+slot*4)


def library():
    # issue preserves R8/R10..R14, logs actual requests and responses, bounds CMOK.
    s = 'command:\nsts.l pr,@-r15\n'+call('issue')
    s += 'load 0x8000,r2\ntst r2,r0\n'+guard('bf','io_error')+'lds.l @r15+,pr\nrts\nnop\n'
    s += 'issue:\n'
    for reg in range(4,8): s += f'mov.l r{reg},@r9\nadd #4,r9\n'
    s += put16(0x25890008,0xFFFE)
    s += 'load 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\nadd #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\n'
    s += wait(1)
    for address in [0x25890018,0x2589001C,0x25890020,0x25890024]:
        s += f'load {address:#x},r1\nmov.w @r1,r0\nextu.w r0,r0\nmov.l r0,@r9\nadd #4,r9\n'
    s += 'load 0x25890018,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n'
    s += 'init:\nsts.l pr,@-r15\n'+cmd(0)+cmd(0x100)
    s += 'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+check(2)  # low half is the version word
    s += cmd(0x0400,0xFFFF,0,0x0201)+cmd(0x0480,0xFFFF,0,0xFFFF)
    s += cmd(0x0400,0xFFFF,0,0x0600,True)+check(0xFF00)  # atomic reject
    s += cmd(0x6000,0,0,0)+'lds.l @r15+,pr\nrts\nnop\n'
    # Full raw sector fetch for guest directory parsing; fixed FIFO SCU source.
    # The firmware's original DMA0 handler counts and acknowledges each IRQ.
    s += 'fetch:\nsts.l pr,@-r15\n'+put16(0x25890008,0xFFEB)+cmd(0x4480)+cmd(0x3000)
    s += 'mov r10,r5\nload 150,r0\nadd r0,r5\nmov r5,r4\nshlr16 r4\nload 0x1080,r0\nor r0,r4\nextu.w r5,r5\nload 0x80,r6\nmov #1,r7\n'+call('command')+wait(16)
    s += cmd(0x6300,0,0,1)+get(0x26010204)+'add #1,r0\nmov r0,r7\n'
    for off,value in [(0,0x05818000),(8,2048),(12,2),(20,7)]: s += store(0x25FE0000+off,value)
    s += 'load 0x25fe0004,r1\nmov.l r11,@r1\n'+store(0x25FE0010,0x101)
    s += 'load 0x26010204,r1\nload 200000,r3\nfetch_dma:\nmov.l @r1,r0\ncmp/eq r7,r0\nbt fetch_ready\ndt r3\nbf fetch_dma\nload io_error,r0\njmp @r0\nnop\nfetch_ready:\n'
    s += cmd(0x0600)+'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+check(1024)
    s += 'lds.l @r15+,pr\nrts\nnop\n'
    # load_dir(R10=LSN,R12=byte length): 1..2 bounded sectors, generic names/FIDs.
    s += 'load_dir:\nsts.l pr,@-r15\nload 4097,r0\ncmp/hs r0,r12\n'+guard('bt','directory_error')
    s += 'tst r12,r12\n'+guard('bt','directory_error')+'load 2047,r0\ntst r0,r12\n'+guard('bf','directory_error')+save(G,'r12')
    s += f'load {DIR:#x},r11\nload_dir_loop:\n'+call('fetch')+'load 2048,r0\nadd #1,r10\nadd r0,r11\nsub r0,r12\ntst r12,r12\nbf load_dir_loop\nlds.l @r15+,pr\nrts\nnop\n'
    # find(R10=NUL-terminated exact ISO name) -> R0=record sequence FID or -1,
    # R1=record. Hardware ChangeDir validates numeric fields; this name scan
    # additionally checks record boundaries before accessing identifier bytes.
    s += f'find:\nsts.l pr,@-r15\nload {DIR:#x},r8\n'+get(G,'r12')+'add r8,r12\nmov #0,r13\nfind_loop:\ncmp/hs r12,r8\nbt.l find_missing\nmov.b @r8,r0\nextu.b r0,r14\ntst r14,r14\nbt.l find_padding\nload 34,r2\ncmp/hs r2,r14\n'+guard('bf','directory_error')
    s += 'mov r8,r0\nadd r14,r0\ncmp/hs r0,r12\n'+guard('bf','directory_error')+'mov r8,r0\nload 2047,r2\nand r2,r0\nadd r14,r0\nload 2049,r2\ncmp/hs r2,r0\n'+guard('bt','directory_error')
    s += 'mov r8,r1\nadd #32,r1\nmov.b @r1,r0\nextu.b r0,r5\nmov r5,r0\nadd #33,r0\ncmp/hs r0,r14\n'+guard('bf','directory_error')
    s += 'add #1,r1\nmov r10,r2\nfind_compare:\nmov.b @r2+,r0\nextu.b r0,r0\ntst r0,r0\nbt find_end_name\ntst r5,r5\nbt find_next\nmov.b @r1+,r3\nextu.b r3,r3\ncmp/eq r3,r0\nbf find_next\nadd #-1,r5\nbra find_compare\nnop\nfind_end_name:\ntst r5,r5\nbf find_next\nmov r8,r1\nmov r13,r0\nlds.l @r15+,pr\nrts\nnop\nfind_next:\nadd r14,r8\nadd #1,r13\nbra find_loop\nnop\nfind_padding:\nload 2047,r0\nor r0,r8\nadd #1,r8\nbra find_loop\nnop\nfind_missing:\nmov #-1,r0\nlds.l @r15+,pr\nrts\nnop\n'
    s += 'be32:\nmov #0,r0\nmov #4,r3\nbe_loop:\nshll8 r0\nmov.b @r1+,r2\nextu.b r2,r2\nor r2,r0\ndt r3\nbf be_loop\nrts\nnop\n'
    # open(R8=24-byte handle,R10=name,R11=selector). Handles contain
    # FID, extent LSN, length, byte cursor, open flag, private selector.
    s += 'open:\nsts.l pr,@-r15\nmov.l r8,@-r15\nmov.l r11,@-r15\n'+call('find')
    s += 'mov.l @r15+,r11\nmov.l @r15+,r8\nload 0xffffffff,r2\ncmp/eq r2,r0\nbt open_missing\nmov.l r0,@r8\nmov r1,r14\nadd #6,r1\n'+call('be32')
    s += 'mov r8,r1\nadd #4,r1\nmov.l r0,@r1\nmov r14,r1\nadd #14,r1\n'+call('be32')
    s += 'mov r8,r1\nadd #8,r1\nmov.l r0,@r1\nadd #4,r1\nmov #0,r0\nmov.l r0,@r1\nadd #4,r1\nmov #1,r0\nmov.l r0,@r1\nadd #4,r1\nmov.l r11,@r1\nmov #0,r0\nopen_missing:\nlds.l @r15+,pr\nrts\nnop\n'
    s += 'close:\nmov r8,r1\nadd #16,r1\nmov #0,r0\nmov.l r0,@r1\nrts\nnop\n'
    s += 'seek:\nmov r8,r1\nadd #16,r1\nmov.l @r1,r0\ntst r0,r0\nbt api_error\nmov r8,r1\nadd #8,r1\nmov.l @r1,r0\ncmp/hs r10,r0\nbf api_error\nadd #4,r1\nmov.l r10,@r1\nmov #0,r0\nrts\nnop\napi_error:\nmov #-1,r0\nrts\nnop\n'
    # read(R8=handle,R10=destination,R11=count) -> R0=actual bytes. Each command
    # uses the handle's own selector. Cursor commit follows transfer fences.
    s += 'read:\nsts.l pr,@-r15\nmov r8,r1\nadd #16,r1\nmov.l @r1,r0\ntst r0,r0\nbt.l read_closed\nmov r8,r1\nadd #8,r1\nmov.l @r1+,r12\nmov.l @r1,r13\nsub r13,r12\ncmp/hs r12,r11\nbt read_clamped\nmov r11,r12\nread_clamped:\n'+save(G+4,'r12')
    s += 'tst r12,r12\nbt.l read_done\nmov.l r10,@-r15\n'
    s += put16(0x25890008,0xFDFF)+'mov r13,r5\n'+('shlr r5\n'*11)
    s += 'load 0x7400,r4\nmov r8,r1\nadd #20,r1\nmov.l @r1,r6\nshll8 r6\nmov.l @r8,r7\n'+call('command')+wait(512)
    s += 'mov.l @r15+,r10\nload 2047,r0\nand r0,r13\nread_sector:\nload 2048,r14\nsub r13,r14\ncmp/hs r14,r12\nbt read_take\nmov r12,r14\nread_take:\n'
    s += put16(0x25890008,0xFF7F)+'load 0x6300,r4\nmov #0,r5\nmov r8,r1\nadd #20,r1\nmov.l @r1,r6\nshll8 r6\nmov #1,r7\n'+call('command')
    s += 'load 0x25890008,r1\nmov.w @r1,r0\nload 0x82,r2\nand r2,r0\n'+check(2)
    s += 'mov r13,r5\nadd r14,r5\nmov #0,r4\nmov #0,r7\nfifo_word:\nload 0x25818000,r1\nmov.w @r1,r0\nload 0x26070c80,r2\nmov.w r0,@r2\nadd #1,r7\nmov #2,r6\nmov r2,r1\nfifo_byte:\nmov.b @r1+,r0\n'
    s += 'cmp/hs r13,r4\nbf fifo_skip\ncmp/hs r5,r4\nbt fifo_skip\nmov.b r0,@r10\nadd #1,r10\nfifo_skip:\nadd #1,r4\ndt r6\nbf fifo_byte\ncmp/hs r5,r4\nbf fifo_word\n'
    s += save(G+8,'r7')+cmd(0x0600)+'load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+save(G+12)
    # Count relation is the unchanged inherited-main negative oracle. The exact
    # one-word prefetch amount is a model policy; the strict inequality is Sega's.
    s += get(G+8,'r2')+'load 1024,r3\ncmp/eq r3,r2\nbt read_full_count\ncmp/hs r0,r2\n'+guard('bt','prefetch_error')+'cmp/hs r0,r3\n'+guard('bf','prefetch_error')+'bra read_fenced\nnop\nread_full_count:\ncmp/eq r2,r0\n'+guard('bf','prefetch_error')+'read_fenced:\n'
    s += get(OUT+0x18)+'tst r0,r0\nbf read_not_first\n'+get(G+8)+save(OUT+0x18)+get(G+12)+save(OUT+0x1C)+'read_not_first:\n'
    s += 'load 0x25890008,r1\nmov.w @r1,r0\nload 0x82,r2\nand r2,r0\n'+check(0x80)
    s += 'sub r14,r12\nmov #0,r13\ntst r12,r12\nbf.l read_sector\n'
    # Release the unused suffix; buffer lifetime is independent of file cursor.
    s += 'load 0x5100,r4\nmov #0,r5\nmov r8,r1\nadd #20,r1\nmov.l @r1,r6\nshll8 r6\nmov #0,r7\n'+call('command')+'load 0x25890024,r1\nmov.w @r1,r0\ntst r0,r0\nbt read_done\n'
    s += 'load 0x6200,r4\nmov #0,r5\nmov r8,r1\nadd #20,r1\nmov.l @r1,r6\nshll8 r6\nload 0xffff,r7\n'+call('command')
    s += 'read_done:\n'+get(G+4,'r2')+'mov r8,r1\nadd #12,r1\nmov.l @r1,r0\nadd r2,r0\nmov.l r0,@r1\nmov r2,r0\nlds.l @r15+,pr\nrts\nnop\nread_closed:\nmov #-1,r0\nlds.l @r15+,pr\nrts\nnop\n'
    return s


def program():
    global _serial
    _serial=0
    s = 'bsr entered\nnop\nentered:\nsts pr,r0\n'+save(OUT)
    s += f'load {TRACE:#x},r9\n'+store(OUT+4,1)+call('init')
    # Guards and output regions start with a nonzero canary; the guest never
    # writes unused suffixes. Host checks compare every byte of this 64 KiB area.
    s += 'load 0x06080000,r1\nload 65536,r3\nload 0xa5,r0\ncanaries:\nmov.b r0,@r1\nadd #1,r1\ndt r3\nbf canaries\n'
    s += get(M['root'],'r10')+get(M['root_size'],'r12')+call('load_dir')
    s += f'load {NAMES:#x},r10\n'+call('find')+'load 0xffffffff,r2\ncmp/eq r2,r0\n'+guard('bt','directory_error')+'mov r1,r14\n'+save(OUT+0x10)
    s += 'mov r0,r7\nmov r14,r1\nadd #6,r1\n'+call('be32')+save(G+16)
    s += 'mov r14,r1\nadd #14,r1\n'+call('be32')+save(G+20)
    s += put16(0x25890008,0xFDFF)+cmd(0x7000,0,0x00FF,0xFFFF)+wait(512)
    s += get(OUT+0x10,'r7')+'load 0x7000,r4\nmov #0,r5\nload 0x0300,r6\n'+put16(0x25890008,0xFDFF)+call('command')+wait(512)+cmd(0x7200)
    s += get(G+16,'r10')+get(G+20,'r12')+call('load_dir')
    # Raw directory fetch changes only filter0. Child file table remains held.
    s += open_file(HA,8,1)+open_file(HB,24,2)
    s += get(HA+8)+save(OUT+0x20)+get(HB+8)+save(OUT+0x24)
    s += f'load {NAMES+40:#x},r10\n'+call('find')+save(OUT+0xC)+check(-1)
    s += cmd(0x7400,0,0x1700,0xFFFF,True)+check(0xFF00)
    # Abort a real pending read, retain its buffer/filter, then repeat normally.
    s += 'load 0x7400,r4\nmov #0,r5\nload 0x0100,r6\n'+get(HA,'r7')+call('command')+cmd(0x7500)
    s += store(OUT+4,2)
    s += read_file(HA,None,3,0)+check(3)
    s += read_file(HB,None,2050,1)+check(2050)
    s += read_file(HA,2046,2057,2)
    s += read_file(HB,1,7,3)+check(7)
    s += read_file(HA,0,9999,4)
    s += f'load {HA:#x},r8\n'+call('close')
    s += read_file(HA,None,1,5)+save(OUT+0x60)+check(-1)+open_file(HA,8,1)
    s += read_file(HA,None,13,5)+check(13)
    # EOF and invalid seek do not issue a command or advance the cursor.
    s += f'load {HB:#x},r8\n'+get(HB+8,'r10')+call('seek')+check(0)
    s += read_file(HB,None,9,6)+save(OUT+0x64)+check(0)
    s += get(HB+8,'r10')+'add #1,r10\n'+call('seek')+save(OUT+0x68)+check(-1)
    s += f'load {HB:#x},r8\n'+call('close')+open_file(HB,24,2)
    s += read_file(HB,None,9999,6)
    # Guest-computed byte sums drive actual VDP2 back screen, not a host marker.
    s += 'mov #0,r4\n'
    for label,address,length in [('alpha',BUF+4*0x2000,OUT+0x20),('beta',BUF+6*0x2000,OUT+0x24)]:
        s += f'load {address:#x},r1\nload {length:#x},r2\nmov.l @r2,r3\nsum_{label}:\nmov.b @r1+,r0\nextu.b r0,r0\nadd r0,r4\ndt r3\nbf sum_{label}\n'
    s += save(OUT+8,'r4')+'load 0x7fff,r0\nand r0,r4\nload 0x8000,r0\nor r0,r4\nload 0x25e00000,r1\nmov.w r4,@r1\n'
    # Same optional original cold/SYSRES ownership contract as #85.
    s += 'load 0x7f00,r1\nmov.l @r1,r0\ntst #1,r0\nbt no_sysres\nload 0x20180001,r1\nmov.b @r1,r0\nextu.b r0,r0\nload 0x5a,r2\ncmp/eq r2,r0\nbt no_sysres\n'
    s += put8(0x20180001,0x5A)+smpc(0x0D)+'no_sysres:\n'+store(OUT+4,3)+store(OUT+0xFC,0x600D)
    s += save(OUT+0x28,'r9')+'park:\nsleep\nbra park\nnop\n'
    for label,code in [('failed',0xE090),('prefetch_error',0xE091),('io_error',0xE092),('directory_error',0xE093)]:
        s += label+':\n'+store(OUT+0xFC,code)+save(OUT+0x28,'r9')+'load park,r0\njmp @r0\nnop\n'
    s += library()
    code = a.assemble(s,base=BASE)
    assert len(code)<0x4000, len(code)
    names = b'DATA\0'.ljust(8,b'\0')+b'ALPHA.DAT;1\0'.ljust(16,b'\0')+b'BETA.DAT;1\0'.ljust(16,b'\0')+b'ABSENT;1\0'
    return s, code.ljust(0x4000,b'\0')+names


def build():
    source,code=program()
    outputs={'cd-runtime.s':source.encode(),'cd-runtime.bin':code}
    license=(HERE/'LICENSE').read_bytes()
    for name,root,child,alpha,beta,alen,blen,reverse in [
        ('cd-runtime',20,48,64,72,5003,2051,False),
        ('cd-runtime-moved',27,60,80,96,4096,6145,True),
    ]:
        iso=bytearray(a.build(code,license,'ORIGINAL RUNTIME CD','RUNTIME-001 V1.000'))
        iso.extend(bytes(128*2048-len(iso)))
        iso[20*2048:]=bytes(len(iso)-20*2048)
        struct.pack_into('>IIIIII',iso,0xE0,0x1000,0,0x060FE000,0,BASE,0xDEADBEEC)
        aip='bsr aip_entered\nnop\naip_entered:\nsts pr,r0\nload 0x260102f0,r1\nmov.l r0,@r1\nload 0x060020f0,r1\nmov.l @r1,r1\njmp @r1\nnop\n'
        iso[0xE20:0xE20+len(a.assemble(aip,base=0x06002E20))]=a.assemble(aip,base=0x06002E20)
        iso[16*2048+80:16*2048+88]=a.both32(128)
        iso[16*2048+156:16*2048+190]=a.record(b'\0',root,2048,True)
        # Both path table entries describe DATA as well as root.
        for at,endian in [(18,'<'),(19,'>')]:
            table=b'\1\0'+struct.pack(endian+'IH',root,1)+b'\0\0'
            table+=b'\4\0'+struct.pack(endian+'IH',child,1)+b'DATA'
            iso[at*2048:(at+1)*2048]=table.ljust(2048,b'\0')
        iso[16*2048+132:16*2048+140]=a.both32(22)
        children=[a.record(b'DATA',child,4096 if reverse else 2048,True),a.record(b'LICENSE.TXT;1',120,len(license))]
        if reverse: children.reverse()
        records=a.record(b'\0',root,2048,True)+a.record(b'\1',root,2048,True)+a.record(b'RUNTIME.BIN;1',32,len(code))+b''.join(children)
        iso[root*2048:root*2048+len(records)]=records
        records=a.record(b'\0',child,4096 if reverse else 2048,True)+a.record(b'\1',root,2048,True)
        files=[a.record(b'ALPHA.DAT;1',alpha,alen),a.record(b'BETA.DAT;1',beta,blen)]
        if reverse:
            files.reverse()
            records+=files[0]
            records=records.ljust(2048,b'\0')+files[1]
        else: records+=b''.join(files)
        iso[child*2048:child*2048+len(records)]=records
        iso[32*2048:32*2048+len(code)]=code
        iso[alpha*2048:alpha*2048+alen]=bytes((i*17+3+(i>>8))&255 for i in range(alen))
        iso[beta*2048:beta*2048+blen]=bytes(((i*29+7)^(i>>4))&255 for i in range(blen))
        iso[120*2048:120*2048+len(license)]=license
        outputs[name+'.iso']=bytes(iso)
    return outputs


if __name__=='__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for name,data in build().items():
        path=HERE/name
        if args.check: assert path.read_bytes()==data,f'Rebuild {name}'
        else: path.write_bytes(data)
        print(f'{name}: SHA-256 {hashlib.sha256(data).hexdigest()}')
