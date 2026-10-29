#!/usr/bin/env python3
"""Original MIT firmware-executed Saturn/ISO first-read discovery. Adjacent LICENSE.

This generates ordinary SH-2 machine code, not a host loader. Firmware metadata
and transfer decisions use bytes fetched from the scheduled CD hardware only.
"""
import struct
from build_scu import assembler as a, store
from build_clock import put8, put16

R = 0x26010000
IP = 0x06002000
STAGE = 0x06017000
DIRECTORY = 0x06018000
M = {name: R+0x300+i*4 for i,name in enumerate([
    'ip_size','entry','stack','slave_stack','destination','volume','root','root_size',
    'file','size','loaded','phase','reads','slave_ready','descriptors_end'])}
_serial = 0


def call(label):
    return f'load {label},r0\njsr @r0\nnop\n'


def guard(branch, label='metadata_error'):
    """Condition for failure; absolute target avoids SH-2 branch-range assumptions."""
    global _serial
    _serial += 1
    skip=f'guard_{_serial}'
    inverse={'bt':'bf','bf':'bt'}[branch]
    return f'{inverse} {skip}\nload {label},r0\njmp @r0\nnop\n{skip}:\n'


def equal(value, label='metadata_error'):
    return f'load {value:#x},r2\ncmp/eq r2,r0\n'+guard('bf',label)


def get(name, reg):
    return f'load {M[name]:#x},r1\nmov.l @r1,{reg}\n'


def save(name, reg='r0'):
    return f'load {M[name]:#x},r1\nmov.l {reg},@r1\n'


def field(address, offset=0, dual=False):
    s=f'load {address:#x},r1\n' if isinstance(address,int) else f'mov {address},r1\n'
    if offset: s+=f'load {offset},r0\nadd r0,r1\n'
    return s+call('dual32' if dual else 'be32')


def byte(address, offset=0):
    s=f'load {address:#x},r1\n' if isinstance(address,int) else f'mov {address},r1\n'
    if offset:s+=f'add #{offset},r1\n'
    return s+'mov.b @r1,r0\nextu.b r0,r0\n'


def sector(lba, destination):
    return f'load {lba},r10\nload {destination:#x},r11\n'+call('read_sector')


