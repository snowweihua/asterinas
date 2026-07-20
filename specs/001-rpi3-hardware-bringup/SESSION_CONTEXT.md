# RPi3 Debug Session - 2026-07-20

## Current Task T035
Kernel crashes with `Synchronous Abort` during frame metadata initialization in `kspace::init_kernel_page_table()`.

## Root Cause Found & Fixed (this session)

### Fix 10: `frame_paddr_base()` now uses `dram_base()` at runtime

**Problem**: `frame_paddr_base = 0x4000_0000` (hardcoded const) but RPi3 DRAM starts at `0x0`.

**Solution**: Changed `frame_paddr_base()` from `const fn` to `fn` that calls `crate::arch::board::dram_base()`.

**Impact**:
- RPi3: `dram_base()` reads DTB memory node → returns `0x0` → `frame_paddr_base = 0x0`
- QEMU virt: `dram_base()` returns `0x4000_0000` → `frame_paddr_base = 0x4000_0000`

**Also changed**:
- `frame_to_meta()` and `meta_to_frame()` in meta.rs: `const fn` → `fn` (required since they call non-const `frame_paddr_base()`)

**Build**: `cargo osdk build --release --target-arch aarch64 --boot-method qemu-direct --scheme aarch64`
**Deploy**: `aarch64-linux-gnu-objcopy -O binary ... && cp ... /mnt/d/pi_sd/asterina.img`

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
