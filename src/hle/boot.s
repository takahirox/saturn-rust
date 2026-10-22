; Original independent HLE reset trampoline. MIT: adjacent LICENSE.
; Slave application entry is the work-RAM handoff pointer at 06000250.
; Both processors execute real reset instructions and separate BCR1 views.
; Select external SCU interrupt vectors after the architectural reset.
load 0xfffffee0,r1
mov #1,r0
mov.w r0,@r1
load 0xffffffe0,r1
mov.l @r1,r0
load 0x8000,r1
tst r1,r0
bt master
load 0x060fe000,r15
load 0x06000400,r0
ldc r0,vbr
load 0x06000250,r1
wait_entry:
mov.l @r1,r0
tst r0,r0
bt wait_entry
jmp @r0
nop
master:
load 0x06000000,r0
ldc r0,vbr
load 0x06002e20,r0
jmp @r0
nop
