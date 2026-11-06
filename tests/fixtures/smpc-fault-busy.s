load 0x100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x10001f,r1
load 0x9,r0
mov.b r0,@r1
load 0x10001f,r1
load 0x8,r0
mov.b r0,@r1
load 0x2601a200,r1
load 0x55,r0
mov.b r0,@r1
park:
sleep
bra park
nop
