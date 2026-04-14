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

    /* Enable MMU with early boot configuration (same as BSP boot) */
    /* SCTLR_EL1: mandatory RES1 bits + enable MMU */
    ldr     x0, =SCTLR_MMU_ENABLED
    msr     sctlr_el1, x0
    isb

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

    /* x19 still has PerApRawInfo pointer - pass it as argument to ap_early_entry */
    mov     x0, x19

    /* Jump to Rust ap_early_entry (now using VA) */
    ldr     x1, =ap_early_entry
    br      x1

/* Data section for the PerApRawInfo pointer - filled by BSP */
.global __ap_boot_info_array_pointer
.align 8
__ap_boot_info_array_pointer:
    .quad   0

/* Page table root pointer - filled by BSP before kicking APs */
.global __boot_page_table_pointer
.align 8
__boot_page_table_pointer:
    .quad   0