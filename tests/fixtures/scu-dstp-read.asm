; physical section 00000200
load 0x2600d000,r8
load 0x25fe0060,r1
mov.l @r1,r0
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
