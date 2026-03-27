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
    /*
     * ARM64 Linux Image header (64 bytes, offsets 0x00..0x3f).
     *
     * Present at the ELF entry point so that QEMU -machine virt recognises
     * this as an ARM64 Linux kernel and passes the generated FDT physical
     * address in x0 (per the AArch64 Linux boot protocol), rather than
     * leaving x0 as zero.
     *
     *   [0x00] code0       – branch past the header to _start_real
     *   [0x04] code1       – reserved (0)
     *   [0x08] text_offset – load offset from start of RAM (0x80000)
     *   [0x10] image_size  – effective kernel image size (0 = use file size)
     *   [0x18] flags       – kernel flags (0 = unspecified / little-endian)
     *   [0x20] res2        – reserved (0)
     *   [0x28] res3        – reserved (0)
     *   [0x30] res4        – reserved (0)
     *   [0x38] magic       – ARM64_IMAGE_MAGIC = 0x644d5241
     *   [0x3c] res5        – reserved (0)
     */
    b       _start_real              /* [0x00] code0 */
    .long   0                        /* [0x04] code1 */
    .quad   0x80000                  /* [0x08] text_offset (LMA − RAM base) */
    .quad   0                        /* [0x10] image_size */
    .quad   0                        /* [0x18] flags */
    .quad   0                        /* [0x20] res2 */
    .quad   0                        /* [0x28] res3 */
    .quad   0                        /* [0x30] res4 */
    .long   0x644d5241               /* [0x38] ARM64_IMAGE_MAGIC */
    .long   0                        /* [0x3c] res5 */

_start_real:
    /* AArch64 Linux boot protocol: x0 = DTB physical address, x1 = 0 (reserved).
     * Save to callee-saved registers before any debug macros clobber them. */
    mov     x19, x0
    mov     x20, x1
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
    /* Load boot page tables */
    /* TTBR0: identity map (low VA == PA, covers boot code at 0x40080000) */
    adr     x2, boot_l4pt
    msr     ttbr0_el1, x2
    /* TTBR1: separate kernel table mapping high VA → DRAM (PA 0x40000000+) */
    adr     x2, boot_l4pt_kern
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

    /* Set SP to boot_stack_top (now in DRAM at ~0x400c5dd0 with KERNEL_LMA=0x40080000).
     * adr is PC-relative so it computes the correct physical/identity-mapped address. */
    adr     x7, boot_stack_top
    mov     sp, x7

    /* Jump to virtual-address _start_virt */
    ldr     x8, =_start_virt
    blr     x8

    /* Should never return */
1:  wfe
    b       1b

/* --------------------------------------------------------------------------
 * Boot page tables (4 KB granule, 4 levels: PGD -> PUD -> PMD -> PTE)
 *
 * Two separate PGD tables:
 *   boot_l4pt      (TTBR0): identity map — VA == PA for the full 4 GB range.
 *     [0] -> boot_l3pt_low  (first 4 GB via PUD with 1 GB blocks / L2 for GB0)
 *
 *   boot_l4pt_kern (TTBR1): kernel high-half — VA 0xffff000000000000 → DRAM.
 *     [0] -> boot_l3pt_high (first 4 GB, remapped to start at PA 0x40000000)
 *
 * With KERNEL_LMA=0x40080000, kernel code/data sit in QEMU DRAM (PA 0x40000000+).
 * TTBR1 entry[0] maps the 1st GB of the high half directly to the DRAM 1 GB block
 * so that VA 0xffff000000XXXXXX → PA 0x40000000+XXXXXX (writable RAM).
 * -------------------------------------------------------------------------- */
.balign 4096
.globl boot_l4pt
boot_l4pt:
    .quad boot_l3pt_low + MM_TYPE_TABLE     /* slot 0: identity */
    .zero 8 * 255
    .quad boot_l3pt_high + MM_TYPE_TABLE    /* slot 256: high half */
    .zero 8 * (512 - 257)
