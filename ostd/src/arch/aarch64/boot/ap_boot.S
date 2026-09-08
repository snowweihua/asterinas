/* SPDX-License-Identifier: MPL-2.0 */

/* AArch64 AP boot stub for SMP boot via PSCI CPU_ON.
 *
 * Execution flow:
 *   1. AP starts at ap_boot_entry at EL2 (set by ATF/BL31) with MMU off
 *   2. Drop from EL2 to EL1 via HCR_EL2 + ERET
 *   3. Set up EL1 page tables (TTBR0/1, TCR, MAIR)
 *   4. Enable MMU on EL1
 *   5. Set up stack and TPIDR_EL1
 *   6. Jump to ap_early_entry Rust function
 */

.equ KERNEL_CODE_BASE_VADDR, 0xffff000000000000

/* SCR_EL3 for the EL3->EL2 drop below (mirrors boot.S SCR_VALUE):
 * NS=1, RES1=[5:4]=3, SMD=1, HCE=1, RW=1 (EL2 in AArch64). */
.equ AP_SCR_VALUE, ((1 << 0) | (3 << 4) | (1 << 7) | (1 << 8) | (1 << 10))
/* SPSR to enter EL2h with DAIF masked (mirrors boot.S SPSR_EL3_VALUE). */
.equ AP_SPSR_EL3_VALUE, ((7 << 6) | 0b1001)

.global ap_boot_entry
ap_boot_entry:
/* FIRST: Save x0 (context ID / PerApRawInfo pointer) to x19.
 * PSCI CPU_ON passes the context ID in x0; QEMU's spin-table stub zeroes
 * x0-x3 before jumping, so derive the pointer from the BSP-filled global
 * array instead (PerApRawInfo is two pointers = 16 bytes, indexed by
 * affinity-1; see ostd/src/boot/smp.rs and update this if it changes). */
    mov     x19, x0
    cbnz    x19, ap_info_ok
    mrs     x19, mpidr_el1
    and     x19, x19, #0x3
    sub     x19, x19, #1
    lsl     x19, x19, #4
    adr     x20, __ap_boot_info_array_pointer
    ldr     x20, [x20]
    add     x19, x20, x19
ap_info_ok:
    /* Write marker to confirm AP reached this code - use 0x41000 where BSP expects it */
    mov     x0, #0xABCD
    ldr     x1, =0x41000
    str     x0, [x1]

    /* Per-CPU entry marker to PA (0x42000 + cpu_id*0x1000).
     * BSP polls these after the wake attempts. */
    mrs     x1, mpidr_el1
    and     x1, x1, #0x3
    add     x2, x1, #0x42
    lsl     x2, x2, #12
    add     x1, x1, #0x50
    str     w1, [x2]

    /* Checkpoint marker (cpu_id+0x60) at PA (0x46000 + cpu*0x1000). */
    mrs     x1, mpidr_el1
    and     x1, x1, #0x3
    add     x2, x1, #0x46
    lsl     x2, x2, #12
    add     x1, x1, #0x60
    str     w1, [x2]

    /* QEMU starts secondaries at EL3 (no secure firmware drops them);
     * PSCI/TF-A starts them at EL2. Drop EL3->EL2 first when needed. */
    mrs     x20, CurrentEL
    lsr     x20, x20, #2
    cmp     x20, #3
    b.ne    ap_el2_start
    ldr     x20, =AP_SCR_VALUE
    msr     SCR_EL3, x20
    ldr     x20, =AP_SPSR_EL3_VALUE
    msr     SPSR_EL3, x20
    adr     x20, ap_el2_start
    msr     ELR_EL3, x20
    isb
    eret
