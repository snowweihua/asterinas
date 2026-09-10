/* SPDX-License-Identifier: MPL-2.0 */

/*
 * AArch64 fallible memory-copy/set/atomic helpers.
 *
 * These functions copy/set/read/write user-space memory from EL1 (kernel mode).
 * On AArch64, a kernel access to an unmapped user-space address causes a Data
 * Abort (EC = 0x25) at the current EL.  The `sync_exception_current` handler
 * searches the `.ex_table` section for the faulting PC and, if found, redirects
 * execution to the corresponding fixup label, which returns an appropriate error
 * code instead of hanging the kernel.
 *
 * Contract (mirrors the x86_64 implementations):
 *   __memcpy_fallible(dst, src, size) -> size_t failed_bytes
 *   __memset_fallible(dst, val, size) -> size_t failed_bytes
 *   __atomic_load_fallible(ptr)       -> u64 value, or !0u64 on fault
 *   __atomic_cmpxchg_fallible(ptr, old, new) -> u64 prev, or !0u64 on fault
 */

.text

/* ── __memcpy_fallible ──────────────────────────────────────────────────── */
/*
 * Copy `size` bytes from `src` to `dst`, one byte at a time.
 * Returns the number of bytes that were NOT copied (0 = all OK).
 *
 * The byte-by-byte loop lets the exception-table fixup know the exact count
 * of remaining bytes when a fault occurs.
 *
 * AArch64 calling convention (AAPCS64):
 *   x0 = dst  (modified: points past last written byte on exit)
 *   x1 = src  (modified: points past last read byte on exit)
 *   x2 = size (modified: counts down to 0, holds remaining on fault)
 *   w3 = scratch (byte value)
 * Returns: x0 = failed_bytes
 */
.global __memcpy_fallible
.type __memcpy_fallible, %function
__memcpy_fallible:
    cbz  x2, .Lmcf_done       /* size == 0 */
.Lmcf_loop:
.Lmcf_load:                   /* ← exception-table entry (fault while loading) */
    ldrb w3, [x1], #1         /* load byte from src; post-increment x1 */
.Lmcf_store:                  /* ← exception-table entry (fault while storing) */
    strb w3, [x0], #1         /* store byte to dst; post-increment x0 */
    subs x2, x2, #1           /* decrement remaining count */
    b.ne .Lmcf_loop
.Lmcf_done:
    mov  x0, x2               /* return 0 (all bytes copied) */
    ret
    /* Both fixups share the same epilogue: return x2 = remaining failed bytes */
.Lmcf_fixup:
    mov  x0, x2
    ret
.size __memcpy_fallible, . - __memcpy_fallible

.pushsection .ex_table, "a"
    .balign 8
    .quad .Lmcf_load           /* instruction that may fault */
    .quad .Lmcf_fixup          /* recovery: return remaining count */
    .quad .Lmcf_store
    .quad .Lmcf_fixup
.popsection


/* ── __memset_fallible ──────────────────────────────────────────────────── */
/*
 * Fill `size` bytes at `dst` with `val` (low 8 bits of x1).
 * Returns the number of bytes that were NOT set (0 = all OK).
 *
 *   x0 = dst
 *   x1 = value (u8)
 *   x2 = size
 */
.global __memset_fallible
.type __memset_fallible, %function
__memset_fallible:
    cbz  x2, .Lmsf_done
.Lmsf_loop:
.Lmsf_store:                  /* ← exception-table entry */
    strb w1, [x0], #1
    subs x2, x2, #1
    b.ne .Lmsf_loop
.Lmsf_done:
    mov  x0, x2
    ret
.Lmsf_fixup:
    mov  x0, x2
    ret
.size __memset_fallible, . - __memset_fallible

.pushsection .ex_table, "a"
    .balign 8
    .quad .Lmsf_store
    .quad .Lmsf_fixup
.popsection


/* ── __atomic_load_fallible ─────────────────────────────────────────────── */
/*
 * Atomically load a 32-bit value from user-space pointer `ptr`.
 * Returns the loaded value (zero-extended to u64), or !0u64 on fault.
 *
 *   x0 = ptr (*const u32)
 * Returns: x0 = loaded_value | !0u64
 *
 * LDAR provides acquire semantics and is available on ARMv8.0.
 */
.global __atomic_load_fallible
.type __atomic_load_fallible, %function
__atomic_load_fallible:
.Lalf_load:                   /* ← exception-table entry */
    ldar w1, [x0]             /* atomic load (acquire) */
    uxtw x0, w1               /* zero-extend w1 → x0 */
    ret
.Lalf_fixup:
    mov  x0, #-1              /* return !0u64 */
    ret
.size __atomic_load_fallible, . - __atomic_load_fallible

.pushsection .ex_table, "a"
    .balign 8
    .quad .Lalf_load
    .quad .Lalf_fixup
.popsection


/* ── __atomic_cmpxchg_fallible ──────────────────────────────────────────── */
/*
 * Atomically compare-and-exchange a 32-bit word at `ptr`.
 * If *ptr == old_val, replace it with new_val.
 * Returns the previous value at `ptr` (zero-extended), or !0u64 on fault.
 *
 *   x0 = ptr  (*mut u32)
 *   w1 = old_val (u32)
 *   w2 = new_val (u32)
 * Returns: x0 = old_value | !0u64
 *
 * Uses ARMv8.0-compatible LDAXR/STLXR loop (no LSE required).
 * CLREX is issued in the fixup so the exclusive monitor is released.
 */
.global __atomic_cmpxchg_fallible
.type __atomic_cmpxchg_fallible, %function
__atomic_cmpxchg_fallible:
.Lacf_ldaxr:                  /* ← exception-table entry */
    ldaxr w3, [x0]            /* load-acquire exclusive */
    cmp   w3, w1              /* compare with expected */
    b.ne  .Lacf_mismatch
.Lacf_stlxr:                  /* ← exception-table entry */
    stlxr w4, w2, [x0]       /* store-release exclusive; w4 = success (0) or fail (!0) */
    cbnz  w4, __atomic_cmpxchg_fallible  /* retry on exclusive-monitor loss */
.Lacf_mismatch:
    uxtw  x0, w3             /* return previous value */
    ret
.Lacf_fixup:
    clrex                    /* release exclusive monitor */
    mov   x0, #-1            /* return !0u64 */
    ret
.size __atomic_cmpxchg_fallible, . - __atomic_cmpxchg_fallible

.pushsection .ex_table, "a"
    .balign 8
    .quad .Lacf_ldaxr
    .quad .Lacf_fixup
    .quad .Lacf_stlxr
    .quad .Lacf_fixup
.popsection
