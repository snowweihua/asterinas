/* SPDX-License-Identifier: MPL-2.0 */

/*
 * AArch64 trap handler implementation for Asterinas ostd.
 *
 * RawUserContext memory layout (all fields 8 bytes):
 *  offset   0: x0
 *  offset   8: x1
 *  ...
 *  offset 232: x29
 *  offset 240: lr  (x30)
 *  offset 248: elr_el1
 *  offset 256: spsr_el1
 *  offset 264: esr_el1
 *  offset 272: sp_el0
 * Total = 280 bytes = 35 slots of 8 bytes.
 *
 * run_user() kernel frame layout (128 bytes, 16 slots):
 *  [ksp+  0]: x19      [ksp+  8]: x20
 *  [ksp+ 16]: x21      [ksp+ 24]: x22
 *  [ksp+ 32]: x23      [ksp+ 40]: x24
 *  [ksp+ 48]: x25      [ksp+ 56]: x26
 *  [ksp+ 64]: x27      [ksp+ 72]: x28
 *  [ksp+ 80]: x29 (fp) [ksp+ 88]: x30 (return address)
 *  [ksp+ 96]: kernel sp before frame
 *  [ksp+104]: RawUserContext*
 *  [ksp+112]: (padding) [ksp+120]: (padding)
 */

/*
 * Save volatile caller-saved registers to a TrapFrame on the kernel stack.
 * Sets x0 = sp (TrapFrame*).
 */
