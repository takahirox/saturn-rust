load 0x200fffe1,r0
ldc r0,vbr
trapa #32
load 0x2601a200,r1
load 0x55,r0
mov.b r0,@r1
park:
sleep
bra park
nop
