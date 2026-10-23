; Original MIT one-shot rotation diagnostic.
load 0x2600dd00,r8
load 0x25e00000,r1
load 0x3e0,r6
load 32768,r0
mov #16,r7
row_0:
mov #32,r2
fill_0:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_0
add r6,r0
dt r7
bf row_0
load 32800,r0
mov #16,r7
row_1:
mov #32,r2
fill_1:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_1
add r6,r0
dt r7
bf row_1
load 32832,r0
mov #16,r7
row_2:
mov #32,r2
fill_2:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_2
add r6,r0
dt r7
bf row_2
load 32864,r0
mov #16,r7
row_3:
mov #32,r2
fill_3:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_3
add r6,r0
dt r7
bf row_3
load 32896,r0
mov #16,r7
row_4:
mov #32,r2
fill_4:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_4
add r6,r0
dt r7
bf row_4
load 32928,r0
mov #16,r7
row_5:
mov #32,r2
fill_5:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_5
add r6,r0
dt r7
bf row_5
load 32960,r0
mov #16,r7
row_6:
mov #32,r2
fill_6:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_6
add r6,r0
dt r7
bf row_6
load 32992,r0
mov #16,r7
row_7:
mov #32,r2
fill_7:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_7
add r6,r0
dt r7
bf row_7
load 33024,r0
mov #16,r7
row_8:
mov #32,r2
fill_8:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_8
add r6,r0
dt r7
bf row_8
load 33056,r0
mov #16,r7
row_9:
mov #32,r2
fill_9:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_9
add r6,r0
dt r7
bf row_9
load 33088,r0
mov #16,r7
row_10:
mov #32,r2
fill_10:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_10
add r6,r0
dt r7
bf row_10
load 33120,r0
mov #16,r7
row_11:
mov #32,r2
fill_11:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_11
add r6,r0
dt r7
bf row_11
load 33152,r0
mov #16,r7
row_12:
mov #32,r2
fill_12:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_12
add r6,r0
dt r7
bf row_12
load 33184,r0
mov #16,r7
row_13:
mov #32,r2
fill_13:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_13
add r6,r0
dt r7
bf row_13
load 33216,r0
mov #16,r7
row_14:
mov #32,r2
fill_14:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_14
add r6,r0
dt r7
bf row_14
load 33248,r0
mov #16,r7
row_15:
mov #32,r2
fill_15:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_15
add r6,r0
dt r7
bf row_15
load 33280,r0
mov #16,r7
row_16:
mov #32,r2
fill_16:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_16
add r6,r0
dt r7
bf row_16
load 33312,r0
mov #16,r7
row_17:
mov #32,r2
fill_17:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_17
add r6,r0
dt r7
bf row_17
load 33344,r0
mov #16,r7
row_18:
mov #32,r2
fill_18:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_18
add r6,r0
dt r7
bf row_18
load 33376,r0
mov #16,r7
row_19:
mov #32,r2
fill_19:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_19
add r6,r0
dt r7
bf row_19
load 33408,r0
mov #16,r7
row_20:
mov #32,r2
fill_20:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_20
add r6,r0
dt r7
bf row_20
load 33440,r0
mov #16,r7
row_21:
mov #32,r2
fill_21:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_21
add r6,r0
dt r7
bf row_21
load 33472,r0
mov #16,r7
row_22:
mov #32,r2
fill_22:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_22
add r6,r0
dt r7
bf row_22
load 33504,r0
mov #16,r7
row_23:
mov #32,r2
fill_23:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_23
add r6,r0
dt r7
bf row_23
load 33536,r0
mov #16,r7
row_24:
mov #32,r2
fill_24:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_24
add r6,r0
dt r7
bf row_24
load 33568,r0
mov #16,r7
row_25:
mov #32,r2
fill_25:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_25
add r6,r0
dt r7
bf row_25
load 33600,r0
mov #16,r7
row_26:
mov #32,r2
fill_26:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_26
add r6,r0
dt r7
bf row_26
load 33632,r0
mov #16,r7
row_27:
mov #32,r2
fill_27:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_27
add r6,r0
dt r7
bf row_27
load 33664,r0
mov #16,r7
row_28:
mov #32,r2
fill_28:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_28
add r6,r0
dt r7
bf row_28
load 33696,r0
mov #16,r7
row_29:
mov #32,r2
fill_29:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_29
add r6,r0
dt r7
bf row_29
load 33728,r0
mov #16,r7
row_30:
mov #32,r2
fill_30:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_30
add r6,r0
dt r7
bf row_30
load 33760,r0
mov #16,r7
row_31:
mov #32,r2
fill_31:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_31
add r6,r0
dt r7
bf row_31
load 32768,r0
mov #16,r7
row_32:
mov #32,r2
fill_32:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_32
add r6,r0
dt r7
bf row_32
load 32800,r0
mov #16,r7
row_33:
mov #32,r2
fill_33:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_33
add r6,r0
dt r7
bf row_33
load 32832,r0
mov #16,r7
row_34:
mov #32,r2
fill_34:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_34
add r6,r0
dt r7
bf row_34
load 32864,r0
mov #16,r7
row_35:
mov #32,r2
fill_35:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_35
add r6,r0
dt r7
bf row_35
load 32896,r0
mov #16,r7
row_36:
mov #32,r2
fill_36:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_36
add r6,r0
dt r7
bf row_36
load 32928,r0
mov #16,r7
row_37:
mov #32,r2
fill_37:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_37
add r6,r0
dt r7
bf row_37
load 32960,r0
mov #16,r7
row_38:
mov #32,r2
fill_38:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_38
add r6,r0
dt r7
bf row_38
load 32992,r0
mov #16,r7
row_39:
mov #32,r2
fill_39:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_39
add r6,r0
dt r7
bf row_39
load 33024,r0
mov #16,r7
row_40:
mov #32,r2
fill_40:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_40
add r6,r0
dt r7
bf row_40
load 33056,r0
mov #16,r7
row_41:
mov #32,r2
fill_41:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_41
add r6,r0
dt r7
bf row_41
load 33088,r0
mov #16,r7
row_42:
mov #32,r2
fill_42:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_42
add r6,r0
dt r7
bf row_42
load 33120,r0
mov #16,r7
row_43:
mov #32,r2
fill_43:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_43
add r6,r0
dt r7
bf row_43
load 33152,r0
mov #16,r7
row_44:
mov #32,r2
fill_44:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_44
add r6,r0
dt r7
bf row_44
load 33184,r0
mov #16,r7
row_45:
mov #32,r2
fill_45:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_45
add r6,r0
dt r7
bf row_45
load 33216,r0
mov #16,r7
row_46:
mov #32,r2
fill_46:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_46
add r6,r0
dt r7
bf row_46
load 33248,r0
mov #16,r7
row_47:
mov #32,r2
fill_47:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_47
add r6,r0
dt r7
bf row_47
load 33280,r0
mov #16,r7
row_48:
mov #32,r2
fill_48:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_48
add r6,r0
dt r7
bf row_48
load 33312,r0
mov #16,r7
row_49:
mov #32,r2
fill_49:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_49
add r6,r0
dt r7
bf row_49
load 33344,r0
mov #16,r7
row_50:
mov #32,r2
fill_50:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_50
add r6,r0
dt r7
bf row_50
load 33376,r0
mov #16,r7
row_51:
mov #32,r2
fill_51:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_51
add r6,r0
dt r7
bf row_51
load 33408,r0
mov #16,r7
row_52:
mov #32,r2
fill_52:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_52
add r6,r0
dt r7
bf row_52
load 33440,r0
mov #16,r7
row_53:
mov #32,r2
fill_53:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_53
add r6,r0
dt r7
bf row_53
load 33472,r0
mov #16,r7
row_54:
mov #32,r2
fill_54:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_54
add r6,r0
dt r7
bf row_54
load 33504,r0
mov #16,r7
row_55:
mov #32,r2
fill_55:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_55
add r6,r0
dt r7
bf row_55
load 33536,r0
mov #16,r7
row_56:
mov #32,r2
fill_56:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_56
add r6,r0
dt r7
bf row_56
load 33568,r0
mov #16,r7
row_57:
mov #32,r2
fill_57:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_57
add r6,r0
dt r7
bf row_57
load 33600,r0
mov #16,r7
row_58:
mov #32,r2
fill_58:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_58
add r6,r0
dt r7
bf row_58
load 33632,r0
mov #16,r7
row_59:
mov #32,r2
fill_59:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_59
add r6,r0
dt r7
bf row_59
load 33664,r0
mov #16,r7
row_60:
mov #32,r2
fill_60:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_60
add r6,r0
dt r7
bf row_60
load 33696,r0
mov #16,r7
row_61:
mov #32,r2
fill_61:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_61
add r6,r0
dt r7
bf row_61
load 33728,r0
mov #16,r7
row_62:
mov #32,r2
fill_62:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_62
add r6,r0
dt r7
bf row_62
load 33760,r0
mov #16,r7
row_63:
mov #32,r2
fill_63:
mov.w r0,@r1
add #2,r1
add #1,r0
dt r2
bf fill_63
add r6,r0
dt r7
bf row_63
load 0x25e40000,r1
load 128,r2
coefficients:
mov #0,r0
mov.w r0,@r1
add #2,r1
mov #4,r0
mov.w r0,@r1
add #2,r1
mov #8,r0
mov.w r0,@r1
add #2,r1
mov #12,r0
mov.w r0,@r1
add #2,r1
dt r2
bf coefficients
load 0x25e60000,r1
load 0xa0000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0x0,r0
mov.l r0,@r1
load 0x25e60014,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e6001c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e6002c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e6004c,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e60050,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x20000,r0
mov.l r0,@r1
load 0x25e60058,r1
load 0x10000,r0
mov.l r0,@r1
load 0x100,r1
mov.l @r1,r0
load 635830284,r1
mov.l r0,@r1
load 0x25f8000e,r1
load 0x303,r0
mov.w r0,@r1
load 0x25f80020,r1
load 0x10,r0
mov.w r0,@r1
load 0x25f8002a,r1
load 0x3200,r0
mov.w r0,@r1
load 0x25f800fc,r1
load 0x4,r0
mov.w r0,@r1
load 0x25f800bc,r1
load 0x3,r0
mov.w r0,@r1
load 0x25f800be,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f800b0,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f800b4,r1
load 0xf,r0
mov.w r0,@r1
load 0x25f800b6,r1
load 0x2,r0
mov.w r0,@r1
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25f80004,r1
setup_blank:
mov.w @r1,r0
tst #8,r0
bt setup_blank
load 0x25f80004,r1
start_field:
mov.w @r1,r0
tst #8,r0
bf start_field
load 64,r3
line_64:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_64
load 0x25e60000,r1
load 0x12c0000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0x40000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x140000,r0
mov.l r0,@r1
load 127,r3
line_127:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_127
load 0x25e60000,r1
load 0x640000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0x40000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x80000,r0
mov.l r0,@r1
load 0x25f800b2,r1
load 0x7,r0
mov.w r0,@r1
load 128,r3
line_128:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_128
load 0x25e60000,r1
load 0xc80000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0x80000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0xc0000,r0
mov.l r0,@r1
load 159,r3
line_159:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_159
load 0x25e6000c,r1
load 0x30000,r0
mov.l r0,@r1
load 0x25e60058,r1
load 0x20000,r0
mov.l r0,@r1
load 191,r3
line_191:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_191
load 0x25e60010,r1
load 0x10000,r0
mov.l r0,@r1
load 0x25e60000,r1
load 0x140000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0x80000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x100000,r0
mov.l r0,@r1
load 0x25f800b2,r1
load 0x7,r0
mov.w r0,@r1
load 192,r3
line_192:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_192
load 0x25e60000,r1
load 0x1e0000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0xa0000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x140000,r0
mov.l r0,@r1
load 0x25f800b2,r1
load 0x7,r0
mov.w r0,@r1
load 193,r3
line_193:
load 0x25f80002,r1
mov.w @r1,r0
load 0x25f8000a,r1
mov.w @r1,r0
cmp/hs r3,r0
bf line_193
load 0x25e60000,r1
load 0x280000,r0
mov.l r0,@r1
load 0x25e60004,r1
load 0xc0000,r0
mov.l r0,@r1
load 0x25e60054,r1
load 0x180000,r0
mov.l r0,@r1
load 0x25f800b2,r1
load 0x7,r0
mov.w r0,@r1
load 0x25f80004,r1
complete:
mov.w @r1,r0
tst #8,r0
bt complete
load 0x25e60000,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25e60004,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25e60054,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25e6000c,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25e60010,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 0x25e60058,r1
mov.l @r1,r0
mov.l r0,@r8
add #4,r8
load 635830272,r1
mov.l @r1,r0
load 635830284,r1
mov.l @r1,r2
add r2,r0
mov.l r0,@r8
add #4,r8
load 635830276,r1
mov.l @r1,r0
load 635830288,r1
mov.l @r1,r2
add r2,r0
mov.l r0,@r8
add #4,r8
load 635830356,r1
mov.l @r1,r0
load 635830360,r1
mov.l @r1,r2
add r2,r0
mov.l r0,@r8
add #4,r8
load 0x600d,r0
mov.l r0,@r8
add #4,r8
park:
sleep
bra park
nop
