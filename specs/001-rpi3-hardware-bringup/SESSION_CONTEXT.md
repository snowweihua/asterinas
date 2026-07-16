# RPi3 Debug Session - 2026-07-16 (Morning)

## Current Task T030
Kernel hangs somewhere after `[init.8] after cpu::init_on_bsp` - needs further markers to pinpoint.

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

## Boot Progress
Kernel now reaches `[IAH.1] inside call_once before BootInfo` on RPi3!

Sequence that works:
```
[init.0] start
[init.1] enable_cpu_features done
[init.1b] before init_early_allocator
[init.2] init_early_allocator done
[init.3] before serial::init
[init.4] after serial::init
[init.5] before logger::init
[init.6] after logger::init
[init.7] before cpu::init_on_bsp
[cpu.1] init_on_bsp start
[cpu.2] count_processors done
[cpu.3] before copy_bsp_for_ap
[cpu.4] after copy_bsp_for_ap
[cpu.5] before set_this_cpu_id
[cpu.6] after set_this_cpu_id
[cpu.7] before init_num_cpus
[cpu.8] after init_num_cpus
[init.8] after cpu::init_on_bsp
[init.9] after meta::init
[init.A] after allocator::init
[IAH] START
[IAH] TEST_ONCE start
[IAH] TEST_ONCE inside
[IAH] TEST_ONCE done
[IAH] got EARLY_INFO
[IAH] before INFO.call_once
[IAH.1] inside call_once before BootInfo
```
(Hangs inside BootInfo construction - `to_string()` or `to_vec()` calls failing)

## Next Steps
1. Investigate BootInfo construction hang - likely heap allocation issue during `to_string()`/`to_vec()`
2. T030 will be complete when `/ #` prompt appears

## Commits on aarch64_support
- dcce6611 docs: add commit-on-progress rule to AGENTS.md
- 63a5a48d aarch64/cpu: add cpu.1-8 markers to narrow hang in init_on_bsp
- 8349f815 aarch64/mm: use EARLY_INFO instead of boot_info in EarlyFrameAllocator
- fd3875b3 aarch64/cpu: replace spin::Once with SimpleOnce in copy_bsp_for_ap
