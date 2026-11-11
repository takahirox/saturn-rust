.section 0x200
; SPDX-License-Identifier: MIT
; Original resident SH-2 code with guest-started peer polling and shared RAM.
load 0x06002000,r8
mov.l @r8,r0
tst #255,r0
bf.l slave
mov #1,r0
mov.l r0,@r8
load 0xffffffe8,r1
load 0xa55a0000,r0
mov.l r0,@r1
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0x20100063,r1
mov.b r0,@r1
load 0x2010001f,r1
mov #2,r0
mov.b r0,@r1
load 0x06002020,r4
master_loop:
mov.l @r4,r5
add #1,r3
add r5,r3
mov r3,r6
xor r5,r6
mov.l r6,@r4
bra master_loop
nop
slave:
load 0x060fe000,r15
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0xfffffe11,r1
slave_poll:
mov.b @r1,r0
tst #128,r0
bt slave_poll
bra slave_poll
nop
