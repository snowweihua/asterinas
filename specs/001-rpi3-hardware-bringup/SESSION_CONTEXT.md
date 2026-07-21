# RPi3 Debug Session - 2026-07-21

## Current Task T030
Boot to shell with interactive command working.

## Fix 12 (2026-07-21): Revert get_slot physical-path slot formula

**Problem**: Fix 11 (commit 97e69605) incorrectly changed the slot calculation from:
```
frame_idx = (paddr - frame_paddr_base) / PAGE_SIZE;
slot_paddr = meta_paddr_base + frame_idx * size_of::<MetaSlot>();
```
to:
```
page_offset = paddr % PAGE_SIZE; slot_offset = page_offset / 64;
slot_paddr = meta_paddr_base + page_offset + slot_offset * 64;
```
This computed `meta_paddr_base` for ALL page-aligned frames (page_offset=0).

**Solution**: Reverted to the correct `frame_idx` formula. Also removed the
incorrect `paddr < meta_paddr_base` check.

**Status**: Built and deployed to RPi3 TFTP. Awaiting hardware power cycle.

---

## Fix 10: `frame_paddr_base()` now uses `dram_base()` at runtime

**Problem**: `frame_paddr_base = 0x4000_0000` (hardcoded const) but RPi3 DRAM starts at `0x0`.

**Solution**: Changed `frame_paddr_base()` from `const fn` to `fn` that calls `crate::arch::board::dram_base()`.

**Impact**:
- RPi3: `dram_base()` reads DTB memory node → returns `0x0` → `frame_paddr_base = 0x0`
- QEMU virt: `dram_base()` returns `0x4000_0000` → `frame_paddr_base = 0x4000_0000`

**Also changed**:
- `frame_to_meta()` and `meta_to_frame()` in meta.rs: `const fn` → `fn` (required since they call non-const `frame_paddr_base()`)

### Fix 11: `get_slot` bootstrap path check

**Problem**: When `frame_paddr_base == 0`, `get_slot` was incorrectly taking the VA path (`fpb != 0`) during bootstrap.

**Solution**: Changed condition from `fpb != 0` to `!IN_BOOTSTRAP && fpb != 0`:
- Bootstrap: use physical address path (slots identity-mapped in boot_pt)
- After bootstrap: use virtual address path

## Boot Progress (this session)

Kernel now reaches further than before:
```
[init.8] after cpu::init_on_bsp
[meta.init] max_paddr computed
[meta.init] frame_paddr_base computed
[meta.init] tot_nr_frames computed
[meta.init] meta_frames allocated
[meta.get.0] start
[gs.fpb]=0 paddr=2b77000
[gs.in_boot]=1 fpb!=0=0 [gs.use_phys]
[gs.meta_base]=2b7b000
*** Synchronous Abort ***
```

## Previous Session Fixes (2026-07-17)

### Fix 1-9 (from 2026-07-17 session)
- spin::Once → SimpleOnce in cpu/extension.rs and cpu/local/mod.rs
- boot_info() → EARLY_INFO.get() in allocator.rs and meta.rs
- BootInfo: String→&str, Vec→&[MemoryRegion]
- early_marker → pl011_puts in kspace/mod.rs
- SpinLock: atomic swap → relaxed store + fence
- BuddySet::insert_chunk: recalculate buddy_addr after coalesce
- tot_nr_frames calculation change
- add_free_memory range adjustment

## Key Files Analyzed

### `ostd/src/mm/frame/meta.rs`
- `init()`: Computes `tot_nr_frames` and allocates metadata frames
- `get_slot()`: Computes metadata slot address for a frame paddr
- `get_from_unused()`: Calls `get_slot` then does `compare_exchange` on slot's ref_count
- `init_slots()`: Initializes all slots with `REF_COUNT_UNUSED`

### `ostd/src/mm/frame/allocator.rs`
- `init()`: Adds usable memory to buddy allocator
- Clamping logic: `if r2.start < frame_paddr_base { frame_paddr_base }` causes issues when usable memory is below frame_paddr_base

### `ostd/src/arch/aarch64/mm/mod.rs`
- `frame_paddr_base()` is now runtime, not const

## Debug Markers Currently In Code

### meta.rs
- `[meta.init] max_paddr computed`
- `[meta.init] frame_paddr_base computed`
- `[meta.init] tot_nr_frames computed`
- `[meta.init] meta_frames allocated`
- `[meta.get.0] start`
- `[gs.fpb]=` frame_paddr_base
- `[gs.in_boot]=` IN_BOOTSTRAP_CONTEXT value
- `[gs.use_phys]` when using physical path
- `[gs.meta_base]=` meta base address

