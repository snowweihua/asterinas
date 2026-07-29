# RPi3 Debug Session - 2026-07-29 (Session 11)

## Current Task
Fix x30 (link register) corruption on RPi3 Cortex-A53 — root cause of ALL hangs and crashes.

## Session 11 Summary

### CRITICAL DISCOVERY: x30 Corruption is Sysmatic
The crash address `0x3af610a8` is in `.eh_frame_hdr` section, NOT executable code. x30 gets corrupted to point at data, causing instruction abort on `ret`. This is deterministic — always the SAME address.

### Root Cause Chain
1. Four separate functions were STUBBED OUT as workarounds for x30 corruption:
   - `alloc_frame_with()` → infinite loop
   - `FrameAllocator::alloc()` → always returns None
   - `add_free_memory()` → no-op (never adds memory to buddy pool)
   - `disable_local()` / `enable_local()` → no-op (DAIFSet/DAIFClr corrupts LR)

2. These stubs caused:
   - `component::init_all` hang (frame alloc infinite loop → heap alloc hangs)
   - No memory available for allocations (all pools empty)
   - No IRQ disabling (spin locks unprotected)

### FIXES Applied
| Function | Original | Fix | Status |
|----------|----------|-----|--------|
| `disable_local/enable_local` | No-op | Save/restore LR around DAIFSet/DAIFClr | ✓ WORKS |
| `alloc_frame_with` | Infinite loop | Delegate to `alloc_segment_with` | ✓ Returns (not hangs) but x30 crash on return |
| `FrameAllocator::alloc` | Returns None | Use cache/pools allocation path | ✓ Allocates frames successfully |
| `add_free_memory` | No-op | Add to GLOBAL_POOL via OnDemandGlobalLock | ✗ Crashes at `SpinLock::lock()` |
| `pools::alloc` | AArch64 separate path | Unified with standard path | ✓ Code unified |

### Current State
- Frame allocator CACHE and POOLS work — 3 successful allocations observed
- Crash at `[pafm.1]` during `add_free_memory` → `global_pool.get()` → `SpinLock::lock()`
- Crash always at `elr=0x3af610a8` (same address every time)
- `alloc_frame_with` delegates to `alloc_segment_with` — allocations succeed, but generic Result return still crashes

### x30 Corruption Analysis
- **Always deterministic** — crash address `0x3af610a8` never changes
- Affects: generic `Result<X, Error>` returns, trait object vtable calls, closures
- `alloc_segment_with` (non-generic Result) works — allocations succeed
- DAIFSet/DAIFClr corruption fixed with LR save/restore
- Attempted `target-cpu=cortex-a53` via `.cargo/config.toml` — ignored by Docker OSDK build

### Files Modified
- `ostd/src/arch/aarch64/irq.rs`: disable_local/enable_local with LR save/restore
- `ostd/src/mm/frame/allocator.rs`: alloc_frame_with delegates to alloc_segment_with
- `osdk/deps/frame-allocator/src/lib.rs`: FrameAllocator::alloc restores cache/pools
- `osdk/deps/frame-allocator/src/pools/mod.rs`: add_free_memory, unified alloc
- `ostd/src/lib.rs`: `#![allow(unsafe_op_in_unsafe_fn)]` for edition 2024
- `kernel/src/lib.rs`: Debug markers for alloc/frame test
- `kernel/libs/comp-sys/component/src/lib.rs`: Debug markers, mini_uart_puts
- All `kernel/comps/*/src/lib.rs`: `#[init_component]` debug markers

### Git Log (Recent)
```
464afdd6 aarch64/x30: investigate x30 corruption — crash always at 0x3af610a8
06711cda aarch64/pools: fix add_free_memory no-op
6db6eacf aarch64/frame_alloc: restore FrameAllocator::alloc cache/pools path
eee02302 aarch64/frame_alloc: fix alloc_frame_with infinite loop
9c4b108e aarch64/irq: implement disable_local/enable_local with LR save/restore
e75714e1 debug: isolate component::init_all hang to heap allocator
```

### Known Blockers
1. **x30 corruption is systemic** — individual LR save/restore fixes don't scale
2. **Cannot modify compiler/linker flags** — Docker OSDK build ignores `.cargo/config.toml`
3. **Crash at 0x3af610a8** in `SpinLock::lock()` during `add_free_memory` prevents pool population

### Next Actions
1. Find where Docker/OSDK sets compiler/linker flags (MCP build server config?)
2. Add `-C target-cpu=cortex-a53` or `--fix-cortex-a53-843419` linker flag
3. As fallback: implement assembly-level LR protection wrapper for critical paths
4. Or: fix `add_free_memory` to bypass `LOCAL_POOL`/`SpinLock` during init phase
