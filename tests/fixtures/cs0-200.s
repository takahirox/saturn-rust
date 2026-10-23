; Original MIT CS0 guest; adjacent LICENSE.
load 0xffffffe0,r1
mov.l @r1,r0
load 0x8000,r2
tst r2,r0
bf slave
load 0x800,r1
jmp @r1
nop
slave:
load 0x2800,r1
jmp @r1
nop
