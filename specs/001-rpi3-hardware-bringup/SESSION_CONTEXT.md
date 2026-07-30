# RPi3 Debug Session — 2026-07-30 (Session 13)

## Current Task
Diagnose and fix the deterministic "Synchronous Abort" (ESR `0x02000000`) crash at `0x3AF610A8` / `0xFFFFFFFFFC900A8`. The crash consistently happens during frame allocator initialization (`add_free_memory → insert_chunk`).

## Session 13 Summary

### Key Finding: Crash narrowed to `insert_before` metadata access
With the coalescing loop SKIPPED (workaround), the crash moves from `cursor_mut_at(buddy_addr)` to `push_front → insert_before`. The crash is NOT specific to any single function — it occurs whenever frame metadata is dereferenced through the MetaSlot pointer obtained during `from_unused`.

### Boot Marker Trace (latest)
```
[ic.sz] [ic.fu] [ic.co] [ic.lp]   ← insert_chunk prologue
[ic.pf]                            ← about to call push_front → insert_before
CRASH at 0x3AF610A8
(no [ic.ts] marker — crash is inside push_front/insert_before)
```

### Previous trace (with coalescing enabled)
```
[ic.lp.chk] [ic.lp.bud] [ic.lp.lst] [ic.lp.cur]
CRASH at 0x3AF610A8
(crash inside cursor_mut_at → get_slot → ...)
```

### Critical discovery: IN_BOOTSTRAP_CONTEXT timing
`allocator::init()` runs at `lib.rs:init():156`, BEFORE:
- `init_kernel_page_table()` (line 163)
- `activate_kernel_page_table()` (line 189)  
- `IN_BOOTSTRAP_CONTEXT.store(false, ...)` (line 195)

Therefore `IN_BOOTSTRAP_CONTEXT` is still **TRUE** during the crash. The `get_slot()` function uses the **bootstrap path**:
```rust
let meta_paddr_base = FRAME_META_PADDR_BASE.load(Ordering::Relaxed); // PA
let frame_idx = (paddr - frame_paddr_base) / PAGE_SIZE;
let slot_paddr = meta_paddr_base + frame_idx * size_of::<MetaSlot>();
slot_paddr as *mut MetaSlot  // ← RAW PHYSICAL ADDRESS POINTER
```

This raw PA pointer is dereferenced via TTBR0 identity mapping (`boot_l3pt_low[0] → boot_l2pt_gb0`), which correctly maps PA 0x0-0x3FFFFFFF to the same VA.

### Why SError (ESR 0x02000000) instead of translation fault?
The ESR `0x02000000` has EC = `0b000000` ("Unknown reason"), characteristic of an **SError** (asynchronous external abort). This suggests the crash is NOT from a data access fault but from:
1. A **speculative instruction fetch** that crosses the DRAM boundary into non-existent memory (0x3B400000+), triggering AXI external abort → SError
2. Even with per-2MB boot page table entries (boot_l2pt_gb0 fix), the Cortex-A53 prefetcher may speculatively access past valid DRAM
3. Or: a bug in the boot page table fix itself — `boot_l2pt_gb0` marks 0x3B000000-0x3EFFFFFF as Normal memory, but only 0x3B000000-0x3B400000 is actual DRAM. Speculative access to 0x3B400000-0x3EFFFFFF goes to non-existent address → external abort

### The gap: unmapped DRAM tail
RPi3 DRAM: 0x0 – 0x3B400000 (948 MB).  
`boot_l2pt_gb0` maps 0x00000000–0x3FFFFFFF with **all Normal memory** entries (except 0x3F000000+ which is Device).  
The range 0x3B400000–0x3EFFFFFF (60 MB) is mapped as Normal memory but has **no physical memory** — any access triggers AXI external abort.

### Relevant registers (consistent across all crashes)
```
x16 = 0x0260612C  (IP0, scratch — always the same)
x17 = 0x00000008  (IP1, scratch — always the same)
x19 = 0x3AF4C440  (near top of DRAM, ~944 MB)
x20 = 0x3B35C368  (near top of DRAM, ~947 MB)
x21 = 0x3AF4C4B0  (x19 + 0x70)
x29 = 0x00000003  (frame pointer — clearly corrupted)
```

### Working Theory
The frame being inserted is at PA 0x3AF4C440 (near end of DRAM). During `insert_before`, the frame's metadata slot is accessed via TTBR0 identity mapping (PA pointer). The instruction stream for `insert_before` is fetched through TTBR1 (kernel mapping). The Cortex-A53 instruction prefetcher may speculatively fetch past the end of DRAM (0x3B400000) into the 60 MB Normal-mapped-but-not-real region, hitting an AXI external abort. This generates a SError, which TF-A at EL3 catches and prints as "Synchronous Abort."

Consistent crash address `0x3AF610A8` may be a stale ELR value from an earlier context (TF-A BL31 initialization) or the PA of a page table entry being walked during the prefetch.

### Next Actions
1. **Mask gaps in boot_l2pt_gb0**: Change entries from 0x3B400000 to 0x3EFFFFFF from PTE_NORMAL_2M to invalid/absent. This prevents speculative access to non-existent memory.
2. **Use larger stack**: Increase boot stack from 256 KiB to 512 KiB (ruling out stack overflow).
3. **Check if crash still happens with completely empty BuddySet**: Skip push_front entirely as a test.
4. **Try boot with IRQ/FIQ/SError masking at EL1**: Modify DAIF to mask SError interrupts during critical sections.
