# RPi3 Hardware Bring-Up Context

This file records durable findings only. Temporary probe chronology and superseded hypotheses are intentionally omitted.

## Current Status

- Target: Raspberry Pi 3 Model B, AArch64, single-core runtime.
- Boot reaches kernel component initialization on the physical board.
- Verified through the latest clean image:
  - metadata mapping completes;
  - kernel page-table activation completes;
  - all 12 static component records are discovered and sorted;
  - bootstrap components reach `[cmp.systree] init`.
- The latest clean image stops intermittently during a later allocator refill before the logger component. No new exception was observed in those runs.
- Logger post-fix hardware verification is therefore partial.

## Durable RPi3 Constraints

### Exclusive Atomics Are Unsafe During Bring-Up

The RPi3 setup faults on Cortex-A53 exclusive operations (`ldxr`, `ldaxr`, `stxr`, `ldaxrb`, and related CAS loops) even when the target memory is otherwise readable and writable. The board is deliberately treated as single-core during this bring-up.

Confirmed affected areas include:

- page-table node locks;
- frame reference-count transitions;
- allocator free-size updates;
- heap slab reference-count initialization;
- runtime `Once` initialization;
- Arc weak-count updates;
- component inventory registration;
- softirq enabled-mask updates.

The project uses plain load/store or boot-safe single-core helpers only at runtime-confirmed RPi3 boundaries. Generic atomic behavior remains unchanged for other targets.

## Confirmed Fixes

### Page-Table Metadata Subtree

- The new kernel page table must not copy the boot slot-0 descriptor into metadata root index `0x1c0`.
- That copied descriptor caused the metadata cursor to borrow static boot page tables whose metadata had the wrong type or poison level.
- The metadata root is now left empty so the cursor allocates a managed subtree from the reserved bootstrap page-table pool.
- Hardware evidence:
  - managed cursor subtree reached successfully;
  - `[kspace.m4] metadata mapped`;
  - `[init.C] after kspace::init`;
  - `[init.5a] after activate_kernel_page_table`.

### Bootstrap Page-Table Pool

- Boot page-table frames are reserved from a managed pool and initialized with the correct `PageTablePageMeta` level.
- The pool uses plain mutable state during single-core bootstrap.
- The former bootstrap lock fault was confirmed at `PageTableNodeRef::lock()` and bypassed only in bootstrap context.

### Frame Allocator and Metadata Pointers

- Intrusive buddy-list links created before managed page-table activation can retain physical metadata pointers.
- `LinkedList::take_current()` restores the frame from its live metadata pointer.
- `MetaSlot::frame_paddr()` distinguishes retained low physical pointers from mapped high virtual metadata pointers.
- Hardware confirmed local buddy allocation, balancing, frame-cache refills, buddy splitting, right-child insertion, and `pools::alloc()` completion.
- Several apparent allocator boundaries were UART observability artifacts; no allocator workaround was added for them.

### RPi3-Safe Initialization

- `ostd::sync::Once` dispatches to `SimpleOnce` on RPi3 and retains `spin::Once` elsewhere.
- Confirmed migrations include task handlers, scheduler state, user page-fault handling, RNG, bootstrap component singletons, and bottom-half handlers.
- `arc_new_cyclic()` and `weak_clone()` provide the corresponding single-core Arc path for systree construction.

### Static Component Registration

- AArch64 does not execute the normal `.init_array` registration path.
- Component records are emitted into a retained `.component_registry` linker section and enumerated directly on AArch64.
- The OSDK run-base cache includes generated linker scripts so linker changes invalidate stale generated bases.
- Hardware confirms 12 records are discovered and sorted.

### Component Metadata Allocation

- Generated component names and paths are static literals, but the old implementation copied them into owned `String` values during bootstrap.
- `ComponentInfo` now stores `&'static str` and registry matching uses borrowed path slices.
- Hardware subsequently completed metadata parsing, registry matching, sorting, and component calls through block, console, input, PCI, softirq, and systree.

### Logger Backend

- The original logger failure was captured as an EL1 synchronous abort in `spin::once::Once::try_call_once_slow`.
- The faulting operation was the `spin::Once` exclusive state transition in the OSTD logger injection path.
- `ostd/src/logger.rs` now uses boot `SimpleOnce`.
- This is the minimal mechanism-matched fix, but the clean post-fix image has not yet reached `[cmp.logger] init`; do not claim full logger toggle verification yet.

## Latest Hardware Evidence

- Boot now reaches `[cmp.block] init` deterministically on the committed image.
- All six bootstrap components are discovered, sorted, and dispatched.
- The hang moved to the logger `info!` allocation that runs after the first component init.

## Root Cause: 16-Byte Register Returns Corrupt x30

- The RPi3's Cortex-A53 corrupts the link register (x30) when a function returns a
  16-byte aggregate in registers (`Result<Frame<M>>`, `Result<UniqueFrame<M>>`,
  `Option<Paddr>`, `(FreeChunk, FreeChunk)`), depending on instruction alignment.
- The codebase already documented this at `unique.rs:44-50` and
  `page_table/mod.rs:37-44` ("16-byte generic `Result<UniqueFrame<M>>` return
  that triggers a Cortex-A53 epilogue bug (x30 corruption)").
- The frame-allocator hot path returned 16-byte aggregates everywhere, so the
  hang location moved with instrumentation and code layout.
- Fixes eliminate those returns by using a single-register `*const ()`/`Paddr`
  sentinel pattern:
  - `split_free` returns a single `FreeChunk` instead of `(FreeChunk, FreeChunk)`.
  - `alloc_chunk`, `pools::alloc`, `CacheArray::alloc`, `cache::alloc`, `pop_front`
    return `Paddr` with `NO_PADDR` sentinel instead of `Option<Paddr>`.
  - `GlobalFrameAllocator::alloc` returns `Paddr` with `NO_PADDR` sentinel.
  - `alloc_frame_with`/`Slab::new` route through `Frame::init_unused`/
    `UniqueFrame::init_unused` (single-register) internally.
- Commits: `59081c80`, `f47a99af`, `b014a696`.
- Remaining 16-byte register returns in the heap/frame path:
  `MetaSlot::get_from_unused`, `Frame::from_unused`, `UniqueFrame::from_unused`,
  and the public `Result`-returning wrappers. These are the next targets.

## Operational Notes

- Required physical verification sequence:
  - build with the AArch64 OSDK image;
  - convert ELF to `/tmp/asterina.img`;
  - deploy to `/mnt/d/pi_sd/asterina.img`;
  - power off;
  - clear serial buffer;
  - power on;
  - wait approximately 60 seconds;
  - read serial repeatedly until empty.
- Runtime evidence is authoritative; do not promote a suspected boundary to a root cause without a reproducible observation and a toggle or equivalent causal proof.
- Temporary UART probes and `.debug-journal.md` must not be retained in the working tree.

## Next Investigation

- Eliminate the remaining 16-byte register returns in the heap/frame path:
  `MetaSlot::get_from_unused`, `Frame::from_unused`, `UniqueFrame::from_unused`,
  and the public `Result`-returning wrappers, using the single-register
  `*const ()`/`Paddr` sentinel pattern.
- Reach `[cmp.logger] init` and verify that logger initialization completes
  without the former `spin::Once` abort.
- Keep changes scoped to runtime-confirmed RPi3 failures; do not globally replace
  remaining `spin::Once` uses without hardware evidence.
