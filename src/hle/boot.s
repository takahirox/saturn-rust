; Original independent HLE reset trampoline. MIT: adjacent LICENSE.
; SH7604 §§5.3/11.2; System Program guide interrupt table; STN-28 §4.4.
; CPU-local setup executes on every reset, without rewriting RAM vectors.
; SR.I is still 15. Disable requests before setting priority/vector/flags.
load 0xfffffe10,r1
mov #1,r0
mov.b r0,@r1
load 0xfffffee2,r1
mov #0,r0
mov.w r0,@r1
load 0xfffffe60,r1
load 0x0f00,r0
mov.w r0,@r1
load 0xfffffe66,r1
load 0x6465,r0
mov.w r0,@r1
load 0xfffffe68,r1
load 0x6600,r0
mov.w r0,@r1
; Read-before-zero acknowledges observed FTCSR flags, never a capture strobe.
load 0xfffffe11,r1
mov.b @r1,r0
mov #0,r0
mov.b r0,@r1
load 0xfffffe10,r1
load 0x81,r0
mov.b r0,@r1
; Separate master SCU external vectors from supported slave DCC autovectors.
load 0xffffffe0,r1
mov.l @r1,r0
load 0x8000,r1
tst r1,r0
bt master
load 0xfffffee0,r1
mov #0,r0
mov.w r0,@r1
load 0x060fe000,r15
load 0x06000400,r0
ldc r0,vbr
; STN-28 §4.3 entry index 94, distinct from capture-handler index 164.
load 0x06000250,r1
wait_entry:
mov.l @r1,r0
tst r0,r0
bt wait_entry
jmp @r0
nop
master:
load 0xfffffee0,r1
mov #1,r0
mov.w r0,@r1
load 0x06000000,r0
ldc r0,vbr
load 0x06002e20,r0
jmp @r0
nop
