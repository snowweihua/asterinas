# RPi3 Debug Session - 2026-07-16 (Evening)

## Current Task T030
Kernel crashes with `Synchronous Abort` after successful frame allocation in `kspace::init_kernel_page_table()`.

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

### Fix 6: `early_marker` replaced with `pl011_puts` (partial)
- **Files**: `ostd/src/mm/kspace/mod.rs`
- **Issue**: `early_marker` writes directly to UART without checking LSR bit 6 (TX empty), hangs if TX buffer is full
- **Fix**: Changed `[kspace.c]` and `[kspace.d]` markers to use `pl011_puts`. `early_marker` in kspace was using `crate::early_marker` (lib.rs version) which lacks LSR check.
- **Commit**: 907a896e

### Fix 9: `early_marker` in kspace/mod.rs was NOT using pl011_puts
- **File**: `ostd/src/mm/kspace/mod.rs:219,245`
- **Issue**: `unsafe { crate::early_marker(b'Y') }` calls `crate::early_marker` defined in `ostd/src/lib.rs:71-83`, which writes directly to mini-UART DR (0x3F215040) without checking TX empty flag. This can hang if TX buffer is full.
- **Fix**: Replaced with `crate::arch::boot::pl011_puts(b"[kspace.c/d] ...")`
- **Commit**: 907a896e

### Fix 7: Buddy allocator coalescing bug (CRITICAL)
- **File**: `osdk/deps/frame-allocator/src/set.rs`
- **Issue**: `BuddySet::insert_chunk()` calculated buddy address ONCE before coalescing loop but never recalculated after each merge. After chunks coalesce from order N to N+1, the buddy address was still computed using size_of_order(N) instead of size_of_order(N+1).
- **Fix**: Changed from for-loop with enumerate to explicit loop with `current_order` tracking. After each `merge_free()`, update `current_order = chunk.order()` and recalculate `buddy_addr` with new order.
- **Commit**: 7f61fe9b

### Fix 8: SpinLock atomic swap hangs on RPi3 AXI bridge
- **File**: `ostd/src/sync/spin.rs`
- **Issue**: `AtomicBool::swap()` uses LDXR/STXR internally which fails spuriously on RPi3 AXI bridge - STXR appears to succeed locally but never becomes globally visible, causing infinite retry.
- **Fix**: Use relaxed store + fence instead of atomic swap for UP early boot.
- **Commit**: 8d55e298

## Boot Progress - 2026-07-16 (Latest)
```
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
[pools.d] got pool
[pools.e] alloc_chunk done
[cache.f] done
[FA] after cache::alloc
(SYNCHRONOUS ABORT - no further markers print)
```
**Confirmed**: Frame allocation succeeds but crash occurs AFTER `alloc_frame_with` returns.

## Mystery: Missing Marker
- Added `[node.alloc] calling alloc_frame_with` marker BEFORE `alloc_frame_with` call
- Added `[fa.0] alloc_frame_with start` marker at START of `alloc_frame_with`
- **Serial output shows**: `[fa] before get_global_frame_allocator` (INSIDE alloc_frame_with) BUT NOT the before/after markers
- This suggests markers in the function prologue or call setup are being lost/dropped

## Crash Analysis
- **Crash state**: Synchronous Abort at `0x3af610a8`, `x0 = 0`
- **elr**: `fffffffffcc900a8` (reloc) / `000000003af610a8` (actual)
- **Code at crash**: `3af61060` (instruction that faulted)
- **Frame allocator returns successfully** - `[FA] after cache::alloc` prints
- **Crash happens AFTER** `alloc_frame_with` completes but BEFORE caller continues

## Files Modified (this session)
- `ostd/src/boot/mod.rs` - BootInfo zero-allocation
- `ostd/src/mm/frame/allocator.rs` - EARLY_INFO fix, debug markers
- `ostd/src/mm/frame/meta.rs` - EARLY_INFO fix, debug markers
- `ostd/src/mm/kspace/mod.rs` - pl011_puts markers
- `ostd/src/sync/spin.rs` - Relaxed store SpinLock fix
- `ostd/src/mm/page_table/node/mod.rs` - Debug markers
- `osdk/deps/frame-allocator/src/set.rs` - Buddy coalescing bug fix

## Commits on aarch64_support
- b97c5178 debug: add markers to trace Synchronous Abort after frame allocation
- 8d55e298 aarch64/SpinLock: use relaxed store instead of atomic swap
- 7f61fe9b aarch64/buddy: fix buddy address recalculation after coalesce
- ec50b486 mm/kspace: replace early_marker with pl011_puts

## Next Steps
1. Investigate why markers inside `alloc_frame_with` prologue are missing
2. Add marker AFTER `alloc_frame_with` returns to confirm the return succeeds
3. Check if crash is in the `Result` unwrapping or subsequent operations
4. Consider if `paddr_to_vaddr` or metadata slot access is causing the crash
