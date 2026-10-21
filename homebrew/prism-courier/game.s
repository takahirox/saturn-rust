; Prism Courier: original SH-2 guest, MIT: adjacent LICENSE.
; Steer the cyan prism through the gap in the red barrier to the green dock.
; Touching the barrier loses; Start restarts from either outcome.
; Graphics use VDP1 commands/texture and a VDP2 background; sound is an
; independently executing original 68k program. Host code never paints gameplay.
start:
    ; Install our own VDP1 draw-end handler in a guest vector table.
    load 0x0601c000,r0
    ldc r0,vbr
    load 0x0601c134,r1
    load 0x06006800,r0
    mov.l r0,@r1
    load 0x05fe00a0,r1
    load 0x9fff,r0
    mov.l r0,@r1
    mov #0,r0
    ldc r0,sr
    load 0x06006000,r1
    load 0x05c00000,r2
    mov #64,r3
copy_commands:
    mov.l @r1,r0
    mov.l r0,@r2
    add #4,r1
    add #4,r2
    dt r3
    bf copy_commands
    load 0x06006400,r1
    load 0x05c01000,r2
    mov #32,r3
copy_texture:
    mov.l @r1,r0
    mov.l r0,@r2
    add #4,r1
    add #4,r2
    dt r3
    bf copy_texture
    load 0x06007000,r1
    load 0x05a00000,r2
    load 0x1100,r3
copy_sound:
    mov.l @r1,r0
    mov.l r0,@r2
    add #4,r1
    add #4,r2
    dt r3
    bf copy_sound
    load 0x00100063,r1
    mov #1,r0
    mov.b r0,@r1
    load 0x0010001f,r1
    mov #6,r0
    mov.b r0,@r1
    ; Guest clears the 512x256 RGB555 background, with 32-pixel stripes.
    load 0x05e00000,r1
    load 131072,r2
    load 0x8821,r5
    mov #0,r6
background:
    mov r5,r0
    add r6,r0
    mov.w r0,@r1
    add #2,r1
    add #1,r6
    mov #16,r0
    cmp/eq r0,r6
    bf next_pixel
    mov #0,r6
next_pixel:
    dt r2
    bf background
    ; Four NBG0 direct-color reads and four CPU grants per display cycle.
    load 0x05f80010,r1
    load 0x4444eeee,r0
    mov.l r0,@r1
    load 0x05f80000,r1
    load 0x8000,r0
    mov.w r0,@r1
    load 0x05f80020,r1
    load 0x101,r0
    mov.w r0,@r1
    load 0x05f80028,r1
    mov #50,r0
    mov.w r0,@r1
    load 0x05f80078,r1
    load 0x10000,r0
    mov.l r0,@r1
    add #4,r1
    mov.l r0,@r1
    load 0x05f800f8,r1
    mov #1,r0
    mov.w r0,@r1
    load 0x05f800e0,r1
    mov #32,r0
    mov.w r0,@r1
    load 0x05f800f0,r1
    mov #2,r0
    mov.w r0,@r1
    load 0x05d00000,r1
    mov #0,r0
    mov.w r0,@r1
    add #2,r1
    mov.w r0,@r1
    add #4,r1
    mov.w r0,@r1
    add #2,r1
    mov.w r0,@r1
    add #2,r1
    load 0x80ff,r0
    mov.w r0,@r1
    load 0x05d00004,r1
    mov #2,r0
    mov.w r0,@r1
    ; Configure the first digital port with guest direct-access MMIO.
    load 0x20100079,r1
    mov #96,r0
    mov.b r0,@r1
    load 0x2010007d,r1
    mov #1,r0
    mov.b r0,@r1
    load 0x05f80004,r7
    mov #0,r13
    mov #0,r14
new_game:
    mov #24,r8
    mov #112,r9
    mov #0,r10
    mov #0,r11
    load 0x8421,r12
    bra paint
    nop
frame:
wait_active:
    mov.w @r7,r0
    tst #8,r0
    bf wait_active
wait_blank:
    mov.w @r7,r0
    tst #8,r0
    bt wait_blank
    load 0x20100075,r1
    mov #64,r0
    mov.b r0,@r1
    mov #32,r2
settle_start:
    dt r2
    bf settle_start
    mov.b @r1,r0
    tst #8,r0
    bf start_released
    mov #0,r0
    cmp/eq r0,r13
    bt after_start
    mov #0,r13
    bra new_game
    nop
start_released:
    mov #1,r13
after_start:
    mov #0,r0
    cmp/eq r0,r11
    bf.l paint
    mov #32,r0
    mov.b r0,@r1
    mov #32,r2
settle_pad:
    dt r2
    bf settle_pad
    mov.b @r1,r6
    mov r6,r0
    tst #8,r0
    bf left
    add #4,r8
left:
    mov r6,r0
    tst #4,r0
    bf down
    mov #16,r0
    cmp/hs r8,r0
    bt down
    add #-4,r8
down:
    mov r6,r0
    tst #2,r0
    bf up
    load 176,r0
    cmp/hs r0,r9
    bt up
    add #4,r9
up:
    mov r6,r0
    tst #1,r0
    bf collision
    mov #48,r0
    cmp/hs r9,r0
    bt collision
    add #-4,r9
collision:
    load 140,r0
    cmp/hs r0,r8
    bf progress
    load 168,r0
    cmp/hs r0,r8
    bt progress
    mov #92,r0
    cmp/hs r0,r9
    bf lost
    load 128,r0
    cmp/hs r0,r9
    bf progress
lost:
    mov #2,r11
    load 0x801f,r12
progress:
    mov r10,r0
    shll r0
    shll2 r0
    shll2 r0
    add #64,r0
    cmp/hs r0,r8
    bf win_test
    add #1,r10
win_test:
    load 280,r0
    cmp/hs r0,r8
    bf paint
    mov #1,r11
    load 0x83e0,r12
paint:
    load 0x05c000ac,r1
    mov.w r8,@r1
    add #2,r1
    mov.w r9,@r1
    load 0x05c000c6,r1
    mov.w r12,@r1
    add #1,r14
    load 0x06018000,r1
    mov.l r8,@r1
    add #4,r1
    mov.l r9,@r1
    add #4,r1
    mov.l r10,@r1
    add #4,r1
    mov.l r11,@r1
    add #4,r1
    mov.l r14,@r1
    bra frame
    nop
