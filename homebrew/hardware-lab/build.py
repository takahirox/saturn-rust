#!/usr/bin/env python3
"""Original hardware laboratory disc + BINARY stereo track. MIT: LICENSE.

Guest source is original SH-2 assembly. The sound CPU and SCU DSP use the
repository's original independently authored programs, with their MIT license.
Every result is computed by instructions and device MMIO, not host game hooks.
"""
import argparse
import hashlib
import importlib.util
from pathlib import Path
import struct
HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
def module(name,path):
    spec=importlib.util.spec_from_file_location(name,path)
    m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
a=module('assembler',ROOT/'homebrew/star-collector/build.py')
snd=module('sound',ROOT/'tests/fixtures/build_sound.py')
scu=module('scu',ROOT/'tests/fixtures/build_scu.py')
BASE=0x06004000
R=0x06010000

def store(address,value,width='l'):
    return f'load {address:#x},r1\nload {value:#x},r0\nmov.{width} r0,@r1\n'
def call(command,b=0,c=0,d=0):
    return f'load {command:#x},r4\nload {b:#x},r5\nload {c:#x},r6\nload {d:#x},r7\nbsr command\nnop\n'

def main_source():
    s='start:\nmov #-16,r0\nldc r0,sr\nload 0x06000000,r0\nldc r0,vbr\n'
    for command in [3,7]:
        s+=store(0x20100063,1,'b')+store(0x2010001F,command,'b')
        s+=f'load 0x20100063,r1\nstop_{command}:\nmov.b @r1,r0\ntst #1,r0\nbf stop_{command}\n'
    for off in range(0,64,4): s+=store(R+off,0)
    s+=store(0x06000114,BASE+0x1A00)+store(0x0600012C,BASE+0x1C00)+store(0x06000140,BASE+0x1E00)
    s+=store(0x25FE00A0,0x37DF)+store(0x25FE00A8,1)
    s+=store(0x2589000C,4,'w')
    s+='mov #0,r0\nldc r0,sr\n'
    # Real word backup access, preserving the odd physical lane across restarts.
    s+='load 0x20180000,r1\nmov.w @r1,r0\nadd #1,r0\nmov.w r0,@r1\nload 0x06010024,r1\nmov.l r0,@r1\n'
    s+=store(0x06000250,BASE+0x1800)
    s+=store(R+0xC,35)
    s+=store(0x20100063,1,'b')+store(0x2010001F,2,'b')
    s+='load 0x06010010,r1\nslave_wait:\nmov.l @r1,r0\nmov #42,r2\ncmp/eq r2,r0\nbf slave_wait\n'
    # SCU DSP program executes 7*6 + 7*6 and raises a guest interrupt.
    for address,value in [(0x25FE0088,0),(0x25FE008C,7),(0x25FE0088,64),(0x25FE008C,6),(0x25FE0080,0x8000)]: s+=store(address,value)
    for word in [0x00001E00]+scu.PRODUCT:s+=store(0x25FE0084,word)
    s+=store(0x25FE0080,0x8000)+store(0x25FE0080,0x10000)
    # Copy original 68k reset/vector/code data through SH-2 instructions.
    s+='load 0x0600a000,r2\nload 0x25a00000,r3\nload 0x1100,r4\ncopy_sound:\nmov.l @r2+,r0\nmov.l r0,@r3\nadd #4,r3\ndt r4\nbf copy_sound\n'
    s+='load 0x20100063,r1\nsmpc_wait:\nmov.b @r1,r0\ntst #1,r0\nbf smpc_wait\n'
    s+=store(0x20100063,1,'b')+store(0x2010001F,6,'b')
    # CD audio is routed through an actual 68k-uploaded effects program.
    # No direct EXTS bypass can make the new scene audible if DSP is absent.
    s+=store(0x25B00216,0,'w')+store(0x25B00236,0,'w')
    s+=call(0x3000)
    # Runtime read of source-authored sector 210 (LBA60), independent of boot.
    s+=call(0x1080,210,0x80,1)
    s+='sector_wait:\n'+call(0x5100)+'mov #1,r2\ncmp/eq r2,r0\nbf sector_wait\n'
    s+=call(0x6300,0,0,1) # Get and delete: restart must read a fresh sector.
    # SCU DMA consumes the CD FIFO with fixed A-bus source.
    for off,val in [(0,0x05818000),(4,0x06011000),(8,2048),(12,2),(20,7),(16,0x101)]:s+=store(0x25FE0000+off,val)
    s+='load 0x06010004,r1\ndma_wait:\nmov.l @r1,r0\ntst r0,r0\nbt dma_wait\n'
    s+=call(0x0600)
    s+='load 0x06011000,r2\nload 512,r3\nmov #0,r4\nsum_data:\nmov.l @r2+,r0\nadd r0,r4\ndt r3\nbf sum_data\nload 0x06010014,r1\nmov.l r4,@r1\n'
    # CD-DA begins at track 2/FAD214 and repeats through guest CD commands.
    s+=call(0x1080,214,0x0F80,8)
    s+='load 0x06006200,r1\njsr @r1\nnop\n'
    # Signal completion only after actual slave, DSP and DMA results are present.
    s+='load 0x06010000,r1\ndsp_wait:\nmov.l @r1,r0\ntst r0,r0\nbt dsp_wait\n'
    # Wait for real sound-CPU initialization, then log
    # the executing effects-DSP's independently known .5*0x120000 result.
    s+='load 0x25a00800,r1\nsound_wait:\nmov.l @r1,r0\ntst r0,r0\nbt sound_wait\n'
    # EFREG is cleared at each sample start, then instruction 27 publishes
    # this constant. Observe a full 44.1-kHz period instead of racing that
    # clear with a single read. OR preserves the constant and the zero-gain
    # mutation; no expected DSP value is supplied by the guest.
    # 128 iterations * at least 5 main clocks > 26,874,100 / 44,100.
    s+='load 0x25b00ec2,r1\nload 128,r2\nmov #0,r4\neffect_read:\nmov.w @r1,r0\nor r0,r4\ndt r2\nbf effect_read\nextu.w r4,r0\nload 0x06010028,r1\nmov.l r0,@r1\n'
    s+=store(R+0x20,0x600D)
    s+=store(0x20100079,0x60,'b')+store(0x2010007D,1,'b')+store(0x20100075,0x40,'b')
    s+='restart_wait:\nload 0x20100075,r1\nmov.b @r1,r0\ntst #8,r0\nbf restart_wait\nload 0x06004000,r1\njmp @r1\nnop\n'
    # Common command routine: R4–7 command words; returns CR4 in R0.
    s+='command:\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\nload 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\nadd #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\nload 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\nload 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n'
    return s

