mov #-16,r0
ldc r0,sr
load 0x3000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
load 0x200,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
load 0x25818000,r8
load 0x06020000,r9
load 204,r10
toc_copy:
mov.w @r8,r0
mov.w r0,@r9
add #2,r9
dt r10
bf toc_copy
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
load 0x100,r1
mov.w @r1,r12
extu.w r12,r12
load 0x06021000,r9
load 0x1080,r4
load 0x96,r5
load 0x80,r6
mov r12,r7
bsr command
nop
sector_wait:
load 0x5100,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
tst #255,r0
bt sector_wait
load 0x6300,r4
load 0x0,r5
load 0x0,r6
load 0x1,r7
bsr command
nop
load 1024,r10
sector_copy:
mov.w @r8,r0
mov.w r0,@r9
add #2,r9
dt r10
bf sector_copy
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
dt r12
bf sector_wait
load 0x1100,r4
load 0x96,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
load 0x25b00400,r1
load 0x20f,r0
mov.w r0,@r1
load 0x25b00216,r1
load 0xff,r0
mov.w r0,@r1
load 0x25b00236,r1
load 0xef,r0
mov.w r0,@r1
load 0x102,r1
mov.w @r1,r5
extu.w r5,r5
tst r5,r5
bt done
load 0x1080,r4
load 0x80,r6
mov #8,r7
bsr command
nop
done:
load 0x600d,r0
load 0x0601fffc,r1
mov.l r0,@r1
park:
sleep
bra park
nop
command:
load 0x25890008,r1
mov #-2,r0
mov.w r0,@r1
load 0x25890018,r1
mov.w r4,@r1
add #4,r1
mov.w r5,@r1
add #4,r1
mov.w r6,@r1
add #4,r1
mov.w r7,@r1
load 0x25890008,r1
command_wait:
mov.w @r1,r0
tst #1,r0
bt command_wait
load 0x25890024,r1
mov.w @r1,r0
extu.w r0,r0
rts
nop
