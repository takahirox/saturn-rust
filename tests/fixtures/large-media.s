mov #-16,r0
ldc r0,sr
load 0x06000000,r0
ldc r0,vbr
load 0x20180000,r1
mov.w @r1,r0
add #1,r0
mov.w r0,@r1
load 0x600012c,r1
load 0x6005000,r0
mov.l r0,@r1
load 0x6000140,r1
load 0x6005200,r0
mov.l r0,@r1
load 0x25fe00a0,r1
load 0x37ff,r0
mov.l r0,@r1
load 0x25fe00a8,r1
load 0x1,r0
mov.l r0,@r1
load 0x2589000c,r1
load 0x4,r0
mov.w r0,@r1
mov #0,r0
ldc r0,sr
load 0x3000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
read_command:
load 0x1083,r4
load 0xf836,r5
load 0x80,r6
load 0x1,r7
bsr command
nop
sector_wait:
load 0x5100,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
mov #1,r2
cmp/eq r2,r0
bf sector_wait
load 0x6300,r4
load 0x0,r5
load 0x0,r6
load 0x1,r7
bsr command
nop
load 0x25fe0000,r1
load 0x5818000,r0
mov.l r0,@r1
load 0x25fe0004,r1
load 0x6011000,r0
mov.l r0,@r1
load 0x25fe0008,r1
load 0x800,r0
mov.l r0,@r1
load 0x25fe000c,r1
load 0x2,r0
mov.l r0,@r1
load 0x25fe0014,r1
load 0x7,r0
mov.l r0,@r1
load 0x25fe0010,r1
load 0x101,r0
mov.l r0,@r1
load 0x06010004,r1
dma_wait:
mov.l @r1,r0
tst r0,r0
bt dma_wait
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
bsr command
nop
load 0x06011000,r2
load 512,r3
load 7,r4
sum_data:
mov.l @r2+,r0
add r0,r4
dt r3
bf sum_data
load 0x06010000,r1
mov.l r4,@r1
load 0x25b00400,r1
load 0x20f,r0
mov.w r0,@r1
load 0x25b00216,r1
load 0xff,r0
mov.w r0,@r1
load 0x25b00236,r1
load 0xef,r0
mov.w r0,@r1
load 0x1084,r4
load 0x2072,r5
load 0xf80,r6
load 0x12c,r7
bsr command
nop
load 0x601000c,r1
load 0x600d,r0
mov.l r0,@r1
load 0x20100079,r1
load 0x60,r0
mov.b r0,@r1
load 0x2010007d,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100075,r1
load 0x40,r0
mov.b r0,@r1
restart_wait:
load 0x20100075,r1
mov.b @r1,r0
tst #8,r0
bf restart_wait
load 0x20100063,r1
load 0x1,r0
mov.b r0,@r1
load 0x2010001f,r1
load 0xd,r0
mov.b r0,@r1
park:
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

; DMA handler at 0x06005000
mov.l r0,@-r15
mov.l r1,@-r15
load 0x6010004,r1
load 0x1,r0
mov.l r0,@r1
mov.l @r15+,r1
mov.l @r15+,r0
rte
nop

; CD IRQ handler at 0x06005200
mov.l r0,@-r15
mov.l r1,@-r15
load 0x25890008,r1
load 0xfffb,r0
mov.w r0,@r1
load 0x25fe00a8,r1
load 0x1,r0
mov.l r0,@r1
load 0x06010008,r1
mov.l @r1,r0
add #1,r0
mov.l r0,@r1
mov.l @r15+,r1
mov.l @r15+,r0
rte
nop
