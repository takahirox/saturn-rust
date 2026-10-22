"""Original sound-bus fault guests and independent frame vectors, MIT: LICENSE.

MC68000UM §§5.4.1/6.1/6.3.9.1, fig.6-7. No results sampled from any core.
PC literals use assembled boundaries and the declared word-boundary prefetch
policy, with deferred next-instruction fetch for MOVEM loads (UM table8-10);
all other expected fields below are source-authored constants.
"""
import struct


def install(image, words, base=0x1000):
    # One RAM result record is 32 bytes: 14-byte frame, padding, D0/D1/A0,
    # completed high-RAM word, padding. A6 is advanced by the guest handler.
    words += [0x4DF9,0,0x5900]  # LEA $5900,A6
    vectors = []
    cases = [
        ('read second word', 0x7FFFE, [0x2010], 0x7FFFE, 0x12345678,0xDEADBEEF,0x80000,0x2010,0x15,0xBABE),
        ('write second word',0x7FFFE,[0x2080],0x7FFFE,0x12345678,0xDEADBEEF,0x80000,0x2080,5,0x1234),
        ('five word immediate',0x7FFFE,[0x23FC,0x1234,0x5678,7,0xFFFE],0x7FFFE,0x12345678,0xDEADBEEF,0x80000,0x23FC,5,0x1234),
        ('predecrement low first',0x80002,[0x2100],0x7FFFE,0x12345678,0xDEADBEEF,0x80000,0x2100,5,0xBABE),
        ('movem completed register',0x7FFFC,[0x4CD8,3],0x80000,0xCAFEBABE,0xDEADBEEF,0x80000,0x4CD8,0x15,0xBABE),
        ('movem partial next register',0x7FFFA,[0x4CD8,3],0x7FFFE,0xABCDCAFE,0xDEADBEEF,0x80000,0x4CD8,0x15,0xBABE),
        ('address error',1,[0x2010],1,0x12345678,0xDEADBEEF,1,0x2010,0x15,0xBABE),
        ('user data fault',0x80000,[0x2010],0x80000,0x12345678,0xDEADBEEF,0x80000,0x2010,0x11,0xBABE),
        ('branch target fetch',0x80000,[0x4ED0],0x80000,0x12345678,0xDEADBEEF,0x80000,0x4ED0,0x16,0xBABE),
        ('sequential prefetch',0x7FFFC,[0x4ED0],0x7FFFC,3,0xDEADBEEF,0x80000,0x4E71,0x16,0x4E71),
        # PRM pp.4-125/4-173: these otherwise write-only operations first
        # read the destination. A failed read must be recorded as a read.
        ('scc destination read',0x80000,[0x50D0],0x80000,0x12345678,0xDEADBEEF,0x80000,0x50D0,0x15,0xBABE),
        ('move sr destination read',0x80000,[0x40D0],0x80000,0x12345678,0xDEADBEEF,0x80000,0x40D0,0x15,0xBABE),
    ]
    # A1 source faults before A0's destination predecrement. All integer
    # sizes and both BCD instructions execute, rather than classifying opcodes.
    for name,op,size in [('addx byte',0xD109,1),('addx word',0xD149,2),
                         ('addx long',0xD189,4),('subx byte',0x9109,1),
                         ('subx word',0x9149,2),('subx long',0x9189,4),
                         ('abcd',0xC109,1),('sbcd',0x8109,1)]:
        cases.append((name+' source fault',0x7FFFE,[0x43F9,8,size,op],
                      0x7FFFE,0x12345678,0xDEADBEEF,0x80000,op,0x15,0xBABE))
    # MOVEM loads perform a final word read after every selected register,
    # including an empty mask. Real sound-RAM bounds cause that cycle to fault.
    cases += [
        ('movem long terminal',0x7FFFC,[0x4CD8,1],0x80000,0xCAFEBABE,0xDEADBEEF,0x80000,0x4CD8,0x15,0xBABE),
        ('movem word terminal',0x7FFFE,[0x4C98,1],0x80000,0xFFFFBABE,0xDEADBEEF,0x80000,0x4C98,0x15,0xBABE),
        ('movem empty terminal',0x80000,[0x4CD8,0],0x80000,0x12345678,0xDEADBEEF,0x80000,0x4CD8,0x15,0xBABE),
        ('movem listed base terminal',0x7FFF8,[0x4CD8,0x0101],0x80000,0x1234ABCD,0xDEADBEEF,0x80000,0x4CD8,0x15,0xBABE),
    ]
    for name, a0, code, final_a0, d0, d1, fault, ir, ssw, retained in cases:
        # Restore guard data using ordinary MC68000 word writes before each case.
        for address,value in [(0x7FFF8,0x1234),(0x7FFFA,0xABCD),(0x7FFFC,0xCAFE),(0x7FFFE,0xBABE)]:
            words += [0x33FC,value,7,address&65535]
        if name == 'sequential prefetch':
            words += [0x33FC,0x7003,7,0xFFFC,0x33FC,0x4E71,7,0xFFFE]
        if name == 'user data fault':
            words += [0x207C,0,0x6000,0x4E60] # MOVEA #$6000,A0; MOVE A0,USP
        words += [0x41F9,a0>>16,a0&65535,0x203C,0x1234,0x5678,0x223C,0xDEAD,0xBEEF]
        # D6 carries a guest recovery label, not an assumed fault PC offset.
        resume = base + (len(words)+3+2+len(code))*2
        words += [0x2C3C,resume>>16,resume&65535]
        sr = 0x001B if name == 'user data fault' else 0x271B
        words += [0x46FC,sr] + code
        # UM §6.3.9.1: straight-line operand/branch faults have prefetched one
        # successor word; failed sequential lookahead has its own internal PC.
        pc = 0x80002 if name == 'sequential prefetch' else resume+(0 if len(code)==5 or ir in [0x4C98,0x4CD8] else 2)
        if name == 'sequential prefetch': sr=0x2710 # MOVEQ #3 retained X
        expected = [(ssw<<16)|(fault>>16), ((fault&65535)<<16)|ir,
                    (sr<<16)|(pc>>16), (pc&65535)<<16, d0,d1,final_a0,retained<<16]
        vectors.append((name,expected))
    handler = [0x3CD7,0x2CEF,2,0x3CEF,6,0x3CEF,8,0x2CEF,10,0x425E,
               0x2CC0,0x2CC1,0x2CC8,0x3CF9,7,0xFFFE,0x425E,
               0x2F46,10,0x3F7C,0x2700,8,0x4FEF,8,0x4E73]
    for v in [2,3]: struct.pack_into('>I',image,v*4,0x3A00)
    struct.pack_into('>'+'H'*len(handler),image,0x3A00,*handler)
    return ''.join(name+'\t'+'\t'.join(f'{v:08X}' for v in expected)+'\n' for name,expected in vectors)