/* PGD for TTBR1: kernel high-half maps to DRAM */
.balign 4096
.globl boot_l4pt_kern
boot_l4pt_kern:
    .quad boot_l3pt_high + MM_TYPE_TABLE    /* slot 0:   VA 0xffff000000000000 → DRAM */
    .zero 8 * 255                           /* slots 1-255: empty */
    .quad boot_l3pt_linear + MM_TYPE_TABLE  /* slot 256: VA 0xffff800000000000 → linear map */
    .zero 8 * (512 - 257)

/* L3 (PUD) for the low / identity half: first 4 GB */
.balign 4096
boot_l3pt_low:
    .quad boot_l2pt_gb0 + MM_TYPE_TABLE     /* 0x00000000–0x3FFFFFFF: via L2 */
    .quad ( PTE_NORMAL_1G | (1 << 30) )     /* 0x40000000–0x7FFFFFFF: RAM 1 GB block */
    .quad ( PTE_NORMAL_1G | (2 << 30) )     /* 0x80000000–0xBFFFFFFF */
    .quad ( PTE_NORMAL_1G | (3 << 30) )     /* 0xC0000000–0xFFFFFFFF */
    .zero 8 * (512 - 4)

/* L3 (PUD) for the kernel high half: maps to DRAM (PA + 0x40000000) */
.balign 4096
boot_l3pt_high:
    .quad ( PTE_NORMAL_1G | (1 << 30) )     /* VA[0–1GB)   → PA 0x40000000–0x7FFFFFFF (DRAM) */
    .quad ( PTE_NORMAL_1G | (2 << 30) )     /* VA[1–2GB)   → PA 0x80000000–0xBFFFFFFF */
    .quad ( PTE_NORMAL_1G | (3 << 30) )     /* VA[2–3GB)   → PA 0xC0000000–0xFFFFFFFF */
    .zero 8 * (512 - 3)

/*
 * L3 (PUD) for the linear mapping half (TTBR1 PGD slot 256).
 *
 * paddr_to_vaddr(pa) = pa + 0xffff800000000000.  To reach a physical address
 * in DRAM (0x40000000..0x60000000) through the linear mapping the VA is
 * 0xffff800040000000..0xffff800060000000 → PUD slot 1 → PA 0x40000000.
 *
 * We map all 4 GB of the 32‑bit PA space here (slots 0–3) so that memory
 * allocated during early boot (before activate_kernel_page_table) still
 * appears at its correct paddr_to_vaddr address.
 */
.balign 4096
boot_l3pt_linear:
    .quad ( PTE_NORMAL_1G | (0 << 30) )     /* slot 0: VA 0xffff800000000000 → PA 0x00000000 */
    .quad ( PTE_NORMAL_1G | (1 << 30) )     /* slot 1: VA 0xffff800040000000 → PA 0x40000000 (DRAM) */
    .quad ( PTE_NORMAL_1G | (2 << 30) )     /* slot 2: VA 0xffff800080000000 → PA 0x80000000 */
    .quad ( PTE_NORMAL_1G | (3 << 30) )     /* slot 3: VA 0xffff8000c0000000 → PA 0xC0000000 */
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
    /* Fix up SP: before enabling the MMU we set SP from an `adr` instruction,
     * which gives the physical / identity-mapped address of boot_stack_top
     * (somewhere around 0x400cXXXX).  Now that TTBR1 maps PA 0x40000000..
     * 0x7fffffff to VA 0xffff000000000000..0xffff00003fffffff, the same frame
     * is reachable at VA = PA - 0x40000000 + 0xffff000000000000.
     * Rust stack-variable addresses come from SP, so we MUST switch to the
     * high-VA stack pointer here before any Rust frame is allocated. */
    mov     x7, sp
    ldr     x8, =0x40000000
    sub     x7, x7, x8              /* SP - DRAM_BASE */
    ldr     x8, =0xffff000000000000
    add     sp, x7, x8              /* SP_virt = SP_phys - DRAM_BASE + HIGH_HALF */

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

    /* Restore x0 (DTB paddr, per AArch64 boot protocol) and x1 (reserved=0)
     * that were saved in x19/x20 at _start before debug macros clobbered them. */
    mov     x0, x19
    mov     x1, x20
    b       aarch64_boot
