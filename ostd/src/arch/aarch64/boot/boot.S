/* SPDX-License-Identifier: MPL-2.0 */

/* AArch64 early boot stub.
 *
 * Execution starts at _start.  The ARM64 Image header carries text_offset=0x80000,
 * so U-Boot's booti places the kernel at DRAM_BASE+0x80000:
 *   - QEMU virt: DRAM_BASE=0x40000000 → loaded at PA 0x40080000
 *   - RPi3 3B:   DRAM_BASE=0x00000000 → loaded at PA 0x00080000
 * Platform detection via PC at _start_real sets x21=DRAM_BASE and patches the
 * boot page tables so the same binary works on both boards.  The stub:
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
/* Enable MMU, data cache (C, bit 2) and instruction cache (I, bit 12).
 * The Cortex-A53 on RPi3 does not support LDXR/STXR to non-cacheable
 * memory; the exclusive monitor needs C and I enabled. */
SCTLR_MMU_ENABLED     = ( SCTLR_RES1 | (1 << 12) | (1 << 2) | 1 )

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

/* Macro: write character to PL011 at 0x09000000 (QEMU, no MMU, polled).
 * For RPi3 early output, pl011_puts_asm in mod.rs is used after MMU is on. */
.macro  early_uart_putchar char
    mov     w0, #\char
    ldr     x1, =0x09000000
