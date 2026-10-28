load 0x0600d800,r8
load 0x25d00002,r1
load 0x2,r0
mov.w r0,@r1
load 0x25f80000,r1
load 0x8000,r0
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
load 0x6,r0
mov.w r0,@r1
load 0x25c00024,r1
load 0x8c0,r0
mov.w r0,@r1
load 0x25c00026,r1
load 0x801f,r0
mov.w r0,@r1
load 0x25c00030,r1
load 0xfff,r0
mov.w r0,@r1
load 0x25c00040,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25d00004,r1
load 0x1,r0
mov.w r0,@r1
load 0x25d00014,r1
wait_line:
mov.w @r1,r0
mov #4,r2
cmp/eq r2,r0
bf wait_line
load 0x25c80000,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c803fe,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25d0000c,r1
load 0x0,r0
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
load 0x25d00010,r1
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
load 0x25d00002,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f80004,r1
in_partial:
mov.w @r1,r0
tst #8,r0
bt in_partial
out_partial:
mov.w @r1,r0
tst #8,r0
bf out_partial
load 0x25d00012,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25d00010,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c80000,r1
load 0xffff,r0
mov.w r0,@r1
load 0x25c9cb76,r1
load 0xffff,r0
mov.w r0,@r1
load 0x25c9cb78,r1
load 0xffff,r0
mov.w r0,@r1
load 0x25d00006,r1
load 0x1234,r0
mov.w r0,@r1
load 0x25d00008,r1
load 0x0,r0
mov.w r0,@r1
load 0x25d0000a,r1
load 0x80ff,r0
mov.w r0,@r1
load 0x25d00002,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f80004,r1
in_publish:
mov.w @r1,r0
tst #8,r0
bt in_publish
out_publish:
mov.w @r1,r0
tst #8,r0
bf out_publish
load 0x25f80004,r1
wait_enable:
mov.w @r1,r0
tst #8,r0
bt wait_enable
load 0x25d00000,r1
load 0x8,r0
mov.w r0,@r1
load 0x25d00002,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f80004,r1
in_erase:
mov.w @r1,r0
tst #8,r0
bt in_erase
out_erase:
mov.w @r1,r0
tst #8,r0
bf out_erase
load 0x25c80000,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c9cb76,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x25c9cb78,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r8
add #4,r8
load 0x600d,r0
mov.l r0,@r8
add #4,r8
park:
sleep
bra park
nop
