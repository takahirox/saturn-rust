load 0x0600e500,r8
load 0x25d00002,r1
load 0x2,r0
mov.w r0,@r1
load 0x25c00000,r1
load 0x9,r0
mov.w r0,@r1
load 0x25c00014,r1
load 0x1ff,r0
mov.w r0,@r1
load 0x25c00016,r1
load 0xff,r0
mov.w r0,@r1
load 0x25c00020,r1
load 0x1,r0
mov.w r0,@r1
load 0x25c00024,r1
load 0xc8,r0
mov.w r0,@r1
load 0x25c00026,r1
load 0x200,r0
mov.w r0,@r1
load 0x25c00028,r1
load 0x100,r0
mov.w r0,@r1
load 0x25c0002a,r1
load 0x101,r0
mov.w r0,@r1
load 0x25c00034,r1
load 0xfff,r0
mov.w r0,@r1
load 0x25c00036,r1
load 0x0,r0
mov.w r0,@r1
load 0x25c00040,r1
load 0x0,r0
mov.w r0,@r1
load 0x25c00044,r1
load 0xc8,r0
mov.w r0,@r1
load 0x25c00046,r1
load 0x200,r0
mov.w r0,@r1
load 0x25c00048,r1
load 0x100,r0
mov.w r0,@r1
load 0x25c0004a,r1
load 0x101,r0
mov.w r0,@r1
load 0x25c0004e,r1
load 0x2,r0
mov.w r0,@r1
load 0x25c00060,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25c00800,r1
load 0x1111,r0
mov.w r0,@r1
load 0x25c00802,r1
load 0x3333,r0
mov.w r0,@r1
load 0x25c01000,r1
load 0x8010,r0
mov.w r0,@r1
load 0x25c01002,r1
load 0x8011,r0
mov.w r0,@r1
load 0x25c01004,r1
load 0x8012,r0
mov.w r0,@r1
load 0x25c01006,r1
load 0x8013,r0
mov.w r0,@r1
load 0x25c01008,r1
load 0x8014,r0
mov.w r0,@r1
load 0x25c0100a,r1
load 0x8015,r0
mov.w r0,@r1
load 0x25c0100c,r1
load 0x8016,r0
mov.w r0,@r1
load 0x25c0100e,r1
load 0x8017,r0
mov.w r0,@r1
load 0x25c01010,r1
load 0x8018,r0
mov.w r0,@r1
load 0x25c01012,r1
load 0x8019,r0
mov.w r0,@r1
load 0x25c01014,r1
load 0x801a,r0
mov.w r0,@r1
load 0x25c01016,r1
load 0x801b,r0
mov.w r0,@r1
load 0x25c01018,r1
load 0x801c,r0
mov.w r0,@r1
load 0x25c0101a,r1
load 0x801d,r0
mov.w r0,@r1
load 0x25c0101c,r1
load 0x801e,r0
mov.w r0,@r1
load 0x25c0101e,r1
load 0x801f,r0
mov.w r0,@r1
load 0x25d00004,r1
load 0x1,r0
mov.w r0,@r1
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
nop
load 0x100,r1
mov.w @r1,r0
load 0x25c00800,r1
mov.w r0,@r1
load 0x25c00802,r1
load 0x4444,r0
mov.w r0,@r1
load 0x25d00010,r1
poll_character:
mov.w @r1,r0
tst #2,r0
bt poll_character
load 0x25c80000,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c801fe,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c803fe,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80800,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80802,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80804,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80806,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80808,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8080a,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8080c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8080e,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25d00010,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25d00014,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25fe00a4,r1
mov.l @r1,r0
load 0x2000,r2
and r2,r0
mov.l r0,@r8
add #4,r8
load 0x600d,r0
mov.l r0,@r8
add #4,r8
park:
sleep
bra park
nop
