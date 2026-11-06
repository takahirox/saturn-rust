; Master/reset entry at 0x200
load 0x2601e200,r1
mov.l @r1,r0
tst r0,r0
bt master
load 0x10000,r1
jmp @r1
nop
master:
load 0x2601e203,r1
load 0x1,r0
mov.b r0,@r1
load 0x2601e000,r8
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x20000,r3
jsr @r3
nop
load 0x100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x10001f,r1
load 0x2,r0
mov.b r0,@r1
load 0x100063,r1
slave_started:
mov.b @r1,r0
tst #1,r0
bf slave_started
load 0x2601e204,r1
wait_slave:
mov.l @r1,r0
tst r0,r0
bt wait_slave
load 0x20000,r3
jsr @r3
nop
load 0x20100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100001,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100003,r1
load 0x0,r0
mov.b r0,@r1
load 0x20100005,r1
load 0xf0,r0
mov.b r0,@r1
load 0x2010001f,r1
load 0x10,r0
mov.b r0,@r1
load 0x100063,r1
master_status:
mov.b @r1,r0
tst #1,r0
bf master_status
load 0x20100021,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x20100023,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x20100025,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x20100027,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x20100029,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010002b,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010002d,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010002f,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x20100039,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010003b,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010003d,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
load 0x2010003f,r1
mov.b @r1,r0
extu.b r0,r0
mov.l r0,@r8
add #4,r8
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
load 0x600d,r0
mov.l r0,@r8
add #4,r8
park:
sleep
bra park
nop
; Slave entry at 0x10000
load 0x060fe000,r15
load 0x2601e100,r8
load 0x100063,r1
master_finished:
mov.b @r1,r0
tst #1,r0
bf master_finished
load 0x20000,r3
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
load 0x100063,r1
slave_memory:
mov.b @r1,r0
tst #1,r0
bf slave_memory
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
load 0x20000,r3
jsr @r3
nop
load 0x20100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100001,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100003,r1
load 0x0,r0
mov.b r0,@r1
load 0x20100005,r1
load 0xf0,r0
mov.b r0,@r1
load 0x2010001f,r1
load 0x2,r0
mov.b r0,@r1
load 0x100063,r1
slave_command:
mov.b @r1,r0
tst #1,r0
bf slave_command
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
load 0x2601e207,r1
load 0x1,r0
mov.b r0,@r1
load 0x600d,r0
mov.l r0,@r8
add #4,r8
park:
sleep
bra park
nop
; Shared window routine at 0x20000
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
