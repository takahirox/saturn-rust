; SPDX-License-Identifier: MIT
; Original continuous two-address SH-2 DMA throughput guest.
; Source/destination are disjoint 64 KiB WRAM regions; no firmware or SDK.
load 0xffffffe0,r1
load 0xa55a0000,r0
mov.l r0,@r1
load 0xffffffe8,r1
load 0xa55a0000,r0
mov.l r0,@r1
load 0x06010000,r1
load 0x11223344,r0
load 0x4000,r7
fill:
mov.l r0,@r1
add #4,r1
dt r7
bf fill
load 0xffffff80,r4
load 0xffffff84,r9
load 0xffffff88,r10
load 0xffffff8c,r11
load 0xffffffb0,r12
load 0x06010000,r1
load 0x06020000,r2
load 0x4000,r3
load 0x5e11,r5
load 0x06030000,r8
mov #1,r6
again:
mov.l r1,@r4
mov.l r2,@r9
mov.l r3,@r10
mov.l @r11,r0
mov.l r5,@r11
mov.l r6,@r12
wait:
mov.l @r10,r0
tst r0,r0
bf wait
mov.l @r8,r0
add #1,r0
mov.l r0,@r8
bra again
nop
