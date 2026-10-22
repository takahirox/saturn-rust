; Original MIT: adjacent LICENSE.
load 0x2000,r1
load 0x25a00000,r2
load 0x140,r3
copy:
mov.l @r1,r0
mov.l r0,@r2
add #4,r1
add #4,r2
dt r3
bf copy
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
load 0xfffffe12,r1
load 0xfffffe13,r3
mov #0,r0
mov.b r0,@r1
mov.b r0,@r3
wait_divide:
mov.b @r1,r0
mov.b @r3,r0
extu.b r0,r0
mov #32,r2
cmp/hs r2,r0
bf wait_divide
load 0x25a00024,r1
load 0x400,r0
mov.l r0,@r1
load 0x25a0090c,r1
wait_trace:
mov.l @r1,r0
tst r0,r0
bt wait_trace
load 0x2600ef00,r8
load 0x25a00900,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25a00904,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25a00908,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25a0090c,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
finished:
sleep
bra finished
nop
