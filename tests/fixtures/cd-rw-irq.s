mov.l r0,@-r15
mov.l r1,@-r15
load 0x26010300,r1
mov.l @r1,r0
add #1,r0
mov.l r0,@r1
load 0x25890008,r1
load 0xfbff,r0
mov.w r0,@r1
load 0x25fe00a8,r1
load 0x1,r0
mov.l r0,@r1
mov.l @r15+,r1
mov.l @r15+,r0
rte
nop