def build():
    program=bytearray(0xA400)
    def section(offset,source):
        code=a.assemble(source,base=BASE+offset)
        assert offset+len(code)<=len(program)
        program[offset:offset+len(code)]=code
        return len(code)
    main=main_source();assert section(0,main)<0x1800
    slave='load 0x0601000c,r1\nmov.l @r1,r0\nadd #7,r0\nload 0x06010010,r1\nmov.l r0,@r1\npark_slave:\nsleep\nbra park_slave\nnop\n'
    assert section(0x1800,slave)<0x200
    push='mov.l r0,@-r15\nmov.l r1,@-r15\n'
    pop='mov.l @r15+,r1\nmov.l @r15+,r0\nrte\nnop\n'
    dsp=push+'load 0x25fe0080,r1\nmov.l @r1,r0\n'+store(0x25FE0088,128)+'load 0x25fe008c,r1\nmov.l @r1,r0\nload 0x06010008,r1\nmov.l r0,@r1\n'+store(R,1)+pop
    dma=push+store(R+4,1)+pop
    cd=push+store(0x25890008,0xFFFB,'w')+store(0x25FE00A8,1)+'load 0x06010018,r1\nmov.l @r1,r0\nadd #1,r0\nmov.l r0,@r1\n'+pop
    for offset,source in [(0x1A00,dsp),(0x1C00,dma),(0x1E00,cd)]: assert section(offset,source)<0x200
    # VDP2 table-driven rotation/line/window scene, all source-authored MMIO.
    effects=(ROOT/'tests/fixtures/effects.s').read_text().split('load 0x600a000,r1')[0]
    video='load 0x0601001c,r1\nmov #1,r0\nmov.l r0,@r1\n'+effects+'rts\nnop\n'
    assert section(0x2200,video)<0x3E00
    program[0x6000:0xA400]=snd.sound_image(effects=True)
    iso=bytearray(a.build(bytes(program),(HERE/'LICENSE').read_bytes(),'HARDWARE LAB','HWLAB-001 V1.000'))
    # Independent 512-word runtime vector. Its byte mutation must change guest sum.
    for i in range(512):struct.pack_into('>I',iso,60*2048+i*4,(i+1)*3)
    audio=bytearray(2352*8)
    for i in range(len(audio)//4):
        left=(i%32-16)*512;right=(15-i%32)*256
        struct.pack_into('<hh',audio,i*4,left,right)
    # Cooked data and separate original PCM keep media generation independent
    # of the emulator implementation under test.
    cue='FILE "hardware-lab.iso" BINARY\n  TRACK 01 MODE1/2048\n    INDEX 01 00:00:00\nFILE "hardware-lab-audio.bin" BINARY\n  TRACK 02 AUDIO\n    INDEX 01 00:00:00\n'
    return {'hardware-lab.iso':bytes(iso),'hardware-lab-audio.bin':bytes(audio),'hardware-lab.cue':cue.encode()},main,slave

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--check',action='store_true');args=p.parse_args()
    outputs,main,slave=build()
    outputs['main.s']=main.encode();outputs['slave.s']=slave.encode()
    for name,data in outputs.items():
        target=HERE/name if name.endswith('.s') else ROOT/'web'/name
        if args.check:assert target.read_bytes()==data,f'Rebuild {name}'
        else:target.write_bytes(data)
        if not name.endswith('.s'):print(f'{"Verified" if args.check else "Built"} {name}; SHA-256 {hashlib.sha256(data).hexdigest()}')
