load 0x0600e400,r8
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
load 0x1f7,r0
mov.w r0,@r1
load 0x25c00036,r1
load 0x3,r0
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
load 0x102,r0
mov.w r0,@r1
load 0x25c0004a,r1
load 0x102,r0
mov.w r0,@r1
load 0x25c0004e,r1
load 0x8,r0
mov.w r0,@r1
load 0x25c00060,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25c00800,r1
load 0x1111,r0
mov.w r0,@r1
load 0x25c00802,r1
load 0x1111,r0
mov.w r0,@r1
load 0x25c00810,r1
load 0x123,r0
mov.w r0,@r1
load 0x25c00812,r1
load 0x4567,r0
mov.w r0,@r1
load 0x25c00814,r1
load 0x89ab,r0
mov.w r0,@r1
load 0x25c00816,r1
load 0xcdef,r0
mov.w r0,@r1
load 0x100,r1
mov.w @r1,r0
load 0x25c01000,r4
mov #16,r5
initial_palette:
mov.w r0,@r4
add #2,r4
add #1,r0
dt r5
bf initial_palette
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
load 0x102,r1
mov.w @r1,r0
load 0x25c01000,r4
mov #16,r5
changed_palette:
mov.w r0,@r4
add #2,r4
add #1,r0
dt r5
bf changed_palette
load 0x25d00010,r1
poll_lookup:
mov.w @r1,r0
tst #2,r0
bt poll_lookup
load 0x25c80000,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c803ee,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80c00,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80fee,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82000,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82002,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82004,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82006,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82008,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8200a,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8200c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8200e,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82400,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82402,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82404,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82406,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c82408,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8240a,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8240c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c8240e,r1
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
