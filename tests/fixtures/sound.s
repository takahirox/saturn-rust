; Original SH-2 host for independently authored MC68EC000 PCM diagnostic.
; MIT: adjacent LICENSE. All uploads, control and result writes execute in guest.
load 0x2000,r1
load 0x05a00000,r2
load 0x1100,r3
copy:
mov.l @r1,r0
mov.l r0,@r2
add #4,r1
add #4,r2
dt r3
bf copy
load 0x00100063,r1
mov #1,r0
mov.b r0,@r1
load 0x0010001f,r1
mov #6,r0
mov.b r0,@r1
; Wait for the 68k's computation and IRQ handler, not a host-installed result.
load 0x05a00800,r1
wait:
mov.l @r1,r0
tst #255,r0
bt wait
load 0x05a00804,r1
wait_irq:
mov.l @r1,r0
tst #255,r0
bt wait_irq
load 0x06007000,r2
mov.l r0,@r2
load 0x05a00800,r1
mov.l @r1,r0
add #4,r2
mov.l r0,@r2
sleep
