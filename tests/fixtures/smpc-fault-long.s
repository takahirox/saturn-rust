load 0x20100000,r1
load 0x00120034,r0
mov.l r0,@r1
load 0x2601a200,r1
load 0x55,r0
mov.b r0,@r1
park:
sleep
bra park
nop
