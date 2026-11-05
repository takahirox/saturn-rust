load 0x20100060,r1
mov.l @r1,r0
load 0x2601a200,r1
load 0x55,r0
mov.b r0,@r1
park:
sleep
bra park
nop
