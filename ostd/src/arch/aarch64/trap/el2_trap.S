/* SPDX-License-Identifier: MPL-2.0 */

.macro save_el2_context
    stp x0,  x1,  [sp, #-(8 * 38)]!
    stp x2,  x3,  [sp, #8 *  2]
    stp x4,  x5,  [sp, #8 *  4]
    stp x6,  x7,  [sp, #8 *  6]
    stp x8,  x9,  [sp, #8 *  8]
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
    str x30,      [sp, #8 * 30]
    mrs x0, elr_el2
    mrs x1, spsr_el2
    mrs x2, esr_el2
    mrs x3, far_el2
    stp x0,  x1,  [sp, #8 * 31]
    stp x2,  x3,  [sp, #8 * 33]
.endm

.macro el2_putc
    movz   x1, #0x3F20, lsl #16
    movk   x1, #0x1000
    strb   w0, [x1]
.endm

.text
.balign 4

el2_puts:
    mov    x2, x30
    mov    x3, x0
el2_puts_loop:
    ldrb   w0, [x3], #1
    cbz    w0, el2_puts_done
    el2_putc
    b      el2_puts_loop
el2_puts_done:
    ret    x2

el2_puthex:
    mov    x3, x30
    mov    x2, x0
    mov    w0, #'0'
    el2_putc
    mov    w0, #'x'
    el2_putc
    mov    w1, #60
el2_puthex_loop:
    lsr    x0, x2, x1
    and    w0, w0, #0xf
    cmp    w0, #10
    add    w4, w0, #'0'
    add    w0, w0, #('a' - 10)
    csel   w0, w4, w0, lo
    el2_putc
    subs   w1, w1, #4
    b.pl   el2_puthex_loop
    mov    w0, #'\n'
    el2_putc
    ret    x3

el2_msg_sync:  .asciz "\r\n[EL2] SYNC  ESR="
el2_msg_irq:   .asciz "\r\n[EL2] IRQ   ESR="
el2_msg_fiq:   .asciz "\r\n[EL2] FIQ   ESR="
el2_msg_serr:  .asciz "\r\n[EL2] SERR  ESR="
el2_msg_esr:   .asciz " ESR="
el2_msg_elr:   .asciz " ELR="
el2_msg_far:   .asciz " FAR="
.balign 4

el2_exception_handler:
    save_el2_context
    bl     el2_puts
    adr    x0, el2_msg_esr
    bl     el2_puts
    ldr    x0, [sp, #8 * 33]
    bl     el2_puthex
    adr    x0, el2_msg_elr
    bl     el2_puts
    ldr    x0, [sp, #8 * 31]
    bl     el2_puthex
    adr    x0, el2_msg_far
    bl     el2_puts
    ldr    x0, [sp, #8 * 34]
    bl     el2_puthex
el2_spin:
    wfe
    b      el2_spin

.section .vector.vector_table_el2, "ax"
.global vector_table_el2
.balign 0x800
vector_table_el2:

.macro vector_serr
    movz x1, #0x3F20, lsl #16
    movk x1, #0x1000
    mov  w0, #83
    strb w0, [x1]
    adr  x0, el2_msg_serr
    b    el2_exception_handler
.endm

.macro vector_sync
    movz x1, #0x3F20, lsl #16
    movk x1, #0x1000
    mov  w0, #89
    strb w0, [x1]
    adr  x0, el2_msg_sync
    b    el2_exception_handler
.endm

.macro vector_irq
    movz x1, #0x3F20, lsl #16
    movk x1, #0x1000
    mov  w0, #73
    strb w0, [x1]
    adr  x0, el2_msg_irq
    b    el2_exception_handler
.endm

.macro vector_fiq
    movz x1, #0x3F20, lsl #16
    movk x1, #0x1000
    mov  w0, #70
    strb w0, [x1]
    adr  x0, el2_msg_fiq
    b    el2_exception_handler
.endm

.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr
