# RPi3 Boot Debug Recovery Notes (2026-07-14)

## Current Problem
Kernel hangs in `mm::kspace::init_kernel_page_table(meta_pages)` when trying to allocate a frame for a new kernel page table.

## Hang Location (Pinpointed)
1. `init_kernel_page_table()` calls `PageTable::new_kernel_page_table()`
2. `new_kernel_page_table()` calls `PageTableNode::alloc(level)`
3. `PageTableNode::alloc()` calls `FrameAllocOptions::new().alloc_frame_with(meta)`
4. `alloc_frame_with()` calls `get_global_frame_allocator().alloc(layout)`
5. The hang happens INSIDE `get_global_frame_allocator()` - it never reaches `FrameAllocator::alloc()`

## What's Working
- `spin::Once` → `SimpleOnce` replacement complete
- `Vec::with_capacity(0)` works (no actual allocation)
- Empty `String::new()` works (no allocation)
- Frame allocator was successfully initialized earlier
- The early frame allocator works for boot allocations

## What's Failing
- Frame allocation through `FrameAllocator::alloc()` in the global frame allocator
- The hang is INSIDE `get_global_frame_allocator()` itself - before reaching `FrameAllocator::alloc()`
- The `log::info!()` added to `FrameAllocator::alloc()` was NEVER printed

## Key Serial Output (last successful markers):
```
[kspace] W: start
[pt] 0: start
[ptnode] alloc: start
[ptnode] alloc: meta created
[falloc] start
[falloc] layout created
[falloc] calling global allocator
```
(Hangs after "calling global allocator" - INSIDE `get_global_frame_allocator()`)

## Suspected Root Cause
`get_global_frame_allocator()` does:
```rust
unsafe { __GLOBAL_FRAME_ALLOCATOR_REF }
```

Where `__GLOBAL_FRAME_ALLOCATOR_REF` is a `static` created by the `#[global_frame_allocator]` macro:
```rust
#[no_mangle]
static __GLOBAL_FRAME_ALLOCATOR_REF: &'static dyn GlobalFrameAllocator = &#static_name;
```

The hang is likely due to:
1. The `__GLOBAL_FRAME_ALLOCATOR_REF` static being in an unmapped memory region
2. The reference value being invalid/null
3. Some linker script issue placing the static incorrectly

## Files Changed
- `ostd/src/boot/mod.rs` - SimpleOnce implementation + debug markers
- `ostd/src/arch/aarch64/cpu/extension.rs` - SimpleOnce
- `ostd/src/cpu/local/mod.rs` - SimpleOnce for CPU_LOCAL_STORAGES
- `ostd/src/mm/frame/allocator.rs` - Fixed boot_info() → EARLY_INFO.get() + debug markers
- `ostd/src/mm/frame/meta.rs` - Fixed boot_info() → EARLY_INFO.get()
- `ostd/src/mm/kspace/mod.rs` - Added debug markers
- `ostd/src/mm/page_table/mod.rs` - Added debug markers
- `ostd/src/mm/page_table/node/mod.rs` - Added debug markers
- `osdk/deps/frame-allocator/src/lib.rs` - Added debug markers

## Next Debug Steps
1. Check if `__GLOBAL_FRAME_ALLOCATOR_REF` is accessible (add marker before the static load)
2. Check the linker script for the static's section placement
3. Consider if the global frame allocator is being referenced before it's initialized
4. Try bypassing the global frame allocator and using the early allocator directly
