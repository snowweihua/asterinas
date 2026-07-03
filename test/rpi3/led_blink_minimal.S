/* SPDX-License-Identifier: MPL-2.0 */

/* Minimal RPi3 test: Just blink ACT LED forever.
 * This verifies that the GPU firmware loads kernel8.img and the ARM CPU
 * starts executing at PA 0x80000. No MMU, no page tables, no Rust code.
 * 
 * Build: aarch64-linux-gnu-as -o led_blink.o led_blink.S
 *        aarch64-linux-gnu-ld -Ttext=0x80000 -o led_blink.elf led_blink.o
 *        aarch64-linux-gnu-objcopy -O binary led_blink.elf kernel8_minimal.img
 */

.section ".boot", "awx", @progbits
.globl _start
_start:
    /* ARM64 Linux Image header */
    b       _start_real
    .long   0
    .quad   0x80000
    .quad   0
    .quad   0
    .quad   0
    .quad   0
    .quad   0
    .long   0x644d5241
    .long   0

_start_real:
    /* GPIO base = 0x3F200000 */
    ldr     x3, =0x3F200000
    
    /* Set GPIO 47 as output (GPFSEL4, field 7 = bits 21-23) */
    ldr     w4, [x3, #0x10]
    bic     w4, w4, #(0b111 << 21)
    orr     w4, w4, #(0b001 << 21)
    str     w4, [x3, #0x10]

    /* Infinite blink loop */
1:
    /* LED ON */
    mov     w4, #(1 << 15)
    str     w4, [x3, #0x20]
    
    /* Delay ~500ms */
    ldr     x5, =0x200000
2:  subs    x5, x5, #1
    bne     2b
    
    /* LED OFF */
    mov     w4, #(1 << 15)
    str     w4, [x3, #0x2C]
    
    /* Delay ~500ms */
    ldr     x5, =0x200000
3:  subs    x5, x5, #1
    bne     3b
    
    b       1b
