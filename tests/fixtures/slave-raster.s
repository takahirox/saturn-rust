; Original MIT: adjacent LICENSE.
.section 0x200
load 0xffffffe0,r1
mov.l @r1,r0
load 0x8000,r2
tst r2,r0
bf.l slave
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
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
master_park:
sleep
bra master_park
nop
slave:
load 0x060fe000,r15
load 0xfffffee0,r1
load 0x0,r0
mov.w r0,@r1
load 0x26004104,r1
load 0x1000,r0
mov.l r0,@r1
load 0x2600410c,r1
load 0x1400,r0
mov.l r0,@r1
load 0x26004000,r0
ldc r0,vbr
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0x1000,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
mov.l @r1,r0
add #16,r1
load 0x100,r1
mov.l @r1,r10
add #4,r1
mov.l @r1,r11
load 0x2600e000,r8
load 0x2600e020,r9
load 0x25f80004,r4
active:
mov.w @r4,r0
tst #4,r0
bf active
load 0x108,r1
mov.l @r1,r0
ldc r0,sr
nop
wait_irq:
sleep
mov.l @r9,r0
mov #2,r1
cmp/eq r1,r0
bf wait_irq
load 0x2600e040,r1
mov r10,r0
add r11,r0
mov.l r0,@r1
add #4,r1
load 0x600d,r0
mov.l r0,@r1
park:
sleep
bra park
nop
.section 0x1000
mov.l @r8,r0
add #1,r0
mov.l r0,@r8
add #7,r10
mov r8,r2
add #4,r2
mov.l r10,@r2
mov r0,r3
shll2 r3
shll r3
add r8,r3
load 0x25f80002,r1
mov.w @r1,r2
load 0x25f8000a,r1
mov.w @r1,r2
extu.w r2,r2
mov.l r2,@r3
add #4,r3
load 0x25f80004,r1
mov.w @r1,r2
extu.w r2,r2
mov.l r2,@r3
mov #2,r1
cmp/eq r1,r0
bf first_h
mov r15,r1
add #4,r1
load 32,r0
mov.l r0,@r1
first_h:
rte
nop
.section 0x1400
mov.l @r9,r0
add #1,r0
mov.l r0,@r9
add #11,r11
mov r9,r2
add #4,r2
mov.l r11,@r2
mov r0,r3
shll2 r3
shll r3
add r9,r3
load 0x25f80002,r1
mov.w @r1,r2
load 0x25f8000a,r1
mov.w @r1,r2
extu.w r2,r2
mov.l r2,@r3
add #4,r3
load 0x25f80004,r1
mov.w @r1,r2
extu.w r2,r2
mov.l r2,@r3
mov #2,r1
cmp/eq r1,r0
bf first_v
mov r15,r1
add #4,r1
load 240,r0
mov.l r0,@r1
first_v:
rte
nop
