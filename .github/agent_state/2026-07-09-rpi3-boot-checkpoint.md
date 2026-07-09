# RPi3 Boot Investigation - 2026-07-09 Checkpoint

## Root Cause Identified: Early Allocator Exhaustion

### The Problem
RPi3 boot hangs at marker `W` (inside `init_kernel_page_table` → `PageTable::new_kernel_page_table`).

### Root Cause Chain

1. **RPi3 has no memory above 4GB** - All 512MB-1GB RAM is at PA 0x00000000-0x20000000

2. **`frame_paddr_base = 0x40000000`** (1GB) - set for QEMU virt which has high memory

3. **`tot_nr_frames = 0`** on RPi3 because `max_paddr (0x20000000) < frame_paddr_base (0x40000000)`

4. **Early allocator reserves ALL under-4G memory** - `EarlyFrameAllocator::new()` finds the largest under-4G region and reserves it all for early allocations

5. **Early return in `meta::init()`** (my fix) - returns empty segment when `tot_nr_frames == 0`, so no metadata frames allocated

6. **`PageTable::new_kernel_page_table()` crashes** - tries to allocate page table frames via `FrameAllocOptions::alloc_frame_with()`, but the frame allocator has NO free frames because:
   - All under-4G memory was reserved by early allocator
   - My `allocator::init()` fix skips adding memory below `frame_paddr_base`, so nothing was added to the frame allocator
   - The `expect("Failed to allocate a page table node")` panics

### Why Previous Approaches Didn't Work

| Approach | Result |
|----------|--------|
| `frame_paddr_base = 0` | `meta::init()` maps metadata at VA `frame_to_meta(0)` which doesn't exist; also `tot_nr_frames` overflow issues |
| `frame_paddr_base = 0x40000000` with `allocator::init()` fix | Frame allocator empty because early allocator consumed all memory |
| Early return in `meta::init()` when `tot_nr_frames == 0` | `PageTable::new_kernel_page_table()` still crashes - no frames available |

### Key Code Paths

```
lib.rs:init_in_first_kthread:
  cpu::init_on_bsp()            → marker 6
  mm::frame::meta::init()       → marker M (early return if tot_nr_frames==0)
  mm::frame::allocator::init()   → marker N (skips ranges below frame_paddr_base)
  mm::kspace::init_kernel_page_table() → marker W (CRASH here)
    PageTable::new_kernel_page_table() → marker X (never reached)
      PageTableNode::alloc() → FrameAllocOptions::alloc_frame_with() → PANIC
```

### Files Modified (this session)

1. `ostd/src/arch/aarch64/mm/mod.rs` - `frame_paddr_base()` kept at `0x40000000`
2. `ostd/src/mm/frame/meta.rs` - Added early return when `tot_nr_frames == 0`
3. `ostd/src/mm/frame/allocator.rs` - Skip adding memory ranges below `frame_paddr_base`
4. `ostd/src/mm/kspace/mod.rs` - Added debug markers W, X, Y, Z
5. `ostd/src/lib.rs` - Added debug markers M, N, P, S, T, U, V
6. `ostd/src/logger.rs` - Changed default log level from Off to Info (T030 fix)

### Debug Probes Still in Code

**lib.rs:**
- Marker '6' after `cpu::init_on_bsp()`
- Marker 'M' after `meta::init()`
- Marker 'N' after `allocator::init()`
- Marker 'P' after `kspace::init_kernel_page_table()`
- Markers 'S', 'T', 'U', 'V' around `sync::init()` and `boot::init_after_heap()`

**kspace/mod.rs:**
- Marker 'W' at start of `init_kernel_page_table()`
- Marker 'X' after `new_kernel_page_table()`
- Marker 'Y' after linear mapping block
- Marker 'Z' after metadata mapping block

### T030 Fix Applied ✅

Fixed in commit `27e0314e`:
- Changed default log level from `LevelFilter::Off` to `LevelFilter::Info` in `ostd/src/logger.rs`
- Also uses `set_logger_racy` for AArch64 RPi3 (CAS hangs on Device memory)

**Root cause**: `get_log_level().unwrap_or(LevelFilter::Off)` returned Off when `ostd.log_level=` not in cmdline, disabling all logging.

### Remaining Issues (T031)

1. **QEMU test has no output** - `qemu-system-aarch64 ... -nographic` produces no serial output locally (T031)
2. **RPi3 crash at marker W** - Early allocator exhausts all memory before `PageTable::new_kernel_page_table()` can allocate

### Suggested Next Steps

1. **Fix T031 (QEMU output)** - Get QEMU serial output working for faster iteration
2. **Fix RPi3 early allocator issue** - The early allocator reserves ALL under-4G memory on RPi3 (no high memory), leaving no frames for page table allocation

### Related Files to Check

- `ostd/src/lib.rs` - early_marker() function definition
- `ostd/src/arch/aarch64/serial.rs` - UART initialization
- `kernel/src/logger.rs` - logger initialization
- `ostd/src/mm/frame/allocator.rs` - EarlyFrameAllocator::new()
- `ostd/src/mm/page_table/node/mod.rs` - PageTableNode::alloc()
