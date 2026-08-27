/*
 * Diagnostic armstub8 - tests spin-table wake mechanism BEFORE U-Boot
 * 
 * This stub:
 * 1. Primary CPU: writes entry address to spin-table for each AP, then issues sev
 * 2. APs: poll spin-table, when entry is non-zero, jump to it
 * 3. If APs can wake and jump, they output 'AP' markers
 */
 
#define LOCAL_CONTROL 0x40000000
#define UART0_BASE    0x3F201000

putchar:
    ldr x1, =UART0_BASE
1:  ldr w3, [x1, #0x18]
    tbnz w3, #5, 1b
    str w0, [x1]
    ret

.globl _start
_start:
    /* Primary CPU - output 'P' */
    mov x0, #'P'
    bl putchar
    
    /* Get core ID */
    mrs x6, mpidr_el1
    and x6, x6, #0x3
    cbz x6, primary_cpu
    
    /* Secondary CPU - output core num, poll spin-table */
    mov x0, #'B'
    bl putchar
    add x0, x6, #'0'
    bl putchar
    mov x0, #' '
    bl putchar
    
    ldr x5, =LOCAL_CONTROL
    
spin_loop:
    /* Compute spin-table address: 0x40000000 + 0xD8 + core*8 */
    mov x7, x6
    lsl x7, x7, #3
    add x7, x7, #0xD8
    add x7, x5, x7
    
    ldr x4, [x7]
    cbz x4, wait
    
    /* Entry ready - output 'K' and jump */
    mov x0, #'K'
    bl putchar
    br x4
    
wait:
    wfe
    b spin_loop

primary_cpu:
    /* Set up spin-table for each secondary CPU */
    /* Address for CPU N = 0x40000000 + 0xD8 + N*8 */
    
    /* CPU 1 */
    ldr x7, =0x400000E0
    mov x0, #0x80000  /* Entry PA - simple loop for testing */
    str x0, [x7]
    
    /* CPU 2 */
    ldr x7, =0x400000E8
    mov x0, #0x80000
    str x0, [x7]
    
    /* CPU 3 */
    ldr x7, =0x400000F0
    mov x0, #0x80000
    str x0, [x7]
    
    /* Issue sev to wake all APs */
    sev
    
    /* Output done */
    mov x0, #'D'
    bl putchar
    mov x0, #'\n'
    bl putchar
    
    /* Jump to U-Boot (kernel_entry32) */
    ldr w4, kernel_entry32
    cbz w4, hang
    mov x0, #0
    mov x1, #0
    mov x2, #0
    mov x3, #0
    br x4
    
hang:
    b hang

.org 0xD8
spin_cpu0:  .quad 0
.org 0xE0
spin_cpu1:  .quad 0
.org 0xE8
spin_cpu2:  .quad 0
.org 0xF0
spin_cpu3:  .quad 0
.org 0xF8
dtb_ptr32:  .word 0x0
.org 0xFC
kernel_entry32: .word 0x0
