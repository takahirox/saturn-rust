; Original MIT SH-2 multiplier guest; see adjacent LICENSE.
; section 00000200
load 0xffffffe0,r1
mov.l @r1,r9
load 0x8000,r2
tst r2,r9
bf slave
mov #0,r11
tst r11,r11
bt clock_ready
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #14,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc14:
mov.b @r1,r0
tst #1,r0
bf wait_smpc14
clock_ready:
load 0x2600e900,r8
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #2,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc2:
mov.b @r1,r0
tst #1,r0
bf wait_smpc2
bra common
nop
slave:
load 0x060fe000,r15
load 0x2600eb00,r8
common:
load 0xffffffe8,r1
load 0xa55a0000,r0
mov.l r0,@r1
load 0xfffffe92,r1
mov #25,r0
mov.b r0,@r1
load 0xfffffe12,r4
load 0xfffffe13,r5
load 0xfffffe91,r7
load 0xc0000400,r12
load 0x30003,r0
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
load 0xc0000500,r12
load 0x70007,r0
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
load 0xc0000600,r12
load 0x3,r0
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
load 0xc0000700,r12
load 0x7,r0
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
mov.l r0,@r12
add #4,r12
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4000,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4000,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4400,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4400,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4800,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4800,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4c00,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x4c00,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5000,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5000,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5400,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5400,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5800,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5800,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5c00,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x5c00,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6000,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6000,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6400,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6400,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6800,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6800,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000400,r12
load 0xc0000500,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6c00,r3
jsr @r3
nop
load 0xc0000400,r12
load 0xc0000500,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x6c00,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7000,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7000,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7400,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7400,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7800,r3
jsr @r3
nop
load 0xc0000600,r12
load 0xc0000700,r13
clrmac
mov #3,r1
mov #7,r2
mov #0,r6
load 0x7800,r3
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x600d,r0
mov.l r0,@r8
add #4,r8
done:
sleep
bra done
nop

; section 00004000
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
muls.w r1,r2
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00004400
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mulu.w r1,r2
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00004800
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
muls.w r1,r2
nop
nop
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00004C00
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mulu.w r1,r2
nop
nop
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00005000
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
nop
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00005400
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mul.l r1,r2
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00005800
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
dmuls.l r1,r2
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00005C00
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
dmulu.l r1,r2
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00006000
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
dmuls.l r1,r2
nop
nop
nop
nop
sts macl,r6
dmuls.l r1,r2
nop
nop
nop
nop
sts macl,r6
dmuls.l r1,r2
nop
nop
nop
nop
sts macl,r6
dmuls.l r1,r2
nop
nop
nop
nop
sts macl,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00006400
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
muls.w r1,r2
clrmac
sts macl,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00006800
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
muls.w r1,r2
lds r1,macl
sts macl,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00006C00
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
mac.w @r12+,@r13+
sts macl,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00007000
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
mac.l @r12+,@r13+
sts macl,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00007400
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
mac.l @r12+,@r13+
nop
nop
nop
nop
sts macl,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop

; section 00007800
mov #2,r0
mov.b r0,@r7
mov #0,r0
mov.b r0,@r7
mov.b @r4,r0
mov.b @r5,r0
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
dmuls.l r1,r2
lds r1,mach
sts mach,r6
nop
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop
