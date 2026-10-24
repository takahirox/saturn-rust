; Original MIT rotation image deadline diagnostic.
load 0x25e7001c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e7002c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e7004c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e70050,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e7009c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e700ac,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e700cc,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e700d0,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25f8000e,r1
load 0x30e,r0
mov.w r0,@r1
load 0x25f80020,r1
load 0x30,r0
mov.w r0,@r1
load 0x25f80038,r1
load 0x8004,r0
mov.w r0,@r1
load 0x25f80030,r1
load 0x8008,r0
mov.w r0,@r1
load 0x25f8003e,r1
load 0x10,r0
mov.w r0,@r1
load 0x25f80060,r1
load 0x3030,r0
mov.w r0,@r1
load 0x25f8002a,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f800fc,r1
load 0x2,r0
mov.w r0,@r1
load 0x25f800f8,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f800bc,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f800be,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25f00002,r1
load 0x801f,r0
mov.w r0,@r1
load 0x25f00004,r1
load 0x83e0,r0
mov.w r0,@r1
load 0x25f00006,r1
load 0xfc00,r0
mov.w r0,@r1
load 0x25e00000,r1
load 0x0,r0
mov.w r0,@r1
load 0x25e60000,r1
load 0x0,r0
mov.w r0,@r1
load 0x100,r1
mov.w @r1,r0
load 0x25e40000,r1
mov.w r0,@r1
load 0xfffffe92,r1
mov #1,r0
mov.b r0,@r1
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25f80004,r1
in_rotation_start:
mov.w @r1,r0
tst #8,r0
bt in_rotation_start
out_rotation_start:
mov.w @r1,r0
tst #8,r0
bf out_rotation_start
load 15,r3
line_15:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_15
load 0x106,r1
mov.w @r1,r2
delay:
dt r2
bf delay
load 0x25e40000,r5
load 0x2222,r6
load 0x102,r1
mov.w @r1,r3
load 0x104,r1
mov.w @r1,r0
load 0x25f80008,r4
load 0x25f80002,r1
tst #1,r0
bt omit_wait
nop
nop
nop
nop
timed_wait:
mov.w @r1,r0
mov.w @r4,r0
cmp/hs r3,r0
bf timed_wait
bra store_done
mov.w r6,@r5
store_done:
mov r0,r8
bra record
nop
omit_wait:
mov.w @r1,r0
mov.w @r4,r0
cmp/hs r3,r0
bf omit_wait
mov r0,r8
record:
load 0x2600dc00,r1
mov.l r8,@r1
mov.w @r5,r0
load 0x2600dc04,r1
mov.l r0,@r1
load 0x100,r1
mov.w @r1,r0
mov.w @r5,r2
add r2,r0
load 0x2600dc08,r1
mov.l r0,@r1
load 20,r3
line_20:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_20
load 0x2600dc0c,r1
load 0x600d,r0
mov.l r0,@r1
park:
sleep
bra park
nop
