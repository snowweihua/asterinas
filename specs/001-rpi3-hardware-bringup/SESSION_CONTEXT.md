# RPi3 Debug Session - 2026-07-21 (Session 3)

## Current Task T030
Boot to shell with interactive command working.

## Current Status: Function Return Crash
**Symptom**: Synchronous Abort at function return on RPi3
- ESR=0x02000000, ELR=0x3af610a8 (or 0x3af620a8), x29=0x3, x26/x28=0xd00dfeed
- Crash happens at `ret` instruction of `alloc_frame_with` (and other functions)
- **Bypass active**: `alloc_frame_with` returns `Err(NoMemory)` early → crash STILL occurs
- This proves corruption happens DURING function execution (stack frame setup/teardown), NOT in allocator logic

## Progress Summary

### Fixes Applied (verified working)
- Fix 10: `frame_paddr_base()` calls `dram_base()` (runtime, not const)
- Fix 11: `get_slot` uses `!IN_BOOTSTRAP` path selection
- Fix 12: Reverted `get_slot` slot formula to correct `frame_idx`-based calculation
- Fix 13: Path selection changed to `!IN_BOOTSTRAP` (no fpb check)
- Fix 14: `mark_unusable_ranges` uses `EARLY_INFO.get()` (not `boot_info()`)
- Fix 15: Stripped verbose per-frame debug markers

### What Works Now
- `meta::init()` completes successfully
- `allocator::init()` completes (with `add_free_memory` no-op)
- `init_after_heap()` completes
- `kspace::init()` starts and reaches `alloc_frame_with`

### Current Bypasses (will need proper fix)
- `FrameAllocator::alloc` → returns `None` early
- `FrameAllocator::add_free_memory` → debug markers only
- `pools::add_free_memory` → `return;` (no-op)
- `alloc_frame_with` → returns `Err(NoMemory)` early
- `disable_local()` → no-op on AArch64
- `enable_local()` → no-op on AArch64

## Crash Analysis

### Consistent Crash Signature
- **ESR**: 0x02000000 (Synchronous Abort, ISS=0)
- **ELR/LR**: 0x3af610a8 (~944MB, near top of 948MB RAM)
- **x29**: 0x3 (corrupted frame pointer)
- **x26/x28**: 0xd00dfeed (memory poisoning pattern)

### Key Observations
1. 0x3af610a8 is NOT in kernel binary, initramfs, or valid code region
2. x29=0x3 is NOT random — specific small value suggesting deliberate write
3. x26/x28=0xd00dfeed is "dead feed" poisoning pattern from early allocator
4. With ALL bypasses active, crash STILL occurs at function return point
5. The crash is at the `ret` instruction itself, not inside the function body

### Hypothesis
**Stack corruption** or **code page mapping corruption** happening during kernel bootstrap. The corrupted frame pointer (x29=0x3) is saved on the stack, and when the function tries to return (`ret`), it loads this corrupted value into PC.

## Files Modified (current state)
- `ostd/src/arch/aarch64/mm/mod.rs` - `frame_paddr_base()` calls `dram_base()`
- `ostd/src/mm/frame/meta.rs` - Debug markers, slot formula, path selection
- `ostd/src/mm/frame/allocator.rs` - Early return bypass in `alloc_frame_with`
- `ostd/src/arch/aarch64/irq.rs` - `disable_local`/`enable_local` made no-ops
- `osdk/deps/frame-allocator/src/lib.rs` - `alloc` returns `None` early
- `osdk/deps/frame-allocator/src/pools/mod.rs` - `add_free_memory` no-op

## Remaining Work

### Immediate: Investigate Stack Corruption
1. **Check `init_after_heap()`**: Located at `ostd/src/boot/mod.rs:147`. Sets up `INFO` (boot info) — might corrupt stack
2. **Check `kspace::init_kernel_page_table`**: Builds page tables using `meta_pages` — could corrupt memory
3. **Add stack canary**: Write known pattern before/after return address slot, check if corrupted
4. **Check `boot_stack_top`**: Verify stack pointer at function entry

### After Crash Fixed
1. Remove all bypasses one by one
2. Fix `add_free_memory` properly
3. Fix `alloc_frame_with` properly
4. Remove `disable_local`/`enable_local` no-ops
5. Boot to shell prompt

## Next Action
Investigate `init_after_heap()` and `kspace::init_kernel_page_table()` for stack corruption. These are the two functions that execute between "kernel works" and "crash occurs".

## Key Files
- `ostd/src/boot/mod.rs:147` - `init_after_heap()`
- `ostd/src/mm/kspace/mod.rs` - `kspace::init_kernel_page_table()`
- `ostd/src/mm/frame/meta.rs` - metadata initialization
- `ostd/src/mm/frame/allocator.rs` - frame allocation
- `osdk/deps/frame-allocator/src/lib.rs` - FrameAllocator implementation

## Git Log (recent)
```
52b7215d aarch64: bypass alloc_frame_with — crash at function return persists
745a155c aarch64: session 2026-07-21 final state — kernel reaches kspace::init
d0e1abbb aarch64: bypass FrameAllocator::alloc (crash persists in virtual call)
15d5841f aarch64: make enable_local no-op (no effect, crash persists at return)
aaabcea6 aarch64: confirmed alloc return crash (no progress, same crash location)
8c45d3c8 aarch64: bypass pools::add_free_memory — kernel reaches init_after_heap and kspace::init
92611588 aarch64/rpi3: strip debug markers from get_slot/get_from_unused — meta::init() now completes
2e682946 aarch64/meta: fix mark_unusable_ranges — use EARLY_INFO instead of boot_info()
29d50912 aarch64/rpi3: BREAKTHROUGH — meta::init() completes with mark_unusable_ranges no-op
ad728eb4 aarch64/meta: add early_alloc region debug markers to pinpoint crash location
```