; Original SH7604 DACK request diagnostic, MIT: adjacent LICENSE.
; The standard board has no requesting DACK peripheral. Edge-triggered
; channels must retain their counts and never use the ignored MMIO address.
load 0xffffff80,r1
load 0x06002000,r0
mov.l r0,@r1
add #4,r1
load 0xffffffff,r0
mov.l r0,@r1
add #4,r1
mov #3,r0
mov.l r0,@r1
add #4,r1
load 0x1049,r0
mov.l r0,@r1
add #4,r1
load 0xffffffff,r0
mov.l r0,@r1
add #4,r1
load 0x06002000,r0
mov.l r0,@r1
add #4,r1
mov #3,r0
mov.l r0,@r1
add #4,r1
load 0x4149,r0
mov.l r0,@r1
load 0xffffffb0,r1
mov #1,r0
mov.l r0,@r1
mov #100,r2
wait:
dt r2
bf wait
load 0xffffff88,r1
load 0x06003000,r2
mov.l @r1,r0
mov.l r0,@r2
add #16,r1
add #4,r2
mov.l @r1,r0
mov.l r0,@r2
load 0xffffffb0,r1
add #4,r2
mov.l @r1,r0
mov.l r0,@r2
add #4,r2
load 0x600d,r0
mov.l r0,@r2
sleep
