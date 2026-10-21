; Original HLE CD player. MIT: adjacent LICENSE.
; ST-162-R1 System Program 6.1; CD Communication ST-162 §§3.4/8.2;
; VDP2 ST-058 bitmap mode; SCSP ST-077 table4.31; SMPC ST-169 table3.36.
; SYS_EXECDMP does not return. No caller stack or application state is needed.
.section 0x18000
load 0xf0,r0
ldc r0,sr
load 0x060ff000,r15
load 0x06000000,r0
ldc r0,vbr
load 0xfffffe92,r1
mov #16,r0
mov.b r0,@r1
load 0xfffffe80,r1
load 0xa500,r0
mov.w r0,@r1
load 0xffffffb0,r1
mov #0,r0
mov.l r0,@r1
; Finish/cancel any outstanding INTBACK before the clock service waits on SF.
mov #8,r4
load 0x19100,r1
jsr @r1
nop
; A clock change restores the video/SCU/sound domains using real SMPC control.
mov #0,r4
load 0x4d00,r1
jsr @r1
nop
; Abort a previous FIFO transfer and restore the drive/selector state.
load 0x0401,r4
mov #0,r5
mov #0,r6
mov #0,r7
load 0x19000,r1
jsr @r1
nop
; Read all 102 longwords of the real TOC into private player work RAM.
load 0x0200,r4
load 0x19000,r1
jsr @r1
nop
load 0x06000d00,r13
mov #0,r10
load 204,r0
cmp/eq r0,r5
bf player_no_toc
load 0x25818000,r1
mov r13,r2
mov #102,r3
player_read_toc:
mov.l @r1,r0
mov.l r0,@r2
add #4,r2
dt r3
bf player_read_toc
load 0x0600,r4
mov #0,r5
mov #0,r6
mov #0,r7
load 0x19000,r1
jsr @r1
nop
load 0x06000e91,r1
mov.b @r1,r10
extu.b r10,r10
player_no_toc:
mov #0,r8
mov #0,r9
mov #0,r11
mov #0,r12
load 0x06000f0c,r1
mov #-1,r0
mov.l r0,@r1
add #4,r1
mov.l r0,@r1
; Select the first audio track, bounded by the actual TOC's last-track byte.
load 0x19400,r1
jsr @r1
nop
; Direct PCM is silent after clock reset. Route only CD-DA EXTS0/1 to L/R.
load 0x25b00400,r1
load 0x020f,r0
mov.w r0,@r1
load 0x25b00216,r1
load 0x00ff,r0
mov.w r0,@r1
load 0x25b00236,r1
load 0x00ef,r0
mov.w r0,@r1
; INTBACK owns the controller ports; both digital and analog report formats
; carry the same first button byte. External latches are disabled for the menu.
load 0x2010007d,r1
mov #0,r0
mov.b r0,@r1
add #2,r1
mov.b r0,@r1
; 512x256 RGB555 bitmap; display stays blank until guest drawing completes.
load 0x25f80020,r1
load 0x0101,r0
mov.w r0,@r1
load 0x25f80028,r1
mov #50,r0
mov.w r0,@r1
load 0x25f80078,r1
load 0x10000,r0
mov.l r0,@r1
add #4,r1
mov.l r0,@r1
load 0x25f800f8,r1
mov #1,r0
mov.w r0,@r1
load 0x25f80010,r1
load 0x4444,r0
mov.w r0,@r1
add #2,r1
load 0xeeee,r0
mov.w r0,@r1
load 0x25e00000,r1
load 131072,r2
load 0x8841,r0
player_clear:
mov.w r0,@r1
add #2,r1
dt r2
bf player_clear
load 0x19300,r1
jsr @r1
nop
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x18800,r1
jmp @r1
nop

