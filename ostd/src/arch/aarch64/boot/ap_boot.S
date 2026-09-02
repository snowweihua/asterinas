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
/* FIRST: Save x0 (context ID / PerApRawInfo pointer) to x19 */
    mov     x19, x0
    /* Write marker to confirm AP reached this code - use 0x41000 where BSP expects it */
    mov     x0, #0xABCD
    ldr     x1, =0x41000
    str     x0, [x1]

    /* AP unique marker - AP has entered boot stub (distinguish from BSP) */
    mov     x0, #0xCC
    bl      ap_putchar
    mov     x0, #0xDD
    bl      ap_putchar

    /* AP early marker - AP started */
    mov     x0, #0x41
    bl      ap_putchar
    mov     x0, #0x50
    bl      ap_putchar

    /* Set up page tables from global __boot_page_table_pointer.
     * pt_ptr is the BSP's TTBR1 root (boot_l4pt_kern), which maps:
     *   - VA 0xffff000000000000 + [0,1GB) -> DRAM  (kernel high-half, executable)
     *   - VA 0xffff800000000000 + [0,4GB) -> linear map
     * The stub itself runs at low identity addresses 0x344000 which boot_l4pt_kern
     * also maps (via boot_l3pt_high[0] -> boot_l2pt_gb0). So program BOTH TTBR0
     * and TTBR1 to this root. Previously only TTBR0 was set, so the branch to the
     * high-half VA of ap_early_entry used the stale TTBR1 left by firmware and
     * faulted — the AP never reached ap_early_entry. */
    ldr     x0, =__boot_page_table_pointer
    ldr     x0, [x0]                 /* Load page table root PA */
    msr     ttbr0_el1, x0
    msr     ttbr1_el1, x0            /* TTBR1: high-half kernel VAs (ap_early_entry) */

    /* Program TCR_EL1 and MAIR_EL1 explicitly.
     * Unlike the BSP, the AP never set these before enabling the MMU, so it
     * inherited whatever BL31/firmware left behind. If those differ from the
     * kernel's expectations (T1SZ=16/TG1=4KB, MAIR attr1=Normal-WBWA for
     * page-table walks), the MMU walk of the freshly-loaded TTBR0/TTBR1 can
     * fault right after MMU enable. Use the exact same values as boot.S. */
    ldr     x0, =TCR_VALUE
    msr     tcr_el1, x0
    ldr     x0, =MAIR_VALUE
    msr     mair_el1, x0
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

    /* Set up stack from PerApRawInfo.stack_top (x19 + 0).
     * stack_top is ALREADY a high-half virtual address (set by the BSP via
     * paddr_to_vaddr). It must NOT be converted again. */
    ldr     x0, [x19, #0]            /* Load stack_top (already VA) */
    mov     sp, x0

    /* Set up TPIDR_EL1 (CPU local base) from PerApRawInfo.cpu_local (x19 + 8).
     * cpu_local is also already a high-half VA. */
    ldr     x0, [x19, #8]            /* Load cpu_local (already VA) */
    msr     tpidr_el1, x0

    /* AP early marker - boot stub done, jumping to Rust */
    mov     x0, #0x44
    bl      ap_putchar
    mov     x0, #0x54
    bl      ap_putchar

    /* Marker XX - AP is about to branch to Rust ap_early_entry */
    mov     x0, #0x58
    bl      ap_putchar
    mov     x0, #0x58
    bl      ap_putchar

    /* Pass cpu_id to ap_early_entry as argument 0.
     * On RPi3, the logical cpu_id equals the MPIDR affinity level 0 (0-3).
     * Compute it here (AFTER all putchar calls, which clobber x0). */
    mrs     x0, mpidr_el1
    and     x0, x0, #0x3            /* x0 = cpu_id */

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
    /* PL011 UART DATA register at PA 0x3F201000, FR at +0x18 */
    ldr     x1, =0x3F201018
1:  ldr     x2, [x1]           /* Read UARTFR */
    and     x2, x2, #0x20       /* Check TXFF bit (bit 5) */
    cbnz    x2, 1b              /* If TXFF set, wait */
    ldr     x1, =0x3F201000
    str     x0, [x1]            /* Write character to UART DATA register */
    ret