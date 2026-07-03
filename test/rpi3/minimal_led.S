/* SPDX-License-Identifier: MPL-2.0 */

/* Minimal RPi3 test kernel - Just turn LED on solid and loop forever.
 * This verifies the GPU firmware can load and execute kernel8.img.
 * 
 * Build: 
 *   # Assemble to ELF
 *   /root/.rustup/.../rust-lld -flavor gnu -Ttext=0x80000 -e _start -o test.elf test.S
 *   # Convert to raw binary
 *   /root/.rustup/.../rust-objcopy -O binary test.elf kernel8_minimal.img
 */

.section ".boot", "awx", @progbits
.globl _start

_start:
    /* ARM64 Linux Image header (64 bytes) */
    b       _start_real              /* [0x00] branch past header */
    .long   0                        /* [0x04] reserved */
    .quad   0x80000                  /* [0x08] text_offset = load offset */
    .quad   0                        /* [0x10] image_size = 0 (use file size) */
    .quad   0                        /* [0x18] flags */
    .quad   0                        /* [0x20] reserved */
    .quad   0                        /* [0x28] reserved */
    .quad   0                        /* [0x30] reserved */
    .long   0x644d5241               /* [0x38] ARM64_IMAGE_MAGIC */
    .long   0                        /* [0x3c] reserved */

_start_real:
    /* Save x0 (DTB pointer) */
    mov     x19, x0

    /* Turn ACT LED ON solid (GPIO 47 on RPi3) */
    /* GPIO base = 0x3F200000 */
    ldr     x3, =0x3F200000
    
    /* Set GPIO 47 as output: GPFSEL4 field 7 (bits 21-23) */
    ldr     w4, [x3, #0x10]           /* read GPFSEL4 */
    bic     w4, w4, #(0b111 << 21)    /* clear field 7 */
    orr     w4, w4, #(0b001 << 21)    /* set as output */
    str     w4, [x3, #0x10]
    
    /* Turn LED ON (GPSET1, bit 15 = GPIO 47) */
    mov     w4, #(1 << 15)
    str     w4, [x3, #0x20]

    /* Infinite loop */
1:  wfe
    b       1b