.section 0x18800
player_frame:
load 0x25f80004,r1
player_active:
mov.w @r1,r0
tst #8,r0
bf player_active
player_blank:
mov.w @r1,r0
tst #8,r0
bt player_blank
load 0x19a00,r1
jsr @r1
nop
load 255,r14
xor r12,r14
and r13,r14
mov r13,r12
; Left/right step through audio tracks only. A held key has one edge.
mov r14,r0
tst #192,r0
bt player_play_keys
load 0x11ff,r4
load 0xffff,r5
mov #0,r6
mov #0,r7
load 0x19000,r1
jsr @r1
nop
mov #0,r9
mov #0,r11
mov r14,r0
tst #64,r0
bt player_next
load 0x19500,r1
jsr @r1
nop
bra player_play_keys
nop
player_next:
load 0x19400,r1
jsr @r1
nop
player_play_keys:
mov r14,r0
tst #1,r0
bf player_pause
tst #4,r0
bf player_play
tst #8,r0
bt player_status
tst r11,r11
bt player_play
player_pause:
load 0x11ff,r4
load 0xffff,r5
mov #0,r6
mov #0,r7
load 0x19000,r1
jsr @r1
nop
mov #0,r11
bra player_status
nop
player_play:
tst r8,r8
bt player_status
load 0x1000,r4
mov r8,r5
shll8 r5
add #1,r5
mov #0,r6
mov #0,r7
cmp/eq r9,r8
bf player_play_range
load 0x10ff,r4
load 0xffff,r5
load 0xffff,r6
load 0xffff,r7
bra player_issue_play
nop
player_play_range:
cmp/eq r10,r8
bt player_issue_play
mov r8,r7
add #1,r7
shll8 r7
add #1,r7
player_issue_play:
load 0x19000,r1
jsr @r1
nop
mov #1,r11
mov r8,r9
player_status:
mov #0,r4
mov #0,r5
mov #0,r6
mov #0,r7
load 0x19000,r1
jsr @r1
nop
load 0xf00,r0
and r4,r0
load 0x300,r1
cmp/eq r1,r0
bt player_paint
load 0x400,r1
cmp/eq r1,r0
bt player_paint
; Natural completion must start the selected track again; an explicit pause
; instead retains the current FAD for the next play request.
tst r11,r11
bt player_not_completed
mov #0,r9
player_not_completed:
mov #0,r11
load 0x100,r1
cmp/eq r1,r0
bt player_paint
mov #0,r9
player_paint:
; This is live player state used below for display, not an acceptance marker.
load 0x06000f00,r1
mov.l r8,@r1
add #4,r1
mov.l r11,@r1
add #4,r1
mov.l r10,@r1
; The rail's position is computed from the selected TOC track number.
; Update only the old/new cursor during VBlank, not the whole rail per field.
load 0x06000f0c,r1
mov.l @r1,r13
cmp/eq r13,r8
bf player_cursor_changed
add #4,r1
mov.l @r1,r0
cmp/eq r11,r0
bt.l player_frame
player_cursor_changed:
load 0x06000f0c,r1
mov.l r8,@r1
add #4,r1
mov.l r11,@r1
mov #100,r0
cmp/hs r0,r13
bt player_draw_cursor
mov r13,r1
shll r1
add #32,r1
mov #100,r2
mov #6,r3
mov #24,r4
load 0x9084,r5
load 0x19200,r6
jsr @r6
nop
player_draw_cursor:
mov r8,r1
shll r1
add #32,r1
mov #100,r2
mov #6,r3
mov #24,r4
load 0x83ff,r5
tst r8,r8
bf player_has_audio
load 0x801f,r5
player_has_audio:
tst r11,r11
bt player_indicator
load 0x83e0,r5
player_indicator:
load 0x19200,r6
jsr @r6
nop
bra player_frame
nop

.section 0x19000
; CD command R4-R7 -> four read-protected response words R4-R7.
load 0x25890008,r1
player_command_idle:
mov.w @r1,r0
tst #1,r0
bt player_command_idle
load 0x25890024,r2
mov.w @r2,r0
load 0xfffe,r0
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
player_command_wait:
mov.w @r1,r0
tst #1,r0
bt player_command_wait
load 0x25890018,r1
mov.w @r1,r4
extu.w r4,r4
add #4,r1
mov.w @r1,r5
extu.w r5,r5
add #4,r1
mov.w @r1,r6
extu.w r6,r6
add #4,r1
mov.w @r1,r7
extu.w r7,r7
rts
nop

.section 0x19100
; Timed SMPC command R4, including a previous application's busy command.
load 0x20100063,r1
player_smpc_idle:
mov.b @r1,r0
tst #1,r0
bt player_smpc_issue
load 0x20100061,r2
mov.b @r2,r0
tst #32,r0
bt player_smpc_idle
load 0x20100001,r2
mov #64,r0
mov.b r0,@r2
bra player_smpc_idle
nop
player_smpc_issue:
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r2
mov.b r4,@r2
player_smpc_done:
mov.b @r1,r0
tst #1,r0
bf player_smpc_done
rts
nop

.section 0x19200
; Original bitmap rectangle. x/y/width/height/color in R1-R5, R0-R6 scratch.
mov r2,r0
shll8 r0
shll2 r0
shll r1
add r1,r0
load 0x25e00000,r6
add r6,r0
mov r3,r1
shll r1
load 1024,r6
sub r1,r6
player_rect_row:
mov r3,r2
player_rect_pixel:
mov.w r5,@r0
add #2,r0
dt r2
bf player_rect_pixel
add r6,r0
dt r4
bf player_rect_row
rts
nop

