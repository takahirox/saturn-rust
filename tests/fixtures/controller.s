; Original MIT timed 3D-pad/analog-joystick handshake guest.
load 0x100079,r1
mov #96,r0
mov.b r0,@r1
load 0x10007b,r1
mov #96,r0
mov.b r0,@r1
load 0x10007d,r1
mov #3,r0
mov.b r0,@r1
load 0x100075,r1
load 0x600b000,r8
mov #96,r0
mov.b r0,@r1
mov #32,r0
mov.b r0,@r1
mov #16,r2
settle0:
dt r2
bf settle0
mov #0,r0
mov.b r0,@r1
wait0_0:
mov.b @r1,r0
tst #16,r0
bf wait0_0
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_1:
mov.b @r1,r0
tst #16,r0
bt wait0_1
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_2:
mov.b @r1,r0
tst #16,r0
bf wait0_2
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_3:
mov.b @r1,r0
tst #16,r0
bt wait0_3
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_4:
mov.b @r1,r0
tst #16,r0
bf wait0_4
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_5:
mov.b @r1,r0
tst #16,r0
bt wait0_5
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_6:
mov.b @r1,r0
tst #16,r0
bf wait0_6
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_7:
mov.b @r1,r0
tst #16,r0
bt wait0_7
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_8:
mov.b @r1,r0
tst #16,r0
bf wait0_8
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_9:
mov.b @r1,r0
tst #16,r0
bt wait0_9
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_10:
mov.b @r1,r0
tst #16,r0
bf wait0_10
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_11:
mov.b @r1,r0
tst #16,r0
bt wait0_11
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_12:
mov.b @r1,r0
tst #16,r0
bf wait0_12
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_13:
mov.b @r1,r0
tst #16,r0
bt wait0_13
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait0_14:
mov.b @r1,r0
tst #16,r0
bf wait0_14
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait0_15:
mov.b @r1,r0
tst #16,r0
bt wait0_15
mov.b r0,@r8
add #1,r8
mov #96,r0
mov.b r0,@r1
load 0x100077,r1
load 0x600b020,r8
mov #96,r0
mov.b r0,@r1
mov #32,r0
mov.b r0,@r1
mov #16,r2
settle1:
dt r2
bf settle1
mov #0,r0
mov.b r0,@r1
wait1_0:
mov.b @r1,r0
tst #16,r0
bf wait1_0
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_1:
mov.b @r1,r0
tst #16,r0
bt wait1_1
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_2:
mov.b @r1,r0
tst #16,r0
bf wait1_2
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_3:
mov.b @r1,r0
tst #16,r0
bt wait1_3
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_4:
mov.b @r1,r0
tst #16,r0
bf wait1_4
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_5:
mov.b @r1,r0
tst #16,r0
bt wait1_5
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_6:
mov.b @r1,r0
tst #16,r0
bf wait1_6
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_7:
mov.b @r1,r0
tst #16,r0
bt wait1_7
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_8:
mov.b @r1,r0
tst #16,r0
bf wait1_8
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_9:
mov.b @r1,r0
tst #16,r0
bt wait1_9
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_10:
mov.b @r1,r0
tst #16,r0
bf wait1_10
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_11:
mov.b @r1,r0
tst #16,r0
bt wait1_11
mov.b r0,@r8
add #1,r8
mov #0,r0
mov.b r0,@r1
wait1_12:
mov.b @r1,r0
tst #16,r0
bf wait1_12
mov.b r0,@r8
add #1,r8
mov #32,r0
mov.b r0,@r1
wait1_13:
mov.b @r1,r0
tst #16,r0
bt wait1_13
mov.b r0,@r8
add #1,r8
mov #96,r0
mov.b r0,@r1
load 0x0600B040,r1
load 0x600D,r0
mov.l r0,@r1
park:
sleep
bra park
nop
