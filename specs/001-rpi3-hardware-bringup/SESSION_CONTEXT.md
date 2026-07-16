# RPi3 Debug Session - 2026-07-16

## Current Task T030
Kernel hangs at `INFO.call_once` in `init_after_heap()` - investigating panic handler behavior.

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
- **Also**: Added `is_completed()` and `get_unchecked()` methods to `SimpleOnce` for API compatibility
- **Commit**: fd3875b3

### Fix 3: `mm/frame/allocator.rs` - boot_info vs EARLY_INFO
- **File**: `ostd/src/mm/frame/allocator.rs`
- **Issue**: `EarlyFrameAllocator::new()` called `boot_info()` which uses `INFO` Once (not initialized until `init_after_heap`)
- **Fix**: Changed to use `crate::boot::EARLY_INFO.get()` instead
- **Commit**: bd1d133f

### Fix 4: `mm/frame/meta.rs` - boot_info vs EARLY_INFO
- **File**: `ostd/src/mm/frame/meta.rs`
- **Issue**: Same as Fix 3
- **Fix**: Changed to use `crate::boot::EARLY_INFO.get()` instead

## Boot Progress - Latest (2026-07-16)
```
[IAH] START
[IAH] TEST_ONCE start
[IAH] TEST_ONCE inside
[IAH] TEST_ONCE done
[IAH] got EARLY_INFO
[IAH] before INFO.call_once
[IAH.1] inside call_once
[IAH.X] skipping BootInfo construction
```
**Confirmed**: `INFO.call_once` and `SimpleOnce` work correctly on RPi3.
**Confirmed**: The hang is inside `BootInfo` construction - heap allocations needed for `String` fields.

## Test Code in `init_after_heap()`
Currently testing panic handler behavior with simplified test:
```rust
unsafe { crate::arch::boot::pl011_puts(b"[IAH] before immediate panic\n"); }
panic!("IAH: immediate panic test");
```

## Known Issues (from known-issues.md)
1. **PL011 UART**: Currently using mini UART, need to enable PL011
2. **SimpleOnce revert**: Should investigate reverting SimpleOnce to spin::Once after RPi3 boot works

## Next Steps
1. Verify panic handler works on RPi3 (test in progress)
2. If panic works: construct `BootInfo` without heap using `&'static str` instead of `String`
3. If panic hangs: investigate panic handler on aarch64
4. T030 complete when `/ #` prompt appears

## Files Modified
- `ostd/src/boot/mod.rs` - Test code in `init_after_heap()`
- `ostd/src/mm/frame/allocator.rs` - EARLY_INFO fix
- `ostd/src/mm/frame/meta.rs` - EARLY_INFO fix

## Commits on aarch64_support
- dcce6611 docs: add commit-on-progress rule to AGENTS.md
- 63a5a48d aarch64/cpu: add cpu.1-8 markers to narrow hang in init_on_bsp
- 8349f815 aarch64/mm: use EARLY_INFO instead of boot_info in EarlyFrameAllocator
- fd3875b3 aarch64/cpu: replace spin::Once with SimpleOnce in copy_bsp_for_ap
- 30e96465 docs: add known-issues.md with PL011 UART and SimpleOnce issues