.section 0x19300
; Text uses original bitmap assets and a real guest glyph rasterizer.
sts pr,r0
mov.l r0,@-r15
mov #24,r1
mov #96,r2
load 256,r3
mov #32,r4
load 0x9084,r5
load 0x19200,r6
jsr @r6
nop
load 0x1d000,r3
mov #32,r1
mov #32,r2
load 0xffff,r4
load 0x19600,r5
jsr @r5
nop
load 0x1d020,r3
mov #32,r1
load 168,r2
load 0x83ff,r4
load 0x19600,r5
jsr @r5
nop
load 0x1d040,r3
mov #32,r1
load 192,r2
load 0xffff,r4
load 0x19600,r5
jsr @r5
nop
mov.l @r15+,r0
lds r0,pr
rts
nop

.section 0x19400
; Next audio track, wraps once and skips data using actual TOC CONTROL.
mov r10,r3
tst r3,r3
bt player_no_audio_next
player_find_next:
add #1,r8
cmp/hs r8,r10
bt player_next_in_range
mov #1,r8
player_next_in_range:
mov r8,r1
add #-1,r1
shll2 r1
load 0x06000d00,r2
add r2,r1
mov.l @r1,r0
load 0x40000000,r1
tst r1,r0
bt player_next_found
dt r3
bf player_find_next
player_no_audio_next:
mov #0,r8
player_next_found:
rts
nop

.section 0x19500
mov r10,r3
tst r3,r3
bt player_no_audio_previous
player_find_previous:
mov #1,r0
cmp/hs r8,r0
bf player_previous_decrement
mov r10,r8
bra player_previous_in_range
nop
player_previous_decrement:
add #-1,r8
player_previous_in_range:
mov r8,r1
add #-1,r1
shll2 r1
load 0x06000d00,r2
add r2,r1
mov.l @r1,r0
load 0x40000000,r1
tst r1,r0
bt player_previous_found
dt r3
bf player_find_previous
player_no_audio_previous:
mov #0,r8
player_previous_found:
rts
nop

.section 0x19600
; Original 5x7 font, two pixels per dot. R1=x R2=y R3=text R4=RGB555.
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
mov r1,r8
player_text_char:
mov.b @r3+,r0
extu.b r0,r0
tst r0,r0
bt player_text_done
mov #7,r7
mul.l r7,r0
sts macl,r9
load 0x1d100,r5
add r5,r9
mov r2,r14
shll8 r14
shll2 r14
mov r8,r5
shll r5
add r5,r14
load 0x25e00000,r5
add r5,r14
mov #7,r10
player_text_row:
mov.b @r9+,r11
mov #16,r12
mov #5,r13
player_text_column:
tst r12,r11
bt player_text_empty
mov.w r4,@r14
add #2,r14
mov.w r4,@r14
load 1022,r5
add r5,r14
mov.w r4,@r14
add #2,r14
mov.w r4,@r14
load 1026,r5
sub r5,r14
player_text_empty:
add #4,r14
shlr r12
dt r13
bf player_text_column
load 2028,r5
add r5,r14
dt r10
bf player_text_row
add #12,r8
bra player_text_char
nop
player_text_done:
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x19a00
; Standard INTBACK report, including joystick/3D-pad buttons and X navigation.
; R13 returns active-high direction/A/B/C/Start bits. All reads are real OREGs.
sts pr,r0
mov.l r0,@-r15
load 0x20100001,r1
mov #0,r0
mov.b r0,@r1
add #2,r1
mov #10,r0
mov.b r0,@r1
add #2,r1
load 0xf0,r0
mov.b r0,@r1
mov #16,r4
load 0x19100,r1
jsr @r1
nop
load 0x20100025,r1
mov.b @r1,r13
extu.b r13,r13
load 255,r0
xor r0,r13
load 0x20100023,r1
mov.b @r1,r0
mov #21,r2
cmp/eq r2,r0
bt player_analog_axis
mov #22,r2
cmp/eq r2,r0
bf player_input_done
player_analog_axis:
load 0x20100029,r1
mov.b @r1,r0
extu.b r0,r0
load 192,r2
cmp/hs r2,r0
bf player_axis_left
load 128,r2
or r2,r13
bra player_input_done
nop
player_axis_left:
mov #64,r2
cmp/hs r0,r2
bf player_input_done
or r2,r13
player_input_done:
mov.l @r15+,r0
lds r0,pr
rts
nop
