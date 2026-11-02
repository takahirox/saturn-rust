#!/usr/bin/env python3
"""Original MIT CD fixture containers; adjacent LICENSE.

Regenerate with MAME chdman createcd (0.289 used here), one encoder thread.
--check needs only Python: independently reconstruct the exact logical CHD CD
units, verify raw SHA-1/header, and the frozen compressed-file SHA-256 inventory.
Native/browser tests actually decode the frozen containers against source media.
No decoder, external program, firmware download or compression is used by --check.
"""
import argparse
import binascii
import hashlib
import json
import struct
import subprocess
import tempfile
from build_scu import assembler as assembler
from build_cd_drive import store
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = HERE / 'chd'

def inputs():
    stream = (HERE / 'cd-stream.iso').read_bytes()
    changed = bytearray(stream)
    changed[64 * 2048] ^= 0x1F
    iso = (ROOT / 'web/hardware-lab.iso').read_bytes()
    audio = (ROOT / 'web/hardware-lab-audio.bin').read_bytes()
    mode2 = (HERE / 'mode2.bin').read_bytes()
    # Nonzero stored INDEX 00, non-four-frame track lengths, an unstored
    # PREGAP and POSTGAP. No SRW1, manifest or proprietary input on the disc.
    gaps = 'FILE "data.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 00 00:00:00\nINDEX 01 00:00:02\nFILE "mode2.bin" BINARY\nTRACK 03 MODE2/2352\nPREGAP 00:00:03\nINDEX 01 00:00:00\nPOSTGAP 00:00:02\n'
    mixed = 'FILE "data.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "audio.bin" BINARY\nTRACK 02 AUDIO\nINDEX 01 00:00:00\n'
    single = 'FILE "data.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\n'
    rw_iso = (HERE/'cd-rw.iso').read_bytes()
    rw_audio = (HERE/'cd-rw-audio.bin').read_bytes()
    symbols = (HERE/'cd-rw.srw').read_bytes()[12:]
    interleaved = b''.join(rw_audio[i:i+2352]+symbols[i//2352*96:i//2352*96+96] for i in range(0,len(rw_audio),2352))
    rw_cue = 'FILE "data.iso" BINARY\nTRACK 01 MODE1/2048\nINDEX 01 00:00:00\nFILE "rw.bin" BINARY\nTRACK 02 AUDIO RW_RAW\nINDEX 01 00:00:00\n'
    raw_cue = 'FILE "mode1.bin" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\nFILE "xa.bin" BINARY\nTRACK 02 MODE2/2352\nINDEX 01 00:00:00\nFILE "mode2.bin" BINARY\nTRACK 03 MODE2/2352\nINDEX 01 00:00:00\nFILE "audio.bin" BINARY\nTRACK 04 AUDIO\nINDEX 01 00:00:00\n'
    raw_files = [('mode1.bin', (HERE/'large-media-mode1.raw').read_bytes(), 2352), ('xa.bin', (HERE/'large-media-xa.raw').read_bytes(), 2352), ('mode2.bin', mode2, 2352), ('audio.bin', audio, 2352)]
    return {
        'raw': (raw_cue, raw_files),
        'subcode': (rw_cue, [('data.iso', rw_iso, 2048), ('rw.bin', interleaved, 2448)]),
        'data': (single, [('data.iso', stream, 2048)]),
        'changed': (single, [('data.iso', bytes(changed), 2048)]),
        'mixed': (mixed, [('data.iso', iso, 2048), ('audio.bin', audio, 2352)]),
        'gaps': (gaps, [('data.iso', iso, 2048), ('audio.bin', audio, 2352), ('mode2.bin', mode2, 2352)]),
    }

def units(files):
    result = bytearray()
    for name, data, stride in files:
        for offset in range(0, len(data), stride):
            frame = bytearray(data[offset:offset+stride] + bytes(2448-stride))
            if name in ['audio.bin', 'rw.bin']:
                pcm = frame[:2352]
                frame[0:2352:2], frame[1:2352:2] = pcm[1::2], pcm[0::2]
            result += frame
        result += bytes((-len(data)//stride % 4)*2448)
    return bytes(result)

def probe():
    """Original guest: TOC, full first-track FIFO bytes and stereo CD playback.

    Uses only guest instructions/MMIO. Parameters are in the synthetic ROM,
    independent of production media parsing and identical in native/Wasm tests.
    """
    def call(a,b=0,c=0,d=0):
        return f'load {a:#x},r4\nload {b:#x},r5\nload {c:#x},r6\nload {d:#x},r7\nbsr command\nnop\n'
    s='mov #-16,r0\nldc r0,sr\n'+call(0x3000)+call(0x0200)
    s+='load 0x25818000,r8\nload 0x06020000,r9\nload 204,r10\ntoc_copy:\nmov.w @r8,r0\nmov.w r0,@r9\nadd #2,r9\ndt r10\nbf toc_copy\n'+call(0x0600)
    s+='load 0x100,r1\nmov.w @r1,r12\nextu.w r12,r12\nload 0x06021000,r9\n'
    s+=call(0x1080,150,0x80,128).replace('load 0x80,r7', 'mov r12,r7') # source track length patched from ROM
    # Read exactly the first data track, using the ROM parameter.
    s+='sector_wait:\n'+call(0x5100)+'tst #255,r0\nbt sector_wait\n'+call(0x6300,0,0,1)
    s+='load 1024,r10\nsector_copy:\nmov.w @r8,r0\nmov.w r0,@r9\nadd #2,r9\ndt r10\nbf sector_copy\n'+call(0x0600)
    s+='dt r12\nbf sector_wait\n'+call(0x1100,150)
    s+=store(0x25B00400,0x020F)+store(0x25B00216,0x00FF)+store(0x25B00236,0x00EF)
    s+='load 0x102,r1\nmov.w @r1,r5\nextu.w r5,r5\ntst r5,r5\nbt done\nload 0x1080,r4\nload 0x80,r6\nmov #8,r7\nbsr command\nnop\ndone:\n'
    s+='load 0x600d,r0\nload 0x0601fffc,r1\nmov.l r0,@r1\npark:\nsleep\nbra park\nnop\n'
    s+='command:\nload 0x25890008,r1\nmov #-2,r0\nmov.w r0,@r1\nload 0x25890018,r1\nmov.w r4,@r1\nadd #4,r1\nmov.w r5,@r1\nadd #4,r1\nmov.w r6,@r1\nadd #4,r1\nmov.w r7,@r1\nload 0x25890008,r1\ncommand_wait:\nmov.w @r1,r0\ntst #1,r0\nbt command_wait\nload 0x25890024,r1\nmov.w @r1,r0\nextu.w r0,r0\nrts\nnop\n'
    code=assembler.assemble(s,base=0x200)
    rom=bytearray(0x200+len(code))
    struct.pack_into('>II',rom,0,0x200,0x06100000)
    struct.pack_into('>HH',rom,0x100,128,0)
    rom[0x200:]=code
    lines=['# Original MIT CHD equivalence guest. Adjacent LICENSE.']
    rom.extend(bytes(-len(rom)%16))
    for at in range(0,len(rom),16):
        if any(rom[at:at+16]): lines.append(f'{at:04X} '+' '.join(f'{w:04X}' for w in struct.unpack('>8H',rom[at:at+16])))
    return s, '\n'.join(lines)+'\n'

def self_cycle():
    # A valid v5 Huffman map/CRC whose only hunk refers to itself. The adapter
    # must reject it before the decoder's recursive self-hunk reader is called.
    header=bytearray((OUT/'data-cdlz.chd').read_bytes()[:124])
    header[64:124]=bytes(60)
    text=b'TRACK:1 TYPE:MODE1 SUBTYPE:NONE FRAMES:4 PREGAP:0 PGTYPE:MODE1 PGSUB:NONE POSTGAP:0\0'
    metadata=b'CHT2'+struct.pack('>I',0x01000000|len(text))+bytes(8)+text
    struct.pack_into('>QQQII',header,32,4*2448,124+len(metadata),124,4*2448,2448)
    # Tree: codes0..4 absent, code5 length1, codes6..15 absent via RLE.
    # Symbol5=0 followed by a one-bit self offset0 and zero padding.
    encoded=bytes.fromhex('000001110700')
    crc=binascii.crc_hqx(bytes([5])+bytes(11),0xFFFF)
    map_header=struct.pack('>I',len(encoded))+bytes(6)+struct.pack('>H',crc)+bytes([0,1,0,0])
    return header+metadata+map_header+encoded

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--check', action='store_true')
    p.add_argument('--chdman', default='chdman')
    args = p.parse_args()
    OUT.mkdir(exist_ok=True)
    source, rom = probe()
    for name, text in [('probe.s', source), ('probe.hex', rom)]:
        if args.check: assert (OUT/name).read_text() == text
        else: (OUT/name).write_text(text)
    inventory = {}
    for name, (cue, files) in inputs().items():
        codecs = ['default', 'cdlz', 'cdzl', 'cdfl', 'none'] if name in ['data', 'mixed'] else ['cdlz']
        logical = units(files)
        for codec in codecs:
            filename = f'{name}-{codec}.chd'
            path = OUT / filename
            if not args.check:
                with tempfile.TemporaryDirectory(prefix='saturn-chd-fixture-') as tmp:
                    tmp = Path(tmp)
                    (tmp / 'disc.cue').write_text(cue)
                    for file, data, _ in files:
                        (tmp / file).write_bytes(data)
                    subprocess.run([args.chdman, 'createcd', '-i', str(tmp/'disc.cue'), '-o', str(path), '-f', '-np', '1', '-c', 'cdlz,cdzl,cdfl' if codec == 'default' else codec], check=True, stdout=subprocess.DEVNULL)
            data = path.read_bytes()
            assert data[:8] == b'MComprHD' and struct.unpack_from('>II', data, 8) == (124, 5)
            assert struct.unpack_from('>Q', data, 32)[0] == len(logical), filename
            assert data[64:84] == hashlib.sha1(logical).digest() or codec == 'none', filename
            inventory[filename] = {'sha256': hashlib.sha256(data).hexdigest(), 'raw_sha1': hashlib.sha1(logical).hexdigest(), 'logical_bytes': len(logical)}
    invalid=bytes(self_cycle())
    if args.check: assert (OUT/'self-cycle.chd').read_bytes() == invalid
    else: (OUT/'self-cycle.chd').write_bytes(invalid)
    for name, (cue, _) in inputs().items():
        if args.check: assert (OUT/f'{name}.cue').read_text() == cue
        else: (OUT/f'{name}.cue').write_text(cue)
    manifest = json.dumps(inventory, indent=2, sort_keys=True) + '\n'
    if args.check: assert (OUT/'inventory.json').read_text() == manifest, 'Rebuild CHD fixtures'
    else: (OUT/'inventory.json').write_text(manifest)
    print(f'{"Verified" if args.check else "Built"} {len(inventory)} original MIT CHDs; complete logical CD-unit SHA-1 and compressed SHA-256 inventory')

if __name__ == '__main__': main()
