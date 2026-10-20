; Star Collector, original Saturn SH-2 guest. MIT: see LICENSE.
; No BIOS calls. See README for register and disc contracts.
start:
    load 0x05f80000,r1
    load 0x8000,r0
    mov.w r0,@r1
    load 0x05f80020,r1
    load 0x0101,r0
    mov.w r0,@r1
    load 0x05f80028,r1
    mov #50,r0
    mov.w r0,@r1
    load 0x05f8003c,r1
    mov #0,r0
    mov.w r0,@r1
    load 0x05f80078,r1
    load 0x00010000,r0
    mov.l r0,@r1
    add #4,r1
    mov.l r0,@r1
    load 0x05f800f8,r1
    mov #1,r0
    mov.w r0,@r1
    ; VRAM-A0 cycle pattern: CPU and NBG0 bitmap fetch slots.
    load 0x05f80010,r1
    load 0x4444,r0
    mov.w r0,@r1
    add #2,r1
    load 0xeeee,r0
    mov.w r0,@r1
    load 0x20100075,r1
    mov #96,r0
    mov.b r0,@r1
    add #4,r1
    mov.b r0,@r1
    load 0x2010007d,r1
    mov #1,r0
    mov.b r0,@r1
    add #2,r1
    mov #0,r0
    mov.b r0,@r1
    load 0x05f80004,r7
new_game:
    mov #32,r8
    mov #112,r9
    mov #64,r10
    mov #0,r11
    mov #0,r12
    mov #0,r13
    mov #0,r14
    ; Clear the full 512x256 bitmap ourselves, also on in-game restart.
    load 0x05e00000,r1
    load 131072,r2
    load 0x8421,r0
clear:
    mov.w r0,@r1
    add #2,r1
    dt r2
    bf clear
    ; Playfield, border and progress rail are guest-drawn rectangles.
    mov #16,r1
    mov #48,r2
    load 288,r3
    load 144,r4
    load 0x98c6,r5
    bsr rect
    nop
    mov #20,r1
    mov #52,r2
    load 280,r3
    load 136,r4
    load 0x8421,r5
    bsr rect
    nop
    mov #32,r1
    mov #24,r2
    load 256,r3
    mov #12,r4
    load 0x98c6,r5
    bsr rect
    nop
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
    ; Read Start/A/C/B using TH=1 TR=0, active-low.
    load 0x20100075,r1
    mov #64,r0
    mov.b r0,@r1
    ; At least 2 microseconds settling on a 28 MHz SH-2.
    mov #32,r2
start_settle:
    dt r2
    bf start_settle
    mov.b @r1,r0
    tst #8,r0
    bf released
    mov #0,r0
    cmp/eq r0,r13
    bf.l new_game
    bra after_start
    nop
released:
    mov #8,r13
    ; The completed game still polls Start but freezes movement.
after_start:
    mov #0,r0
    cmp/eq r0,r12
    bf.l diagnostics
    ; Erase old player before changing coordinates.
    mov r8,r1
    mov r9,r2
    mov #8,r3
    mov #8,r4
    load 0x8421,r5
    bsr rect
    nop
    load 0x20100075,r1
    mov #32,r0
    mov.b r0,@r1
    mov #32,r2
pad_settle:
    dt r2
    bf pad_settle
    mov.b @r1,r6
    mov r6,r0
    tst #8,r0
    bt go_right
    bra left_test
    nop
go_right:
    load 288,r0
    cmp/hs r0,r8
    bt left_test
    add #4,r8
left_test:
    mov r6,r0
    tst #4,r0
    bf down_test
    mov #24,r0
    cmp/hs r8,r0
    bt down_test
    add #-4,r8
down_test:
    mov r6,r0
    tst #2,r0
    bf up_test
    load 176,r0
    cmp/hs r0,r9
    bt up_test
    add #4,r9
up_test:
    mov r6,r0
    tst #1,r0
    bf collision
    mov #56,r0
    cmp/hs r9,r0
    bt collision
    add #-4,r9
collision:
    cmp/eq r8,r10
    bf paint
    mov #112,r0
    cmp/eq r0,r9
    bf paint
    add #1,r11
    add #48,r10
    ; One new gold block per collected star.
    mov r11,r1
    shll2 r1
    mov r1,r0
    shll2 r1
    add r0,r1
    add #24,r1
    mov #26,r2
    mov #16,r3
    mov #8,r4
    load 0x83ff,r5
    bsr rect
    nop
    mov #5,r0
    cmp/eq r0,r11
    bf paint
    mov #1,r12
    ; Green completion banner; host neither draws nor decides success.
    mov #96,r1
    load 144,r2
    load 128,r3
    mov #24,r4
    load 0x83e0,r5
    bsr rect
    nop
paint:
    mov #0,r0
    cmp/eq r0,r12
    bf player
    mov r10,r1
    mov #112,r2
    mov #8,r3
    mov #8,r4
    load 0x83ff,r5
    bsr rect
    nop
player:
    mov r8,r1
    mov r9,r2
    mov #8,r3
    mov #8,r4
    load 0xffe0,r5
    bsr rect
    nop
diagnostics:
    add #1,r14
    load 0x06010000,r1
    mov.l r8,@r1
    add #4,r1
    mov.l r9,@r1
    add #4,r1
    mov.l r11,@r1
    add #4,r1
    mov.l r12,@r1
    add #4,r1
    mov.l r14,@r1
    bra frame
    nop

; RGB555 rectangle, x=r1 y=r2 width=r3 height=r4 color=r5.
; Clobbers r0-r6 only. All coordinates remain in the 512x256 bitmap.
rect:
    mov r2,r0
    shll8 r0
    shll2 r0
    shll r1
    add r1,r0
    load 0x05e00000,r6
    add r6,r0
    mov r3,r1
    shll r1
    load 1024,r6
    sub r1,r6
rect_row:
    mov r3,r2
rect_pixel:
    mov.w r5,@r0
    add #2,r0
    dt r2
    bf rect_pixel
    add r6,r0
    dt r4
    bf rect_row
    rts
    nop
