; Original MIT: adjacent LICENSE.
load 0x2000,r1
load 0x25a00000,r2
load 0x440,r3
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
load 0x25a00808,r1
wait_effects:
mov.l @r1,r0
tst r0,r0
bt wait_effects
load 0x25a00800,r1
load 0x2600eb00,r2
mov #3,r3
records:
mov.l @r1,r0
mov.l r0,@r2
add #4,r1
add #4,r2
dt r3
bf records
finished:
sleep
bra finished
nop
