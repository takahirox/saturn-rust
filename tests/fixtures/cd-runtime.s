bsr entered
nop
entered:
sts pr,r0
load 0x26070000,r1
mov.l r0,@r1
load 0x26072000,r9
load 0x26070004,r1
load 0x1,r0
mov.l r0,@r1
load init,r0
jsr @r0
nop
load 0x06080000,r1
load 65536,r3
load 0xa5,r0
canaries:
mov.b r0,@r1
add #1,r1
dt r3
bf canaries
load 0x26010318,r1
mov.l @r1,r10
load 0x2601031c,r1
mov.l @r1,r12
load load_dir,r0
jsr @r0
nop
load 0x6024000,r10
load find,r0
jsr @r0
nop
load 0xffffffff,r2
cmp/eq r2,r0
bf guard_1
load directory_error,r0
jmp @r0
nop
guard_1:
mov r1,r14
load 0x26070010,r1
mov.l r0,@r1
mov r0,r7
mov r14,r1
add #6,r1
load be32,r0
jsr @r0
nop
load 0x26070c10,r1
mov.l r0,@r1
mov r14,r1
add #14,r1
load be32,r0
jsr @r0
nop
load 0x26070c14,r1
mov.l r0,@r1
load 0x25890008,r1
load 0xfdff,r0
mov.w r0,@r1
load 0x7000,r4
load 0x0,r5
load 0xff,r6
load 0xffff,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 512,r2
load 200000,r3
wait_2:
mov.w @r1,r0
tst r2,r0
bf wait_2_done
dt r3
bf wait_2
load io_error,r0
jmp @r0
nop
wait_2_done:
load 0x26070010,r1
mov.l @r1,r7
load 0x7000,r4
mov #0,r5
load 0x0300,r6
load 0x25890008,r1
load 0xfdff,r0
mov.w r0,@r1
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 512,r2
load 200000,r3
wait_3:
mov.w @r1,r0
tst r2,r0
bf wait_3_done
dt r3
bf wait_3
load io_error,r0
jmp @r0
nop
wait_3_done:
load 0x7200,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x26070c10,r1
mov.l @r1,r10
load 0x26070c14,r1
mov.l @r1,r12
load load_dir,r0
jsr @r0
nop
load 0x26070800,r8
load 0x6024008,r10
load 1,r11
load open,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_4
load failed,r0
jmp @r0
nop
guard_4:
load 0x26070820,r8
load 0x6024018,r10
load 2,r11
load open,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_5
load failed,r0
jmp @r0
nop
guard_5:
load 0x26070808,r1
mov.l @r1,r0
load 0x26070020,r1
mov.l r0,@r1
load 0x26070828,r1
mov.l @r1,r0
load 0x26070024,r1
mov.l r0,@r1
load 0x6024028,r10
load find,r0
jsr @r0
nop
load 0x2607000c,r1
mov.l r0,@r1
load 0xffffffff,r2
cmp/eq r2,r0
bt guard_6
load failed,r0
jmp @r0
nop
guard_6:
load 0x7400,r4
load 0x0,r5
load 0x1700,r6
load 0xffff,r7
load issue,r0
jsr @r0
nop
load 0xff00,r2
cmp/eq r2,r0
bt guard_7
load failed,r0
jmp @r0
nop
guard_7:
load 0x7400,r4
mov #0,r5
load 0x0100,r6
load 0x26070800,r1
mov.l @r1,r7
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
load 0x26070004,r1
load 0x2,r0
mov.l r0,@r1
load 0x26070800,r8
load 0x6080004,r10
load 3,r11
load read,r0
jsr @r0
nop
load 0x26070040,r1
mov.l r0,@r1
load 0x3,r2
cmp/eq r2,r0
bt guard_8
load failed,r0
jmp @r0
nop
guard_8:
load 0x26070820,r8
load 0x6082004,r10
load 2050,r11
load read,r0
jsr @r0
nop
load 0x26070044,r1
mov.l r0,@r1
load 0x802,r2
cmp/eq r2,r0
bt guard_9
load failed,r0
jmp @r0
nop
guard_9:
load 0x26070800,r8
load 2046,r10
load seek,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_10
load failed,r0
jmp @r0
nop
guard_10:
load 0x6084004,r10
load 2057,r11
load read,r0
jsr @r0
nop
load 0x26070048,r1
mov.l r0,@r1
load 0x26070820,r8
load 1,r10
load seek,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_11
load failed,r0
jmp @r0
nop
guard_11:
load 0x6086004,r10
load 7,r11
load read,r0
jsr @r0
nop
load 0x2607004c,r1
mov.l r0,@r1
load 0x7,r2
cmp/eq r2,r0
bt guard_12
load failed,r0
jmp @r0
nop
guard_12:
load 0x26070800,r8
load 0,r10
load seek,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_13
load failed,r0
jmp @r0
nop
guard_13:
load 0x6088004,r10
load 9999,r11
load read,r0
jsr @r0
nop
load 0x26070050,r1
mov.l r0,@r1
load 0x26070800,r8
load close,r0
jsr @r0
nop
load 0x26070800,r8
load 0x608a004,r10
load 1,r11
load read,r0
jsr @r0
nop
load 0x26070054,r1
mov.l r0,@r1
load 0x26070060,r1
mov.l r0,@r1
load 0xffffffff,r2
cmp/eq r2,r0
bt guard_14
load failed,r0
jmp @r0
nop
guard_14:
load 0x26070800,r8
load 0x6024008,r10
load 1,r11
load open,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_15
load failed,r0
jmp @r0
nop
guard_15:
load 0x26070800,r8
load 0x608a004,r10
load 13,r11
load read,r0
jsr @r0
nop
load 0x26070054,r1
mov.l r0,@r1
load 0xd,r2
cmp/eq r2,r0
bt guard_16
load failed,r0
jmp @r0
nop
guard_16:
load 0x26070820,r8
load 0x26070828,r1
mov.l @r1,r10
load seek,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_17
load failed,r0
jmp @r0
nop
guard_17:
load 0x26070820,r8
load 0x608c004,r10
load 9,r11
load read,r0
jsr @r0
nop
load 0x26070058,r1
mov.l r0,@r1
load 0x26070064,r1
mov.l r0,@r1
load 0x0,r2
cmp/eq r2,r0
bt guard_18
load failed,r0
jmp @r0
nop
guard_18:
load 0x26070828,r1
mov.l @r1,r10
add #1,r10
load seek,r0
jsr @r0
nop
load 0x26070068,r1
mov.l r0,@r1
load 0xffffffff,r2
cmp/eq r2,r0
bt guard_19
load failed,r0
jmp @r0
nop
guard_19:
load 0x26070820,r8
load close,r0
jsr @r0
nop
load 0x26070820,r8
load 0x6024018,r10
load 2,r11
load open,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_20
load failed,r0
jmp @r0
nop
guard_20:
load 0x26070820,r8
load 0x608c004,r10
load 9999,r11
load read,r0
jsr @r0
nop
load 0x26070058,r1
mov.l r0,@r1
mov #0,r4
load 0x6088004,r1
load 0x26070020,r2
mov.l @r2,r3
sum_alpha:
mov.b @r1+,r0
extu.b r0,r0
add r0,r4
dt r3
bf sum_alpha
load 0x608c004,r1
load 0x26070024,r2
mov.l @r2,r3
sum_beta:
mov.b @r1+,r0
extu.b r0,r0
add r0,r4
dt r3
bf sum_beta
load 0x26070008,r1
mov.l r4,@r1
load 0x7fff,r0
and r0,r4
load 0x8000,r0
or r0,r4
load 0x25e00000,r1
mov.w r4,@r1
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
load 0x3,r0
mov.l r0,@r1
load 0x260700fc,r1
load 0x600d,r0
mov.l r0,@r1
load 0x26070028,r1
mov.l r9,@r1
park:
sleep
bra park
nop
failed:
load 0x260700fc,r1
load 0xe090,r0
mov.l r0,@r1
load 0x26070028,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
prefetch_error:
load 0x260700fc,r1
load 0xe091,r0
mov.l r0,@r1
load 0x26070028,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
io_error:
load 0x260700fc,r1
load 0xe092,r0
mov.l r0,@r1
load 0x26070028,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
directory_error:
load 0x260700fc,r1
load 0xe093,r0
mov.l r0,@r1
load 0x26070028,r1
mov.l r9,@r1
load park,r0
jmp @r0
nop
command:
sts.l pr,@-r15
load issue,r0
jsr @r0
nop
load 0x8000,r2
tst r2,r0
bt guard_21
load io_error,r0
jmp @r0
nop
guard_21:
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
wait_22:
mov.w @r1,r0
tst r2,r0
bf wait_22_done
dt r3
bf wait_22
load io_error,r0
jmp @r0
nop
wait_22_done:
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
init:
sts.l pr,@-r15
load 0x0,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x100,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
load 0x2,r2
cmp/eq r2,r0
bt guard_23
load failed,r0
jmp @r0
nop
guard_23:
load 0x400,r4
load 0xffff,r5
load 0x0,r6
load 0x201,r7
load command,r0
jsr @r0
nop
load 0x480,r4
load 0xffff,r5
load 0x0,r6
load 0xffff,r7
load command,r0
jsr @r0
nop
load 0x400,r4
load 0xffff,r5
load 0x0,r6
load 0x600,r7
load issue,r0
jsr @r0
nop
load 0xff00,r2
cmp/eq r2,r0
bt guard_24
load failed,r0
jmp @r0
nop
guard_24:
load 0x6000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
lds.l @r15+,pr
rts
nop
fetch:
sts.l pr,@-r15
load 0x25890008,r1
load 0xffeb,r0
mov.w r0,@r1
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
mov r10,r5
load 150,r0
add r0,r5
mov r5,r4
shlr16 r4
load 0x1080,r0
or r0,r4
extu.w r5,r5
load 0x80,r6
mov #1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 16,r2
load 200000,r3
wait_25:
mov.w @r1,r0
tst r2,r0
bf wait_25_done
dt r3
bf wait_25
load io_error,r0
jmp @r0
nop
wait_25_done:
load 0x6300,r4
load 0x0,r5
load 0x0,r6
load 0x1,r7
load command,r0
jsr @r0
nop
load 0x26010204,r1
mov.l @r1,r0
add #1,r0
mov r0,r7
load 0x25fe0000,r1
load 0x5818000,r0
mov.l r0,@r1
load 0x25fe0008,r1
load 0x800,r0
mov.l r0,@r1
load 0x25fe000c,r1
load 0x2,r0
mov.l r0,@r1
load 0x25fe0014,r1
load 0x7,r0
mov.l r0,@r1
load 0x25fe0004,r1
mov.l r11,@r1
load 0x25fe0010,r1
load 0x101,r0
mov.l r0,@r1
load 0x26010204,r1
load 200000,r3
fetch_dma:
mov.l @r1,r0
cmp/eq r7,r0
bt fetch_ready
dt r3
bf fetch_dma
load io_error,r0
jmp @r0
nop
fetch_ready:
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
load 0x400,r2
cmp/eq r2,r0
bt guard_26
load failed,r0
jmp @r0
nop
guard_26:
lds.l @r15+,pr
rts
nop
load_dir:
sts.l pr,@-r15
load 4097,r0
cmp/hs r0,r12
bf guard_27
load directory_error,r0
jmp @r0
nop
guard_27:
tst r12,r12
bf guard_28
load directory_error,r0
jmp @r0
nop
guard_28:
load 2047,r0
tst r0,r12
bt guard_29
load directory_error,r0
jmp @r0
nop
guard_29:
load 0x26070c00,r1
mov.l r12,@r1
load 0x6060000,r11
load_dir_loop:
load fetch,r0
jsr @r0
nop
load 2048,r0
add #1,r10
add r0,r11
sub r0,r12
tst r12,r12
bf load_dir_loop
lds.l @r15+,pr
rts
nop
find:
sts.l pr,@-r15
load 0x6060000,r8
load 0x26070c00,r1
mov.l @r1,r12
add r8,r12
mov #0,r13
find_loop:
cmp/hs r12,r8
bt.l find_missing
mov.b @r8,r0
extu.b r0,r14
tst r14,r14
bt.l find_padding
load 34,r2
cmp/hs r2,r14
bt guard_30
load directory_error,r0
jmp @r0
nop
guard_30:
mov r8,r0
add r14,r0
cmp/hs r0,r12
bt guard_31
load directory_error,r0
jmp @r0
nop
guard_31:
mov r8,r0
load 2047,r2
and r2,r0
add r14,r0
load 2049,r2
cmp/hs r2,r0
bf guard_32
load directory_error,r0
jmp @r0
nop
guard_32:
mov r8,r1
add #32,r1
mov.b @r1,r0
extu.b r0,r5
mov r5,r0
add #33,r0
cmp/hs r0,r14
bt guard_33
load directory_error,r0
jmp @r0
nop
guard_33:
add #1,r1
mov r10,r2
find_compare:
mov.b @r2+,r0
extu.b r0,r0
tst r0,r0
bt find_end_name
tst r5,r5
bt find_next
mov.b @r1+,r3
extu.b r3,r3
cmp/eq r3,r0
bf find_next
add #-1,r5
bra find_compare
nop
find_end_name:
tst r5,r5
bf find_next
mov r8,r1
mov r13,r0
lds.l @r15+,pr
rts
nop
find_next:
add r14,r8
add #1,r13
bra find_loop
nop
find_padding:
load 2047,r0
or r0,r8
add #1,r8
bra find_loop
nop
find_missing:
mov #-1,r0
lds.l @r15+,pr
rts
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
open:
sts.l pr,@-r15
mov.l r8,@-r15
mov.l r11,@-r15
load find,r0
jsr @r0
nop
mov.l @r15+,r11
mov.l @r15+,r8
load 0xffffffff,r2
cmp/eq r2,r0
bt open_missing
mov.l r0,@r8
mov r1,r14
add #6,r1
load be32,r0
jsr @r0
nop
mov r8,r1
add #4,r1
mov.l r0,@r1
mov r14,r1
add #14,r1
load be32,r0
jsr @r0
nop
mov r8,r1
add #8,r1
mov.l r0,@r1
add #4,r1
mov #0,r0
mov.l r0,@r1
add #4,r1
mov #1,r0
mov.l r0,@r1
add #4,r1
mov.l r11,@r1
mov #0,r0
open_missing:
lds.l @r15+,pr
rts
nop
close:
mov r8,r1
add #16,r1
mov #0,r0
mov.l r0,@r1
rts
nop
seek:
mov r8,r1
add #16,r1
mov.l @r1,r0
tst r0,r0
bt api_error
mov r8,r1
add #8,r1
mov.l @r1,r0
cmp/hs r10,r0
bf api_error
add #4,r1
mov.l r10,@r1
mov #0,r0
rts
nop
api_error:
mov #-1,r0
rts
nop
read:
sts.l pr,@-r15
mov r8,r1
add #16,r1
mov.l @r1,r0
tst r0,r0
bt.l read_closed
mov r8,r1
add #8,r1
mov.l @r1+,r12
mov.l @r1,r13
sub r13,r12
cmp/hs r12,r11
bt read_clamped
mov r11,r12
read_clamped:
load 0x26070c04,r1
mov.l r12,@r1
tst r12,r12
bt.l read_done
mov.l r10,@-r15
load 0x25890008,r1
load 0xfdff,r0
mov.w r0,@r1
mov r13,r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
shlr r5
load 0x7400,r4
mov r8,r1
add #20,r1
mov.l @r1,r6
shll8 r6
mov.l @r8,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 512,r2
load 200000,r3
wait_34:
mov.w @r1,r0
tst r2,r0
bf wait_34_done
dt r3
bf wait_34
load io_error,r0
jmp @r0
nop
wait_34_done:
mov.l @r15+,r10
load 2047,r0
and r0,r13
read_sector:
load 2048,r14
sub r13,r14
cmp/hs r14,r12
bt read_take
mov r12,r14
read_take:
load 0x25890008,r1
load 0xff7f,r0
mov.w r0,@r1
load 0x6300,r4
mov #0,r5
mov r8,r1
add #20,r1
mov.l @r1,r6
shll8 r6
mov #1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
mov.w @r1,r0
load 0x82,r2
and r2,r0
load 0x2,r2
cmp/eq r2,r0
bt guard_35
load failed,r0
jmp @r0
nop
guard_35:
mov r13,r5
add r14,r5
mov #0,r4
mov #0,r7
fifo_word:
load 0x25818000,r1
mov.w @r1,r0
load 0x26070c80,r2
mov.w r0,@r2
add #1,r7
mov #2,r6
mov r2,r1
fifo_byte:
mov.b @r1+,r0
cmp/hs r13,r4
bf fifo_skip
cmp/hs r5,r4
bt fifo_skip
mov.b r0,@r10
add #1,r10
fifo_skip:
add #1,r4
dt r6
bf fifo_byte
cmp/hs r5,r4
bf fifo_word
load 0x26070c08,r1
mov.l r7,@r1
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
load 0x26070c0c,r1
mov.l r0,@r1
load 0x26070c08,r1
mov.l @r1,r2
load 1024,r3
cmp/eq r3,r2
bt read_full_count
cmp/hs r0,r2
bf guard_36
load prefetch_error,r0
jmp @r0
nop
guard_36:
cmp/hs r0,r3
bt guard_37
load prefetch_error,r0
jmp @r0
nop
guard_37:
bra read_fenced
nop
read_full_count:
cmp/eq r2,r0
bt guard_38
load prefetch_error,r0
jmp @r0
nop
guard_38:
read_fenced:
load 0x26070018,r1
mov.l @r1,r0
tst r0,r0
bf read_not_first
load 0x26070c08,r1
mov.l @r1,r0
load 0x26070018,r1
mov.l r0,@r1
load 0x26070c0c,r1
mov.l @r1,r0
load 0x2607001c,r1
mov.l r0,@r1
read_not_first:
load 0x25890008,r1
mov.w @r1,r0
load 0x82,r2
and r2,r0
load 0x80,r2
cmp/eq r2,r0
bt guard_39
load failed,r0
jmp @r0
nop
guard_39:
sub r14,r12
mov #0,r13
tst r12,r12
bf.l read_sector
load 0x5100,r4
mov #0,r5
mov r8,r1
add #20,r1
mov.l @r1,r6
shll8 r6
mov #0,r7
load command,r0
jsr @r0
nop
load 0x25890024,r1
mov.w @r1,r0
tst r0,r0
bt read_done
load 0x6200,r4
mov #0,r5
mov r8,r1
add #20,r1
mov.l @r1,r6
shll8 r6
load 0xffff,r7
load command,r0
jsr @r0
nop
read_done:
load 0x26070c04,r1
mov.l @r1,r2
mov r8,r1
add #12,r1
mov.l @r1,r0
add r2,r0
mov.l r0,@r1
mov r2,r0
lds.l @r15+,pr
rts
nop
read_closed:
mov #-1,r0
lds.l @r15+,pr
rts
nop
