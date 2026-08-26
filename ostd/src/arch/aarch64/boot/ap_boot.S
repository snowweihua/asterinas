/* SPDX-License-Identifier: MPL-2.0 */

/* AArch64 AP boot stub for SMP boot via PSCI CPU_ON.
 *
 * This code is placed in a special .ap_boot section that is loaded to a
 * known physical address. PSCI CPU_ON will jump to this address to wake
 * up secondary cores.
 *
 * Execution flow:
 *   1. AP starts at ap_boot_entry with MMU off, running on identity-mapped addresses
 *   2. Set up page table from __boot_page_table_pointer (global filled by BSP)
 *   3. Set up stack from PerApRawInfo (passed in x0 as context_id)
 *   4. Set up TPIDR_EL1 (CPU local base) from PerApRawInfo
 *   5. Enable MMU
 *   6. Switch to virtual addresses and jump to ap_early_entry Rust function
 */

.global ap_boot_entry
ap_boot_entry:
    /* AP early marker - AP started */
    mov     x0, #0x41
    bl      ap_putchar
    mov     x0, #0x50
    bl      ap_putchar

    /* Save x0 (context ID / PerApRawInfo pointer) to x19 */
    mov     x19, x0

    /* Set up page table from global __boot_page_table_pointer */
    ldr     x0, =__boot_page_table_pointer
    ldr     x0, [x0]                 /* Load page table root PA */
    msr     ttbr0_el1, x0
    isb

    /* Invalidate TLBs to ensure page table changes take effect */
    dsb     ish
    tlbi    vmalle1
    dsb     ish
    isb

    /* Clean+invalidate data caches and invalidate instruction cache
     * before enabling C/I bits in SCTLR. */
    bl      ap_flush_dcache_all
    ic      iallu
    dsb     nsh
    isb

    /* Enable MMU with early boot configuration (same as BSP boot) */
    /* SCTLR_EL1: mandatory RES1 bits + enable MMU and caches */
    ldr     x0, =SCTLR_MMU_ENABLED
    msr     sctlr_el1, x0
    isb

    /* AP early marker - after MMU enable */
    mov     x0, #0x52
    bl      ap_putchar
    mov     x0, #0x4D
    bl      ap_putchar

    /* Set up stack from PerApRawInfo.stack_top (x19 + 0) */
    ldr     x0, [x19, #0]            /* Load stack_top */
    mov     sp, x0

    /* Set up TPIDR_EL1 (CPU local base) from PerApRawInfo.cpu_local (x19 + 8) */
    ldr     x0, [x19, #8]            /* Load cpu_local */
    msr     tpidr_el1, x0

    /* Now switch SP to virtual address */
    /* Stack was set up with identity-mapped address, need to convert to VA */
    mov     x1, sp
    ldr     x0, =0x40000000
    sub     x1, x1, x0               /* SP - DRAM_BASE */
    ldr     x0, =0xffff000000000000
    add     sp, x1, x0               /* SP_virt = SP_phys - DRAM_BASE + HIGH_HALF */

    /* Zero BSS for this CPU */
.extern __bss
.extern __bss_end
    ldr     x0, =__bss
    ldr     x1, =__bss_end
    cmp     x0, x1
    bge     2f
1:  str     xzr, [x0], #8
    cmp     x0, x1
    blt     1b
2:
    /* AP early marker - after BSS zero */
    mov     x0, #0x42
    bl      ap_putchar
    mov     x0, #0x53
    bl      ap_putchar

    /* x19 still has PerApRawInfo pointer - pass it as argument to ap_early_entry */
    mov     x0, x19

    /* AP early marker - before jump to Rust */
    mov     x0, #0x44
    bl      ap_putchar
    mov     x0, #0x54
    bl      ap_putchar

    /* Jump to Rust ap_early_entry (now using VA) */
    ldr     x1, =ap_early_entry
    br      x1

/* Data section for the PerApRawInfo pointer - filled by BSP */
.global __ap_boot_info_array_pointer
.align 8
__ap_boot_info_array_pointer:
    .quad   0

/* Cache maintenance helpers for AP boot (duplicate of BSP version). */
ap_flush_dcache_all:
    dsb     sy
    mrs     x10, clidr_el1
    ubfx    x11, x10, #24, #3       /* x11 = LoC */
    cbz     x11, 5f
    mov     x15, x30                 /* save lr */
    mov     x16, xzr                 /* no FEAT_CCIDX */
    mov     x0, xzr

2:  add     x12, x0, x0, lsl #1
    lsr     x12, x10, x12
    and     x12, x12, #7
    cmp     x12, #2
    b.lt    4f
    bl      ap_flush_dcache_level
4:  add     x0, x0, #1
    cmp     x11, x0
    b.gt    2b
    mov     x0, xzr
    msr     csselr_el1, x0
    dsb     sy
    isb
    mov     x30, x15
5:  ret

ap_flush_dcache_level:
    lsl     x12, x0, #1
    msr     csselr_el1, x12
    isb
    mrs     x6, ccsidr_el1
    ubfx    x2, x6, #0, #3
    ubfx    x3, x6, #3, #10
    ubfx    x4, x6, #13, #15
    add     x2, x2, #4
    clz     w5, w3

6:  mov     x6, x3
7:  lsl     x7, x6, x5
    orr     x9, x12, x7
    lsl     x7, x4, x2
    orr     x9, x9, x7
    dc      cisw, x9
    subs    x6, x6, #1
    b.ge    7b
    subs    x4, x4, #1
    b.ge    6b
    ret

/* Page table root pointer - filled by BSP before kicking APs */
.global __boot_page_table_pointer
.align 8
__boot_page_table_pointer:
    .quad   0

/* Simple UART putchar for AP early debug - writes to PL011 at PA 0x3F201000 */
ap_putchar:
    /* x0 = character to print */
    /* PL011 UART DATA register at PA 0x3F201000 */
    ldr     x1, =0x3F201000
    /* Write character to UART DATA register */
    str     x0, [x1]
    ret