/* SPDX-License-Identifier: MPL-2.0 */

/* AArch64 early boot stub.
 *
 * Execution starts at _start (loaded at physical address 0x40080000 by QEMU
 * "virt" board via the kernel Image header).  The stub:
 *
 *   1. Drops from EL3 -> EL2 (if at EL3).
 *   2. Drops from EL2 -> EL1 (always).
 *   3. Sets up SCTLR_EL1 (MMU off).
 *   4. Configures TCR_EL1 / MAIR_EL1 / TTBR0/1_EL1 for a flat identity
 *      + high-half mirror via the boot page tables.
 *   5. Enables the MMU.
 *   6. Switches SP and PC to the virtual-address mirror.
 *   7. Jumps to `aarch64_boot` (Rust).
 */

KERNEL_VMA = 0xffff000000000000

/* --------------------------------------------------------------------------
 * Compile-time constants evaluated by the GNU assembler
 * -------------------------------------------------------------------------- */

/* SCR_EL3: NS=1, RES1=[5:4]=3, SMD=1, HCE=1, RW=1 */
SCR_VALUE = ( (1 << 0) | (3 << 4) | (1 << 7) | (1 << 8) | (1 << 10) )

/* SPSR when entering EL2 from EL3: FIQ/IRQ/SError masked, EL2h */
SPSR_EL3_VALUE = ( (7 << 6) | 0b1001 )
/* SPSR when entering EL1 from EL2: FIQ/IRQ/SError masked, EL1h */
SPSR_EL2_VALUE = ( (7 << 6) | 0b0101 )

/* SCTLR_EL1: mandatory RES1 bits + MMU disabled */
SCTLR_RES1            = ( (3 << 28) | (3 << 22) | (1 << 20) | (1 << 11) | (1 << 8) | (1 << 7) )
SCTLR_MMU_DISABLED    = SCTLR_RES1
SCTLR_MMU_ENABLED     = ( SCTLR_RES1 | 1 )

/* TCR_EL1: 48-bit VA both halves, 4 KB granule, inner-shareable,
 * write-back write-allocate caches, 32-bit IPA */
TCR_T0SZ  = 16
TCR_T1SZ  = (16 << 16)
TCR_TG0   = (0 << 14)
TCR_SH0   = (3 << 12)
TCR_ORG0  = (1 << 10)
TCR_IRG0  = (1 << 8)
TCR_TG1   = (2 << 30)
TCR_SH1   = (3 << 28)
TCR_ORG1  = (1 << 26)
TCR_IRG1  = (1 << 24)
TCR_IPS   = (0 << 32)
TCR_VALUE = ( TCR_IPS | TCR_TG1 | TCR_SH1 | TCR_ORG1 | TCR_IRG1 | TCR_T1SZ | TCR_TG0 | TCR_SH0 | TCR_ORG0 | TCR_IRG0 | TCR_T0SZ )

/* MAIR_EL1:
 *   Attr0: Device-nGnRnE 0x00
 *   Attr1: Normal, WB WA  0xFF
 *   Attr2: Normal, NC     0x44
 */
MAIR_VALUE = ( (0x00) | (0xFF << 8) | (0x44 << 16) )

/* Page-table entry bits */
MM_TYPE_BLOCK  = 1
MM_TYPE_TABLE  = 3
PTE_AIDX_DEVICE = (0 << 2)
PTE_AIDX_MEMORY = (1 << 2)
PTE_SH_INNER    = (3 << 8)
PTE_AF          = (1 << 10)

/* 1 GB block descriptors (used for GB 1-3 which are pure RAM or unused) */
PTE_NORMAL_1G  = ( MM_TYPE_BLOCK | PTE_AIDX_MEMORY | PTE_SH_INNER | PTE_AF )
/* 2 MB block descriptors for the MMIO region (0x00000000–0x3FFFFFFF) */
PTE_DEVICE_2M  = ( MM_TYPE_BLOCK | PTE_AIDX_DEVICE | PTE_AF )
/* 2 MB block descriptor for Normal RAM 2 MB blocks (0x40000000–0x7FFFFFFF) */
PTE_NORMAL_2M  = ( MM_TYPE_BLOCK | PTE_AIDX_MEMORY | PTE_SH_INNER | PTE_AF )

/* --------------------------------------------------------------------------
 * Boot section
 * -------------------------------------------------------------------------- */

