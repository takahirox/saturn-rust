; Original MIT; adjacent LICENSE.
load 0xffffffe0,r1
mov.l @r1,r9
load 0x8000,r2
tst r2,r9
bf slave
load 0x2600d000,r8
bra common
nop
slave:
load 0x2600d100,r8
common:
load 0xffffffe8,r1
load 0xa55a0000,r0
mov.l r0,@r1
load 0xfffffe92,r1
mov #25,r0
mov.b r0,@r1
load 0xc0000400,r1
load 0x6a40,r0
mov.w r0,@r1
load 0xc0000402,r1
load 0x6b50,r0
mov.w r0,@r1
load 0xc0000404,r1
load 0x6bbc,r0
mov.w r0,@r1
load 0xc0000406,r1
load 0x4a18,r0
mov.w r0,@r1
load 0xc0000408,r1
load 0x2bab,r0
mov.w r0,@r1
load 0xc000040a,r1
load 0x28b2,r0
mov.w r0,@r1
load 0xc000040c,r1
load 0x7804,r0
mov.w r0,@r1
load 0xc000040e,r1
load 0x6cf3,r0
mov.w r0,@r1
load 0xc0000410,r1
load 0x7c04,r0
mov.w r0,@r1
load 0xc0000412,r1
load 0x6dc2,r0
mov.w r0,@r1
load 0xc0000414,r1
load 0x28d2,r0
mov.w r0,@r1
load 0xc0000416,r1
load 0x7804,r0
mov.w r0,@r1
load 0xc0000418,r1
load 0x6df2,r0
mov.w r0,@r1
load 0xc000041a,r1
load 0x28d2,r0
mov.w r0,@r1
load 0xc000041c,r1
load 0x7804,r0
mov.w r0,@r1
load 0xc000041e,r1
load 0xd02,r0
mov.w r0,@r1
load 0xc0000420,r1
load 0x28d2,r0
mov.w r0,@r1
load 0xc0000422,r1
load 0xdc01,r0
mov.w r0,@r1
load 0xc0000424,r1
load 0xa002,r0
mov.w r0,@r1
load 0xc0000426,r1
load 0x9,r0
mov.w r0,@r1
load 0xc0000428,r1
load 0xffff,r0
mov.w r0,@r1
load 0xc000042a,r1
load 0xfe10,r0
mov.w r0,@r1
load 0xc000042c,r1
load 0xed00,r0
mov.w r0,@r1
load 0xc000042e,r1
load 0x2cd0,r0
mov.w r0,@r1
load 0xc0000430,r1
load 0x2b,r0
mov.w r0,@r1
load 0xc0000432,r1
load 0x9,r0
mov.w r0,@r1
load 0xc0000500,r1
load 0xe050,r0
mov.w r0,@r1
load 0xc0000502,r1
load 0x400e,r0
mov.w r0,@r1
load 0xc0000504,r1
load 0x9,r0
mov.w r0,@r1
load 0xc0000506,r1
load 0xe000,r0
mov.w r0,@r1
load 0xc0000508,r1
load 0x2700,r0
mov.w r0,@r1
load 0xc000050a,r1
load 0x1b,r0
mov.w r0,@r1
load 0xc000050c,r1
load 0xe607,r0
mov.w r0,@r1
load 0xc000050e,r1
load 0xb,r0
mov.w r0,@r1
load 0xc0000510,r1
load 0x9,r0
mov.w r0,@r1
load 0xc0000180,r1
load 0xc0000400,r0
mov.l r0,@r1
load 0xc0000000,r0
ldc r0,vbr
load 0xc0000780,r15
load 0xfffffe91,r1
mov #2,r0
mov.b r0,@r1
mov #0,r0
mov.b r0,@r1
load 0xfffffe16,r1
mov #3,r0
mov.b r0,@r1
load 0xfffffe14,r1
mov #0,r0
mov.b r0,@r1
load 0xfffffe15,r1
mov #32,r0
mov.b r0,@r1
load 0xfffffe10,r1
mov #8,r0
mov.b r0,@r1
load 0xfffffe60,r1
load 0x0600,r0
mov.w r0,@r1
load 0xfffffe66,r1
load 0x0060,r0
mov.w r0,@r1
load 0xfffffe12,r4
load 0xfffffe13,r5
load 0xfffffe16,r7
load 0xc0000500,r3
jsr @r3
nop
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x600d,r0
mov.l r0,@r8
mov #0,r0
ldc r0,vbr
load 0x060ff000,r15
mov #0,r0
ldc r0,sr
nop
load 0x8000,r2
tst r2,r9
bf.l finished
load 0xfffffee0,r1
mov #1,r0
mov.w r0,@r1
load 0x25fe00a0,r1
load 0xb7ff,r0
mov.l r0,@r1
load 0x2600d200,r1
load 0x13579bdf,r0
mov.l r0,@r1
load 0x25fe0000,r1
load 0x600d200,r0
mov.l r0,@r1
load 0x25fe0004,r1
load 0x600d204,r0
mov.l r0,@r1
load 0x25fe0008,r1
load 0x4,r0
mov.l r0,@r1
load 0x25fe000c,r1
load 0x102,r0
mov.l r0,@r1
load 0x25fe0014,r1
load 0x7,r0
mov.l r0,@r1
load 0x25fe0010,r1
load 0x101,r0
mov.l r0,@r1
load 0x2600d018,r1
wait_scu:
mov.l @r1,r0
tst #255,r0
bt wait_scu
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
finished:
mov #-16,r0
ldc r0,sr
nop
sleep
bra finished
nop

; private handler
mov.b @r4,r10
mov.b @r5,r11
extu.b r11,r11
shll8 r10
or r10,r11
mov.l r11,@r8
add #4,r8
mov r15,r12
add #4,r12
mov.l @r12,r13
mov.l r13,@r8
add #4,r8
mov.l @r15,r13
mov.l r13,@r8
add #4,r8
stc sr,r13
mov.l r13,@r8
load 0xfffffe10,r12
mov #0,r13
mov.b r13,@r12
rte
nop

; private kernel
mov #80,r0
ldc r0,sr
nop
mov #0,r0
mov.b r0,@r7
sleep
mov #7,r6
rts
nop

; SCU handler
load 0x2600d018,r12
mov #75,r13
mov.l r13,@r12
rte
nop
