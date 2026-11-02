bsr entered
nop
entered:
sts pr,r0
load 0x26070000,r1
mov.l r0,@r1
load 0x26074000,r9
load 0x26070004,r1
load 0x1,r0
mov.l r0,@r1
load 0xfffffe92,r1
load 0x11,r0
mov.b r0,@r1
load 0x4801,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x4480,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x3000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 0xffeb,r0
mov.w r0,@r1
load 0x26010318,r1
mov.l @r1,r5
load 150,r0
add r0,r5
load 0x1080,r4
load 0x80,r6
mov #1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 16,r2
load 200000,r3
wait_1:
mov.w @r1,r0
tst r2,r0
bf wait_1_done
dt r3
bf wait_1
load io_error,r0
jmp @r0
nop
wait_1_done:
load 0x6300,r4
load 0x0,r5
load 0x0,r6
load 0x1,r7
load command,r0
jsr @r0
nop
load 0x6060000,r10
load 2048,r11
load dma,r0
jsr @r0
nop
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x6060000,r10
load 0x6060800,r12
find:
cmp/hs r12,r10
bf guard_2
load failed,r0
jmp @r0
nop
guard_2:
mov.b @r10,r0
extu.b r0,r13
load 34,r2
cmp/hs r2,r13
bt guard_3
load failed,r0
jmp @r0
nop
guard_3:
mov r10,r0
add r13,r0
cmp/hs r0,r12
bt guard_4
load failed,r0
jmp @r0
nop
guard_4:
mov r10,r1
add #32,r1
mov.b @r1+,r0
load 12,r2
cmp/eq r2,r0
bf next_record
load 0x6023000,r2
mov #12,r3
name:
mov.b @r1+,r0
mov.b @r2+,r4
cmp/eq r4,r0
bf next_record
dt r3
bf name
bra found
nop
next_record:
add r13,r10
bra find
nop
found:
mov r10,r1
add #6,r1
load be32,r0
jsr @r0
nop
load 0x26070010,r1
mov.l r0,@r1
mov r10,r1
add #14,r1
load be32,r0
jsr @r0
nop
load 0x26070014,r1
mov.l r0,@r1
add #1,r0
load 2046,r2
add r2,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
load 0x26070018,r1
mov.l r0,@r1
load 0x25890008,r1
load 0xfdff,r0
mov.w r0,@r1
load 0x7000,r4
load 0x0,r5
load 0x3ff,r6
load 0xffff,r7
load command,r0
jsr @r0
nop
load 0x7500,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 512,r2
load 200000,r3
wait_5:
mov.w @r1,r0
tst r2,r0
bf wait_5_done
dt r3
bf wait_5
load io_error,r0
jmp @r0
nop
wait_5_done:
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
load 0x26070050,r1
mov.l r0,@r1
load 0x7400,r4
load 0x0,r5
load 0x100,r6
load 0xffff,r7
load issue,r0
jsr @r0
nop
load 0xff00,r2
cmp/eq r2,r0
bt guard_6
load failed,r0
jmp @r0
nop
guard_6:
load 0x06080000,r1
load 65536,r3
load 0xa5,r0
canaries:
mov.b r0,@r1
add #1,r1
dt r3
bf canaries
load 0x26071000,r8
mov #0,r13
load 0x26070028,r1
load 0x0,r0
mov.l r0,@r1
load 0x26070004,r1
load 0x2,r0
mov.l r0,@r1
load 0x4801,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x4480,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x3000,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 0xffe3,r0
mov.w r0,@r1
load 0x26070010,r1
mov.l @r1,r5
load 150,r0
add r0,r5
load 0x1080,r4
load 0x80,r6
load 0x26070018,r1
mov.l @r1,r7
load command,r0
jsr @r0
nop
load 0x6080004,r10
load 0x26070018,r1
mov.l @r1,r12
mov #0,r13
mov #0,r14
fast_sector:
load 0x25890008,r1
load 4,r2
load 200000,r3
wait_7:
mov.w @r1,r0
tst r2,r0
bf wait_7_done
dt r3
bf wait_7
load io_error,r0
jmp @r0
nop
wait_7_done:
mov.l r13,@r8
add #4,r8
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x5400,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x5100,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890024,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25890008,r1
load 0xff73,r0
mov.w r0,@r1
load 0x6300,r4
load 0x0,r5
load 0x100,r6
load 0x1,r7
load command,r0
jsr @r0
nop
load 0x26010204,r1
mov.l @r1,r0
load 0x26070030,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
load 0xfffffe12,r1
mov.b @r1+,r0
extu.b r0,r0
shll8 r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov.l r0,@r8
add #4,r8
load 2048,r11
mov #1,r0
cmp/eq r0,r12
bf fast_length
load 0x26070014,r1
mov.l @r1,r11
load 2047,r0
and r0,r11
add #1,r11
shlr r11
shll r11
fast_length:
load dma_start,r0
jsr @r0
nop
tst r13,r13
bt fast_first
mov r10,r4
load 2048,r0
sub r0,r4
load 16,r5
load compute,r0
jsr @r0
nop
load 2032,r5
load compute,r0
jsr @r0
nop
fast_first:
load 0x5000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0xfffffe12,r1
mov.b @r1+,r0
extu.b r0,r0
shll8 r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov.l r0,@r8
add #4,r8
mov.l r14,@r8
add #4,r8
load 0x26070034,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load dma_wait,r0
jsr @r0
nop
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25fe0004,r1
mov.l @r1,r0
load 0x26070038,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x26010204,r1
mov.l @r1,r0
load 0x2607003c,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
mov.l r10,@r8
add #4,r8
mov.l r11,@r8
add #4,r8
load 2048,r0
add r0,r10
add #1,r13
dt r12
bf.l fast_sector
mov r10,r4
load 2048,r0
sub r0,r4
load 0x26070014,r1
mov.l @r1,r5
load 2047,r0
and r0,r5
load compute,r0
jsr @r0
nop
load 0x26070008,r1
mov.l r14,@r1
load 0x25890008,r1
load 16,r2
load 200000,r3
wait_8:
mov.w @r1,r0
tst r2,r0
bf wait_8_done
dt r3
bf wait_8
load io_error,r0
jmp @r0
nop
wait_8_done:
load 0x5100,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890024,r1
mov.w @r1,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_9
load failed,r0
jmp @r0
nop
guard_9:
load 0x26070004,r1
load 0x3,r0
mov.l r0,@r1
load 0x4801,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x6400,r4
load 0x0,r5
load 0x200,r6
load 0xc6,r7
load command,r0
jsr @r0
nop
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x4480,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x3000,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 0xffe3,r0
mov.w r0,@r1
load 0x26070010,r1
mov.l @r1,r5
load 150,r0
add r0,r5
load 0x1080,r4
load 0x80,r6
load 0x26070018,r1
mov.l @r1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 4,r2
load 200000,r3
wait_10:
mov.w @r1,r0
tst r2,r0
bf wait_10_done
dt r3
bf wait_10
load io_error,r0
jmp @r0
nop
wait_10_done:
load 0x25890008,r1
load 0xfffb,r0
mov.w r0,@r1
load 0x25890008,r1
load 4,r2
load 200000,r3
wait_11:
mov.w @r1,r0
tst r2,r0
bf wait_11_done
dt r3
bf wait_11
load io_error,r0
jmp @r0
nop
wait_11_done:
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
load 0x26070020,r1
mov.l r0,@r1
load 12,r2
and r2,r0
load 0xc,r2
cmp/eq r2,r0
bt guard_12
load full_error,r0
jmp @r0
nop
guard_12:
load 0x6088004,r10
load 0x26070018,r1
mov.l @r1,r12
mov #0,r13
mov #0,r14
slow_sector:
mov #2,r0
cmp/hs r0,r12
bf slow_tail
load 0x25890008,r1
load 8,r2
load 200000,r3
wait_13:
mov.w @r1,r0
tst r2,r0
bf wait_13_done
dt r3
bf wait_13
load io_error,r0
jmp @r0
nop
wait_13_done:
slow_tail:
mov.l r13,@r8
add #4,r8
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x5400,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x5100,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890024,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25890008,r1
load 0xff73,r0
mov.w r0,@r1
load 0x6300,r4
load 0x0,r5
load 0x100,r6
load 0x1,r7
load command,r0
jsr @r0
nop
load 0x26010204,r1
mov.l @r1,r0
load 0x26070030,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
load 0xfffffe12,r1
mov.b @r1+,r0
extu.b r0,r0
shll8 r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov.l r0,@r8
add #4,r8
load 2048,r11
mov #1,r0
cmp/eq r0,r12
bf slow_length
load 0x26070014,r1
mov.l @r1,r11
load 2047,r0
and r0,r11
add #1,r11
shlr r11
shll r11
slow_length:
load dma_start,r0
jsr @r0
nop
tst r13,r13
bt slow_first
mov r10,r4
load 2048,r0
sub r0,r4
load 16,r5
load compute,r0
jsr @r0
nop
load 2032,r5
load compute,r0
jsr @r0
nop
slow_first:
load 0x5000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0xfffffe12,r1
mov.b @r1+,r0
extu.b r0,r0
shll8 r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov.l r0,@r8
add #4,r8
mov.l r14,@r8
add #4,r8
load 0x26070034,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load dma_wait,r0
jsr @r0
nop
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25fe0004,r1
mov.l @r1,r0
load 0x26070038,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
load 0x25890008,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x26010204,r1
mov.l @r1,r0
load 0x2607003c,r1
mov.l r0,@r1
mov.l r0,@r8
add #4,r8
mov.l r10,@r8
add #4,r8
mov.l r11,@r8
add #4,r8
load 2048,r0
add r0,r10
add #1,r13
dt r12
bf.l slow_sector
mov r10,r4
load 2048,r0
sub r0,r4
load 0x26070014,r1
mov.l @r1,r5
load 2047,r0
and r0,r5
load compute,r0
jsr @r0
nop
load 0x2607000c,r1
mov.l r14,@r1
load 0x25890008,r1
load 16,r2
load 200000,r3
wait_14:
mov.w @r1,r0
tst r2,r0
bf wait_14_done
dt r3
bf wait_14
load io_error,r0
jmp @r0
nop
wait_14_done:
load 0x5100,r4
load 0x0,r5
load 0x100,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890024,r1
mov.w @r1,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_15
load failed,r0
jmp @r0
nop
guard_15:
load 0x26070024,r1
mov.l r8,@r1
load 0x2607002c,r1
mov.l r9,@r1
load 0x7f00,r1
mov.l @r1,r0
tst #1,r0
bt no_sysres
load 0x20180001,r1
mov.b @r1,r0
extu.b r0,r0
load 0x5a,r2
cmp/eq r2,r0
bt no_sysres
load 0x20180001,r1
load 0x5a,r0
mov.b r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #13,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc13:
mov.b @r1,r0
tst #1,r0
bf wait_smpc13
no_sysres:
load 0x26070004,r1
load 0x4,r0
mov.l r0,@r1
load 0x260700fc,r1
load 0x600d,r0
mov.l r0,@r1
park:
sleep
bra park
nop
failed:
load 0x260700fc,r1
load 0xe090,r0
mov.l r0,@r1
load 0x26070024,r1
mov.l r8,@r1
load 0x2607002c,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
io_error:
load 0x260700fc,r1
load 0xe092,r0
mov.l r0,@r1
load 0x26070024,r1
mov.l r8,@r1
load 0x2607002c,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
full_error:
load 0x260700fc,r1
load 0xe093,r0
mov.l r0,@r1
load 0x26070024,r1
mov.l r8,@r1
load 0x2607002c,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
be32:
mov #0,r0
mov #4,r3
be_loop:
shll8 r0
mov.b @r1+,r2
extu.b r2,r2
or r2,r0
dt r3
bf be_loop
rts
nop
compute:
compute_byte:
mov.b @r4+,r0
extu.b r0,r0
add r0,r14
dt r5
bf compute_byte
mov r14,r0
load 0x7fff,r2
and r2,r0
load 0x8000,r2
or r2,r0
load 0x25e00000,r1
mov.w r0,@r1
load 0x26070028,r1
load 0x1,r0
mov.l r0,@r1
rts
nop
dma:
sts.l pr,@-r15
load dma_start,r0
jsr @r0
nop
load dma_wait,r0
jsr @r0
nop
lds.l @r15+,pr
rts
nop
dma_start:
sts.l pr,@-r15
load 0x26010204,r1
mov.l @r1,r0
add #1,r0
load 0x26070040,r1
mov.l r0,@r1
load 0x25fe0000,r1
load 0x5818000,r0
mov.l r0,@r1
load 0x25fe000c,r1
load 0x2,r0
mov.l r0,@r1
load 0x25fe0014,r1
load 0x107,r0
mov.l r0,@r1
load 0x25fe0004,r1
mov.l r10,@r1
load 0x25fe0008,r1
mov.l r11,@r1
mov #0,r2
load dma_kernel,r0
jsr @r0
nop
load 0x25fe0010,r1
load 0x101,r4
mov #1,r2
load dma_kernel,r0
jsr @r0
nop
load 0x26070034,r1
mov.l r6,@r1
load 0x26070048,r1
mov.l r5,@r1
lds.l @r15+,pr
rts
nop
dma_kernel:
load 0xfffffe12,r7
mov.b @r7,r0
extu.b r0,r0
shll8 r0
add #1,r7
mov.b @r7,r3
extu.b r3,r3
or r3,r0
mov r0,r6
tst r2,r2
bt kernel_warm
mov.l r4,@r1
kernel_warm:
mov #16,r3
mov #0,r5
kernel_spin:
add r14,r5
dt r3
bf kernel_spin
load 0xfffffe12,r7
mov.b @r7,r0
extu.b r0,r0
shll8 r0
add #1,r7
mov.b @r7,r3
extu.b r3,r3
or r3,r0
sub r6,r0
extu.w r0,r6
rts
nop
dma_wait:
load 0x26070040,r1
mov.l @r1,r7
load 0x26010204,r1
load 200000,r3
dma_poll:
mov.l @r1,r0
cmp/eq r7,r0
bt dma_ready
dt r3
bf dma_poll
load io_error,r0
jmp @r0
nop
dma_ready:
rts
nop
command:
sts.l pr,@-r15
load issue,r0
jsr @r0
nop
load 0x8000,r2
tst r2,r0
bt guard_16
load io_error,r0
jmp @r0
nop
guard_16:
lds.l @r15+,pr
rts
nop
issue:
mov.l r4,@r9
add #4,r9
mov.l r5,@r9
add #4,r9
mov.l r6,@r9
add #4,r9
mov.l r7,@r9
add #4,r9
load 0x25890008,r1
load 0xfffe,r0
mov.w r0,@r1
load 0x25890018,r1
mov.w r4,@r1
add #4,r1
mov.w r5,@r1
add #4,r1
mov.w r6,@r1
add #4,r1
mov.w r7,@r1
load 0x25890008,r1
load 1,r2
load 200000,r3
wait_17:
mov.w @r1,r0
tst r2,r0
bf wait_17_done
dt r3
bf wait_17
load io_error,r0
jmp @r0
nop
wait_17_done:
load 0x25890018,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x25890020,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x25890024,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x25890018,r1
mov.w @r1,r0
extu.w r0,r0
rts
nop
