; Original SH-2 backup library. MIT: adjacent LICENSE.
; ST-162-R1 backup library functions 1-11 and BupConfig/Stat/Dir/Date ABI.
; The private SRBUP001 format is specified in README.md, not Sega firmware.
; All persistent accesses are real odd-byte backup-RAM bus cycles.
; R8-R14 and PR are preserved by public calls. Helpers execute in guest ROM.

.section 0x10100
; BUP_SelPart: public copied-library entry.
bup_selpart:
tst r4,r4
bt partition_device
mov #1,r0
rts
nop
partition_device:
mov #0,r0
tst r5,r5
bt partition_ok
mov #9,r0
partition_ok:
rts
nop

.section 0x10200
; BUP_Format: public copied-library entry.
bup_format:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
tst r4,r4
bt format_device
mov #1,r0
bra format_end
nop
format_device:
load 0x20180001,r1
load 32768,r2
mov #0,r0
format_clear:
mov.b r0,@r1
add #2,r1
dt r2
bf format_clear
load 0x20180001,r1
mov #83,r0
mov.b r0,@r1
add #2,r1
mov #82,r0
mov.b r0,@r1
add #2,r1
mov #66,r0
mov.b r0,@r1
add #2,r1
mov #85,r0
mov.b r0,@r1
add #2,r1
mov #80,r0
mov.b r0,@r1
add #2,r1
mov #48,r0
mov.b r0,@r1
add #2,r1
mov #48,r0
mov.b r0,@r1
add #2,r1
mov #49,r0
mov.b r0,@r1
add #2,r1
mov #0,r0
format_end:
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x10400
; BUP_Stat: public copied-library entry.
bup_stat:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
mov.l r5,@-r15
mov #0,r5
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l stat_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l stat_end
load 510,r11
sub r10,r11
mov r6,r8
load 32768,r0
mov.l r0,@r8
add #4,r8
load 512,r0
mov.l r0,@r8
add #4,r8
mov #64,r0
mov.l r0,@r8
add #4,r8
mov r11,r0
shll2 r0
shll2 r0
shll2 r0
mov.l r0,@r8
add #4,r8
mov.l r11,@r8
add #4,r8
mov.l @r15,r0
load 32600,r1
cmp/hs r0,r1
bf stat_large
add #103,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
mov r0,r1
mov r11,r0
load 0x11f00,r7
jsr @r7
nop
bra stat_store
nop
stat_large:
mov #0,r0
stat_store:
mov.l r0,@r8
mov #0,r0
stat_end:
add #4,r15
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x10600
; BUP_Write: public copied-library entry.
bup_write:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l write_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l write_end
tst r9,r9
bt write_size
tst r7,r7
bt write_size
mov #6,r0
bra write_end
nop
write_size:
mov r5,r1
mov.b @r1,r0
tst r0,r0
bt.l write_bad_name
add #11,r1
mov.b @r1,r0
tst r0,r0
bf.l write_bad_name
mov r5,r1
add #22,r1
mov.b @r1,r0
tst r0,r0
bf.l write_bad_name
add #1,r1
mov.b @r1,r0
extu.b r0,r0
mov #5,r2
cmp/hs r0,r2
bf.l write_bad_name
mov r5,r1
add #28,r1
mov.l @r1,r0
load 32600,r1
cmp/hs r0,r1
bf.l write_no_space
add #103,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
mov r0,r14
mov r10,r0
sub r11,r0
add r14,r0
load 510,r1
cmp/hs r0,r1
bf.l write_no_space
tst r9,r9
bt write_allocate
load 0x11e00,r1
jsr @r1
nop
write_allocate:
mov r8,r12
mov r14,r13
shll2 r13
shll2 r13
shll2 r13
mov #0,r0
write_clear:
mov.b r0,@r12
add #2,r12
dt r13
bf write_clear
mov r8,r1
add #2,r1
mov r14,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
mov.b r0,@r1
add #2,r1
mov r14,r0
mov.b r0,@r1
mov r5,r12
mov r8,r3
add #8,r3
mov #32,r13
write_metadata:
mov.b @r12+,r0
mov.b r0,@r3
add #2,r3
dt r13
bf write_metadata
mov r8,r1
add #72,r1
mov r14,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
mov.b r0,@r1
add #2,r1
mov r14,r0
mov.b r0,@r1
mov r5,r1
add #28,r1
mov.l @r1,r13
mov r6,r12
mov r8,r3
add #80,r3
mov #0,r14
tst r13,r13
bt write_checksum
write_data:
mov.b @r12+,r0
extu.b r0,r0
mov.b r0,@r3
add #2,r3
add r0,r14
extu.w r14,r14
dt r13
bf write_data
write_checksum:
mov r8,r1
add #76,r1
mov r14,r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
shlr r0
mov.b r0,@r1
add #2,r1
mov r14,r0
mov.b r0,@r1
load 0xa5,r0
mov.b r0,@r8
mov #0,r0
bra write_end
nop
write_bad_name:
mov #9,r0
bra write_end
nop
write_no_space:
mov #4,r0
write_end:
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x10a00
; BUP_Read: public copied-library entry.
bup_read:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l read_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l read_end
tst r9,r9
bf read_found
mov #5,r0
bra read_end
nop
read_found:
load 0x11d00,r1
jsr @r1
nop
mov r0,r14
mov r9,r1
add #76,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
cmp/eq r14,r0
bf read_broken
mov r9,r1
add #64,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r3
mov r9,r1
add #80,r1
tst r3,r3
bt read_success
read_copy:
mov.b @r1,r0
mov.b r0,@r6
add #2,r1
add #1,r6
dt r3
bf read_copy
read_success:
mov #0,r0
bra read_end
nop
read_broken:
mov #8,r0
read_end:
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x10c00
; BUP_Delete: public copied-library entry.
bup_delete:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l delete_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l delete_end
tst r9,r9
bf delete_found
mov #5,r0
bra delete_end
nop
delete_found:
load 0x11e00,r1
jsr @r1
nop
mov #0,r0
delete_end:
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x10e00
; BUP_Dir: public copied-library entry.
bup_dir:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
mov.l r5,@-r15
mov #0,r5
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l dir_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l dir_end
mov.l @r15,r14
mov r7,r12
extu.w r6,r11
load 0x20180101,r9
mov #0,r10
dir_next:
cmp/hs r8,r9
bt.l dir_result
mov r9,r1
add #2,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r13
shll8 r13
shlr r13
add r9,r13
mov r14,r1
mov r9,r2
add #8,r2
mov #11,r3
dir_match:
mov.b @r1+,r0
tst r0,r0
bt dir_hit
mov.b @r2,r4
cmp/eq r4,r0
bf dir_advance
add #2,r2
dt r3
bf dir_match
dir_hit:
add #1,r10
cmp/hs r10,r11
bf dir_advance
mov r9,r1
add #8,r1
mov #34,r3
dir_copy:
mov.b @r1,r0
mov.b r0,@r12
add #2,r1
add #1,r12
dt r3
bf dir_copy
mov #0,r0
mov.b r0,@r12
add #1,r12
mov.b r0,@r12
add #1,r12
dir_advance:
mov r13,r9
bra dir_next
nop
dir_result:
mov r10,r0
cmp/hs r10,r11
bt dir_end
mov #0,r0
sub r10,r0
dir_end:
add #4,r15
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x11100
; BUP_Verify: public copied-library entry.
bup_verify:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
load 0x11900,r1
jsr @r1
nop
tst r0,r0
bf.l verify_end
load 0x11a00,r1
jsr @r1
nop
tst r0,r0
bf.l verify_end
tst r9,r9
bf verify_found
mov #5,r0
bra verify_end
nop
verify_found:
load 0x11d00,r1
jsr @r1
nop
mov r0,r14
mov r9,r1
add #76,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
cmp/eq r14,r0
bf verify_broken
mov r9,r1
add #64,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r3
mov r9,r1
add #80,r1
tst r3,r3
bt verify_success
verify_bytes:
mov.b @r1,r0
mov.b @r6+,r2
cmp/eq r0,r2
bf verify_mismatch
add #2,r1
dt r3
bf verify_bytes
verify_success:
mov #0,r0
bra verify_end
nop
verify_mismatch:
mov #7,r0
bra verify_end
nop
verify_broken:
mov #8,r0
verify_end:
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x11300
; BUP_GetDate: public copied-library entry.
bup_getdate:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
mov r5,r8
mov r4,r0
load 1440,r1
load 0x11f00,r7
jsr @r7
nop
mov r0,r9
mov r1,r10
add #2,r0
mov #7,r1
load 0x11f00,r7
jsr @r7
nop
mov r8,r2
add #5,r2
mov.b r1,@r2
mov #0,r11
getdate_year:
mov r11,r0
tst #3,r0
bf getdate_year_ordinary
load 120,r1
cmp/eq r1,r11
bt getdate_year_ordinary
load 220,r1
cmp/eq r1,r11
bt getdate_year_ordinary
load 366,r0
bra getdate_year_days
nop
getdate_year_ordinary:
load 365,r0
getdate_year_days:
cmp/hs r0,r9
bf getdate_month_start
sub r0,r9
add #1,r11
bra getdate_year
nop
getdate_month_start:
mov #1,r12
getdate_month:
mov r12,r0
mov r11,r1
load 0x12000,r7
jsr @r7
nop
cmp/hs r0,r9
bf getdate_store
sub r0,r9
add #1,r12
bra getdate_month
nop
getdate_store:
mov r8,r2
mov.b r11,@r2
add #1,r2
mov.b r12,@r2
add #1,r2
add #1,r9
mov.b r9,@r2
mov r10,r0
mov #60,r1
load 0x11f00,r7
jsr @r7
nop
mov r8,r2
add #3,r2
mov.b r0,@r2
add #1,r2
mov.b r1,@r2
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x11600
; BUP_SetDate: public copied-library entry.
bup_setdate:
mov.l r8,@-r15
mov.l r9,@-r15
mov.l r10,@-r15
mov.l r11,@-r15
mov.l r12,@-r15
mov.l r13,@-r15
mov.l r14,@-r15
sts pr,r0
mov.l r0,@-r15
mov r4,r8
mov.b @r8,r11
extu.b r11,r11
mov #0,r9
mov #0,r10
setdate_year:
cmp/eq r11,r10
bt setdate_month_start
mov r10,r0
tst #3,r0
bf setdate_year_ordinary
load 120,r1
cmp/eq r1,r10
bt setdate_year_ordinary
load 220,r1
cmp/eq r1,r10
bt setdate_year_ordinary
load 366,r0
bra setdate_year_days
nop
setdate_year_ordinary:
load 365,r0
setdate_year_days:
add r0,r9
add #1,r10
bra setdate_year
nop
setdate_month_start:
mov r8,r1
add #1,r1
mov.b @r1,r12
extu.b r12,r12
mov #1,r13
setdate_month:
cmp/hs r12,r13
bt setdate_day
mov r13,r0
mov r11,r1
load 0x12000,r7
jsr @r7
nop
add r0,r9
add #1,r13
bra setdate_month
nop
setdate_day:
mov r8,r1
add #2,r1
mov.b @r1,r0
extu.b r0,r0
add #-1,r0
add r0,r9
load 1440,r0
mul.l r9,r0
sts macl,r9
add #1,r1
mov.b @r1,r0
extu.b r0,r0
mov #60,r2
mul.l r2,r0
sts macl,r0
add r0,r9
add #1,r1
mov.b @r1,r0
extu.b r0,r0
add r9,r0
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r14
mov.l @r15+,r13
mov.l @r15+,r12
mov.l @r15+,r11
mov.l @r15+,r10
mov.l @r15+,r9
mov.l @r15+,r8
rts
nop