def discovery():
    global _serial
    _serial=0
    s=store(M['phase'],1)+sector(0,IP)
    for off,value in [(0,0x53454741),(4,0x20534547),(8,0x41534154),(12,0x55524e20)]:
        s+=field(IP+off)+equal(value)
    # Bounded one-to-four distinct area symbols, followed by spaces. These are
    # metadata only; no proprietary security/area object is executed.
    s+='load 0x06002040,r8\nmov #0,r12\nmov #0,r13\nmov #0,r14\nmov #16,r7\narea_loop:\n'
    s+='mov.b @r8+,r0\nextu.b r0,r0\nload 32,r2\ncmp/eq r2,r0\nbt area_space\n'
    s+='tst r14,r14\n'+guard('bf')
    for ch,mask in [('J',1),('T',2),('U',4),('E',8)]:
        s+=f'load {ord(ch)},r2\ncmp/eq r2,r0\nbf area_not_{ch}\nmov #{mask},r6\nbra area_symbol\nnop\narea_not_{ch}:\n'
    s+='load metadata_error,r0\njmp @r0\nnop\narea_symbol:\ntst r6,r13\n'+guard('bf')
    s+='or r6,r13\nadd #1,r12\nload 5,r2\ncmp/hs r2,r12\n'+guard('bt')
    s+='bra area_next\nnop\narea_space:\nmov #1,r14\narea_next:\ndt r7\nbf area_loop\ntst r12,r12\n'+guard('bt')
    for offset in [0xD0,0xD4,0xD8,0xDC,0xE4,0xF8,0xFC]:
        s+=field(IP+offset)+equal(0)
    s+='shll2 r12\nshll2 r12\nshll r12\nload 0x06002e00,r0\nadd r12,r0\n'+save('entry')
    s+=field(IP+0xE0)+save('ip_size')
    s+='load 0x1000,r2\ncmp/hs r2,r0\n'+guard('bf')+'load 0x8001,r2\ncmp/hs r2,r0\n'+guard('bt')+'tst #3,r0\n'+guard('bf')
    # No BIOS libraries are installed. Default stacks follow ST-040 §4.3.
    for off,name,default in [(0xE8,'stack',0x06002000),(0xEC,'slave_stack',0x06001000)]:
        s+=field(IP+off)+'tst r0,r0\nbf '+name+'_specified\n'+f'load {default:#x},r0\nbra {name}_ok\nnop\n{name}_specified:\n'
        s+='tst #3,r0\n'+guard('bf')+'load 0x060e0000,r2\ncmp/hs r2,r0\n'+guard('bf')+'load 0x060fe001,r2\ncmp/hs r2,r0\n'+guard('bt')
        s+=name+'_ok:\n'+save(name)
    s+=field(IP+0xF0)+save('destination')+'load 0x060d0000,r2\ncmp/hs r2,r0\n'+guard('bt')+'tst r0,r0\nbt destination_ok\ntst #3,r0\n'+guard('bf')
    s+=get('ip_size','r2')+'load 0x06002000,r3\nadd r3,r2\nadd #4,r2\ncmp/hs r2,r0\n'+guard('bf')
    s+='destination_ok:\n'+store(M['phase'],2)
    # Read remaining IP sectors, including its original AIP, before discovery.
    s+=get('ip_size','r12')+'load 2048,r0\nsub r0,r12\nload 1,r10\nload 0x06002800,r11\nip_loop:\ntst r12,r12\nbt ip_done\n'+call('read_sector')
    s+='load 2048,r0\ncmp/hs r0,r12\nbf ip_done\nsub r0,r12\nadd #1,r10\nadd r0,r11\nbra ip_loop\nnop\nip_done:\n'
    # Scan a finite descriptor set, requiring exactly one PVD and a terminator.
    s+='load 16,r10\nmov #0,r14\ndescriptor_loop:\nload 32,r2\ncmp/hs r2,r10\n'+guard('bt')
    s+=f'load {STAGE:#x},r11\n'+call('read_sector')
    s+=field(STAGE+1)+equal(0x43443030)+byte(STAGE+5)+equal(ord('1'))+byte(STAGE+6)+equal(1)
    s+=byte(STAGE)+'load 255,r2\ncmp/eq r2,r0\nbt.l descriptor_end\nload 1,r2\ncmp/eq r2,r0\nbt descriptor_primary\n'
    s+='tst r0,r0\nbt.l descriptor_next\nload 2,r2\ncmp/eq r2,r0\n'+guard('bf')+'bra descriptor_next\nnop\ndescriptor_primary:\n'
    s+='tst r14,r14\n'+guard('bf')+'mov #1,r14\n'
    s+=field(STAGE+80,dual=True)+save('volume')+'mov r0,r6\nload 0x260102f8,r1\nmov.l @r1,r0\nload 0xffffff,r2\nand r2,r0\nload 150,r2\nsub r2,r0\ncmp/hs r6,r0\n'+guard('bf')+'mov r6,r0\n'+'load 18,r2\ncmp/hs r2,r0\n'+guard('bf')+'load 450001,r2\ncmp/hs r2,r0\n'+guard('bt')
    for off,value in [(120,0x01000001),(124,0x01000001),(128,0x00080800)]:
        s+=field(STAGE+off)+equal(value)
    s+=byte(STAGE+881)+equal(1)
    s+=f'load {STAGE+156:#x},r8\n'+call('root_record')+byte('r8')+equal(34)+byte('r8',33)+equal(0)
    s+=field('r8',2,True)+save('root')+field('r8',10,True)+save('root_size')
    s+='tst r0,r0\n'+guard('bt')+'load 0x4001,r2\ncmp/hs r2,r0\n'+guard('bt')+'load 2047,r2\ntst r2,r0\n'+guard('bf')
    s+='descriptor_next:\nadd #1,r10\nbra descriptor_loop\nnop\ndescriptor_end:\ntst r14,r14\n'+guard('bt')
    s+='add #1,r10\n'+save('descriptors_end','r10')+get('volume','r0')+'cmp/hs r10,r0\n'+guard('bf')
    s+=get('root','r0')+'cmp/hs r10,r0\n'+guard('bf')+'mov r0,r6\n'+get('root_size','r7')+('shlr r7\n'*11)+'add r6,r7\n'+get('volume','r2')+'cmp/hs r7,r2\n'+guard('bf')
    s+=store(M['phase'],3)+get('root','r10')+get('root_size','r12')+f'load {DIRECTORY:#x},r11\nroot_load:\n'+call('read_sector')
    s+='load 2048,r0\nadd #1,r10\nadd r0,r11\nsub r0,r12\ntst r12,r12\nbf root_load\n'
    # Scan all records; padding advances to the next sector. Only record index
    # 2 is selected; a directory at index 2 is an error, never skipped.
    s+=f'load {DIRECTORY:#x},r8\n'+get('root_size','r14')+'add r8,r14\nmov #0,r12\nroot_scan:\ncmp/hs r14,r8\nbt.l root_done\nmov.b @r8,r0\nextu.b r0,r13\ntst r13,r13\nbt.l root_padding\n'
    s+='mov r13,r0\ntst #1,r0\n'+guard('bf')+'load 34,r2\ncmp/hs r2,r13\n'+guard('bf')+'mov r8,r0\nadd r13,r0\ncmp/hs r0,r14\n'+guard('bf')
    s+='mov r8,r0\nload 2047,r2\nand r2,r0\nadd r13,r0\nload 2049,r2\ncmp/hs r2,r0\n'+guard('bt')
    s+=call('record')
    s+='load 2,r2\ncmp/hs r2,r12\nbt.l root_child\n'+call('root_record')
    s+=field('r8',2,True)+get('root','r2')+'cmp/eq r2,r0\n'+guard('bf')
    s+=field('r8',10,True)+get('root_size','r2')+'cmp/eq r2,r0\n'+guard('bf')
    s+=byte('r8',33)+'cmp/eq r12,r0\n'+guard('bf')+'bra root_next\nnop\nroot_child:\n'
    s+=byte('r8',32)+'load 1,r2\ncmp/eq r2,r0\nbf root_name_ok\n'+byte('r8',33)+'load 2,r2\ncmp/hs r2,r0\n'+guard('bf')+'root_name_ok:\n'
    s+='load 2,r2\ncmp/eq r2,r12\nbf.l root_next\n'+get('destination','r0')+'tst r0,r0\nbt.l root_next\n'+byte('r8',25)+'tst #2,r0\n'+guard('bf')
    s+=field('r8',2,True)+save('file')+get('root','r2')+get('root_size','r4')+('shlr r4\n'*11)+'add r4,r2\ncmp/hs r2,r0\n'+guard('bf')
    s+=field('r8',10,True)+save('size')+'tst r0,r0\n'+guard('bt')+'load 65537,r2\ncmp/hs r2,r0\n'+guard('bt')
    s+='root_next:\nadd r13,r8\nadd #1,r12\nbra root_scan\nnop\nroot_padding:\n'
    # Verify all padding is zero; no record may cross a sector boundary.
    s+='mov r8,r6\nload 2047,r2\nor r2,r6\nadd #1,r6\nroot_padding_byte:\nmov.b @r8+,r0\ntst r0,r0\n'+guard('bf')+'cmp/hs r6,r8\nbf root_padding_byte\nbra root_scan\nnop\nroot_done:\n'
    s+='load 2,r2\ncmp/hs r2,r12\n'+guard('bf')
    s+=get('destination','r0')+'tst r0,r0\nbt no_first_read\nload 3,r2\ncmp/hs r2,r12\n'+guard('bf')+'no_first_read:\n'
    s+=get('destination','r13')+'tst r13,r13\nbt.l handoff\n'+get('size','r12')+'mov r13,r0\nadd r12,r0\n'
    # Two RAM windows exclude the IP, metadata, logs, vectors, stack guard areas
    # and final system-owned 4 KiB. A bounded file cannot wrap a u32 here.
    s+='load 0x06010000,r2\ncmp/hs r0,r2\nbt load_bounds_ok\nload 0x06020000,r2\ncmp/hs r2,r13\n'+guard('bf')+'load 0x060d0000,r2\ncmp/hs r0,r2\n'+guard('bf')+'load_bounds_ok:\n'
    s+=store(M['phase'],4)+get('file','r10')+'file_load:\n'+f'load {STAGE:#x},r11\n'+call('read_sector')
    s+='load 2048,r14\ncmp/hs r14,r12\nbt copy_count_ok\nmov r12,r14\ncopy_count_ok:\nmov r14,r3\nmov r11,r1\ncopy_bytes:\nmov.b @r1+,r0\nmov.b r0,@r13\nadd #1,r13\ndt r3\nbf copy_bytes\nsub r14,r12\n'+get('loaded','r0')+'add r14,r0\n'+save('loaded')
    s+='add #1,r10\ntst r12,r12\nbf file_load\nhandoff:\n'+store(M['phase'],5)
    s+=get('slave_stack','r0')+save('slave_ready')
    s+='slave_stack_wait:\n'+get('slave_ready','r0')+'tst r0,r0\nbf slave_stack_wait\n'
    s+=get('stack','r15')+get('entry','r1')+'jmp @r1\nnop\n'
    return s


