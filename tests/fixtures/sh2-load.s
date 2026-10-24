; Original MIT SH-2 MOV load guest; adjacent LICENSE.
; section 00000200
load 0xffffffe0,r1
mov.l @r1,r9
load 0x8000,r2
tst r2,r9
bf slave
load 0x2600ec00,r8
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
load 0x2600ed00,r8
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
mov #3,r0
mov.l r0,@r12
load 0xfffffe01,r10
mov #90,r0
mov.b r0,@r10
load 0x4000,r3
mov #0,r6
jsr @r3
nop
load 0x4000,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x4400,r3
mov #0,r6
jsr @r3
nop
load 0x4400,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x4800,r3
mov #0,r6
jsr @r3
nop
load 0x4800,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x4c00,r3
mov #0,r6
jsr @r3
nop
load 0x4c00,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x5000,r3
mov #0,r6
jsr @r3
nop
load 0x5000,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x5400,r3
mov #0,r6
jsr @r3
nop
load 0x5400,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x5800,r3
mov #0,r6
jsr @r3
nop
load 0x5800,r3
mov #0,r6
jsr @r3
nop
mov.l r0,@r8
add #4,r8
mov.l r6,@r8
add #4,r8
load 0x5c00,r3
mov #0,r6
jsr @r3
nop
load 0x5c00,r3
mov #0,r6
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
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
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
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
mov.l @r12,r6
nop
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
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
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
nop
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
mov.l @r12,r6
add #1,r6
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
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
mov.l @r12,r6
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
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
mov.l @r12,r6
nop
add #1,r6
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
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
mov.b @r10,r6
nop
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
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r10,r6
add #1,r6
mov.b @r4,r0
mov.b @r5,r0
extu.b r0,r0
rts
nop