.section 0x11900
tst r4,r4
bt check_device
mov #1,r0
rts
nop
check_device:
load 0x20180001,r1
mov.b @r1,r0
mov #83,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #82,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #66,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #85,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #80,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #48,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #48,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov.b @r1,r0
mov #49,r2
cmp/eq r0,r2
bf check_unformatted
add #2,r1
mov #0,r0
rts
nop
check_unformatted:
mov #2,r0
rts
nop

.section 0x11a00
load 0x20180101,r8
mov #0,r9
mov #0,r10
mov #0,r11
scan_next:
load 0x20190001,r1
cmp/hs r1,r8
bt.l scan_success
mov.b @r8,r0
tst r0,r0
bt.l scan_success
extu.b r0,r0
load 0xa5,r1
cmp/eq r0,r1
bf.l scan_broken
mov r8,r1
add #2,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r13
tst r13,r13
bt.l scan_broken
load 510,r1
cmp/hs r13,r1
bf.l scan_broken
mov r13,r12
shll8 r12
shlr r12
add r8,r12
load 0x20190001,r1
cmp/hs r12,r1
bf.l scan_broken
mov r8,r1
add #64,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r14
mov r13,r0
shll2 r0
shll2 r0
shll2 r0
add #-40,r0
cmp/hs r14,r0
bf.l scan_broken
; Validate the public directory terminators and the redundant block count.
mov r8,r1
add #8,r1
mov.b @r1,r0
tst r0,r0
bt.l scan_broken
add #22,r1
mov.b @r1,r0
tst r0,r0
bf.l scan_broken
add #22,r1
mov.b @r1,r0
tst r0,r0
bf.l scan_broken
add #2,r1
mov.b @r1,r0
extu.b r0,r0
mov #5,r2
cmp/hs r0,r2
bf.l scan_broken
mov r8,r1
add #72,r1
mov.b @r1,r0
extu.b r0,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
cmp/eq r13,r0
bf.l scan_broken
tst r5,r5
bt scan_advance
mov r5,r1
mov r8,r2
add #8,r2
mov #12,r3
scan_name:
mov.b @r1+,r0
mov.b @r2,r14
cmp/eq r0,r14
bf scan_advance
tst r0,r0
bt scan_hit
add #2,r2
dt r3
bf scan_name
bra scan_broken
nop
scan_hit:
tst r9,r9
bf scan_broken
mov r8,r9
mov r13,r11
scan_advance:
add r13,r10
mov r12,r8
bra scan_next
nop
scan_success:
mov #0,r0
rts
nop
scan_broken:
mov #8,r0
rts
nop

