mov.l r0,@-r15
mov.l r1,@-r15
load 0x26010000,r1
mov.l @r1,r0
add #1,r0
mov.l r0,@r1
mov.l @r15+,r1
mov.l @r15+,r0
rte
nop
