load 0xffffffe0,r1
mov.l @r1,r0
load 0x8000,r2
tst r2,r0
bt master
load 0x7800,r1
jmp @r1
nop
master:
load 0x06000000,r0
ldc r0,vbr
load 0x26010400,r9
load 0xfffffe92,r1
load 0x0,r0
mov.b r0,@r1
load 0xfffffee0,r1
load 0x1,r0
mov.w r0,@r1
load 0x6000010,r1
load 0x7600,r0
mov.l r0,@r1
load 0x6000018,r1
load 0x7600,r0
mov.l r0,@r1
load 0x6000024,r1
load 0x7600,r0
mov.l r0,@r1
load 0x600011c,r1
load 0x7000,r0
mov.l r0,@r1
load 0x600012c,r1
load 0x7200,r0
mov.l r0,@r1
load 0x6000140,r1
load 0x7400,r0
mov.l r0,@r1
load 0x25fe00a0,r1
load 0xb77f,r0
mov.l r0,@r1
mov #0,r0
ldc r0,sr
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #2,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc2:
mov.b @r1,r0
tst #1,r0
bf wait_smpc2
load 0x2010005f,r1
mov.b @r1,r0
extu.b r0,r0
load 0x26010248,r1
mov.l r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #7,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc7:
mov.b @r1,r0
tst #1,r0
bf wait_smpc7
load 0x2010005f,r1
mov.b @r1,r0
extu.b r0,r0
load 0x2601025c,r1
mov.l r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #9,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc9:
mov.b @r1,r0
tst #1,r0
bf wait_smpc9
load 0x2010005f,r1
mov.b @r1,r0
extu.b r0,r0
load 0x26010264,r1
mov.l r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #8,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc8:
mov.b @r1,r0
tst #1,r0
bf wait_smpc8
load 0x2010005f,r1
mov.b @r1,r0
extu.b r0,r0
load 0x26010260,r1
mov.l r0,@r1
load 0x2601020c,r1
slave_started:
mov.l @r1,r0
tst r0,r0
bt slave_started
load 0x20100001,r1
load 0x1,r0
mov.b r0,@r1
load 0x20100003,r1
load 0x0,r0
mov.b r0,@r1
load 0x20100005,r1
load 0xf0,r0
mov.b r0,@r1
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r1
mov #16,r0
mov.b r0,@r1
load 0x20100063,r1
wait_smpc16:
mov.b @r1,r0
tst #1,r0
bf wait_smpc16
load 0x26010200,r1
status_irq:
mov.l @r1,r0
tst r0,r0
bt status_irq
load 0x20100035,r1
mov.b @r1,r0
extu.b r0,r0
load 0x26010218,r1
mov.l r0,@r1
load 0x25f80000,r1
load 0x8000,r0
mov.w r0,@r1
load 0x25f800ec,r1
load 0x0,r0
mov.w r0,@r1
load 0x25f800ee,r1
load 0x0,r0
mov.w r0,@r1
load 0x25e00000,r1
load 0x801f,r0
mov.w r0,@r1
load 0x401,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 0xfffe,r0
mov.w r0,@r1
load 0x2589000c,r1
load 0x1,r0
mov.w r0,@r1
load 0x25fe00a8,r1
load 0x1,r0
mov.l r0,@r1
load 0x25fe00a0,r1
load 0x377f,r0
mov.l r0,@r1
load 0xf0,r0
ldc r0,sr
load 0x0,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
mov #0,r0
ldc r0,sr
load 0x26010208,r1
cd_irq:
mov.l @r1,r0
tst r0,r0
bt cd_irq
load 0x2589000c,r1
load 0x0,r0
mov.w r0,@r1
load 0x25fe00a0,r1
load 0xb77f,r0
mov.l r0,@r1
load 0x200,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890018,r1
mov.w @r1,r0
extu.w r0,r0
load 0xff00,r2
cmp/eq r2,r0
bf medium_present
load no_media,r0
jmp @r0
nop
medium_present:
load 0x25818000,r1
mov.l @r1,r0
load 0x26010214,r1
mov.l r0,@r1
load 200,r3
load 0x25818000,r1
read_toc:
mov.w @r1,r0
dt r3
bf read_toc
load 0x25818000,r1
mov.l @r1,r0
load 0x260102f8,r1
mov.l r0,@r1
load 0x600,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 0xffbf,r0
mov.w r0,@r1
load 0x3000,r4
load 0x0,r5
load 0x0,r6
load 0x0,r7
load command,r0
jsr @r0
nop
load 0x2601032c,r1
load 0x1,r0
mov.l r0,@r1
load 0,r10
load 0x6002000,r11
load read_sector,r0
jsr @r0
nop
load 0x6002000,r1
load be32,r0
jsr @r0
nop
load 0x53454741,r2
cmp/eq r2,r0
bt guard_1
load metadata_error,r0
jmp @r0
nop
guard_1:
load 0x6002004,r1
load be32,r0
jsr @r0
nop
load 0x20534547,r2
cmp/eq r2,r0
bt guard_2
load metadata_error,r0
jmp @r0
nop
guard_2:
load 0x6002008,r1
load be32,r0
jsr @r0
nop
load 0x41534154,r2
cmp/eq r2,r0
bt guard_3
load metadata_error,r0
jmp @r0
nop
guard_3:
load 0x600200c,r1
load be32,r0
jsr @r0
nop
load 0x55524e20,r2
cmp/eq r2,r0
bt guard_4
load metadata_error,r0
jmp @r0
nop
guard_4:
load 0x06002040,r8
mov #0,r12
mov #0,r13
mov #0,r14
mov #16,r7
area_loop:
mov.b @r8+,r0
extu.b r0,r0
load 32,r2
cmp/eq r2,r0
bt area_space
tst r14,r14
bt guard_5
load metadata_error,r0
jmp @r0
nop
guard_5:
load 74,r2
cmp/eq r2,r0
bf area_not_J
mov #1,r6
bra area_symbol
nop
area_not_J:
load 84,r2
cmp/eq r2,r0
bf area_not_T
mov #2,r6
bra area_symbol
nop
area_not_T:
load 85,r2
cmp/eq r2,r0
bf area_not_U
mov #4,r6
bra area_symbol
nop
area_not_U:
load 69,r2
cmp/eq r2,r0
bf area_not_E
mov #8,r6
bra area_symbol
nop
area_not_E:
load metadata_error,r0
jmp @r0
nop
area_symbol:
tst r6,r13
bt guard_6
load metadata_error,r0
jmp @r0
nop
guard_6:
or r6,r13
add #1,r12
load 5,r2
cmp/hs r2,r12
bf guard_7
load metadata_error,r0
jmp @r0
nop
guard_7:
bra area_next
nop
area_space:
mov #1,r14
area_next:
dt r7
bf area_loop
tst r12,r12
bf guard_8
load metadata_error,r0
jmp @r0
nop
guard_8:
load 0x60020d0,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_9
load metadata_error,r0
jmp @r0
nop
guard_9:
load 0x60020d4,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_10
load metadata_error,r0
jmp @r0
nop
guard_10:
load 0x60020d8,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_11
load metadata_error,r0
jmp @r0
nop
guard_11:
load 0x60020dc,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_12
load metadata_error,r0
jmp @r0
nop
guard_12:
load 0x60020e4,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_13
load metadata_error,r0
jmp @r0
nop
guard_13:
load 0x60020f8,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_14
load metadata_error,r0
jmp @r0
nop
guard_14:
load 0x60020fc,r1
load be32,r0
jsr @r0
nop
load 0x0,r2
cmp/eq r2,r0
bt guard_15
load metadata_error,r0
jmp @r0
nop
guard_15:
shll2 r12
shll2 r12
shll r12
load 0x06002e00,r0
add r12,r0
load 0x26010304,r1
mov.l r0,@r1
load 0x60020e0,r1
load be32,r0
jsr @r0
nop
load 0x26010300,r1
mov.l r0,@r1
load 0x1000,r2
cmp/hs r2,r0
bt guard_16
load metadata_error,r0
jmp @r0
nop
guard_16:
load 0x8001,r2
cmp/hs r2,r0
bf guard_17
load metadata_error,r0
jmp @r0
nop
guard_17:
tst #3,r0
bt guard_18
load metadata_error,r0
jmp @r0
nop
guard_18:
load 0x60020e8,r1
load be32,r0
jsr @r0
nop
tst r0,r0
bf stack_specified
load 0x6002000,r0
bra stack_ok
nop
stack_specified:
tst #3,r0
bt guard_19
load metadata_error,r0
jmp @r0
nop
guard_19:
load 0x060e0000,r2
cmp/hs r2,r0
bt guard_20
load metadata_error,r0
jmp @r0
nop
guard_20:
load 0x060fe001,r2
cmp/hs r2,r0
bf guard_21
load metadata_error,r0
jmp @r0
nop
guard_21:
stack_ok:
load 0x26010308,r1
mov.l r0,@r1
load 0x60020ec,r1
load be32,r0
jsr @r0
nop
tst r0,r0
bf slave_stack_specified
load 0x6001000,r0
bra slave_stack_ok
nop
slave_stack_specified:
tst #3,r0
bt guard_22
load metadata_error,r0
jmp @r0
nop
guard_22:
load 0x060e0000,r2
cmp/hs r2,r0
bt guard_23
load metadata_error,r0
jmp @r0
nop
guard_23:
load 0x060fe001,r2
cmp/hs r2,r0
bf guard_24
load metadata_error,r0
jmp @r0
nop
guard_24:
slave_stack_ok:
load 0x2601030c,r1
mov.l r0,@r1
load 0x60020f0,r1
load be32,r0
jsr @r0
nop
load 0x26010310,r1
mov.l r0,@r1
load 0x060d0000,r2
cmp/hs r2,r0
bf guard_25
load metadata_error,r0
jmp @r0
nop
guard_25:
tst r0,r0
bt destination_ok
tst #3,r0
bt guard_26
load metadata_error,r0
jmp @r0
nop
guard_26:
load 0x26010300,r1
mov.l @r1,r2
load 0x06002000,r3
add r3,r2
add #4,r2
cmp/hs r2,r0
bt guard_27
load metadata_error,r0
jmp @r0
nop
guard_27:
destination_ok:
load 0x2601032c,r1
load 0x2,r0
mov.l r0,@r1
load 0x26010300,r1
mov.l @r1,r12
load 2048,r0
sub r0,r12
load 1,r10
load 0x06002800,r11
ip_loop:
tst r12,r12
bt ip_done
load read_sector,r0
jsr @r0
nop
load 2048,r0
cmp/hs r0,r12
bf ip_done
sub r0,r12
add #1,r10
add r0,r11
bra ip_loop
nop
ip_done:
load 16,r10
mov #0,r14
descriptor_loop:
load 32,r2
cmp/hs r2,r10
bf guard_28
load metadata_error,r0
jmp @r0
nop
guard_28:
load 0x6017000,r11
load read_sector,r0
jsr @r0
nop
load 0x6017001,r1
load be32,r0
jsr @r0
nop
load 0x43443030,r2
cmp/eq r2,r0
bt guard_29
load metadata_error,r0
jmp @r0
nop
guard_29:
load 0x6017005,r1
mov.b @r1,r0
extu.b r0,r0
load 0x31,r2
cmp/eq r2,r0
bt guard_30
load metadata_error,r0
jmp @r0
nop
guard_30:
load 0x6017006,r1
mov.b @r1,r0
extu.b r0,r0
load 0x1,r2
cmp/eq r2,r0
bt guard_31
load metadata_error,r0
jmp @r0
nop
guard_31:
load 0x6017000,r1
mov.b @r1,r0
extu.b r0,r0
load 255,r2
cmp/eq r2,r0
bt.l descriptor_end
load 1,r2
cmp/eq r2,r0
bt descriptor_primary
tst r0,r0
bt.l descriptor_next
load 2,r2
cmp/eq r2,r0
bt guard_32
load metadata_error,r0
jmp @r0
nop
guard_32:
bra descriptor_next
nop
descriptor_primary:
tst r14,r14
bt guard_33
load metadata_error,r0
jmp @r0
nop
guard_33:
mov #1,r14
load 0x6017050,r1
load dual32,r0
jsr @r0
nop
load 0x26010314,r1
mov.l r0,@r1
mov r0,r6
load 0x260102f8,r1
mov.l @r1,r0
load 0xffffff,r2
and r2,r0
load 150,r2
sub r2,r0
cmp/hs r6,r0
bt guard_34
load metadata_error,r0
jmp @r0
nop
guard_34:
mov r6,r0
load 18,r2
cmp/hs r2,r0
bt guard_35
load metadata_error,r0
jmp @r0
nop
guard_35:
load 450001,r2
cmp/hs r2,r0
bf guard_36
load metadata_error,r0
jmp @r0
nop
guard_36:
load 0x6017078,r1
load be32,r0
jsr @r0
nop
load 0x1000001,r2
cmp/eq r2,r0
bt guard_37
load metadata_error,r0
jmp @r0
nop
guard_37:
load 0x601707c,r1
load be32,r0
jsr @r0
nop
load 0x1000001,r2
cmp/eq r2,r0
bt guard_38
load metadata_error,r0
jmp @r0
nop
guard_38:
load 0x6017080,r1
load be32,r0
jsr @r0
nop
load 0x80800,r2
cmp/eq r2,r0
bt guard_39
load metadata_error,r0
jmp @r0
nop
guard_39:
load 0x6017371,r1
mov.b @r1,r0
extu.b r0,r0
load 0x1,r2
cmp/eq r2,r0
bt guard_40
load metadata_error,r0
jmp @r0
nop
guard_40:
load 0x601709c,r8
load root_record,r0
jsr @r0
nop
mov r8,r1
mov.b @r1,r0
extu.b r0,r0
load 0x22,r2
cmp/eq r2,r0
bt guard_41
load metadata_error,r0
jmp @r0
nop
guard_41:
mov r8,r1
add #33,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_42
load metadata_error,r0
jmp @r0
nop
guard_42:
mov r8,r1
load 2,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x26010318,r1
mov.l r0,@r1
mov r8,r1
load 10,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x2601031c,r1
mov.l r0,@r1
tst r0,r0
bf guard_43
load metadata_error,r0
jmp @r0
nop
guard_43:
load 0x4001,r2
cmp/hs r2,r0
bf guard_44
load metadata_error,r0
jmp @r0
nop
guard_44:
load 2047,r2
tst r2,r0
bt guard_45
load metadata_error,r0
jmp @r0
nop
guard_45:
descriptor_next:
add #1,r10
bra descriptor_loop
nop
descriptor_end:
tst r14,r14
bf guard_46
load metadata_error,r0
jmp @r0
nop
guard_46:
add #1,r10
load 0x26010338,r1
mov.l r10,@r1
load 0x26010314,r1
mov.l @r1,r0
cmp/hs r10,r0
bt guard_47
load metadata_error,r0
jmp @r0
nop
guard_47:
load 0x26010318,r1
mov.l @r1,r0
cmp/hs r10,r0
bt guard_48
load metadata_error,r0
jmp @r0
nop
guard_48:
mov r0,r6
load 0x2601031c,r1
mov.l @r1,r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
add r6,r7
load 0x26010314,r1
mov.l @r1,r2
cmp/hs r7,r2
bt guard_49
load metadata_error,r0
jmp @r0
nop
guard_49:
load 0x2601032c,r1
load 0x3,r0
mov.l r0,@r1
load 0x26010318,r1
mov.l @r1,r10
load 0x2601031c,r1
mov.l @r1,r12
load 0x6018000,r11
root_load:
load read_sector,r0
jsr @r0
nop
load 2048,r0
add #1,r10
add r0,r11
sub r0,r12
tst r12,r12
bf root_load
load 0x6018000,r8
load 0x2601031c,r1
mov.l @r1,r14
add r8,r14
mov #0,r12
root_scan:
cmp/hs r14,r8
bt.l root_done
mov.b @r8,r0
extu.b r0,r13
tst r13,r13
bt.l root_padding
mov r13,r0
tst #1,r0
bt guard_50
load metadata_error,r0
jmp @r0
nop
guard_50:
load 34,r2
cmp/hs r2,r13
bt guard_51
load metadata_error,r0
jmp @r0
nop
guard_51:
mov r8,r0
add r13,r0
cmp/hs r0,r14
bt guard_52
load metadata_error,r0
jmp @r0
nop
guard_52:
mov r8,r0
load 2047,r2
and r2,r0
add r13,r0
load 2049,r2
cmp/hs r2,r0
bf guard_53
load metadata_error,r0
jmp @r0
nop
guard_53:
load record,r0
jsr @r0
nop
load 2,r2
cmp/hs r2,r12
bt.l root_child
load root_record,r0
jsr @r0
nop
mov r8,r1
load 2,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x26010318,r1
mov.l @r1,r2
cmp/eq r2,r0
bt guard_54
load metadata_error,r0
jmp @r0
nop
guard_54:
mov r8,r1
load 10,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x2601031c,r1
mov.l @r1,r2
cmp/eq r2,r0
bt guard_55
load metadata_error,r0
jmp @r0
nop
guard_55:
mov r8,r1
add #33,r1
mov.b @r1,r0
extu.b r0,r0
cmp/eq r12,r0
bt guard_56
load metadata_error,r0
jmp @r0
nop
guard_56:
bra root_next
nop
root_child:
mov r8,r1
add #32,r1
mov.b @r1,r0
extu.b r0,r0
load 1,r2
cmp/eq r2,r0
bf root_name_ok
mov r8,r1
add #33,r1
mov.b @r1,r0
extu.b r0,r0
load 2,r2
cmp/hs r2,r0
bt guard_57
load metadata_error,r0
jmp @r0
nop
guard_57:
root_name_ok:
load 2,r2
cmp/eq r2,r12
bf.l root_next
load 0x26010310,r1
mov.l @r1,r0
tst r0,r0
bt.l root_next
mov r8,r1
add #25,r1
mov.b @r1,r0
extu.b r0,r0
tst #2,r0
bt guard_58
load metadata_error,r0
jmp @r0
nop
guard_58:
mov r8,r1
load 2,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x26010320,r1
mov.l r0,@r1
load 0x26010318,r1
mov.l @r1,r2
load 0x2601031c,r1
mov.l @r1,r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
shlr r4
add r4,r2
cmp/hs r2,r0
bt guard_59
load metadata_error,r0
jmp @r0
nop
guard_59:
mov r8,r1
load 10,r0
add r0,r1
load dual32,r0
jsr @r0
nop
load 0x26010324,r1
mov.l r0,@r1
tst r0,r0
bf guard_60
load metadata_error,r0
jmp @r0
nop
guard_60:
load 65537,r2
cmp/hs r2,r0
bf guard_61
load metadata_error,r0
jmp @r0
nop
guard_61:
root_next:
add r13,r8
add #1,r12
bra root_scan
nop
root_padding:
mov r8,r6
load 2047,r2
or r2,r6
add #1,r6
root_padding_byte:
mov.b @r8+,r0
tst r0,r0
bt guard_62
load metadata_error,r0
jmp @r0
nop
guard_62:
cmp/hs r6,r8
bf root_padding_byte
bra root_scan
nop
root_done:
load 2,r2
cmp/hs r2,r12
bt guard_63
load metadata_error,r0
jmp @r0
nop
guard_63:
load 0x26010310,r1
mov.l @r1,r0
tst r0,r0
bt no_first_read
load 3,r2
cmp/hs r2,r12
bt guard_64
load metadata_error,r0
jmp @r0
nop
guard_64:
no_first_read:
load 0x26010310,r1
mov.l @r1,r13
tst r13,r13
bt.l handoff
load 0x26010324,r1
mov.l @r1,r12
mov r13,r0
add r12,r0
load 0x06010000,r2
cmp/hs r0,r2
bt load_bounds_ok
load 0x06020000,r2
cmp/hs r2,r13
bt guard_65
load metadata_error,r0
jmp @r0
nop
guard_65:
load 0x060d0000,r2
cmp/hs r0,r2
bt guard_66
load metadata_error,r0
jmp @r0
nop
guard_66:
load_bounds_ok:
load 0x2601032c,r1
load 0x4,r0
mov.l r0,@r1
load 0x26010320,r1
mov.l @r1,r10
file_load:
load 0x6017000,r11
load read_sector,r0
jsr @r0
nop
load 2048,r14
cmp/hs r14,r12
bt copy_count_ok
mov r12,r14
copy_count_ok:
mov r14,r3
mov r11,r1
copy_bytes:
mov.b @r1+,r0
mov.b r0,@r13
add #1,r13
dt r3
bf copy_bytes
sub r14,r12
load 0x26010328,r1
mov.l @r1,r0
add r14,r0
load 0x26010328,r1
mov.l r0,@r1
add #1,r10
tst r12,r12
bf file_load
handoff:
load 0x2601032c,r1
load 0x5,r0
mov.l r0,@r1
load 0x2601030c,r1
mov.l @r1,r0
load 0x26010334,r1
mov.l r0,@r1
slave_stack_wait:
load 0x26010334,r1
mov.l @r1,r0
tst r0,r0
bf slave_stack_wait
load 0x26010308,r1
mov.l @r1,r15
load 0x26010304,r1
mov.l @r1,r1
jmp @r1
nop
no_media:
load 0x260100fc,r1
load 0xe001,r0
mov.l r0,@r1
bra stopped
nop
stopped:
sleep
bra stopped
nop
read_sector:
sts.l pr,@-r15
load 0x26010314,r1
mov.l @r1,r0
tst r0,r0
bt sector_bounds_ok
cmp/hs r0,r10
bf guard_67
load io_error,r0
jmp @r0
nop
guard_67:
sector_bounds_ok:
load 0x25890008,r1
load 0xffeb,r0
mov.w r0,@r1
mov r10,r5
load 150,r0
add r0,r5
mov r5,r4
shlr16 r4
load 0x1080,r0
or r0,r4
extu.w r5,r5
load 0x80,r6
mov #1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
load 200000,r3
sector_wait:
mov.w @r1,r0
tst #16,r0
bf sector_ready
dt r3
bf sector_wait
load io_error,r0
jmp @r0
nop
sector_ready:
load 0x5100,r4
mov #0,r5
mov #0,r6
mov #0,r7
load command,r0
jsr @r0
nop
load 0x1,r2
cmp/eq r2,r0
bt guard_68
load io_error,r0
jmp @r0
nop
guard_68:
load 0x25890008,r1
mov.w @r1,r0
tst #128,r0
bf guard_69
load io_error,r0
jmp @r0
nop
guard_69:
load 0x25890008,r1
load 0xff7f,r0
mov.w r0,@r1
load 0x6300,r4
mov #0,r5
mov #0,r6
mov #1,r7
load command,r0
jsr @r0
nop
load 0x25890008,r1
mov.w @r1,r0
tst #2,r0
bf guard_70
load io_error,r0
jmp @r0
nop
guard_70:
load 0x26010330,r1
mov.l @r1,r0
add #1,r0
mov r0,r7
load 0x25fe0000,r1
load 0x5818000,r0
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
load 0x25fe0004,r1
mov.l r11,@r1
load 0x25fe0010,r1
load 0x101,r0
mov.l r0,@r1
load 0x26010204,r1
load 200000,r3
dma_wait:
mov.l @r1,r0
cmp/eq r7,r0
bt dma_ready
dt r3
bf dma_wait
load io_error,r0
jmp @r0
nop
dma_ready:
mov r7,r0
load 0x26010330,r1
mov.l r0,@r1
load 0x600,r4
mov #0,r5
mov #0,r6
mov #0,r7
load command,r0
jsr @r0
nop
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
load 0x400,r2
cmp/eq r2,r0
bt guard_71
load io_error,r0
jmp @r0
nop
guard_71:
load 0x26010210,r1
load 0x400,r0
mov.l r0,@r1
load 0x25890008,r1
mov.w @r1,r0
load 0x82,r2
and r2,r0
load 0x80,r2
cmp/eq r2,r0
bt guard_72
load io_error,r0
jmp @r0
nop
guard_72:
lds.l @r15+,pr
rts
nop
be32:
mov #0,r0
mov #4,r3
be32_loop:
shll8 r0
mov.b @r1+,r2
extu.b r2,r2
or r2,r0
dt r3
bf be32_loop
rts
nop
dual32:
sts.l pr,@-r15
mov r1,r5
add #3,r5
mov #0,r4
mov #4,r3
le32_loop:
shll8 r4
mov.b @r5,r2
extu.b r2,r2
or r2,r4
add #-1,r5
dt r3
bf le32_loop
add #4,r1
load be32,r0
jsr @r0
nop
cmp/eq r4,r0
bt guard_73
load metadata_error,r0
jmp @r0
nop
guard_73:
lds.l @r15+,pr
rts
nop
record:
sts.l pr,@-r15
mov r8,r1
add #1,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_74
load metadata_error,r0
jmp @r0
nop
guard_74:
mov r8,r1
add #25,r1
mov.b @r1,r0
extu.b r0,r0
load 3,r2
mov r0,r3
and r2,r3
cmp/eq r3,r0
bt guard_75
load metadata_error,r0
jmp @r0
nop
guard_75:
mov r8,r1
add #26,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_76
load metadata_error,r0
jmp @r0
nop
guard_76:
mov r8,r1
add #27,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_77
load metadata_error,r0
jmp @r0
nop
guard_77:
mov r8,r1
load 28,r0
add r0,r1
load be32,r0
jsr @r0
nop
load 0x1000001,r2
cmp/eq r2,r0
bt guard_78
load metadata_error,r0
jmp @r0
nop
guard_78:
mov r8,r1
add #32,r1
mov.b @r1,r0
extu.b r0,r0
tst r0,r0
bf guard_79
load metadata_error,r0
jmp @r0
nop
guard_79:
mov r0,r6
add #34,r0
mov #-2,r2
and r2,r0
cmp/hs r0,r13
bt guard_80
load metadata_error,r0
jmp @r0
nop
guard_80:
mov r8,r1
load 2,r0
add r0,r1
load dual32,r0
jsr @r0
nop
mov r0,r6
load 0x26010314,r1
mov.l @r1,r2
cmp/hs r2,r6
bf guard_81
load metadata_error,r0
jmp @r0
nop
guard_81:
mov r8,r1
load 10,r0
add r0,r1
load dual32,r0
jsr @r0
nop
mov r0,r7
load 0x40000000,r2
cmp/hs r2,r0
bf guard_82
load metadata_error,r0
jmp @r0
nop
guard_82:
load 2047,r2
add r2,r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
shlr r7
add r6,r7
load 0x26010314,r1
mov.l @r1,r2
cmp/hs r7,r2
bt guard_83
load metadata_error,r0
jmp @r0
nop
guard_83:
lds.l @r15+,pr
rts
nop
root_record:
sts.l pr,@-r15
mov r8,r1
mov.b @r1,r0
extu.b r0,r0
load 34,r2
cmp/hs r2,r0
bt guard_84
load metadata_error,r0
jmp @r0
nop
guard_84:
mov r8,r1
add #1,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_85
load metadata_error,r0
jmp @r0
nop
guard_85:
mov r8,r1
add #25,r1
mov.b @r1,r0
extu.b r0,r0
load 0x2,r2
cmp/eq r2,r0
bt guard_86
load metadata_error,r0
jmp @r0
nop
guard_86:
mov r8,r1
add #26,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_87
load metadata_error,r0
jmp @r0
nop
guard_87:
mov r8,r1
add #27,r1
mov.b @r1,r0
extu.b r0,r0
load 0x0,r2
cmp/eq r2,r0
bt guard_88
load metadata_error,r0
jmp @r0
nop
guard_88:
mov r8,r1
add #32,r1
mov.b @r1,r0
extu.b r0,r0
load 0x1,r2
cmp/eq r2,r0
bt guard_89
load metadata_error,r0
jmp @r0
nop
guard_89:
mov r8,r1
load 28,r0
add r0,r1
load be32,r0
jsr @r0
nop
load 0x1000001,r2
cmp/eq r2,r0
bt guard_90
load metadata_error,r0
jmp @r0
nop
guard_90:
lds.l @r15+,pr
rts
nop
metadata_error:
load 0x260100fc,r1
load 0xe002,r0
mov.l r0,@r1
load stopped,r0
jmp @r0
nop
io_error:
load 0x260100fc,r1
load 0xe003,r0
mov.l r0,@r1
load stopped,r0
jmp @r0
nop
command:
mov.l r4,@r9
add #4,r9
mov.l r5,@r9
add #4,r9
mov.l r6,@r9
add #4,r9
mov.l r7,@r9
add #4,r9
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
load 200000,r3
command_wait:
mov.w @r1,r0
tst #1,r0
bf command_ready
dt r3
bf command_wait
load io_error,r0
jmp @r0
nop
command_ready:
load 0x25890018,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x200,r2
cmp/eq r2,r4
bt command_status_ok
load 0x8000,r2
tst r2,r0
bt guard_91
load io_error,r0
jmp @r0
nop
guard_91:
command_status_ok:
load 0x2589001c,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x25890020,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
load 0x25890024,r1
mov.w @r1,r0
extu.w r0,r0
mov.l r0,@r9
add #4,r9
rts
nop
