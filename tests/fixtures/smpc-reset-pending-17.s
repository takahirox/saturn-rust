load 0x2601a000,r8
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load command_window,r3
jsr @r3
nop
load 0x20100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100001,r1
load 0x12,r0
mov.b r0,@r1
load 0x20100003,r1
load 0x34,r0
mov.b r0,@r1
load 0x20100005,r1
load 0x56,r0
mov.b r0,@r1
load 0x20100007,r1
load 0x78,r0
mov.b r0,@r1
load 0x2010001f,r1
load 0x17,r0
mov.b r0,@r1
load 0x20100063,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010005f,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2601a200,r1
load 0x55,r0
mov.b r0,@r1
park:
sleep
bra park
nop
command_window:
load 0x25f80004,r1
window_active:
mov.w @r1,r0
tst #8,r0
bf window_active
window_blank:
mov.w @r1,r0
tst #8,r0
bt window_blank
load 0xfffffe12,r4
load 0xfffffe13,r5
mov.b @r4,r0
extu.b r0,r0
shll8 r0
mov.b @r5,r1
extu.b r1,r1
or r1,r0
mov r0,r6
load 1200,r7
window_delay:
mov.b @r4,r0
extu.b r0,r0
shll8 r0
mov.b @r5,r1
extu.b r1,r1
or r1,r0
sub r6,r0
extu.w r0,r0
cmp/hs r7,r0
bf window_delay
mov r0,r6
movt r0
mov.l r0,@r8
add #4,r8
load 2000,r7
cmp/hs r6,r7
movt r0
mov.l r0,@r8
add #4,r8
load 0x25f80004,r1
mov.w @r1,r0
mov #8,r2
and r2,r0
mov.l r0,@r8
add #4,r8
rts
nop
