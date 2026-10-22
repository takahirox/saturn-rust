; Original SH7604 two-way RAM/cache diagnostic. MIT: adjacent LICENSE.
; Executes in P1 so instruction fetch never displaces the operand test lines.
load 0x26001000,r1
load 0x12345678,r0
mov.l r0,@r1
load 0xc0000000,r1
load 0xbad0bad0,r0
mov.l r0,@r1
load 0xc0000400,r1
load 0xc0dec0de,r0
mov.l r0,@r1
load 0xfffffe92,r1
mov #8,r0
mov.b r0,@r1
load 0x66001004,r2
mov #0,r0
mov.l r0,@r2
mov #72,r0
mov.b r0,@r1
mov #0,r0
mov.l r0,@r2
mov #9,r0
mov.b r0,@r1
load 0x06001000,r2
load 0x2600a500,r3
mov.l @r2,r0
mov.l r0,@r3
add #4,r3
load 0xabcdef01,r0
mov.l r0,@r2
mov.l @r2,r0
mov.l r0,@r3
add #4,r3
load 0xc0000000,r2
mov.l @r2,r0
mov.l r0,@r3
add #4,r3
load 0xc0000400,r2
mov.l @r2,r0
mov.l r0,@r3
add #4,r3
load 0x26001000,r2
mov.l @r2,r0
mov.l r0,@r3
add #4,r3
load 0x600d,r0
mov.l r0,@r3
park:
sleep
bra park
nop
