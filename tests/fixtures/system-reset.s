; Original SMPC SYSRES guest. MIT: adjacent LICENSE.
; ST-169 §2.3.8: BIOS restart, volatile memory loss, battery retention.
.section 0x200
load 0x00180001,r1
mov.b @r1,r0
mov #0x5a,r3
cmp/eq r3,r0
bt restarted
mov #0x5a,r0
mov.b r0,@r1
load 0x00200000,r1
mov.l r0,@r1
load 0x06001000,r1
mov.l r0,@r1
load 0x05a00100,r1
mov.l r0,@r1
load 0x05c00100,r1
mov.l r0,@r1
load 0x05e00100,r1
mov.l r0,@r1
load 0x05f00000,r1
mov.w r0,@r1
; SMEM battery-backed setting must also survive SYSRES.
load 0x20100001,r1
mov #0x12,r0
mov.b r0,@r1
add #2,r1
mov #0x34,r0
mov.b r0,@r1
add #2,r1
mov #0x56,r0
mov.b r0,@r1
add #2,r1
mov #0x78,r0
mov.b r0,@r1
load 0x20100063,r2
mov #1,r0
mov.b r0,@r2
load 0x2010001f,r1
mov #0x17,r0
mov.b r0,@r1
smem_wait:
mov.b @r2,r0
tst #1,r0
bf smem_wait
mov #1,r0
mov.b r0,@r2
mov #0x0d,r0
mov.b r0,@r1
wait_reset:
bra wait_reset
nop
restarted:
load 0x0600c000,r8
load 0x2010005f,r1
mov.b @r1,r0
mov.l r0,@r8
add #4,r8
load 0x00200000,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x06001000,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x05a00100,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x05c00100,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x05e00100,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x05f00000,r1
mov.w @r1,r0
mov.l r0,@r8
; Query retained SMEM through the documented INTBACK status path.
load 0x20100001,r1
mov #1,r0
mov.b r0,@r1
add #2,r1
mov #0,r0
mov.b r0,@r1
add #2,r1
mov #-16,r0
mov.b r0,@r1
load 0x20100063,r2
mov #1,r0
mov.b r0,@r2
load 0x2010001f,r1
mov #0x10,r0
mov.b r0,@r1
intback_wait:
mov.b @r2,r0
tst #1,r0
bf intback_wait
load 0x20100039,r1
mov #4,r3
copy_smem:
mov.b @r1,r0
add #2,r1
add #4,r8
mov.l r0,@r8
dt r3
bf copy_smem
add #4,r8
load 0x600d,r0
mov.l r0,@r8
park:
sleep
bra park
nop
