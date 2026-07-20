# RPi3 Debug Session - 2026-07-17

## Current Task T030
Kernel crashes with `Synchronous Abort` during frame metadata initialization in `kspace::init_kernel_page_table()`.

## Root Causes Found & Fixed (from previous session)

### Fix 1-9 (previous session)
See SESSION_CONTEXT.md from 2026-07-16 for details on spin::Once, buddy coalescing, SpinLock fixes.

## Latest Root Cause Analysis

### CRITICAL: `frame_paddr_base` mismatch on RPi3

**Problem**: `frame_paddr_base = 0x4000_0000` (compile-time constant for all AArch64) but RPi3 DRAM starts at `0x0`.

**Impact on buddy allocator**:
- RPi3 usable memory: `0x0..0x3BF00000`
- With `frame_paddr_base = 0x4000_0000`, `add_free_memory` clamps range to `0x4000_0000..0x3BF00000` (EMPTY)
- Buddy gets no usable memory → returns frames below `frame_paddr_base`

**Impact on metadata slot calculation**:
- `get_slot(paddr)` uses `(paddr - frame_paddr_base) / PAGE_SIZE` for slot index
- With `frame_paddr_base = 0x4000_0000` and `paddr = 0x100000`: index = `-0x30000000 / 4096` = negative (WRONG!)
- `tot_nr_frames = max_paddr - frame_paddr_base = 0x3BF00000 - 0x40000000 = 0`
- So `tot_nr_frames = 0` and metadata frames shouldn't be tracked

**But crash still happens**: With `frame_paddr_base = 0x4000_0000`:
- `[meta.init] meta_frames allocated` marker prints
- `[meta.get.0] start`, `[meta.get] got slot` print
- Crash INSIDE `compare_exchange` on slot's ref_count

**Analysis**: Even with `tot_nr_frames = 0`, the buddy allocator somehow returns frames. The metadata frame itself (allocated via `early_alloc`) is tracked separately. When `get_slot` checks `paddr >= frame_paddr_base`, it succeeds for the metadata frame paddr (which might be above `frame_paddr_base`). But the returned frame paddr is below `frame_paddr_base`.

### Attempted Fix: `frame_paddr_base = 0`

**Result**: Same crash at `compare_exchange`.

**Analysis**: With `frame_paddr_base = 0`:
- Buddy gets all usable memory (0x0..0x3BF00000)
- `tot_nr_frames = max_paddr / PAGE_SIZE = 60927` (all frames tracked)
- Slot calculation: `slot_paddr = meta_paddr_base + (paddr / PAGE_SIZE) * 32`
- Slot addresses are in kernel high VA range (`0xFFFF_xxxx`)
- But during bootstrap, kernel high VA is NOT mapped in boot page table
- Accessing these addresses causes fault

## Key Files Analyzed

### `ostd/src/mm/frame/meta.rs`
- `init()`: Computes `tot_nr_frames` and allocates metadata frames
- `get_slot()`: Computes metadata slot address for a frame paddr
- `get_from_unused()`: Calls `get_slot` then does `compare_exchange` on slot's ref_count

### `ostd/src/mm/frame/allocator.rs`
- `init()`: Adds usable memory to buddy allocator
- Clamping logic: `if r2.start < frame_paddr_base { frame_paddr_base }` causes issues when usable memory is below frame_paddr_base

### `osdk/deps/frame-allocator/src/lib.rs`
- `FrameAllocator::alloc()`: Calls `cache::alloc` then `TOTAL_FREE_SIZE.sub()`
- Bypassed `TOTAL_FREE_SIZE.sub()` for debugging - no change in crash location

### `ostd/src/arch/aarch64/mm/mod.rs`
- `frame_paddr_base() = 0x4000_0000` (const for all AArch64 platforms)
- Should be `0` for RPi3 where DRAM starts at `0x0`

## Debug Markers Currently In Code

### meta.rs
- `[meta.init] max_paddr computed`
- `[meta.init] frame_paddr_base computed`
- `[meta.init] tot_nr_frames computed`
- `[meta.init] meta_frames allocated`
- `[meta.get.0] start`
- `[meta.get] got slot`
- `[meta.get] before compare_exchange`

### allocator.rs
- `[fa.0] alloc_frame_with start`
- `[fa] before/after get_global_frame_allocator`
- `[fa] before allocator.alloc`
- `[FA] before/after disable_local`
- `[FA] after cache::alloc`
- `[FA] returning res`
- `[cache.a-f]`: Cache alloc markers
- `[pools.a-e]`: Pool alloc markers

### frame-allocator/lib.rs
- Bypassed TOTAL_FREE_SIZE.sub()

## Files Modified (this session)
- `ostd/src/arch/aarch64/mm/mod.rs` - Attempted frame_paddr_base changes
- `ostd/src/mm/frame/meta.rs` - Debug markers, tot_nr_frames calculation changes
- `ostd/src/mm/frame/allocator.rs` - Debug markers, add_free_memory logic changes
- `ostd/src/mm/frame/unique.rs` - Restored unsafe block for ref_count.store
- `osdk/deps/frame-allocator/src/lib.rs` - Bypassed TOTAL_FREE_SIZE.sub()

## Next Steps

1. **Option A**: Change `frame_paddr_base` to 0 AND fix bootstrap slot access
   - When `frame_paddr_base = 0`, slots are at kernel high VA
   - During bootstrap, need to access via physical address instead
   - Modify `get_slot` to use physical address in bootstrap context

2. **Option B**: Make `frame_paddr_base` runtime-dependent
   - Use actual DRAM base from device tree
   - RPi3: `dram_base()` returns 0x0
   - QEMU virt: `dram_base()` returns 0x4000_0000
   - But `frame_paddr_base` is `const fn` which can't call runtime functions

3. **Option C**: Change how `tot_nr_frames` is computed
   - Track ALL usable memory regardless of `frame_paddr_base`
   - Change slot calculation to use `paddr / PAGE_SIZE` directly
   - This would require significant changes to slot index formula

4. **Option D**: Fix the clamping in `add_free_memory`
   - When usable memory is below `frame_paddr_base`, don't add it to buddy
   - Instead, rely on the fact that such memory shouldn't be accessed
   - But this breaks RPi3 where all usable memory is below `frame_paddr_base`
