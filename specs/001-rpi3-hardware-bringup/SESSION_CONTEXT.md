# RPi3 Debug Session - 2026-07-16

## Current Task T030
Kernel hangs inside `kspace::init_kernel_page_table()` - hang is between `new_kernel_page_table()` and after it.

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

## Boot Progress - 2026-07-16 (Latest)
```
[init.B] after init_after_heap
[init.B1] before kspace::init
[kspace.W] start
```
**Confirmed**: Hang is INSIDE `PageTable::<KernelPtConfig>::new_kernel_page_table()`

## Next Steps
1. Add markers inside `new_kernel_page_table()` to narrow down exact hang location
2. T030 complete when `/ #` prompt appears

## Files Modified
- `ostd/src/boot/mod.rs` - BootInfo zero-allocation, test code
- `ostd/src/mm/frame/allocator.rs` - EARLY_INFO fix
- `ostd/src/mm/frame/meta.rs` - EARLY_INFO fix
- `ostd/src/mm/kspace/mod.rs` - pl011_puts markers
- `kernel/src/lib.rs` - Removed .as_str() call

## Commits on aarch64_support
- dcce6611 docs: add commit-on-progress rule to AGENTS.md
- 63a5a48d aarch64/cpu: add cpu.1-8 markers
- 8349f815 aarch64/mm: use EARLY_INFO instead of boot_info
- fd3875b3 aarch64/cpu: replace spin::Once with SimpleOnce
- bd1d133f aarch64/mm: use EARLY_INFO in EarlyFrameAllocator
- b35067af boot: change BootInfo to use &str instead of String
- 19d4617f boot: add [init.B1] marker before kspace::init
- 7c8176be mm/kspace: add markers inside init_kernel_page_table
- ec50b486 mm/kspace: replace early_marker with pl011_puts
