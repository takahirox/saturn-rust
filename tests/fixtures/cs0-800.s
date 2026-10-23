; Original MIT CS0 guest; adjacent LICENSE.
load 0x2601c000,r8
load 0xffffffe0,r1
load 0xa55a0bf0,r0
mov.l r0,@r1
load 0xffffffe8,r1
load 0xa55a0001,r0
mov.l r0,@r1
load 0x20210000,r1
load 0x13579bdf,r0
mov.l r0,@r1
load 0x20210000,r11
mov.l @r11,r0
load 0xfffffe92,r1
mov #17,r0
mov.b r0,@r1
load 0xfffffe12,r4
load 0xfffffe13,r5
load 0xfffffe91,r7
load 0x2001f000,r10
load 0x8000,r12
jsr @r12
nop
jsr @r12
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
mov.l r9,@r8
add #4,r8
xor r9,r6
mov.l r6,@r8
add #4,r8
load 0x20212000,r1
load 0x51,r0
mov.l r0,@r1
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
load 0x1000,r3
quiet:
dt r3
bf quiet
load 0x20212000,r1
load 0xa3,r2
sync:
mov.l @r1,r0
cmp/eq r2,r0
bf sync
load 0x600d,r0
mov.l r0,@r8
add #4,r8
done:
sleep
bra done
nop
