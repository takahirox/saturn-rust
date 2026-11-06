; physical section 00000200
load 0x2600d000,r8
load 0xfffffe92,r1
mov #17,r0
mov.b r0,@r1
load 0x05fe006c,r1
mov.l @r1,r0
done:
sleep
bra done
nop
; physical section 00001800
add #1,r10
rte
nop