def routines():
    s=''
    # CD read uses one fresh sector at a time; GetDelete + DataEnd prevents stale
    # partition reuse. DMA always drains the full sector into safe staging RAM.
    s+='read_sector:\nsts.l pr,@-r15\n'
    s+=get('volume','r0')+'tst r0,r0\nbt sector_bounds_ok\ncmp/hs r0,r10\n'+guard('bt','io_error')+'sector_bounds_ok:\n'
    s+=put16(0x25890008,0xFFEB)
    s+='mov r10,r5\nload 150,r0\nadd r0,r5\nmov r5,r4\nshlr16 r4\nload 0x1080,r0\nor r0,r4\nextu.w r5,r5\nload 0x80,r6\nmov #1,r7\n'+call('command')
    s+='load 0x25890008,r1\nload 200000,r3\nsector_wait:\nmov.w @r1,r0\ntst #16,r0\nbf sector_ready\ndt r3\nbf sector_wait\nload io_error,r0\njmp @r0\nnop\nsector_ready:\n'
    s+='load 0x5100,r4\nmov #0,r5\nmov #0,r6\nmov #0,r7\n'+call('command')+equal(1,'io_error')
    s+='load 0x25890008,r1\nmov.w @r1,r0\ntst #128,r0\n'+guard('bt','io_error')+put16(0x25890008,0xFF7F)
    s+='load 0x6300,r4\nmov #0,r5\nmov #0,r6\nmov #1,r7\n'+call('command')
    s+='load 0x25890008,r1\nmov.w @r1,r0\ntst #2,r0\n'+guard('bt','io_error')
    # Fence DMA with a monotonically increasing guest interrupt counter.
    s+=get('reads','r0')+'add #1,r0\nmov r0,r7\n'
    for off,value in [(0,0x05818000),(8,2048),(12,2),(20,7)]:s+=store(0x25FE0000+off,value)
    s+='load 0x25fe0004,r1\nmov.l r11,@r1\n'+store(0x25FE0010,0x101)
    s+='load 0x26010204,r1\nload 200000,r3\ndma_wait:\nmov.l @r1,r0\ncmp/eq r7,r0\nbt dma_ready\ndt r3\nbf dma_wait\nload io_error,r0\njmp @r0\nnop\ndma_ready:\nmov r7,r0\n'+save('reads')
    s+='load 0x600,r4\nmov #0,r5\nmov #0,r6\nmov #0,r7\n'+call('command')
    s+='load 0x2589001c,r1\nmov.w @r1,r0\nextu.w r0,r0\n'+equal(1024,'io_error')+store(R+0x210,1024)
    s+='load 0x25890008,r1\nmov.w @r1,r0\nload 0x82,r2\nand r2,r0\n'+equal(0x80,'io_error')+'lds.l @r15+,pr\nrts\nnop\n'
    # All metadata fields can be unaligned directory fields. Decode with byte
    # instructions and compare both ISO byte-order copies instead of assuming BE.
    s+='be32:\nmov #0,r0\nmov #4,r3\nbe32_loop:\nshll8 r0\nmov.b @r1+,r2\nextu.b r2,r2\nor r2,r0\ndt r3\nbf be32_loop\nrts\nnop\n'
    s+='dual32:\nsts.l pr,@-r15\nmov r1,r5\nadd #3,r5\nmov #0,r4\nmov #4,r3\nle32_loop:\nshll8 r4\nmov.b @r5,r2\nextu.b r2,r2\nor r2,r4\nadd #-1,r5\ndt r3\nbf le32_loop\nadd #4,r1\n'+call('be32')+'cmp/eq r4,r0\n'+guard('bf')+'lds.l @r15+,pr\nrts\nnop\n'
    # Record shape, supported flags and layout. Validate extents and lengths for
    # every record, including self/parent. XAR/interleave/multi-extent fail closed.
    s+='record:\nsts.l pr,@-r15\n'+byte('r8',1)+equal(0)
    s+=byte('r8',25)+'load 3,r2\nmov r0,r3\nand r2,r3\ncmp/eq r3,r0\n'+guard('bf')
    for offset in [26,27]:s+=byte('r8',offset)+equal(0)
    s+=field('r8',28)+equal(0x01000001)+byte('r8',32)+'tst r0,r0\n'+guard('bt')+'mov r0,r6\nadd #34,r0\nmov #-2,r2\nand r2,r0\ncmp/hs r0,r13\n'+guard('bf')
    s+=field('r8',2,True)+'mov r0,r6\n'+get('volume','r2')+'cmp/hs r2,r6\n'+guard('bt')
    s+=field('r8',10,True)+'mov r0,r7\nload 0x40000000,r2\ncmp/hs r2,r0\n'+guard('bt')+'load 2047,r2\nadd r2,r7\n'+('shlr r7\n'*11)+'add r6,r7\n'+get('volume','r2')+'cmp/hs r7,r2\n'+guard('bf')
    s+='lds.l @r15+,pr\nrts\nnop\n'
    s+='root_record:\nsts.l pr,@-r15\n'+byte('r8')+'load 34,r2\ncmp/hs r2,r0\n'+guard('bf')
    for offset,value in [(1,0),(25,2),(26,0),(27,0),(32,1),(33,0)]:
        # Parent's identifier is checked by the caller; PVD/self must be zero.
        if offset==33:continue
        s+=byte('r8',offset)+equal(value)
    s+=field('r8',28)+equal(0x01000001)+'lds.l @r15+,pr\nrts\nnop\n'
    # Stage-qualified errors never enter AIP, even after a partial file copy.
    for label,code in [('metadata_error',0xE002),('io_error',0xE003)]:
        s+=label+':\n'+store(R+0xFC,code)+'load stopped,r0\njmp @r0\nnop\n'
    return s