.section 0x11d00
mov r9,r1
add #64,r1
mov #0,r0
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
shll8 r0
add #2,r1
mov.b @r1,r2
extu.b r2,r2
or r2,r0
mov r0,r3
mov r9,r1
add #80,r1
mov #0,r0
tst r3,r3
bt checksum_end
checksum_byte:
mov.b @r1,r2
extu.b r2,r2
add r2,r0
extu.w r0,r0
add #2,r1
dt r3
bf checksum_byte
checksum_end:
rts
nop

.section 0x11e00
mov r11,r0
shll8 r0
shlr r0
mov r9,r1
add r0,r1
mov r9,r2
compact_copy:
cmp/hs r8,r1
bt compact_zero_start
mov.b @r1,r3
mov.b r3,@r2
add #2,r1
add #2,r2
bra compact_copy
nop
compact_zero_start:
sub r0,r8
mov #0,r3
compact_zero:
cmp/hs r1,r2
bt compact_done
mov.b r3,@r2
add #2,r2
bra compact_zero
nop
compact_done:
rts
nop

.section 0x11f00
; Independent unsigned restoring division: dividend R0, divisor R1.
; Result R0 quotient/R1 remainder; valid here for nonzero small divisors.
mov r0,r2
mov #0,r0
mov #0,r3
mov #32,r4
udiv_bit:
shll r2
rotcl r3
shll r0
cmp/hs r1,r3
bf udiv_next
sub r1,r3
add #1,r0
udiv_next:
dt r4
bf udiv_bit
mov r3,r1
rts
nop

.section 0x12000
; Month length for BupDate year 0..255 (1980..2235).
mov #2,r2
cmp/eq r2,r0
bt month_feb
mov #4,r2
cmp/eq r2,r0
bt month_short
mov #6,r2
cmp/eq r2,r0
bt month_short
mov #9,r2
cmp/eq r2,r0
bt month_short
mov #11,r2
cmp/eq r2,r0
bt month_short
mov #31,r0
rts
nop
month_short:
mov #30,r0
rts
nop
month_feb:
mov r1,r0
tst #3,r0
bf month_feb_ordinary
load 120,r2
cmp/eq r2,r1
bt month_feb_ordinary
load 220,r2
cmp/eq r2,r1
bt month_feb_ordinary
mov #29,r0
rts
nop
month_feb_ordinary:
mov #28,r0
rts
nop
