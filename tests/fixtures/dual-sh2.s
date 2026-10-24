.section 0x200
; Original diagnostic, MIT (adjacent LICENSE). No Sega BIOS code/services.
; Both processors run the same reset vector (ST-169 table 1.1, ST-202 §1.1).
; Shared result table at 06002000: flag, boots, input, output, master IRQ count,
; master capture, slave IRQ count, slave capture, phase, master marker, done,
; NMI saved PC, NMI saved SR, NMI count.
load 0x06002000,r8
mov.l @r8,r0
tst #255,r0
bf.l slave
mov #1,r0
mov.l r0,@r8
mov #42,r14
; Freeze the counter with external clock select, seed FRC=1234.
load 0xfffffe16,r1
mov #3,r0
mov.b r0,@r1
load 0xfffffe12,r1
mov #0x12,r0
mov.b r0,@r1
add #1,r1
mov #0x34,r0
mov.b r0,@r1
load 0xfffffe60,r1
load 0x0500,r0
mov.w r0,@r1
load 0xfffffe66,r1
load 0x4041,r0
mov.w r0,@r1
load 0xfffffe10,r1
mov #-127,r0
mov.b r0,@r1
; Persistent pointers used only by main, scratch R0..R7 reserved for handlers.
load 0x20100063,r11
load 0x2010001f,r12
mov #1,r0
mov.b r0,@r11
mov #2,r0
mov.b r0,@r12
on_wait:
mov.b @r11,r0
tst #1,r0
bf on_wait
load 0x06002004,r10
ready_wait:
mov.l @r10,r0
mov #1,r1
cmp/eq r1,r0
bf ready_wait
; Send 35; slave guest adds 7 and signals master with its capture wire.
load 0x06002008,r1
mov #35,r0
mov.l r0,@r1
mov #0,r0
ldc r0,sr
nop
load 0x21000000,r1
mov.w r0,@r1
sleep
; Interrupt handler returns here with master stack and register bank intact.
mov #1,r0
mov.b r0,@r11
mov #3,r0
mov.b r0,@r12
off_wait:
mov.b @r11,r0
tst #1,r0
bf off_wait
load 0x06002020,r1
mov #2,r0
mov.l r0,@r1
mov #1,r0
mov.b r0,@r11
mov #2,r0
mov.b r0,@r12
on_again_wait:
mov.b @r11,r0
tst #1,r0
bf on_again_wait
ready_again_wait:
mov.l @r10,r0
mov #2,r1
cmp/eq r1,r0
bf ready_again_wait
load 0x21000000,r1
mov.w r0,@r1
sleep
mov #1,r0
mov.b r0,@r11
mov #3,r0
mov.b r0,@r12
off_again_wait:
mov.b @r11,r0
tst #1,r0
bf off_again_wait
load 0x06002024,r1
mov.l r14,@r1
; NMIREQ must reach master even at I=15, wake SLEEP, and return with RTE.
load 0xf0,r0
ldc r0,sr
nop
mov #1,r0
mov.b r0,@r11
mov #0x18,r0
mov.b r0,@r12
sleep
load 0x06002028,r1
load 0x600d,r0
mov.l r0,@r1
done_sleep:
sleep
bra done_sleep
nop
slave:
load 0x060fe000,r15
mov #17,r14
load 0xfffffe16,r1
mov #3,r0
mov.b r0,@r1
load 0xfffffe12,r1
mov #0x56,r0
mov.b r0,@r1
add #1,r1
mov #0x78,r0
mov.b r0,@r1
load 0xfffffe60,r1
load 0x0700,r0
mov.w r0,@r1
load 0xfffffe66,r1
load 0x4243,r0
mov.w r0,@r1
load 0xfffffe10,r1
mov #-127,r0
mov.b r0,@r1
; DCC raster levels are live; accept only our level-7 FRT above them.
mov #0x60,r0
ldc r0,sr
nop
load 0x06002004,r1
mov.l @r1,r0
add #1,r0
mov.l r0,@r1
slave_sleep:
sleep
bra slave_sleep
nop

.section 0x1000
; Master input-capture IRQ: flags require read before clear (H §11.2.5).
load 0xfffffe11,r1
mov.b @r1,r0
mov #0,r0
mov.b r0,@r1
load 0x06002010,r2
mov.l @r2,r0
add #1,r0
mov.l r0,@r2
; Read capture upper then lower through shared TEMP (H §11.3).
load 0xfffffe18,r1
mov.b @r1,r0
shll8 r0
add #1,r1
mov.b @r1,r3
add r3,r0
load 0x06002014,r2
mov.l r0,@r2
rte
nop

.section 0x1400
; Slave input-capture IRQ, with a genuine independent exception stack.
load 0xfffffe11,r1
mov.b @r1,r0
mov #0,r0
mov.b r0,@r1
load 0x06002008,r1
mov.l @r1,r0
add #7,r0
add #4,r1
mov.l r0,@r1
load 0x06002018,r2
mov.l @r2,r0
add #1,r0
mov.l r0,@r2
load 0xfffffe18,r1
mov.b @r1,r0
shll8 r0
add #1,r1
mov.b @r1,r3
add r3,r0
load 0x0600201c,r2
mov.l r0,@r2
load 0x21800000,r1
mov.w r0,@r1
rte
nop

.section 0x1800
; Master NMI handler logs the hardware exception frame then returns.
mov.l @r15,r0
load 0x0600202c,r1
mov.l r0,@r1
mov r15,r2
add #4,r2
mov.l @r2,r0
add #4,r1
mov.l r0,@r1
add #4,r1
mov #1,r0
mov.l r0,@r1
rte
nop
