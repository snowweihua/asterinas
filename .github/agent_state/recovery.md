# RPi3 Boot Debug Recovery Notes (2026-07-15)

## Current Problem
Kernel hangs during the first component's frame-cache refill, inside `osdk_frame_allocator::pools::alloc`.

## Hang Location (Pinpointed)
1. `match_and_call` dispatches a bootstrap component (e.g. block/console/input/PCI/softirq/systree).
2. The component init path calls `FrameAllocOptions::new().alloc_frame_with(meta)`.
3. `alloc_frame_with()` calls `get_global_frame_allocator().alloc(layout)`.
4. The concrete `FrameAllocator::alloc()` routes through `cache::alloc` and `CacheArray::alloc`.
5. `CacheArray::alloc()` cache miss prints `[CA.c] pop_front miss, calling pools::alloc` and calls `pools::alloc()`.
6. No further markers appear (no `[CA.d]`, no `[EL1-SYNC]`, no `[spin.L]`); execution stops inside `pools::alloc`.

## What's Working
- All boot-time frame/heap setup through `allocator::init`, `kspace::init`, and `activate_kernel_page_table`.
- Static component discovery and sorting (`[cmp.*]` dispatch sequence).
- CPU-local cache fast path (`[CA.b] pop_front hit`) for at least the first few frames.

## What's Failing
- `pools::alloc()` during the first cache refill after component dispatch.
- The failure is downstream of the previous `get_global_frame_allocator()`/early-alloc suspicion; the global allocator reference is resolved and the allocator body is executing.

## Key Serial Output (last successful markers)
```
[cache.a] start
[cache.b] before CACHE.get_with
[cache.c] after CACHE.get_with
[cache.d] before borrow_mut
[cache.e] after borrow_mut
[CA.a] start
[CA.b] pop_front hit
[CA.a] start
[CA.b] pop_front hit
...
[CA.a] start
[CA.c] pop_front miss, calling pools::alloc
```

## Suspected Root Cause
A 16-byte aggregate function return on the allocator hot path corrupts x30 and causes the eventual `ret` from `pools::alloc` (or one of its callees) to land in `.eh_frame`/invalid memory, producing a silent SError/abort routed to EL3.

Candidate 16-byte returns on this path:
- `MetaSlot::get_slot()` returns `Result<&'static MetaSlot, GetFrameError>` (16 bytes, currently `#[inline(never)]`).
- `MetaSlot::get_from_in_use()` returns `Result<*const Self, GetFrameError>` (16 bytes).
- Public `Frame::from_unused`/`UniqueFrame::from_unused` `Result` wrappers (not hit in this exact path but latent).

## Current Experiment
- Mark `MetaSlot::get_slot` as `#[inline(always)]` so its `Result` is folded into callers and its standalone `ret` disappears.
- Rebuild, deploy, and verify whether the boot boundary moves.

## Files Changed (in this experiment)
- `ostd/src/mm/frame/meta.rs` — `get_slot` from `#[inline(never)]` to `#[inline(always)]`.

## Next Debug Steps
1. Verify whether inlining `get_slot` moves the hang.
2. If the hang moves, continue eliminating remaining 16-byte returns (`get_from_in_use`, `Frame::from_unused`, `UniqueFrame::from_unused`, `alloc_frame_with`).
3. If the hang persists at `[CA.c]`, use the `set.rs` nop compensation block to shift `pools::alloc`/`alloc_chunk` `ret` alignment.
