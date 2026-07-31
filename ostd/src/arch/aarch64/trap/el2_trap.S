/* SPDX-License-Identifier: MPL-2.0 */

/*
 * EL2 (Hypervisor) trap handler for RPi3 debugging.
 *
 * Key design principles:
 * 1. Use helper functions for UART polling (no local label conflicts)
 * 2. ALWAYS poll UART LSR bit 6 (TX empty) before writing each character
 * 3. Dump ALL registers on exception entry
 */

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

.text
.balign 4

/*
 * UART putchar with polling - x0 = char to print
 * Clobbers x1, x2
 */
el2_uart_putc:
    movz   x1, #0x3F20, lsl #16
    movk   x1, #0x5054
el2_uart_wait:
    ldrb   w2, [x1]
    tbz    w2, #6, el2_uart_wait
    movz   x1, #0x3F20, lsl #16
    movk   x1, #0x5040
    strb   w0, [x1]
    ret x30

/*
 * Print newline
 */
el2_newline:
    stp x30, x1, [sp, #-16]!
    mov w0, #'\r'
    bl el2_uart_putc
    mov w0, #'\n'
    bl el2_uart_putc
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Print 64-bit hex number in x6
 */
el2_puthex:
    stp x30, x4, [sp, #-16]!
    mov x5, x6           // value to print
    mov w0, #'0'
    bl el2_uart_putc
    mov w0, #'x'
    bl el2_uart_putc
    mov x4, #60          // shift counter (60, 56, 52, ... 0)
el2_hex_loop:
    lsr x0, x5, x4
    and w0, w0, #0xf
    cmp w0, #10
    add w3, w0, #'0'
    add w0, w0, #('a' - 10)
    csel w0, w3, w0, lo
    bl el2_uart_putc
    subs x4, x4, #4
    b.ge el2_hex_loop
    bl el2_newline
    ldp x30, x4, [sp], #16
    ret x30

/*
 * Print string - x0 = string address
 */
el2_puts:
    stp x30, x6, [sp, #-16]!
    mov x6, x0            // string pointer
el2_puts_loop:
    ldrb w0, [x6], #1
    cbz w0, el2_puts_done
    bl el2_uart_putc
    b el2_puts_loop
el2_puts_done:
    ldp x30, x6, [sp], #16
    ret x30

/*
 * Helper: print " E=" then hex from x6
 */
el2_print_E:
    stp x30, x1, [sp, #-16]!
    mov w0, #'E'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " S=" then hex from x6
 */
el2_print_S:
    stp x30, x1, [sp, #-16]!
    mov w0, #'S'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " R=" then hex from x6
 */
el2_print_R:
    stp x30, x1, [sp, #-16]!
    mov w0, #'R'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " F=" then hex from x6
 */
el2_print_F:
    stp x30, x1, [sp, #-16]!
    mov w0, #'F'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " A=" then hex from x6
 */
el2_print_A:
    stp x30, x1, [sp, #-16]!
    mov w0, #'A'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " L=" then hex from x6
 */
el2_print_L:
    stp x30, x1, [sp, #-16]!
    mov w0, #'L'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " P=" then hex from x6
 */
el2_print_P:
    stp x30, x1, [sp, #-16]!
    mov w0, #'P'
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " xN=" (where N is 0-9 in w5) then hex from x6
 */
el2_print_xN:
    stp x30, x1, [sp, #-16]!
    mov w0, #'x'
    bl el2_uart_putc
    mov w0, w5
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Helper: print " xNM=" (where NM is two digits in w5,w4) then hex from x6
 */
el2_print_xNM:
    stp x30, x1, [sp, #-16]!
    mov w0, #'x'
    bl el2_uart_putc
    mov w0, w5
    bl el2_uart_putc
    mov w0, w4
    bl el2_uart_putc
    mov w0, #'='
    bl el2_uart_putc
    bl el2_puthex
    ldp x30, x1, [sp], #16
    ret x30

/*
 * Full EL2 exception handler - dumps ALL registers then spins
 */
el2_exception_handler:
    save_el2_context
    bl el2_newline
    bl el2_newline

    // Print "EL2 EXCEPTION"
    .Lel2_exc:
    .asciz "\r\n=== EL2 EXCEPTION ==="
    adr x0, .Lel2_exc
    bl el2_puts

    // ESR (offset 33)
    mov w0, #'E'
    bl el2_print_E
    mov w0, #'S'
    bl el2_print_S
    mov w0, #'R'
    bl el2_print_R
    ldr x6, [sp, #8 * 33]
    bl el2_puthex

    // ELR (offset 31)
    mov w0, #'E'
    bl el2_print_E
    mov w0, #'L'
    bl el2_print_L
    mov w0, #'R'
    bl el2_print_R
    ldr x6, [sp, #8 * 31]
    bl el2_puthex

    // SPSR (offset 32)
    mov w0, #'S'
    bl el2_print_S
    mov w0, #'P'
    bl el2_print_P
    mov w0, #'S'
    bl el2_print_S
    mov w0, #'R'
    bl el2_print_R
    ldr x6, [sp, #8 * 32]
    bl el2_puthex

    // FAR (offset 34)
    mov w0, #'F'
    bl el2_print_F
    mov w0, #'A'
    bl el2_print_A
    mov w0, #'R'
    bl el2_print_R
    ldr x6, [sp, #8 * 34]
    bl el2_puthex

    bl el2_newline

    // Dump all x registers
    .Lxregs_label:
    .asciz "\r\n=== REGS ==="
    adr x0, .Lxregs_label
    bl el2_puts

    // x0
    mov w5, #'0'
    ldr x6, [sp, #8 * 0]
    bl el2_print_xN

    // x1
    mov w5, #'1'
    ldr x6, [sp, #8 * 1]
    bl el2_print_xN

    // x2
    mov w5, #'2'
    ldr x6, [sp, #8 * 2]
    bl el2_print_xN

    // x3
    mov w5, #'3'
    ldr x6, [sp, #8 * 3]
    bl el2_print_xN

    // x4
    mov w5, #'4'
    ldr x6, [sp, #8 * 4]
    bl el2_print_xN

    // x5
    mov w5, #'5'
    ldr x6, [sp, #8 * 5]
    bl el2_print_xN

    // x6
    mov w5, #'6'
    ldr x6, [sp, #8 * 6]
    bl el2_print_xN

    // x7
    mov w5, #'7'
    ldr x6, [sp, #8 * 7]
    bl el2_print_xN

    // x8
    mov w5, #'8'
    ldr x6, [sp, #8 * 8]
    bl el2_print_xN

    // x9
    mov w5, #'9'
    ldr x6, [sp, #8 * 9]
    bl el2_print_xN

    // x10
    mov w5, #'1'
    mov w4, #'0'
    ldr x6, [sp, #8 * 10]
    bl el2_print_xNM

    // x11
    mov w5, #'1'
    mov w4, #'1'
    ldr x6, [sp, #8 * 11]
    bl el2_print_xNM

    // x12
    mov w5, #'1'
    mov w4, #'2'
    ldr x6, [sp, #8 * 12]
    bl el2_print_xNM

    // x13
    mov w5, #'1'
    mov w4, #'3'
    ldr x6, [sp, #8 * 13]
    bl el2_print_xNM

    // x14
    mov w5, #'1'
    mov w4, #'4'
    ldr x6, [sp, #8 * 14]
    bl el2_print_xNM

    // x15
    mov w5, #'1'
    mov w4, #'5'
    ldr x6, [sp, #8 * 15]
    bl el2_print_xNM

    // x16
    mov w5, #'1'
    mov w4, #'6'
    ldr x6, [sp, #8 * 16]
    bl el2_print_xNM

    // x17
    mov w5, #'1'
    mov w4, #'7'
    ldr x6, [sp, #8 * 17]
    bl el2_print_xNM

    // x18
    mov w5, #'1'
    mov w4, #'8'
    ldr x6, [sp, #8 * 18]
    bl el2_print_xNM

    // x19
    mov w5, #'1'
    mov w4, #'9'
    ldr x6, [sp, #8 * 19]
    bl el2_print_xNM

    // x20
    mov w5, #'2'
    mov w4, #'0'
    ldr x6, [sp, #8 * 20]
    bl el2_print_xNM

    // x21
    mov w5, #'2'
    mov w4, #'1'
    ldr x6, [sp, #8 * 21]
    bl el2_print_xNM

    // x22
    mov w5, #'2'
    mov w4, #'2'
    ldr x6, [sp, #8 * 22]
    bl el2_print_xNM

    // x23
    mov w5, #'2'
    mov w4, #'3'
    ldr x6, [sp, #8 * 23]
    bl el2_print_xNM

    // x24
    mov w5, #'2'
    mov w4, #'4'
    ldr x6, [sp, #8 * 24]
    bl el2_print_xNM

    // x25
    mov w5, #'2'
    mov w4, #'5'
    ldr x6, [sp, #8 * 25]
    bl el2_print_xNM

    // x26
    mov w5, #'2'
    mov w4, #'6'
    ldr x6, [sp, #8 * 26]
    bl el2_print_xNM

    // x27
    mov w5, #'2'
    mov w4, #'7'
    ldr x6, [sp, #8 * 27]
    bl el2_print_xNM

    // x28
    mov w5, #'2'
    mov w4, #'8'
    ldr x6, [sp, #8 * 28]
    bl el2_print_xNM

    // x29
    mov w5, #'2'
    mov w4, #'9'
    ldr x6, [sp, #8 * 29]
    bl el2_print_xNM

    // x30
    mov w5, #'3'
    mov w4, #'0'
    ldr x6, [sp, #8 * 30]
    bl el2_print_xNM

    bl el2_newline
    .Lhang:
    .asciz "### HALT ###"
    adr x0, .Lhang
    bl el2_puts

el2_spin:
    wfe
    b el2_spin

.section .vector.vector_table_el2, "ax"
.global vector_table_el2
.balign 0x800
vector_table_el2:

/* Write 'X'/'I'/'F'/'S' to UART immediately on vector entry */
.macro vector_serr
    movz x1, #0x3F20, lsl #16
    movk x1, #0x5040
    mov  w0, #83          // 'S' for SError
    strb w0, [x1]
    b el2_exception_handler
.endm

.macro vector_sync
    movz x1, #0x3F20, lsl #16
    movk x1, #0x5040
    mov  w0, #88          // 'X' for sync exception
    strb w0, [x1]
    b el2_exception_handler
.endm

.macro vector_irq
    movz x1, #0x3F20, lsl #16
    movk x1, #0x5040
    mov  w0, #73          // 'I' for IRQ
    strb w0, [x1]
    b el2_exception_handler
.endm

.macro vector_fiq
    movz x1, #0x3F20, lsl #16
    movk x1, #0x5040
    mov  w0, #70          // 'F' for FIQ
    strb w0, [x1]
    b el2_exception_handler
.endm

/* Current EL with SP0 */
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr

/* Current EL with SPx */
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr

/* Lower EL AArch64 */
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr

/* Lower EL AArch32 */
.balign 0x80; vector_sync
.balign 0x80; vector_irq
.balign 0x80; vector_fiq
.balign 0x80; vector_serr
