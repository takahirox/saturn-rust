; SPDX-License-Identifier: MIT
; Original system/control stack and RTE throughput guest.
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0x12345678,r2
lds r2,pr
load 0x1234,r2
lds r2,mach
load 0x06008000,r2
ldc r2,gbr
mov #0,r2
ldc r2,vbr
stack_loop:
mov #3,r2
mov #7,r3
mul.l r2,r3
sts.l pr,@-r15
sts.l mach,@-r15
sts.l macl,@-r15
stc.l sr,@-r15
stc.l gbr,@-r15
stc.l vbr,@-r15
ldc.l @r15+,vbr
ldc.l @r15+,gbr
ldc.l @r15+,sr
lds.l @r15+,macl
lds.l @r15+,mach
lds.l @r15+,pr
mov #0,r2
mov.l r2,@-r15
ldc.l @r15+,sr
mov #-16,r2
ldc r2,sr
stc sr,r2
mov.l r2,@-r15
load returned,r2
mov.l r2,@-r15
bra return_handler
nop
return_handler:
rte
nop
returned:
add #1,r0
bra stack_loop
nop