ap_el2_start:
    /* ======================================================================
     * EL2 -> EL1 drop.
     *
     * ATF/BL31 brings up secondary cores at EL2. All the page-table and
     * MMU registers we need to program (TTBR0_EL1, TCR_EL1, SCTLR_EL1, …)
     * are EL1 registers — they are *accessible* from EL2 but they govern
     * EL1's translation regime.  While the CPU is at EL2, its own
     * accesses go through EL2's translation (which is unconfigured / off),
     * so the identity VA==PA mapping we set up in TTBR0_EL1 is invisible
     * to EL2.  Any high-half VA (0xffff_…) fetched by the CPU while at
     * EL2 would be treated as a physical address, which is far beyond
     * physical RAM → synchronous external abort → silent firmware loop.
     *
     * Fix: drop to EL1 immediately, *then* configure and enable MMU.
     * ====================================================================== */

    /* HCR_EL2.RW = 1: tell EL2 that EL1 runs AArch64 (not AArch32).
     * Read-modify-write to preserve any other bits the firmware set. */
    mrs     x0, HCR_EL2
    orr     x0, x0, #(1 << 31)      /* RW bit */
    msr     HCR_EL2, x0

    /* Set up EL1 system registers (accessible from EL2).
     * These have no effect until we actually enter EL1. */
    adr     x0, __boot_page_table_pointer
    ldr     x0, [x0]                /* Load page table root PA */
    msr     ttbr0_el1, x0
    msr     ttbr1_el1, x0

    ldr     x0, =TCR_VALUE
    msr     tcr_el1, x0
    ldr     x0, =MAIR_VALUE
    msr     mair_el1, x0
    isb

    /* Checkpoint after TTBR loads: marker (cpu_id+0xa0) at PA (0x60000 + cpu*0x1000). */
    mrs     x1, mpidr_el1
    and     x1, x1, #0x3
    add     x2, x1, #0x60
    lsl     x2, x2, #12
    add     x1, x1, #0xa0
    str     w1, [x2]

    /* Prepare ERET to EL1h:
     *   SPSR_EL2: M[3:0]=0101 (EL1h), DAIF=1111 (all masked), M[4]=0 (AArch64)
     *   ELR_EL2:  continuation address (physical, since EL1 MMU is still off) */
    ldr     x0, =0x3C5
    msr     SPSR_EL2, x0
    adr     x0, ap_el1_entry
    msr     ELR_EL2, x0
    isb
    eret

ap_el1_entry:
    /* Now executing at EL1h with MMU still off.
     * All accesses use physical addresses.
     * EL1 system registers are now "live". */

    /* Invalidate TLBs so freshly-loaded TTBR take effect */
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

    /* Enable MMU with early boot configuration (same as BSP boot).
     * After MSR + ISB, the CPU begins using EL1's page tables. */
    ldr     x0, =SCTLR_MMU_ENABLED
    msr     sctlr_el1, x0
    isb

    /* AP early marker - after MMU enable */
    /* Set up stack from PerApRawInfo.stack_top (x19 + 0).
     * stack_top is ALREADY a high-half virtual address (set by the BSP via
     * paddr_to_vaddr). It must NOT be converted again. */
    ldr     x0, [x19, #0]            /* Load stack_top (already VA) */
    mov     sp, x0

    /* Set up TPIDR_EL1 (CPU local base) from PerApRawInfo.cpu_local (x19 + 8).
     * cpu_local is also already a high-half VA. */
    ldr     x0, [x19, #8]            /* Load cpu_local (already VA) */
    msr     tpidr_el1, x0

    /* Pass cpu_id to ap_early_entry as argument 0.
     * On RPi3, the logical cpu_id equals the MPIDR affinity level 0 (0-3).
     * Compute it here from MPIDR. */
    mrs     x0, mpidr_el1
    and     x0, x0, #0x3            /* x0 = cpu_id */

    /* Write CurrentEL to DRAM s3 (base+12) so BSP can confirm exception level. */
    mrs     x8, CurrentEL
    lsr     x8, x8, #2
    mov     x9, #0x68
    lsl     x9, x9, #12
    add     x9, x9, x0, lsl #12
    str     w8, [x9, #12]           /* s3 = CurrentEL >> 2 */

    /* Jump to Rust ap_early_entry via its high-half VA (mapped by TTBR1). */
    ldr     x1, =ap_early_entry
    /* Checkpoint before Rust branch: marker (cpu_id+0xb0) at PA (0x64000 + cpu*0x1000). */
    add     x2, x0, #0xb0
    mov     x3, #0x64
    lsl     x3, x3, #12
    add     x3, x3, x0, lsl #12
    str     w2, [x3]

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
