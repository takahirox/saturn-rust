load 0x25f80010,r1
load 0x4eee,r0
mov.w r0,@r1
load 0x25f80012,r1
load 0xeeee,r0
mov.w r0,@r1
load 0x25f80020,r1
load 0x1,r0
mov.w r0,@r1
load 0x25f80028,r1
load 0x2,r0
mov.w r0,@r1
load 0x25f80078,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f8007a,r1
load 0x100,r0
mov.w r0,@r1
load 0x25f8007c,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f8007e,r1
load 0x100,r0
mov.w r0,@r1
load 0x25f800f8,r1
load 0x1,r0
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
load 0x100,r1
mov.w @r1,r0
load 0x25e00000,r1
mov.w r0,@r1
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25f80004,r1
held_blank_in:
mov.w @r1,r0
tst #8,r0
bt held_blank_in
held_blank_out:
mov.w @r1,r0
tst #8,r0
bf held_blank_out
load 16,r3
line_16:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_16
load 0x25e00000,r1
load 0x2222,r0
mov.w r0,@r1
load 0x25f80004,r1
held_finish:
mov.w @r1,r0
tst #8,r0
bt held_finish
load 0x2600df04,r1
load 0x600d,r0
mov.l r0,@r1
held_park:
sleep
bra held_park
nop
