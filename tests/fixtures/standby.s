; Original SH7604 software standby/NMI guest. MIT: adjacent LICENSE.
; H table 14.3/§14.4.2: SCI/FRT initialize; NMI wakes even with SR.I=15.
.section 0x200
load 0x0600d000,r8
load 0xfffffe01,r1
mov #12,r0
mov.b r0,@r1
; Queue an independent SMPC NMI before stopping the CPU oscillator.
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #0x18,r0
mov.b r0,@r1
load 0xfffffe91,r1
load 0xc8,r0
mov.b r0,@r1
load 0x1000,r1
jmp @r1
nop
.section 0x1000
sleep
; Resumed at the instruction after SLEEP, with the CPU register preserved.
load 0xfffffe91,r1
mov #0,r0
mov.b r0,@r1
load 0x0600d010,r1
load 0x600d,r0
mov.l r0,@r1
park:
sleep
bra park
nop
.section 0x1800
mov.l @r15,r0
mov.l r0,@r8
add #4,r8
mov r15,r1
add #4,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0xfffffe91,r1
mov.b @r1,r0
mov.l r0,@r8
add #4,r8
load 0xfffffe01,r1
mov.b @r1,r0
mov.l r0,@r8
rte
nop
