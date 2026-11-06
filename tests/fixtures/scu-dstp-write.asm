; physical section 00000200
load 0x2600d000,r8
load 0x25fe0060,r1
load 0xa55a1234,r0
mov #100,r2
repeat:
mov.l r0,@r1
dt r2
bf repeat
load 0x600d,r0
mov.l r0,@r8
add #4,r8
done:
sleep
bra done
nop
; physical section 00001800
add #1,r10
rte
nop
