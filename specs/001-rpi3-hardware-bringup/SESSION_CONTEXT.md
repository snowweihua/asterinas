# RPi3 Debug Session - 2026-07-24 (Session 6) — DMA/GIC Skipped on RPi3, Persistent x30 Corruption Persists

## Current Task T030
Boot to shell with interactive command working.

## Session 6 Summary

### What Works (Fixed This Session)
1. **GIC skip on RPi3** (`ostd/src/arch/aarch64/gic.rs`): `gic::init_on_bsp()` now checks `BoardType::cached() != 2` before calling `GIC.call_once()`. On RPi3 (board type 2), the QEMU virt GIC hardware (at 0x08000000) is skipped. Functions that need GIC return early on RPi3.

2. **DMA init skip on AArch64** (`ostd/src/mm/dma/mod.rs`): `dma::init()` is skipped on AArch64 (no `DMA_MAPPING_SET.call_once()`). DMA mapping tracking uses `spin::Once` which uses Acquire semantics that may trigger the same x30 corruption.

3. **Progress past 4 init stages**: Kernel now boots past:
   - `sync::init()` (RCU init using SimpleOnce with Relaxed)
   - `dma::init()` (skipped on aarch64)
   - `trap::init()` (RPi3 exception vectors)
   - `gic::init_on_bsp()` (skipped on RPi3)

### What's Still Blocked (Persistent Crash)
The crash happens AFTER `[late.gic] after gic::init_on_bsp` — inside `io::construct_io_mem_allocator_builder()`:
- **Symptom**: ESR=0x02000000, ELR=0x3AF610A8 (SAME address across ALL code paths)
- **x29** = 0x3 (corrupted FP)
- **Consistency**: Same ELR regardless of which function is crashing. The crash is NOT function-specific — it happens at the SAME address during execution of ANY function that follows a specific pattern.
- **Key observation**: The crash is INSIDE `spin::Once::call_once()` before the closure runs (in gic case), OR during simple function calls (construct_io_mem_allocator_builder). The common thread is that ALL functions that involve ANY kind of atomic operation or memory access through the page table eventually trigger the crash.

### The Root Cause Hypothesis (Unconfirmed)
The crash at ELR=0x3AF610A8 appears to be a **speculative execution / page table walking issue** on Cortex-A53. The address 0x3AF610A8 falls within the kernel text region. The MMU page table walk for ANY memory access may trigger a speculative fetch that hits the peripheral MMIO region (0x3F000000+) through a incorrectly attributed page table entry, corrupting the link register (x30) on the stack.

The boot page tables use `boot_l2pt_gb0` for the first 1GB, which correctly maps 0x3F000000+ as Device memory. But there may be OTHER page table entries (e.g., in the new kernel page table, or in the linear mapping table) that still have 1 GiB block entries with Normal memory attributes that cover BOTH DRAM and peripheral MMIO.

### Next Steps to Try
1. **Bypass `io::construct_io_mem_allocator_builder()`** on RPi3 — add `BoardType::cached() == 2` check
2. **Bypass `boot_all_aps()`** on RPi3 — SMP bringup may be failing
3. **Bypass `timer::init()`** on RPi3 — timer hardware may not exist
4. **Add markers BEFORE each function call** in `late_init_on_bsp()` to narrow down further

### Tools & Infrastructure
- `tools/build_mcp_server.py` — Unified build+deploy MCP server on port 8912
- `tools/serial_mcp_server.py` — MCP HTTP server on port 8910 for reading RPi3 serial console
- `.reasonix/config.toml` — MCP plugin registrations

### Boot Log (Current State)
```
[init.C] after kspace::init
[sync.init.0] start
[sync.init.1] after rcu::init
[init] after sync::init
[dma.init.0] start
[dma.init.1] done (skipped on aarch64)
[init.dma] after dma::init
[late.0] start
[late.trap] after trap::init
[gic.0] before call_once
[gic.4] skipped on RPi3
[late.gic] after gic::init_on_bsp
[late.1] after construct_io_mem_allocator_builder  ← NEXT CRASH POINT
```

### Files Modified This Session
- `ostd/src/arch/aarch64/gic.rs` — GIC init conditional on board type, RPi3 skip
- `ostd/src/arch/aarch64/mod.rs` — Added debug markers for late_init stages
- `ostd/src/mm/dma/mod.rs` — Skip DMA init on AArch64
- `ostd/src/sync/mod.rs` — Added debug markers for sync::init
- `ostd/src/sync/rcu/mod.rs` — Added debug markers for rcu::init
- `ostd/src/sync/rcu/monitor.rs` — Added debug markers for RcuMonitor::new
- `ostd/src/lib.rs` — Debug markers for init stages

### Git Log
```
71d681ae T030 session end: save session state, update workflow for build MCP
41c7a709 T030: fix boot crash on RPi3 — generalized LDARB/LDXR workaround
405031d5 aarch64/npt: bypass PT spinlock on RPi3 — Cortex-A53 exclusive-monitor issue
2e9dc318 aarch64/once: bypass SimpleOnce::load(Acquire) — LDARB faults on RPi3
c1b4299e aarch64/boot: replace 1GiB block entry in boot_l3pt_high with L2 table
77a38d99 aarch64/meta: fix frame_paddr() during bootstrap on RPi3 (frame_paddr_base==0)
```

### Next Session Action
Bypass `io::construct_io_mem_allocator_builder()` on RPi3 (add BoardType check), then bypass subsequent functions (`boot_all_aps`, `timer::init`, `io::init`) one by one until we find the actual failing component. The goal is to identify whether this is:
1. A specific hardware component that's not present/emulated on RPi3
2. A page table issue that only manifests during certain memory access patterns
3. Something else entirely
