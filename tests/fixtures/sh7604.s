.section 0x200
; Original SH7604 diagnostic, MIT (adjacent LICENSE). H §§4/5/9/10/12/13.
; Mailbox 06003000: q32,r32,q64,r64,DMA IRQ,WDT IRQ,serial status,
; reset count,reset cause,alignment-error handler,done.
load 0x06003000,r8
load 0xffffff00,r1
mov #7,r0
mov.l r0,@r1
add #4,r1
mov #-100,r0
mov.l r0,@r1
mov.l @r1,r0
mov.l r0,@r8
load 0xffffff10,r1
mov.l @r1,r0
add #4,r8
mov.l r0,@r8
; 00000001:00000000 / 3 = 55555555 remainder 1.
load 0xffffff00,r1
mov #3,r0
mov.l r0,@r1
add #16,r1
mov #1,r0
mov.l r0,@r1
add #4,r1
mov #0,r0
mov.l r0,@r1
mov.l @r1,r0
add #4,r8
mov.l r0,@r8
add #-4,r1
mov.l @r1,r0
add #4,r8
mov.l r0,@r8
; DMA source is computed by the guest, not installed by the test host.
load 0x06004000,r1
load 0x12345678,r0
mov #4,r2
fill:
mov.l r0,@r1
add #1,r0
add #4,r1
dt r2
bf fill
load 0xfffffee2,r1
load 0x0650,r0
mov.w r0,@r1
load 0xffffffA0,r1
mov #0x44,r0
mov.l r0,@r1
load 0x4500,r0
load 0xfffffee4,r1
mov.w r0,@r1
load 0xffffff80,r1
load 0x06004000,r0
mov.l r0,@r1
add #4,r1
load 0x06005000,r0
mov.l r0,@r1
add #4,r1
mov #4,r0
mov.l r0,@r1
add #4,r1
load 0x5a05,r0
mov.l r0,@r1
mov #0,r0
ldc r0,sr
nop
; Warm the master cache with the pre-DMA destination. DMA must not snoop it.
load 0xfffffe92,r1
mov #0x11,r0
mov.b r0,@r1
load 0x06005000,r1
mov.l @r1,r0
load 0x0600302c,r2
mov.l r0,@r2
load 0xffffffb0,r1
mov #1,r0
mov.l r0,@r1
; IRQ may arrive before the next instruction. Poll the guest-written mailbox
; rather than assuming that a SLEEP will always precede the request.
load 0x06003010,r9
dma_wait:
mov.l @r9,r0
tst #255,r0
bt dma_wait
; The cached destination remains stale until a guest associative purge.
load 0x06005000,r1
mov.l @r1,r0
load 0x06003030,r2
mov.l r0,@r2
load 0x46005000,r1
mov #0,r0
mov.l r0,@r1
load 0x06005000,r1
mov.l @r1,r0
load 0x06003034,r2
mov.l r0,@r2
; A misaligned DMA source must dispatch vector 10 even with SR.I=15.
load 0xf0,r0
ldc r0,sr
nop
load 0xffffff80,r1
load 0x06004001,r0
mov.l r0,@r1
add #4,r1
load 0x06005020,r0
mov.l r0,@r1
add #4,r1
mov #1,r0
mov.l r0,@r1
add #4,r1
load 0x5605,r0
mov.l r0,@r1
load 0x0600303c,r9
error_wait:
mov.l @r9,r0
tst #255,r0
bt error_wait
mov #0,r0
ldc r0,sr
nop
; WDT interval timer at phi/8192 avoids completion before SLEEP.
load 0xfffffe80,r1
load 0xa53f,r0
mov.w r0,@r1
load 0x5aff,r0
mov.w r0,@r1
sleep
; SCI async 8N1, BRR=0: 320 reference clocks per character.
load 0xfffffe01,r1
mov #0,r0
mov.b r0,@r1
add #1,r1
mov #0x20,r0
mov.b r0,@r1
add #1,r1
mov #0x55,r0
mov.b r0,@r1
add #1,r1
mov.b @r1,r0
mov #0,r0
mov.b r0,@r1
serial_wait:
mov.b @r1,r0
tst #4,r0
bt serial_wait
load 0x06003018,r2
mov.l r0,@r2
; Watchdog manual reset must retain RAM and use vectors 8/12, not 0/4.
load 0xfffffe82,r1
load 0x5a7f,r0
mov.w r0,@r1
load 0xfffffe80,r1
load 0xa578,r0
mov.w r0,@r1
load 0x5aff,r0
mov.w r0,@r1
reset_wait:
sleep
bra reset_wait
nop

.section 0x1000
; DMA channel 0 end interrupt; read TE then clear it and disable channel.
load 0xffffff8c,r1
mov.l @r1,r0
mov #0,r0
mov.l r0,@r1
load 0x06003010,r1
mov #1,r0
mov.l r0,@r1
rte
nop

.section 0x1200
; WDT interval interrupt; read OVF then keyed clear and stop.
load 0xfffffe80,r1
mov.b @r1,r0
load 0xa518,r0
mov.w r0,@r1
load 0x06003014,r1
mov #1,r0
mov.l r0,@r1
rte
nop

.section 0x1400
; DMA address-error handler: cancel channel then read/clear DMAOR.AE.
load 0xffffff8c,r1
mov #0,r0
mov.l r0,@r1
load 0xffffffb0,r1
mov.l @r1,r0
mov #1,r0
mov.l r0,@r1
load 0x0600303c,r1
mov.l r0,@r1
rte
nop

.section 0x1800
load 0xfffffe92,r1
mov.b @r1,r0
load 0x06003038,r2
mov.l r0,@r2
load 0x0600301c,r1
mov #1,r0
mov.l r0,@r1
load 0xfffffe83,r1
mov.b @r1,r0
load 0x06003020,r1
mov.l r0,@r1
; A misaligned SP during TRAPA completes entry, then dispatches vector 9.
load 0x060ff001,r15
trapa #32
sleep

.section 0x1c00
; This handler must never retire an instruction: stacking error outranks fetch.
load 0xbad,r0
sleep

.section 0x1d00
load 0x060ff000,r15
load 0x06003024,r1
mov #1,r0
mov.l r0,@r1
add #4,r1
load 0x600d,r0
mov.l r0,@r1
sleep
