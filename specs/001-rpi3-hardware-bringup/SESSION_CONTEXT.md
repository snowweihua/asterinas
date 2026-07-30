# RPi3 Debug Session - 2026-07-29 (Session 12)

## Current Task
Fix `add_free_memory()` stub in `pools/mod.rs` to actually add free memory to the buddy allocator — and resolve the deterministic "Synchronous Abort" crash at `0x3AF610A8`.

## Session 12 Summary

### New Discovery: `add_free_memory` is a Stub
`pools::add_free_memory()` at `osdk/deps/frame-allocator/src/pools/mod.rs:113` is a **stub** — takes `addr`/`size` but does nothing. Body is only a debug print for aarch64. This means ALL free memory (kernel image, metadata, free RAM) is NEVER added to the buddy allocator.

### Impact
- Buddy allocator has **ZERO free frames** — all allocations return `Err(NoMemory)`
- `FrameAllocOptions::alloc_frame()` test prints `[mac.2c] frame alloc FAILED`
- The x30 corruption crash at `0xFFFFFFFFFC900A8`/`0x3AF610A8` is a **consequence** of the broken allocator (allocator operating on uninitialized/corrupted state), NOT a separate page table bug
- AArch64 boots past init because it uses the **early allocator** exclusively — `#[cfg(not(target_arch = "aarch64"))]` guards skip frame allocation for linear/meta/kernel mappings

### What We Did
1. **Identified the stub**: `add_free_memory` at `pools/mod.rs:113`
2. **Implemented the fix**: Uses `split_to_chunks(addr, size)` to split free memory ranges into buddy chunks, then inserts them into `LOCAL_POOL` (order < 18) or `GLOBAL_POOL` (order >= 18) via `BuddySet::insert_chunk()`
3. **Confirmed the crash**: With the fix, the crash happens EARLIER in boot — during `add_free_memory` at `[pafm.g]` (inside `local_pool.insert_chunk()`), same address `0x3AF610A8` as before

### Debug Print Trace
```
[pafm.0]  - start of add_free_memory
[pafm.a]  - after function entry
[pafm.b]  - after LOCAL_POOL.get_with(guard)
[pafm.c]  - after borrow_mut()
[pafm.d]  - after OnDemandGlobalLock::new()
[pafm.e]  - inside for_each closure (first chunk yielded)
[pafm.g]  - about to call local_pool.insert_chunk(addr, order)
CRASH at 0xFFFFFFFFFC900A8 / 0x3AF610A8
```

### Crash Analysis
| Property | Value |
|----------|-------|
| Virtual ELR | `0xFFFFFFFFFC900A8` |
| Physical ELR | `0x3AF610A8` (944 MB, top of 948 MB DRAM) |
| ESR | `0x02000000` — "Unknown reason" (likely undefined instruction) |
| x29 (FP) | `0x3` — **clearly corrupted** |
| x16 (IP0) | `0x260612C` — unusual, not a valid code pointer |
| x17 (IP1) | `0x8` — tiny, definitely corrupted |
| Code at crash | `00000031 00000000 3af50b30 00000000` — garbage data, not instructions |

- ELR == LR == `0x3AF610A8` — suggests CPU returned to a corrupted address
- x29 = 3 confirms stack corruption (frame pointer should point to previous frame)
- x16/x17 corrupted indicates function pointer dispatch or veneer issue
- ALL crashes (old code in component::init_all AND new code in add_free_memory) are at EXACTLY the same address

### Common Code Path
Both old crash path (component::init_all) and new crash path (add_free_memory → insert_chunk) call:
- `FreeChunk::from_unused()` → `UniqueFrame::from_unused()` → `MetaSlot::get_from_unused()` → `get_slot()`

The `get_slot()` function accesses frame metadata in `FRAME_METADATA_RANGE` (VA `0xFFFF_E000_0000_0000`). This is the common code that crashes.

### Key Address Ranges (aarch64, ADDR_WIDTH=48)
| Region | VA Base | Description |
|--------|---------|-------------|
| Kernel code | `0xFFFF_0000_0000_0000` | .text, .data, .bss |
| Linear mapping | `0xFFFF_8000_0000_0000` | pa → va via `pa + LINEAR_MAPPING_BASE` |
| Vmalloc | `0xFFFF_C000_0000_0000` | Dynamic kernel mappings |
| Frame metadata | `0xFFFF_E000_0000_0000` | `frame_to_meta()` translates pa to here |
| Crash VA | `0xFFFF_FFFF_FC90_00A8` | Above metadata range — NOT in any defined range |

### Hypothesis
The crash is a **stack corruption / return-address clobber** issue, NOT a page fault. The CPU jumps to `0x3AF610A8` (data in RAM) and tries to execute garbage, getting an undefined instruction exception.

Possible causes:
1. **Stack overflow** in `insert_chunk` or its callees — the buddy allocator's coalescing loop or linked-list operations overflow the kernel stack
2. **Buffer overflow** in frame metadata access — `frame_to_meta()` writes out of bounds, corrupting adjacent memory including stack
3. **Corrupted linked list** — `LinkedList::cursor_mut_at()` or `push_front()` writes to freed/corrupted memory
4. **Compiler optimization bug** — the `BuddySet::insert_chunk()` monomorphization generates bad code for aarch64-cortex-a53

### Next Actions
1. Verify `dram_base()` for RPi3 to understand frame_paddr_base
2. Check if `FRAME_METADATA_RANGE` is properly mapped (page table entries exist for `0xFFFF_E000_0000_0000`)
3. Add debug prints INSIDE `insert_chunk` / `from_unused` to narrow crash point further
4. Alternatively: test with QEMU aarch64 virt to see if crash reproduces there with more debug info
5. If QEMU works, the issue is hardware-specific (Cortex-A53 erratum or MMU/page table bug)

### Files Modified
- `osdk/deps/frame-allocator/src/pools/mod.rs`: Fixed `add_free_memory()` stub + added debug prints
