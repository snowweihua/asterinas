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
- Crash now AFTER [mac.3] in `alloc_frame_with` generic Result return, NOT in `add_free_memory`
- With no-op `add_free_memory`, AFM cycles complete without crash — confirms add_free_memory is crash-prone, not essential during init
- Crash always at `elr=0x3af610a8` — same address every time
- "Synchronous Abort" handler is TF-A/U-Boot, NOT our kernel trap handler

### x30 Corruption Analysis
- **Always deterministic** — crash address `0x3af610a8` never changes despite code modifications
- Affects: generic `Result<Frame<M>, Error>` return, vtable/closure calls
- `alloc_segment_with` (generic `Result<Segment<M>>`) WORKS — not all generic Results crash, Frame vs Segment layout difference?
- DAIFSet/DAIFClr corruption fixed with LR save/restore using x9 temp register (no stack ops)
- `extern "C"` calling convention didn't fix
- `target-cpu=cortex-a53` via Docker build didn't help (cached)
- C ABI, raw pointer access, try_lock() — all crash at same address

### Next Actions
1. Make `alloc_frame_with` avoid generic Result return — use non-generic helper with raw pointer
2. Or: use `extern "C"` returning i64 (scalar) instead of Result<Frame<M>> 
3. Use `alloc_segment_with(1)` + raw paddr extraction as workaround
