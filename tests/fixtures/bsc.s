; Original SH7604 BSC/module-stop diagnostic. MIT: adjacent LICENSE.
; SH7604 §§5.2.2/7.1.4/7.2/7.5.7/7.11.1/14.2.1.
; Both processors execute this code and select independent mailbox/stack by
; BCR1.MASTER, rather than a host-written CPU identity or a shared-RAM heuristic.
.section 0x200
load 0xffffffe2,r1
mov.w @r1,r0
extu.w r0,r0
mov r0,r9
load 0x06008000,r8
load 0x8000,r2
tst r2,r0
bt master
load 0x06008040,r8
load 0x060fe000,r15
master:
mov.l r0,@r8
load 0xffffffe0,r1
load 0xa55a83f0,r0
mov.l r0,@r1
mov.l @r1,r0
mov r8,r2
add #4,r2
mov.l r0,@r2
; Stop both FRT and SCI. The FRT must initialize and remain at zero.
load 0xfffffe91,r1
mov #3,r0
mov.b r0,@r1
load 0xfffffe12,r1
mov.b @r1,r0
add #1,r1
mov.b @r1,r0
mov r8,r2
add #20,r2
mov.l r0,@r2
; Resume timer and observe an actual count, not register readback alone.
load 0xfffffe91,r1
mov #0,r0
mov.b r0,@r1
load 0xfffffe12,r1
wait_count:
mov.b @r1,r0
add #1,r1
mov.b @r1,r0
add #-1,r1
tst #255,r0
bt wait_count
mov r8,r2
add #24,r2
mov.l r0,@r2
; Start refresh interval interrupt at /16 with compare 20.
load 0xfffffee2,r1
mov #0x50,r0
mov.w r0,@r1
load 0xfffffee4,r1
mov #0x50,r0
mov.w r0,@r1
load 0xfffffff8,r1
load 0xa55a0014,r0
mov.l r0,@r1
load 0xfffffff0,r1
load 0xa55a0050,r0
mov.l r0,@r1
; Mask DCC HBlank while retaining the level-5 refresh IRQ.
mov #0x20,r0
ldc r0,sr
nop
mov r8,r2
add #12,r2
wait_irq:
mov.l @r2,r0
tst #255,r0
bt wait_irq
; Master starts slave only after finishing its own diagnostic.
load 0x8000,r2
tst r2,r9
bf finished
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #2,r0
mov.b r0,@r1
finished:
; No more IRQs are used by this diagnostic.
mov #-16,r0
ldc r0,sr
nop
mov r8,r2
add #28,r2
load 0x600d,r0
mov.l r0,@r2
park:
sleep
bra park
nop
.section 0x1800
; R8 is CPU-local result pointer; preserve all interrupted scratch registers.
add #-12,r15
mov.l r0,@r15
add #4,r15
mov.l r1,@r15
add #4,r15
mov.l r2,@r15
load 0xfffffff0,r1
mov.l @r1,r0
mov r8,r2
add #8,r2
mov.l r0,@r2
; A flagged read followed by keyed zero clears CMF and stops the counter.
load 0xa55a0000,r0
mov.l r0,@r1
mov r8,r2
add #12,r2
mov.l @r2,r0
add #1,r0
mov.l r0,@r2
mov.l @r15,r2
add #-4,r15
mov.l @r15,r1
add #-4,r15
mov.l @r15,r0
add #12,r15
rte
nop
