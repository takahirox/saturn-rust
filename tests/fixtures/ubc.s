; Original guest UBC diagnostics. MIT: adjacent LICENSE.
; SH7604 §§6.3.2/6.3.3/6.3.5. Break target PCs are fixed section addresses.
.section 0x200
load 0x0600a000,r8
load 0xffffff40,r1
load 0x1000,r0
mov.l r0,@r1
load 0xffffff48,r1
mov #0x56,r0
mov.w r0,@r1
load 0xffffff78,r1
load 0x1000,r0
mov.w r0,@r1
mov #0,r0
ldc r0,sr
nop
load 0x1000,r1
jmp @r1
nop
.section 0x1000
; Before-execution: handler observes R9=0 and saved PC=1000.
mov #11,r9
; After-execution: handler observes R9=22 and saved PC=1102.
load 0xffffff40,r1
load 0x1100,r0
mov.l r0,@r1
load 0xffffff48,r1
mov #0x56,r0
mov.w r0,@r1
load 0xffffff78,r1
load 0x1400,r0
mov.w r0,@r1
load 0x1100,r1
jmp @r1
nop
.section 0x1100
mov #22,r9
; A before-execution breakpoint on a branch slot is deferred to its target.
load 0xffffff40,r1
load 0x1202,r0
mov.l r0,@r1
load 0xffffff48,r1
mov #0x56,r0
mov.w r0,@r1
load 0xffffff78,r1
load 0x1000,r0
mov.w r0,@r1
load 0x1200,r1
jmp @r1
nop
.section 0x1200
bra target
mov #33,r9
nop
target:
; Data breakpoint on a guest-computed word, channel B masked range.
load 0xffffff60,r1
load 0x0600b0ff,r0
mov.l r0,@r1
add #4,r1
load 0xff,r0
mov.l r0,@r1
load 0xffffff70,r1
load 0xa512,r0
mov.l r0,@r1
load 0xffffff68,r1
mov #0x6a,r0
mov.w r0,@r1
load 0xffffff78,r1
load 0x1008,r0
mov.w r0,@r1
load 0x0600b020,r1
load 0xa500,r0
add #18,r0
mov.w r0,@r1
; DMA write breakpoint matches a channel-B peripheral byte/data/write.
load 0xffffff60,r1
load 0x0600b100,r0
mov.l r0,@r1
add #4,r1
mov #0,r0
mov.l r0,@r1
load 0xffffff68,r1
load 0xa9,r0
mov.w r0,@r1
load 0xffffff70,r1
load 0xa5a5,r0
mov.l r0,@r1
load 0xffffff78,r1
load 0x1008,r0
mov.w r0,@r1
load 0xffffff80,r1
load 0x0600b020,r0
mov.l r0,@r1
add #4,r1
load 0x0600b100,r0
mov.l r0,@r1
add #4,r1
mov #1,r0
mov.l r0,@r1
add #4,r1
load 0x5201,r0
mov.l r0,@r1
load 0xffffffb0,r1
mov #1,r0
mov.l r0,@r1
load 0x0600a050,r1
load 0x600d,r0
mov.l r0,@r1
park:
sleep
bra park
nop
.section 0x1800
; Append {saved PC, R9, BRCR, stacked SR}, then disable both conditions.
mov.l @r15,r10
mov.l r10,@r8
add #4,r8
mov.l r9,@r8
add #4,r8
load 0xffffff78,r11
mov.w @r11,r10
extu.w r10,r10
mov.l r10,@r8
add #4,r8
mov r15,r11
add #4,r11
mov.l @r11,r10
mov.l r10,@r8
add #4,r8
load 0xffffff48,r11
mov #0,r10
mov.w r10,@r11
load 0xffffff68,r11
mov.w r10,@r11
rte
nop
