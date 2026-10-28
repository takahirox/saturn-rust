; Original independent system services. MIT: adjacent LICENSE.
; ST-162-R1 System Program §§1,2.2, services 1.1-3.2 and 5.1.
; SHC arguments R4-R7, result R0; R8-R14 are callee-saved.
; All operations, callbacks and semaphore TAS execute as SH-2 instructions.
.section 0x4000
; SYS_SETSINT: current processor's VBR, zero restores default handler.
stc vbr,r1
mov r4,r0
shll2 r0
add r0,r1
tst r5,r5
bf set_vector
load 0x4b00,r5
mov #64,r0
cmp/hs r0,r4
bf set_vector
mov #96,r0
cmp/hs r0,r4
bt set_vector
mov #78,r0
cmp/eq r0,r4
bt set_vector
mov #79,r0
cmp/eq r0,r4
bt set_vector
mov r4,r0
add #-64,r0
shll2 r0
shll2 r0
shll r0
load 0x8000,r5
add r0,r5
set_vector:
mov.l r5,@r1
rts
nop
.section 0x4100
; SYS_GETSINT
stc vbr,r1
mov r4,r0
shll2 r0
add r0,r1
mov.l @r1,r0
rts
nop
.section 0x4200
; SYS_SETUINT
load 0x06000b00,r1
mov r4,r0
add #-64,r0
shll2 r0
add r0,r1
tst r5,r5
bf set_callback
load 0x4b10,r5
set_callback:
mov.l r5,@r1
rts
nop
.section 0x4300
; SYS_GETUINT
load 0x06000b00,r1
mov r4,r0
add #-64,r0
shll2 r0
add r0,r1
mov.l @r1,r0
rts
nop
.section 0x4400
; SYS_SETSCUIM: exclude interrupts before publishing the pair of values.
stc sr,r3
load 0xf0,r0
or r3,r0
ldc r0,sr
mov r4,r2
load 0x4c00,r1
jmp @r1
nop
.section 0x4500
; SYS_CHGSCUIM: atomic read/modify/publish, never read write-only IMS.
stc sr,r3
load 0xf0,r0
or r3,r0
ldc r0,sr
load 0x06000348,r1
mov.l @r1,r2
and r4,r2
or r5,r2
load 0x4c00,r1
jmp @r1
nop
.section 0x4600
; SYS_TASSEM: uncached shared byte and actual architectural TAS.
load 0x26000a00,r1
add r4,r1
tas.b @r1
movt r0
rts
nop
.section 0x4700
; SYS_CLRSEM
load 0x26000a00,r1
add r4,r1
mov #0,r0
mov.b r0,@r1
rts
nop
.section 0x4800
; SYS_CHGUIPR: copy, rather than retain the caller's 32-word table.
stc sr,r3
load 0xf0,r0
or r3,r0
ldc r0,sr
load 0x06000b80,r1
mov #32,r2
copy_priority:
mov.l @r4+,r0
mov.l r0,@r1
add #4,r1
dt r2
bf copy_priority
ldc r3,sr
rts
nop
.section 0x4b00
; Empty architectural interrupt handler.
rte
nop
.section 0x4b10
; Empty SHC callback.
rts
nop
.section 0x4c00
; Publish R2 with interrupts masked, restore caller SR in R3.
; ST-210 #7 prohibits the earlier library manual's IST writes. Acknowledge
; A-bus via the production AIACK port instead of writing prohibited IST.
load 0x06000348,r1
mov.l r2,@r1
load 0x25fe00a0,r1
mov.l r2,@r1
load 0x8000,r0
tst r0,r2
bf mask_done
load 0x25fe00a8,r1
mov #1,r0
mov.l r0,@r1
mask_done:
ldc r3,sr
rts
nop
.section 0x4d00
; SYS_CHGSYSCK: original standby protocol, SH7604 table 14.3.
; Preserve caller SR, vector, SBYCR and CCR; NMI uses architectural RTE.
stc sr,r3
load 0xf0,r0
or r3,r0
ldc r0,sr
stc vbr,r6
add #44,r6
mov.l @r6,r5
load 0x4b00,r0
mov.l r0,@r6
load 0xfffffe92,r1
mov.b @r1,r7
mov #0,r0
mov.b r0,@r1
load 0xfffffe91,r1
mov.b @r1,r2
mov.l r2,@-r15
; Select a finite WDT oscillator wait (1048576 clocks), with timer disabled.
load 0xfffffe80,r1
load 0xa506,r0
mov.w r0,@r1
load 0x20100063,r1
clock_wait_idle:
mov.b @r1,r0
tst #1,r0
bf clock_wait_idle
mov #1,r0
mov.b r0,@r1
mov #15,r0
tst r4,r4
bt clock_issue
mov #14,r0
clock_issue:
load 0x2010001f,r2
mov.b r0,@r2
load 0xfffffe91,r1
load 0xe0,r0
mov.b r0,@r1
sleep
; Standby has initialized FRT/SCI. Completion NMI sets DMAOR.NMIF
; and returns here. DMA remains inhibited until the application restarts it.
mov.l @r15+,r0
load 0xfffffe91,r1
mov.b r0,@r1
mov.l r5,@r6
; Re-enable the HLE boot communication capture service. This is firmware
; recovery; table 14.3 itself initializes TIER to 1 and retains INTC vectors.
load 0xfffffe10,r1
load 0x81,r0
mov.b r0,@r1
load 0xfffffe92,r1
mov r7,r0
mov #16,r2
or r2,r0
mov.b r0,@r1
load 0x06000324,r1
mov.l r4,@r1
; Restore hardware IMS from the retained public shadow (System Program guide).
load 0x06000348,r1
mov.l @r1,r0
load 0x25fe00a0,r1
mov.l r0,@r1
load 0x8000,r2
tst r2,r0
bf clock_mask_restored
load 0x25fe00a8,r1
mov #1,r0
mov.l r0,@r1
clock_mask_restored:
; Service postcondition: hot reset enabled, via ordinary SMPC command.
load 0x20100063,r1
mov #1,r0
mov.b r0,@r1
load 0x2010001f,r2
mov #25,r0
mov.b r0,@r2
clock_wait_enable:
mov.b @r1,r0
tst #1,r0
bf clock_wait_enable
ldc r3,sr
rts
nop
.section 0x5000
; Original BUP_Init, ST-162-R1 backup library function 1.
; Copy 16 KiB of original guest code to the application's expansion buffer.
; Helpers remain ordinary guest ROM subroutines. No host service interception.
mov r4,r2
load 0x10000,r1
load 4096,r3
bup_copy_library:
mov.l @r1+,r0
mov.l r0,@r2
add #4,r2
dt r3
bf bup_copy_library
; Relocate the ten public entry pointers to the copied instruction addresses.
; Public BUP_VECTOR_ADDRESS is RAM354; function slots start at offset four.
mov.l r5,@r4
load 0x06000354,r1
mov.l r4,@r1
load 0x10000,r0
mov r4,r3
sub r0,r3
mov r4,r2
add #4,r2
mov #10,r1
bup_relocate:
mov.l @r2,r0
add r3,r0
mov.l r0,@r2
add #4,r2
dt r1
bf bup_relocate
load 0x00010001,r0
mov.l r0,@r6
mov #0,r0
add #4,r6
mov.l r0,@r6
add #4,r6
mov.l r0,@r6
rts
nop
.section 0x9000
; The per-vector entry has saved R0 and put the cause index in R0.
; A C callback may destroy R0-R7, MACH, MACL and PR. Preserve them all,
; restore the previous mask for nesting, and use architectural RTE/SR.
mov.l r1,@-r15
mov.l r2,@-r15
mov.l r3,@-r15
mov.l r4,@-r15
mov.l r5,@-r15
mov.l r6,@-r15
mov.l r7,@-r15
sts mach,r1
mov.l r1,@-r15
sts macl,r1
mov.l r1,@-r15
sts pr,r1
mov.l r1,@-r15
load 0x06000348,r1
mov.l @r1,r2
mov.l r2,@-r15
shll2 r0
load 0x06000b80,r3
add r0,r3
mov.l @r3,r3
extu.w r3,r4
or r4,r2
mov.l r2,@r1
load 0x25fe00a0,r1
mov.l r2,@r1
load 0x06000b00,r1
add r0,r1
mov.l @r1,r1
shlr16 r3
ldc r3,sr
jsr @r1
nop
load 0xf0,r0
ldc r0,sr
mov.l @r15+,r2
load 0x06000348,r1
mov.l r2,@r1
load 0x25fe00a0,r1
mov.l r2,@r1
mov.l @r15+,r1
lds r1,pr
mov.l @r15+,r1
lds r1,macl
mov.l @r15+,r1
lds r1,mach
mov.l @r15+,r7
mov.l @r15+,r6
mov.l @r15+,r5
mov.l @r15+,r4
mov.l @r15+,r3
mov.l @r15+,r2
mov.l @r15+,r1
mov.l @r15+,r0
rte
nop