.macro save_volatile_to_stack el:req
    stp x0, x1, [sp, #-(8 * 34)]!
    stp x2, x3,   [sp, #8 *  2]
    stp x4, x5,   [sp, #8 *  4]
    stp x6, x7,   [sp, #8 *  6]
    stp x8, x9,   [sp, #8 *  8]
    stp x10, x11, [sp, #8 * 10]
    stp x12, x13, [sp, #8 * 12]
    stp x14, x15, [sp, #8 * 14]
    stp x16, x17, [sp, #8 * 16]
    stp x18, x19, [sp, #8 * 18]
    stp x20, x21, [sp, #8 * 20]
    stp x22, x23, [sp, #8 * 22]
    stp x24, x25, [sp, #8 * 24]
    stp x26, x27, [sp, #8 * 26]
    stp x28, x29, [sp, #8 * 28]
    mrs x1, elr_\el
    mrs x2, spsr_\el
    mrs x3, esr_\el
    stp lr, x1,  [sp, #8 * 30]
    stp x2, x3,  [sp, #8 * 32]
    mov x0, sp
.endm

.macro restore_volatile_from_stack el:req
    ldp x2, x3,   [sp, #8 *  2]
    ldp x4, x5,   [sp, #8 *  4]
    ldp x6, x7,   [sp, #8 *  6]
    ldp x8, x9,   [sp, #8 *  8]
    ldp x10, x11, [sp, #8 * 10]
    ldp x12, x13, [sp, #8 * 12]
    ldp x14, x15, [sp, #8 * 14]
    ldp x16, x17, [sp, #8 * 16]
    ldp x18, x19, [sp, #8 * 18]
    ldp x20, x21, [sp, #8 * 20]
    ldp x22, x23, [sp, #8 * 22]
    ldp x24, x25, [sp, #8 * 24]
    ldp x26, x27, [sp, #8 * 26]
    ldp x28, x29, [sp, #8 * 28]
    ldp lr, x1,   [sp, #8 * 30]
    msr elr_\el, x1
    ldp x0, x1,   [sp, #8 * 32]
    msr spsr_\el, x0
    msr esr_\el, x1
    ldp x0, x1,   [sp], #8 * 34
.endm

.macro current_exception_sp0 handler:req el:req
    msr spsel, #1
    save_volatile_to_stack \el
    bl \handler
    restore_volatile_from_stack \el
    msr spsel, #0
    eret
.endm

.macro current_exception_spx handler:req el:req
    save_volatile_to_stack \el
    bl \handler
    restore_volatile_from_stack \el
    eret
.endm


/*
 * vector_table macro: instantiate AArch64 vector table for a given EL.
 *
 * Each slot is exactly 0x80 bytes and contains only a single branch
 * instruction.  The real save/restore logic is in .text trampolines below.
 *
 * Lower-EL (EL0) entries save x0 to the scratch slot in the run_user frame
 * and branch to __el0_exception_handler.
 */
.macro vector_table el:req
.section .vector.vector_table_\el, "ax"
.global vector_table_\el
.balign 0x800
vector_table_\el:

/* --- Group 1: Current EL with SP0 -------------------------------------------*/
.balign 0x80
sync_cur_sp0_\el:   b __tramp_sync_sp0_\el
.balign 0x80
irq_cur_sp0_\el:    b __tramp_irq_sp0_\el
.balign 0x80
fiq_cur_sp0_\el:    b __tramp_fiq_sp0_\el
.balign 0x80
serr_cur_sp0_\el:   b __tramp_serr_sp0_\el

/* --- Group 2: Current EL with SPx -------------------------------------------*/
.balign 0x80
sync_cur_spx_\el:   b __tramp_sync_spx_\el
.balign 0x80
irq_cur_spx_\el:    b __tramp_irq_spx_\el
.balign 0x80
fiq_cur_spx_\el:    b __tramp_fiq_spx_\el
.balign 0x80
serr_cur_spx_\el:   b __tramp_serr_spx_\el

/* --- Group 3: Lower EL (AArch64) -------------------------------------------*/
.balign 0x80
sync_lower_64_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
irq_lower_64_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
fiq_lower_64_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
serr_lower_64_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler

/* --- Group 4: Lower EL (AArch32) -------------------------------------------*/
.balign 0x80
sync_lower_32_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
irq_lower_32_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
fiq_lower_32_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler
.balign 0x80
serr_lower_32_\el:
    str  x0, [sp, #112]
    b __el0_exception_handler

.endm

/* --- Trampoline routines for kernel-mode (EL1→EL1) exceptions ---------------
 *
 * These live in .text and may be as large as needed.
 */
.section .text

/* SP0 group: switch to SPx, save, call handler, restore, switch back */
__tramp_sync_sp0_el1:
    msr  spsel, #1
    save_volatile_to_stack el1
    bl   sync_exception_current
    restore_volatile_from_stack el1
    msr  spsel, #0
    eret

__tramp_irq_sp0_el1:
    msr  spsel, #1
    save_volatile_to_stack el1
    bl   irq_current
    restore_volatile_from_stack el1
    msr  spsel, #0
    eret

__tramp_fiq_sp0_el1:
    msr  spsel, #1
    save_volatile_to_stack el1
    bl   fiq_current
    restore_volatile_from_stack el1
    msr  spsel, #0
    eret

__tramp_serr_sp0_el1:
    msr  spsel, #1
    save_volatile_to_stack el1
    bl   serr_current
    restore_volatile_from_stack el1
    msr  spsel, #0
    eret

/* SPx group: already on SPx, just save/call/restore */
__tramp_sync_spx_el1:
    save_volatile_to_stack el1
    bl   sync_exception_current
    restore_volatile_from_stack el1
    eret

__tramp_irq_spx_el1:
    save_volatile_to_stack el1
    bl   irq_current
    restore_volatile_from_stack el1
    eret

__tramp_fiq_spx_el1:
    save_volatile_to_stack el1
    bl   fiq_current
    restore_volatile_from_stack el1
    eret

__tramp_serr_spx_el1:
    save_volatile_to_stack el1
    bl   serr_current
    restore_volatile_from_stack el1
    eret

vector_table el1

/*
 * __el0_exception_handler
 *
 * Called on any exception from EL0 (AArch64 or AArch32 mode).
 * On entry:
 *   sp (= sp_el1) = top of run_user kernel frame (not modified by CPU on EL0 exception)
 *   The kernel frame at [sp..sp+128) was allocated by run_user().
 *   [sp+104] = RawUserContext*
 *   [sp+88]  = return address (x30 saved by run_user)
 *   [sp+80]  = saved x29 (fp)
 *   [sp+0..sp+79] = saved x19..x28
 *   All user registers (x0..x30, elr_el1, spsr_el1, esr_el1) are live.
 *
 * x0 and x1 are used as scratch. The original user values of x0..x1 are
 * saved LAST using sp_el0 as a two-slot scratch area.
 */
.section .text
.global __el0_exception_handler
__el0_exception_handler:
    /* x0 was saved to [sp, #112] (slot 14) by the vector entry before branching here.
     * x1..x30, elr_el1, spsr_el1, esr_el1 are all live (user register values).
     * sp = sp_el1 = top of run_user kernel frame.
     * [sp+104] = RawUserContext*
     */

    /* Load ctx pointer (overwrites x0 which is now safe to lose) */
    ldr  x0, [sp, #104]           /* x0 = RawUserContext* */

    /* Save x1..x30 */
    str  x1,       [x0, #8 *  1]
    stp  x2, x3,   [x0, #8 *  2]
    stp  x4, x5,   [x0, #8 *  4]
    stp  x6, x7,   [x0, #8 *  6]
    stp  x8, x9,   [x0, #8 *  8]
    stp  x10, x11, [x0, #8 * 10]
    stp  x12, x13, [x0, #8 * 12]
    stp  x14, x15, [x0, #8 * 14]
    stp  x16, x17, [x0, #8 * 16]
    stp  x18, x19, [x0, #8 * 18]
    stp  x20, x21, [x0, #8 * 20]
    stp  x22, x23, [x0, #8 * 22]
    stp  x24, x25, [x0, #8 * 24]
    stp  x26, x27, [x0, #8 * 26]
    stp  x28, x29, [x0, #8 * 28]
    str  x30,      [x0, #8 * 30]  /* user lr */

    /* Recover saved user x0 from frame scratch slot and store it */
    ldr  x1, [sp, #112]
    str  x1, [x0, #8 *  0]        /* user x0 */

    /* Save ELR, SPSR, ESR */
    mrs  x1, elr_el1
    str  x1, [x0, #8 * 31]
    mrs  x1, spsr_el1
    str  x1, [x0, #8 * 32]
    mrs  x1, esr_el1
    str  x1, [x0, #8 * 33]
    /* Save SP_EL0 (user stack pointer) */
    mrs  x1, sp_el0
    str  x1, [x0, #8 * 34]

    /* Restore kernel callee-saved registers from run_user frame */
    ldp  x19, x20, [sp, #8 *  0]
    ldp  x21, x22, [sp, #8 *  2]
    ldp  x23, x24, [sp, #8 *  4]
    ldp  x25, x26, [sp, #8 *  6]
    ldp  x27, x28, [sp, #8 *  8]
    ldp  x29, x30, [sp, #8 * 10]  /* x30 = return address into run_user caller */
    ldr  x1,       [sp, #8 * 12]  /* x1 = saved kernel sp before run_user frame */
    mov  sp, x1

    /* Return to RawUserContext::run() caller in Rust */
    ret


/*
 * run_user(ctx: *mut RawUserContext)    [extern "C"]
 *
 * Loads user registers from ctx and enters EL0 via eret.
 * Returns when an exception fires from EL0 (via __el0_exception_handler).
 *
 * x0 = ctx pointer on entry.
 */
.global run_user
run_user:
    /* Allocate 128-byte kernel frame */
    sub  sp, sp, #128

    /* Save callee-saved registers */
    stp  x19, x20, [sp, #8 *  0]
    stp  x21, x22, [sp, #8 *  2]
    stp  x23, x24, [sp, #8 *  4]
    stp  x25, x26, [sp, #8 *  6]
    stp  x27, x28, [sp, #8 *  8]
    stp  x29, x30, [sp, #8 * 10]

    /* Save kernel sp (before frame) and ctx pointer */
    add  x1, sp, #128
    str  x1, [sp, #8 * 12]   /* slot 12: kernel sp before sub */
    str  x0, [sp, #8 * 13]   /* slot 13: ctx pointer */

    /* Load user system regs from ctx */
    ldr  x1, [x0, #8 * 31]
    msr  elr_el1, x1           /* user PC */
    ldr  x1, [x0, #8 * 32]
    msr  spsr_el1, x1          /* user PSTATE */
    ldr  x1, [x0, #8 * 34]
    msr  sp_el0, x1            /* user stack pointer */

    /* Load user GP regs x2..x30 (load x0, x1 last) */
    ldp  x2, x3,   [x0, #8 *  2]
    ldp  x4, x5,   [x0, #8 *  4]
    ldp  x6, x7,   [x0, #8 *  6]
    ldp  x8, x9,   [x0, #8 *  8]
    ldp  x10, x11, [x0, #8 * 10]
    ldp  x12, x13, [x0, #8 * 12]
    ldp  x14, x15, [x0, #8 * 14]
    ldp  x16, x17, [x0, #8 * 16]
    ldp  x18, x19, [x0, #8 * 18]
    ldp  x20, x21, [x0, #8 * 20]
    ldp  x22, x23, [x0, #8 * 22]
    ldp  x24, x25, [x0, #8 * 24]
    ldp  x26, x27, [x0, #8 * 26]
    ldp  x28, x29, [x0, #8 * 28]
    ldr  x30,      [x0, #8 * 30]
    ldp  x0, x1,   [x0, #8 *  0]   /* load x0, x1 last; x0 no longer ctx ptr */

    /* Enter EL0 */
    eret
