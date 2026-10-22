load 0x0601000c,r1
mov.l @r1,r0
add #7,r0
load 0x06010010,r1
mov.l r0,@r1
park_slave:
sleep
bra park_slave
nop
