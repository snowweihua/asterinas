# RPi3 Debug Session - 2026-07-16

## Current Task T030
Kernel hangs in `SpinLock::try_acquire_lock` inside `global_pool.get()` - hang is at `[pools.c]` marker, meaning we successfully reach the SpinLock acquisition but `swap` atomic hangs.

## Root Causes Found & Fixed

### Fix 1: `cpu::extension.rs` - spin::Once hang
- **File**: `ostd/src/arch/aarch64/cpu/extension.rs`
- **Issue**: Uses `spin::Once` which hangs on RPi3 (LDXR/STXR atomic issue on RPi3 AXI bridge)
- **Fix**: Changed to `use crate::boot::SimpleOnce as Once;`
- **Commit**: fd3875b3

### Fix 2: `cpu/local/mod.rs` - CPU_LOCAL_STORAGES spin::Once hang
- **File**: `ostd/src/cpu/local/mod.rs`
- **Issue**: `CPU_LOCAL_STORAGES: Once<&'static [Paddr]>` uses `spin::Once` which hangs on RPi3
- **Fix**: Changed to `use crate::boot::SimpleOnce as Once;`
- **Also**: Added `is_completed()` and `get_unchecked()` methods to `SimpleOnce`
- **Commit**: fd3875b3

### Fix 3: `mm/frame/allocator.rs` - boot_info vs EARLY_INFO
- **File**: `ostd/src/mm/frame/allocator.rs`
- **Issue**: `EarlyFrameAllocator::new()` called `boot_info()` which uses `INFO` Once
- **Fix**: Changed to use `crate::boot::EARLY_INFO.get()` instead
- **Commit**: bd1d133f

### Fix 4: `mm/frame/meta.rs` - boot_info vs EARLY_INFO
- **File**: `ostd/src/mm/frame/meta.rs`
- **Issue**: Same as Fix 3
- **Fix**: Changed to use `crate::boot::EARLY_INFO.get()` instead

### Fix 5: `BootInfo` - heap allocation during construction
- **File**: `ostd/src/boot/mod.rs`
- **Issue**: `BootInfo` uses `String` and `Vec` fields requiring heap allocation
- **Fix**: Changed to `&'static str` and `&'static [MemoryRegion]` (zero-allocation)
- **Commit**: b35067af

### Fix 6: `early_marker` replaced with `pl011_puts`
- **File**: `ostd/src/mm/kspace/mod.rs`
- **Issue**: `early_marker` has a known issue on RPi3
- **Fix**: Changed markers to use `pl011_puts`
- **Commit**: ec50b486

### Fix 7: Buddy allocator coalescing bug (CRITICAL)
- **File**: `osdk/deps/frame-allocator/src/set.rs`
- **Issue**: `BuddySet::insert_chunk()` calculated buddy address ONCE before coalescing loop but never recalculated after each merge. After chunks coalesce from order N to N+1, the buddy address was still computed using size_of_order(N) instead of size_of_order(N+1).
- **Fix**: Changed from for-loop with enumerate to explicit loop with `current_order` tracking. After each `merge_free()`, update `current_order = chunk.order()` and recalculate `buddy_addr` with new order.
- **Commit**: 7f61fe9b

## Boot Progress - 2026-07-16 (Latest)
```
[init.B] after init_after_heap
[init.B1] before kspace::init
[kspace.W] start
[node.alloc] start
[node.alloc] meta created
[node.alloc] before FrameAllocOptions
[fa] before get_global_frame_allocator
[fa] after get_global_frame_allocator
[fa] before allocator.alloc
[FA] before disable_local
[FA] after disable_local
[cache.a] start
[cache.b] before CACHE.get_with
[cache.c] after CACHE.get_with
[cache.d] before borrow_mut
[cache.e] after borrow_mut
[CA.a] start
[CA.c] pop_front miss, calling pools::alloc
[pools.a] start
[pools.b] got global_pool
[pools.c] calling global_pool.get()
```
**Confirmed**: Hang is INSIDE `OnDemandGlobalLock::get()` → `GLOBAL_POOL.lock()` → `SpinLock::acquire_lock()` → `SpinLock::try_acquire_lock()` using `swap` atomic.

## Next Steps
1. Build with SpinLock markers to confirm hang at `[spin.t] before swap`
2. Investigate if `swap` itself is affected by RPi3 AXI bridge (not just compare_exchange)
3. Consider using memory barriers or disabled interrupts approach instead of atomic swap
4. T030 complete when `/ #` prompt appears

## Known Blockers
- RPi3 AXI bridge causes LDXR/STXR atomics to fail spuriously
- Even `swap` with `Ordering::Acquire` may be affected since internally it uses LDXR/STXR

## Files Modified (this session)
- `ostd/src/boot/mod.rs` - BootInfo zero-allocation, test code
- `ostd/src/mm/frame/allocator.rs` - EARLY_INFO fix
- `ostd/src/mm/frame/meta.rs` - EARLY_INFO fix
- `ostd/src/mm/kspace/mod.rs` - pl011_puts markers
- `ostd/src/sync/spin.rs` - ADDED markers at [spin.t] and [spin.L]
- `osdk/deps/frame-allocator/src/set.rs` - Buddy coalescing bug fix
- `osdk/deps/frame-allocator/src/pools/mod.rs` - Debug markers
- `osdk/deps/frame-allocator/src/cache.rs` - Debug markers
- `kernel/src/lib.rs` - Removed .as_str() call

## Commits on aarch64_support
- 7f61fe9b aarch64/buddy: fix buddy address recalculation after coalesce
- dcce6611 docs: add commit-on-progress rule to AGENTS.md
- 63a5a48d aarch64/cpu: add cpu.1-8 markers
- 8349f815 aarch64/mm: use EARLY_INFO instead of boot_info
- fd3875b3 aarch64/cpu: replace spin::Once with SimpleOnce
- bd1d133f aarch64/mm: use EARLY_INFO in EarlyFrameAllocator
- b35067af boot: change BootInfo to use &str instead of String
- 19d4617f boot: add [init.B1] marker before kspace::init
- 7c8176be mm/kspace: add markers inside init_kernel_page_table
- ec50b486 mm/kspace: replace early_marker with pl011_puts