/* Macro: write character in w0 to PL011 at 0x09000000 (no MMU, polled). */
.macro  early_uart_putchar char
    mov     w0, #\char
    ldr     x1, =0x09000000
1:  ldr     w2, [x1, #0x18]         /* FR register */
    tbnz    w2, #5, 1b              /* wait while TXFF=1 */
    str     w0, [x1, #0x00]         /* write to DR */
.endm

.section ".boot", "awx", @progbits
.globl _start
_start:
    /* Save QEMU's x0 (hart_id) and x1 (DTB paddr) to callee-saved registers
     * before any debug macros clobber them. */
    mov     x19, x0
    mov     x20, x1
    early_uart_putchar 'A'          /* debug: reached _start */
    mov     x0, x19
    mov     x1, x20
    /* Detect current exception level [3:2] of CurrentEL */
    mrs     x2, CurrentEL
    and     x2, x2, #0b1100
    cmp     x2, #0b1100
    beq     switch_to_el1_from_el3
    cmp     x2, #0b1000
    beq     switch_to_el1_from_el2
    b       jump_to_main           /* already EL1 */

/* --------------------------------------------------------------------------
 * EL3 -> EL2 -> EL1
 * -------------------------------------------------------------------------- */
switch_to_el1_from_el3:
    early_uart_putchar '3'          /* debug: EL3 path */
    ldr     x1, =SCR_VALUE
    msr     scr_el3, x1
    ldr     x2, =SPSR_EL3_VALUE
    msr     spsr_el3, x2
    adr     x3, switch_to_el1_from_el2
    msr     elr_el3, x3
    eret

/* --------------------------------------------------------------------------
 * EL2 -> EL1
 * -------------------------------------------------------------------------- */
switch_to_el1_from_el2:
    early_uart_putchar '2'          /* debug: EL2 path */
    /* Allow EL1/EL0 access to physical counter/timer registers */
    mrs     x0, cnthctl_el2
    orr     x0, x0, #3
    msr     cnthctl_el2, x0
    msr     cntvoff_el2, xzr

    /* HCR_EL2: RW=1 (EL1 in AArch64), SWIO=1 */
    mov     x1, #(1 << 31)
    orr     x1, x1, #(1 << 1)
    msr     hcr_el2, x1

    /* Ensure MMU disabled at EL1 on entry */
    ldr     x3, =SCTLR_MMU_DISABLED
    msr     sctlr_el1, x3

    ldr     x4, =SPSR_EL2_VALUE
    msr     spsr_el2, x4
    adr     x4, jump_to_main
    msr     elr_el2, x4
    eret

/* --------------------------------------------------------------------------
 * EL1: configure paging, enable MMU, jump to virtual addresses
 * -------------------------------------------------------------------------- */
jump_to_main:
    early_uart_putchar 'M'          /* debug: reached EL1 jump_to_main */
    /* Load boot page tables */
    adr     x2, boot_l4pt
    msr     ttbr0_el1, x2
    msr     ttbr1_el1, x2
    isb

    ldr     x3, =TCR_VALUE
    msr     tcr_el1, x3

    ldr     x4, =MAIR_VALUE
    msr     mair_el1, x4

    /* Invalidate TLBs before enabling MMU */
    dsb     ish
    tlbi    vmalle1
    dsb     ish
    isb

    /* Enable MMU */
    ldr     x5, =SCTLR_MMU_ENABLED
    msr     sctlr_el1, x5
    isb

    /* Switch to using the virtual address stack.
     * We cannot use ldr sp, =sym on AArch64 (no ADR pseudo for sp).
     * Instead: compute phys addr of boot_stack_top, then add KERNEL_VMA. */
    adr     x7, boot_stack_top
    ldr     x8, =KERNEL_VMA
    add     x7, x7, x8
    mov     sp, x7

    /* Jump to virtual-address _start_virt */
    early_uart_putchar 'V'          /* debug: about to jump to _start_virt */
    ldr     x8, =_start_virt
    blr     x8

    /* Should never return */
1:  wfe
    b       1b

/* --------------------------------------------------------------------------
 * Boot page tables (4 KB granule, 4 levels: PGD -> PUD -> PMD -> PTE)
 *
 * We need only two entries in PGD:
 *   [0]   -> boot_l3pt_low  (identity: VA == PA, first 4 GB)
 *   [256] -> boot_l3pt_high (high-half: VA 0xffff000000000000, first 4 GB)
 *
 * L3 (PGD) tables point to L2 (PUD) tables which either:
 *   - Use 1 GB block descriptors for RAM-only GBs (1-3), OR
 *   - Point to an L1 (PMD) table for the first GB which has mixed attrs:
 *       0x00000000–0x3FFFFFFF: Device-nGnRnE (MMIO: UART, etc.)
 *       0x40000000–0x7FFFFFFF: Normal WB WA  (QEMU RAM)
 * -------------------------------------------------------------------------- */
.balign 4096
.globl boot_l4pt
boot_l4pt:
    .quad boot_l3pt_low + MM_TYPE_TABLE     /* slot 0: identity */
    .zero 8 * 255
    .quad boot_l3pt_high + MM_TYPE_TABLE    /* slot 256: high half */
    .zero 8 * (512 - 257)

/* L3 (PUD) for the low / identity half: first 4 GB */
.balign 4096
boot_l3pt_low:
    .quad boot_l2pt_gb0 + MM_TYPE_TABLE     /* 0x00000000–0x3FFFFFFF: via L2 */
    .quad ( PTE_NORMAL_1G | (1 << 30) )     /* 0x40000000–0x7FFFFFFF: RAM 1 GB block */
    .quad ( PTE_NORMAL_1G | (2 << 30) )     /* 0x80000000–0xBFFFFFFF */
    .quad ( PTE_NORMAL_1G | (3 << 30) )     /* 0xC0000000–0xFFFFFFFF */
    .zero 8 * (512 - 4)

/* L3 (PUD) for the high half: same physical mapping as low */
.balign 4096
boot_l3pt_high:
    .quad boot_l2pt_gb0 + MM_TYPE_TABLE     /* 0x00000000–0x3FFFFFFF: via L2 */
    .quad ( PTE_NORMAL_1G | (1 << 30) )     /* 0x40000000–0x7FFFFFFF */
    .quad ( PTE_NORMAL_1G | (2 << 30) )     /* 0x80000000–0xBFFFFFFF */
    .quad ( PTE_NORMAL_1G | (3 << 30) )     /* 0xC0000000–0xFFFFFFFF */
    .zero 8 * (512 - 4)

/*
 * L2 (PMD) for the first 1 GB (0x00000000–0x3FFFFFFF), 2 MB per entry.
 *
 * Keep almost everything as Normal memory so early kernel text/rodata remain
 * executable and readable with expected behavior.  Override only the PL011
 * UART window (0x09000000–0x091FFFFF, entry index 72) to Device memory.
 */
.balign 4096
boot_l2pt_gb0:
    .set _mb2, 0
    .rept 512
    .if _mb2 == 0x09000000
    .quad ( PTE_DEVICE_2M | _mb2 )
    .else
    .quad ( PTE_NORMAL_2M | _mb2 )
    .endif
    .set _mb2, _mb2 + (1 << 21)
    .endr

/* --------------------------------------------------------------------------
 * Boot stack
 * -------------------------------------------------------------------------- */
.section ".boot.stack", "aw", @nobits
.balign 4096
.globl boot_stack_bottom
boot_stack_bottom:
    .skip 0x40000                           /* 256 KiB */
.globl boot_stack_top
boot_stack_top:

/* --------------------------------------------------------------------------
 * Virtual-address entry point (MMU now enabled)
 * -------------------------------------------------------------------------- */
.text
.globl _start_virt
_start_virt:
    early_uart_putchar 'R'          /* debug: reached _start_virt in high VMA */

    /* Initialize TPIDR_EL1 to &__cpu_local_start so that CPU-local accesses
     * work before the full CPU-local initialisation in Rust.  This mirrors
     * what RISC-V does with `la gp, __cpu_local_start`. */
.extern __cpu_local_start
    ldr     x9, =__cpu_local_start
    msr     tpidr_el1, x9

    /* Zero the BSS section so Rust globals start in a known state. */
.extern __bss
.extern __bss_end
    ldr     x10, =__bss
    ldr     x11, =__bss_end
    cmp     x10, x11
    bge     2f
1:  str     xzr, [x10], #8
    cmp     x10, x11
    blt     1b
2:

    early_uart_putchar 'B'          /* debug: TPIDR_EL1 set, about to call Rust */

    /* Restore x0 (hart_id) and x1 (DTB paddr) that were saved in x19/x20
     * at _start before the debug macros clobbered them. */
    mov     x0, x19
    mov     x1, x20
    b       aarch64_boot