1:  ldr     w2, [x1, #0x18]         /* FR register */
    tbnz    w2, #5, 1b              /* wait while TXFF=1 */
    str     w0, [x1, #0x00]         /* write to DR */
.endm

/* Macro: write a single ASCII character to RPi3 mini-UART (0x3F215040).
 * Uses movz/movk to build addresses without any literal pool.
 * Only executes when x21 == 0 (RPi3). */
.macro  rpi3_putch char
    cbnz    x21, 9998f
    movz    x26, #0x3F21, lsl #16
    movk    x26, #0x5040
    mov     w27, #\char
    str     w27, [x26]
9998:
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
    .quad   0x80000                  /* [0x08] text_offset (LMA − RAM base; QEMU: 0x40080000, RPi3: 0x80000) */
    .quad   0                        /* [0x10] image_size */
    .quad   0                        /* [0x18] flags */
    .quad   0                        /* [0x20] res2 */
    .quad   0                        /* [0x28] res3 */
    .quad   0                        /* [0x30] res4 */
    .long   0x644d5241               /* [0x38] ARM64_IMAGE_MAGIC */
    .long   0                        /* [0x3c] res5 */

_start_real:
    /* AArch64 Linux boot protocol: x0 = DTB physical address, x1 = 0 (reserved).
     * Save to callee-saved registers immediately before anything clobbers them. */
    mov     x19, x0
    mov     x20, x1

    /* PC-based platform detection: determine DRAM base for this platform.
     *   PC < 0x40000000 → RPi3 (booti relocated us to DRAM_BASE+0x80000 = 0x80000)
     *   PC >= 0x40000000 → QEMU virt (kernel loaded at DRAM_BASE+0x80000 = 0x40080000)
     * Store DRAM_BASE in x21; survives EL-switch eret and is used in:
     *   - jump_to_main: patch boot_l3pt_high[0] for correct high-VA → PA mapping
     *   - _start_virt: fix up SP from physical to virtual address */
    adr     x21, .                          /* x21 = current PC (physical) */
    ldr     x22, =0x40000000               /* QEMU/RPi4 DRAM boundary */
    cmp     x21, x22
    csel    x21, xzr, x22, lo              /* RPi3: x21 = 0; QEMU: x21 = 0x40000000 */

    /* DEBUG MARKER 'A': verify _start_real executes on RPi3.
     * First re-enable the PL011 UART (U-Boot cleanup may have disabled it),
     * then output 'A'.  Only runs when x21=0 (RPi3). */
    /* No UART init needed — U-Boot left both UARTs enabled.
     * rpi3_putch writes to both PL011 and mini-UART. */
.Lno_rpi3_uart_init:
    rpi3_putch 'A'

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

    /* Install our own EL2 exception vectors.  The BCM2837 routes AXI
     * bus errors directly to EL2 (bypassing HCR_EL2.AMO=0).  If we
     * do not replace U-Boot's VBAR_EL2 here, U-Boot's handler will
     * print "Synchronous Abort" and reset the CPU on any external
     * abort.  Our handler (vector_table_el2) prints ESR/ELR/FAR and
     * spins, giving us useful diagnostic output.
     *
     * ldr = loads the 64-bit virtual address from a literal pool
     * (adr cannot span the VA gap between .boot and .vector sections). */
    ldr     x9, =vector_table_el2
    msr     vbar_el2, x9
    isb

    /* Ensure MMU disabled at EL1 on entry */
    ldr     x3, =SCTLR_MMU_DISABLED
    msr     sctlr_el1, x3

    /* CPACR_EL1: FPEN=0b11 → no FP/SIMD trap at EL0 or EL1.
     * Must set this before EL1 code runs; Rust release builds can
     * emit SIMD/FP instructions, which otherwise fault with EC=0x07. */
    mov     x3, #(3 << 20)
    msr     cpacr_el1, x3
    isb

    /* CPTR_EL2: clear TFP (bit 10) so FP/SIMD from EL1 is not trapped to EL2. */
    mrs     x3, cptr_el2
    bic     x3, x3, #(1 << 10)
    msr     cptr_el2, x3
    isb

    ldr     x4, =SPSR_EL2_VALUE
    msr     spsr_el2, x4
    adr     x4, jump_to_main
    msr     elr_el2, x4
    eret

/* --------------------------------------------------------------------------
 * EL1: configure paging, enable MMU, jump to virtual addresses
 * -------------------------------------------------------------------------- */
jump_to_main:
    /* DEBUG MARKER 'B': at EL1, before page-table patching or MMU enable. */
    rpi3_putch 'B'

    /* -----------------------------------------------------------------------
     * Patch ALL page-table pointer entries with runtime physical addresses.
     *
     * The static .quad entries in the page tables below use LINKER-TIME
     * addresses (≈ LMA + offset).  With the QEMU linker script LMA=0x40080000
     * but the RPi3 loading the image at 0x80000, those linker-time addresses
     * point 0x40000000 bytes past the actual runtime location — outside RPi3
     * DRAM.  We use ADR (PC-relative, always gives the RUNTIME PA) to compute
     * each table's actual physical address and store it into the parent entry.
     *
     * x21 = DRAM_BASE (0x00000000 for RPi3, 0x40000000 for QEMU virt)
     *        set in _start_real by PC-range comparison.
     *
     * Tables patched:
     *   boot_l4pt[0]      → runtime PA of boot_l3pt_low   (TTBR0 identity PGD)
     *   boot_l4pt[256]    → runtime PA of boot_l3pt_high  (TTBR0 high-half PGD)
     *   boot_l4pt_kern[0] → runtime PA of boot_l3pt_high  (TTBR1 kernel PGD)
     *   boot_l4pt_kern[256]→ runtime PA of boot_l3pt_linear(TTBR1 linear PGD)
     *   boot_l3pt_low[0]  → runtime PA of boot_l2pt_gb0   (identity PMD for GB0)
     *   boot_l3pt_high[0] → PTE_NORMAL_1G | DRAM_BASE     (kernel high-half → DRAM)
     * ----------------------------------------------------------------------- */

    /* boot_l4pt[0] = runtime PA(boot_l3pt_low) | MM_TYPE_TABLE */
    adr     x9,  boot_l3pt_low
    add     x9,  x9, #MM_TYPE_TABLE
    adr     x10, boot_l4pt
    str     x9,  [x10]

    /* boot_l4pt[256] = runtime PA(boot_l3pt_high) | MM_TYPE_TABLE */
    adr     x9,  boot_l3pt_high
    add     x9,  x9, #MM_TYPE_TABLE
    add     x10, x10, #(256 * 8)
    str     x9,  [x10]

    /* boot_l4pt_kern[0] = same boot_l3pt_high PA (x9 still valid) */
    adr     x10, boot_l4pt_kern
    str     x9,  [x10]

    /* boot_l4pt_kern[256] = runtime PA(boot_l3pt_linear) | MM_TYPE_TABLE */
    adr     x9,  boot_l3pt_linear
    add     x9,  x9, #MM_TYPE_TABLE
    add     x10, x10, #(256 * 8)
    str     x9,  [x10]

    /* boot_l3pt_low[0] = runtime PA(boot_l2pt_gb0) | MM_TYPE_TABLE */
    adr     x9,  boot_l2pt_gb0
    add     x9,  x9, #MM_TYPE_TABLE
    adr     x10, boot_l3pt_low
    str     x9,  [x10]

	/* boot_l3pt_high[0] = runtime PA(boot_l2pt_gb0) | MM_TYPE_TABLE
	 *
	 * Use a TABLE descriptor pointing to boot_l2pt_gb0 instead of a 1 GiB
	 * block descriptor.  boot_l2pt_gb0 maps the first 1 GB with per‑2 MiB
	 * attributes (Normal for DRAM, Device for the peripheral window at
	 * 0x3F000000+).  On Cortex‑A53 a 1 GiB block entry spanning both DRAM
	 * and Device memory causes speculative instruction fetches to hit the
	 * peripheral bus, which can corrupt x30 (return address) and trigger
	 * external aborts — the root cause of the persistent "Synchronous
	 * Abort" at 0x3AF610A8. */
	adr     x9,  boot_l2pt_gb0
	add     x9,  x9,  #MM_TYPE_TABLE
	adr     x10, boot_l3pt_high
	str     x9,  [x10]

	/* boot_l3pt_linear[0] = same table descriptor (boot_l2pt_gb0) on RPi3.
	 * QEMU keeps the static 1 GiB block for slot 0 (PA 0x00000000 not DRAM). */
	cbz     x21, 2f          /* RPi3 only */
	b       3f
2:	adr     x10, boot_l3pt_linear
	str     x9,  [x10]       /* x9 still holds boot_l2pt_gb0 | MM_TYPE_TABLE */
3:

    /* On RPi3 (DRAM_BASE == 0), only the first 1 GB of the linear map
     * (PA 0x00000000-0x3FFFFFFF) is valid.  Slots 1-3 of boot_l3pt_linear
     * point to PA 0x40000000-0xFFFFFFFF which does NOT exist on RPi3.
     *
     * Zero them out so speculative page walks or accidental accesses
     * through those addresses get a translation fault instead of silently
     * accessing non-existent memory (which can cause external aborts).
     *
     * On QEMU (DRAM_BASE == 0x40000000) slots 1-3 are valid and left alone. */
    cbz     x21, 0f                 /* if DRAM_BASE == 0 (RPi3): zero slots 1-3 */
    b       1f
0:
    adr     x10, boot_l3pt_linear
    str     xzr, [x10, #(1 * 8)]    /* slot 1 → 0 (invalid) */
    str     xzr, [x10, #(2 * 8)]    /* slot 2 → 0 (invalid) */
    str     xzr, [x10, #(3 * 8)]    /* slot 3 → 0 (invalid) */
    /* Same for boot_l3pt_high slots 1-2: non-existent PA on RPi3 */
    adr     x10, boot_l3pt_high
    str     xzr, [x10, #(1 * 8)]    /* slot 1: PA 0x80000000 → 0 (invalid) */
    str     xzr, [x10, #(2 * 8)]    /* slot 2: PA 0xC0000000 → 0 (invalid) */
    /* Same for boot_l3pt_low slots 1-3 (TTBR0 identity map): non-existent PA on RPi3 */
    adr     x10, boot_l3pt_low
    str     xzr, [x10, #(1 * 8)]    /* slot 1: PA 0x40000000 → 0 (invalid) */
    str     xzr, [x10, #(2 * 8)]    /* slot 2: PA 0x80000000 → 0 (invalid) */
    str     xzr, [x10, #(3 * 8)]    /* slot 3: PA 0xC0000000 → 0 (invalid) */
    dsb     sy
    isb
1:

    dsb     sy
    isb

    /* Load boot page tables (using runtime addresses via adr) */
    adr     x2, boot_l4pt
    msr     ttbr0_el1, x2           /* TTBR0: identity map VA==PA */
    adr     x2, boot_l4pt_kern
    msr     ttbr1_el1, x2           /* TTBR1: high-half kernel + linear map */
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

    /* Clean+invalidate all data caches by set/way, then invalidate
     * the instruction cache, before enabling C and I. */
    bl      flush_dcache_all
    ic      iallu
    dsb     nsh
    isb

    /* Enable MMU and caches */
    ldr     x5, =SCTLR_MMU_ENABLED
    msr     sctlr_el1, x5
    isb

    /* DEBUG MARKER 'C': MMU enabled, still running in identity-mapped low VA. */
    rpi3_putch 'C'

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
 * Cache maintenance helpers.
 * Clean+invalidate all data caches by set/way.  Does not use the stack.
 * Clobbers x0-x7, x9-x16, x30 (saved/restored in x15).
 * -------------------------------------------------------------------------- */
flush_dcache_all:
    dsb     sy
    mrs     x10, clidr_el1
    ubfx    x11, x10, #24, #3       /* x11 = LoC (Level of Coherency) */
    cbz     x11, 5f
    mov     x15, x30                 /* save lr */
    mov     x16, xzr                 /* Cortex-A53 / QEMU virt have no FEAT_CCIDX */
    mov     x0, xzr                  /* start at cache level 0 */

2:  add     x12, x0, x0, lsl #1     /* x12 = 3 * level */
    lsr     x12, x10, x12
    and     x12, x12, #7            /* x12 = cache type for this level */
    cmp     x12, #2
    b.lt    4f                       /* skip if no data/unified cache here */
    bl      flush_dcache_level
4:  add     x0, x0, #1
    cmp     x11, x0
    b.gt    2b
    mov     x0, xzr
    msr     csselr_el1, x0          /* restore CSSSELR to level 0 */
    dsb     sy
    isb
    mov     x30, x15                 /* restore lr */
5:  ret

flush_dcache_level:
    lsl     x12, x0, #1
    msr     csselr_el1, x12         /* select cache level */
    isb
    mrs     x6, ccsidr_el1
    ubfx    x2, x6, #0, #3          /* x2 = log2(line size) - 4 */
    ubfx    x3, x6, #3, #10         /* x3 = number of ways - 1 */
    ubfx    x4, x6, #13, #15        /* x4 = number of sets - 1 */
    add     x2, x2, #4              /* x2 = log2(line size) */
    clz     w5, w3                  /* x5 = bit position of number of ways */

6:  mov     x6, x3
7:  lsl     x7, x6, x5
    orr     x9, x12, x7             /* way + level */
    lsl     x7, x4, x2
    orr     x9, x9, x7               /* set + way + level */
    dc      cisw, x9                 /* clean and invalidate by set/way */
    subs    x6, x6, #1
    b.ge    7b
    subs    x4, x4, #1
    b.ge    6b
    ret

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

/* L3 (PUD) for the kernel high half: maps to DRAM via boot_l2pt_gb0 (per-2MB) */
.balign 4096
boot_l3pt_high:
    .quad boot_l2pt_gb0 + MM_TYPE_TABLE     /* VA[0–1GB)   → via L2 table  (DRAM + peripherals) */
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
 * L2 page table for GB0 (0x00000000–0x3FFFFFFF).
 *
 * Keep almost everything as Normal memory so early kernel text/rodata remain
 * executable and readable with expected behavior.  Override:
 *   - QEMU UART window (0x09000000–0x09FFFFFF) to Device memory (one 2 MB block)
 *   - RPi3 peripheral window (0x3F000000–0x3FFFFFFF) to Device memory
 *     (all 8 x 2 MB blocks: PL011 at 0x3F201000, aux/mini-uart at 0x3F215000, etc.)
 */
.balign 4096
/*
 * L2 (PMD) page table for the first 1 GB — boot_l2pt_gb0.
 *
 * On RPi3 3B DRAM is 948 MiB (PA 0x00000000–0x3B3FFFFF).  The BCM2837
 * SoC leaves PA 0x3B400000–0x3EFFFFFF *unpopulated* — no device responds
 * at those addresses.  Speculative instruction fetches (Cortex-A53) into
 * this gap can trigger an AXI external abort which arrives as an SError
 * (ESR 0x02000000) at EL3, crashing the kernel through TF-A.
 *
 * We therefore leave the gap entries *absent* (zero).  Normal memory
 * resumes at 0x3F000000 (RPi3 MMIO window) which is mapped Device.
 *
 * QEMU virt: DRAM starts at 0x40000000 so this table is ONLY used for
 * the low 1 GB identity pass-through; the QEMU UART at 0x09000000 is
 * still mapped as Device.
 */
boot_l2pt_gb0:
    .set _mb2, 0
    .rept 512
    .if _mb2 == 0x09000000
    .quad ( PTE_DEVICE_2M | _mb2 )          /* QEMU UART */
    .elseif _mb2 >= 0x3B400000 && _mb2 < 0x3F000000
    .quad 0                                  /* gap – no physical memory */
    .elseif _mb2 >= 0x3F000000
    .quad ( PTE_DEVICE_2M | _mb2 )          /* RPi3 peripheral bus */
    .else
    .quad ( PTE_NORMAL_2M | _mb2 )          /* DRAM */
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
    .skip 0x80000                           /* 512 KiB */
.globl boot_stack_top
boot_stack_top:

/* --------------------------------------------------------------------------
 * Virtual-address entry point (MMU now enabled)
 * -------------------------------------------------------------------------- */
.text
.globl _start_virt
_start_virt:
    /* Fix up SP: before enabling the MMU we set SP from an `adr` instruction,
     * which gives the physical / identity-mapped address of boot_stack_top.
     * TTBR1 maps PA DRAM_BASE+X to VA 0xffff000000000000+X, so:
     *   SP_virt = SP_phys - DRAM_BASE + 0xffff000000000000
     * x21 = DRAM_BASE (0 for RPi3, 0x40000000 for QEMU) set in _start_real.
     * Rust stack-variable addresses come from SP, so we MUST switch to the
     * high-VA stack pointer here before any Rust frame is allocated. */
    mov     x7, sp
    sub     x7, x7, x21             /* SP - DRAM_BASE */
    ldr     x8, =0xffff000000000000
    add     sp, x7, x8              /* SP_virt = SP_phys - DRAM_BASE + HIGH_HALF */

    /* DEBUG MARKER 'D': _start_virt reached, SP now in high VA, MMU on. */
    rpi3_putch 'D'

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

    /* DEBUG MARKER 'E': _start_virt done — about to branch into Rust. */
    rpi3_putch 'E'

    /* Install our own EL1 exception vectors NOW — before the Rust prologue runs —
     * so any fault in aarch64_boot's prologue goes to our handler (prints
     * ESR/ELR/FAR and loops) instead of U-Boot's handler (resets the CPU).
     * vector_table_el1 is in .text at a high VA; adr gives the correct address
     * because we are already running at the high VA here. */
.extern vector_table_el1
    adr     x9, vector_table_el1
    msr     vbar_el1, x9
    isb

    /* DEBUG MARKER 'F': VBAR_EL1 installed.  If we see 'F' on serial but not
     * any [sec] output, the prologue DID NOT fault.
     * If we see 'E' but not 'F', something between 'E' and here is wrong (impossible). */
    rpi3_putch 'F'

    /* Restore x0 (DTB paddr, per AArch64 boot protocol) and x1 (reserved=0)
     * that were saved in x19/x20 at _start before debug macros clobbered them. */
    mov     x0, x19
    mov     x1, x20
    b       aarch64_boot
