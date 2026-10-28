; Original MIT: adjacent LICENSE.
load 0xffffffe8,r4
load 0xa55aaafe,r0
mov.l r0,@r4
load 0x2000,r1
load 0x25a00000,r2
load 0x400,r3
copy:
mov.l @r1,r0
mov.l r0,@r2
add #4,r1
add #4,r2
dt r3
bf copy
load 0x25b0041e,r1
mov #32,r0
mov.w r0,@r1
load 0x25b00424,r1
mov #32,r0
mov.w r0,@r1
load 0x25b00426,r1
mov #32,r0
mov.w r0,@r1
load 0x25b00428,r1
mov #32,r0
mov.w r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #6,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc6:
mov.b @r1,r0
tst #1,r0
bf wait_smpc6
load 0x25a00948,r1
wait_ready:
mov.l @r1,r0
tst r0,r0
bt wait_ready
load 0x25b00420,r4
load 0x25b00422,r5
mov #32,r6
mov.w r6,@r4
load 0x25a00940,r1
wait_handler:
mov.l @r1,r0
tst r0,r0
bt wait_handler
mov.w r6,@r5
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
mov.w r6,@r4
finished:
sleep
bra finished
nop
