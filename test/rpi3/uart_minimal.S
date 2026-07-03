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
    /* Save DTB pointer */
    mov     x19, x0

    /* GPIO base = 0x3F200000 */
    ldr     x3, =0x3F200000

    /* Set GPIO 14/15 to Alt0 (PL011 UART) */
    ldr     w4, [x3, #0x04]        /* GPFSEL1 */
    bic     w4, w4, #(0b111 << 12)  /* clear GPIO 14 */
    bic     w4, w4, #(0b111 << 15)  /* clear GPIO 15 */
    orr     w4, w4, #(0b100 << 12)  /* GPIO 14 = Alt0 */
    orr     w4, w4, #(0b100 << 15)  /* GPIO 15 = Alt0 */
    str     w4, [x3, #0x04]

    /* Disable pull-up/down on GPIO 14/15 (BCM2837 method) */
    mov     w4, #0                   /* PUD = Off */
    str     w4, [x3, #0x94]         /* GPPUD */
    ldr     x5, =0x200000
1:  subs    x5, x5, #1
    bne     1b
    mov     w4, #(0b11 << 14)        /* PUDCLK14 + PUDCLK15 */
    str     w4, [x3, #0x98]         /* GPPUDCLK0 */
    ldr     x5, =0x200000
2:  subs    x5, x5, #1
    bne     2b
    mov     w4, #0
    str     w4, [x3, #0x94]
    str     w4, [x3, #0x98]

    /* Initialize PL011 UART at 0x3F201000 */
    ldr     x6, =0x3F201000         /* UART base */

    /* Wait for UART not busy */
3:  ldr     w4, [x6, #0x18]         /* UARTFR */
    mov     w5, #0x8                /* BUSY bit */
    tst     w4, w5
    bne     3b

    /* Disable UART */
    mov     w4, #0
    str     w4, [x6, #0x30]         /* UARTCR */

    /* Clear pending interrupts */
    mov     w4, #0x7FF
    str     w4, [x6, #0x44]         /* UARTICR */

    /* Set baud rate: IBRD=3, FBRD=16 for 921600 @ 48MHz */
    mov     w4, #3
    str     w4, [x6, #0x24]         /* UARTIBRD */
    mov     w4, #16
    str     w4, [x6, #0x28]         /* UARTFBRD */

    /* Set line control: 8 bits, FIFO enabled */
    mov     w4, #0x70
    str     w4, [x6, #0x2C]         /* UARTLCR_H */

    /* Enable UART, TX, RX */
    mov     w4, #0x301
    str     w4, [x6, #0x30]         /* UARTCR */

    /* Output "Hello RPi3\n" */
    adr     x7, hello_str
    mov     x8, #12                 /* length */

4:  ldrb    w9, [x7], #1
    /* Wait for TX FIFO not full */
5:  ldr     w4, [x6, #0x18]         /* UARTFR */
    mov     w5, #0x20               /* TXFF bit */
    tst     w4, w5
    bne     5b
    str     w9, [x6]                /* UARTDR */
    subs    x8, x8, #1
    bne     4b

    /* Blink LED forever */
    ldr     x3, =0x3F200000
    /* Set GPIO 47 as output */
    ldr     w4, [x3, #0x10]
    bic     w4, w4, #(0b111 << 21)
    orr     w4, w4, #(0b001 << 21)
    str     w4, [x3, #0x10]

6:
    /* LED ON */
    mov     w4, #(1 << 15)
    str     w4, [x3, #0x20]
    ldr     x5, =0x200000
7:  subs    x5, x5, #1
    bne     7b

    /* LED OFF */
    mov     w4, #(1 << 15)
    str     w4, [x3, #0x2C]
    ldr     x5, =0x200000
8:  subs    x5, x5, #1
    bne     8b

    b       6b

hello_str:
    .ascii  "Hello RPi3\n"
    .align  2