## Files Modified (this session)
- `ostd/src/arch/aarch64/mm/mod.rs` - `frame_paddr_base()` now calls `dram_base()`
- `ostd/src/mm/frame/meta.rs` - Debug markers, `get_slot` bootstrap path condition fix

## Remaining Issue

**Synchronous Abort in `get_slot`**:
- `meta_pages = 0x2b7b_0000` (allocated by `early_alloc`)
- First metadata slot access: `paddr=0x2b77_0000` (frame at 0x0), `slot_paddr=0x2b7b_0000`
- `IN_BOOTSTRAP_CONTEXT = true` when `get_slot` is called
- Physical address path selected (correct)
- But `load(Ordering::Relaxed)` on the slot causes translation fault

**Hypothesis**: Even though `FRAME_META_PADDR_BASE.store()` was done, the metadata frames might not be properly mapped in the current page table context, or the slot pointer calculation is still wrong.

## Next Steps

1. **Verify metadata mapping**: Check if early_alloc maps the metadata frames properly
2. **Check slot_paddr calculation**: When `fpb=0`, the formula `meta_base + offset + slot*64` may overflow or miscalculate
3. **Add more granular markers**: Print slot_paddr before accessing ref_count
4. **Consider bypassing metadata init for RPi3**: Skip `mark_unusable_ranges()` for `fpb==0` path to isolate the issue

## Known Blockers
- RPi3 boot hangs at `call_ostd_main()` - partially fixed, now crashes later
- `Synchronous Abort` on metadata slot access - in progress

## Session 2026-07-21: Breakthrough — meta::init() completes

### Fix 12: Revert get_slot physical-path slot formula
**Problem**: Fix 11 incorrectly changed the slot formula from `frame_idx` to `page_offset`-based.
**Solution**: Reverted to `slot_paddr = meta_paddr_base + frame_idx * 64`.
**Status**: Verified on hardware - slots correctly computed and accessed.

### Fix 13: Fix path selection for post-bootstrap
**Problem**: Condition `!IN_BOOTSTRAP && frame_paddr_base != 0` always took physical path on RPi3 (fpb=0).
**Solution**: Changed to `!IN_BOOTSTRAP` only.

### Discovery: Crash at mark_unusable_ranges() call boundary
With no-op stub, meta::init() completes and prints `[init.9] after meta::init`.
The crash is at the function call itself (before any code in mark_unusable_ranges executes).

## Current Status
- `meta::init()` completes with no-op mark_unusable_ranges
- `Segment::from_unused(meta_page_range)` processes thousands of frames successfully
- Kernel crashes AFTER meta::init() returns — likely in allocator::init()
- Next: fix mark_unusable_ranges call crash, then fix post-meta::init crash

## Session 2026-07-21: Major progress — kernel reaches kspace::init

### Fixes accumulated
- Fix 12: get_slot physical-path slot formula reverted to frame_idx
- Fix 13: Path selection uses !IN_BOOTSTRAP (no fpb check)  
- Fix 14: mark_unusable_ranges uses EARLY_INFO (not boot_info)
- Fix 15: Stripped verbose per-frame debug markers

### Current crash: FrameAllocator::alloc return crash
**Crash pattern**: ESR=0x02000000, ELR=0x3af610a8, x29=0x3

**Progress**:
- allocator::init() COMPLETES (with add_free_memory bypassed)
- init_after_heap() COMPLETES
- kspace::init() STARTS node allocation
- FrameAllocator::alloc prints all internal markers correctly
- Crash happens at function return point (after `[FA] returning res`)

**Bypasses tried**:
- `disable_local` no-op → no effect
- `enable_local` no-op → no effect
- `pools::add_free_memory` no-op → fixes crash inside function but return crash persists
- `TOTAL_FREE_SIZE.add` removed → no effect

**Hypothesis**: The crash is at the function return boundary (ELR=LR=same value, x29=0x3 corrupted frame pointer). The `msr DAIFSet/DAIFClr` instructions are NOT the cause. The issue might be with the `ret` instruction itself on RPi3 Cortex-A53, or with how the trait dispatch/function call/return sequence interacts with the kernel's page table setup.

**Next steps**:
- Investigate the x29=0x3 corruption — this is a very specific corrupted frame pointer value
- Check if the issue is related to the stack pointer being wrong after `init_after_heap`
- Look at the `kspace::init` code path to understand what the caller expects after `alloc`
