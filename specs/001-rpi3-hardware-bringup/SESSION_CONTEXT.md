# RPi3 Debug Session - 2026-07-24 (Session 5) — Root PT Reservation + Persistent x30 Corruption

## Current Task T030
Boot to shell with interactive command working.

## Session 5 Summary

### What Works (Fixed This Session)
1. **Root PT reservation** (`ostd/src/mm/page_table/mod.rs`): `reserve_root_pt_page()` allocates a page via `early_alloc` before the early allocator is retired, creating a `Frame<PageTablePageMeta<KernelPtConfig>>` stored in `AARCH64_ROOT_PT_FRAME`. `empty_kernel()` picks it up, avoiding `alloc_frame_with()` entirely.

2. **Empty kernel page table** (`ostd/src/mm/page_table/mod.rs`): `empty_kernel()` on AArch64 takes the pre-allocated root frame. `new_kernel_page_table()` copies boot PTE entries from the active TTBR1 page table for slots 256-511 (kernel space range) and slot 0 (kernel code), bypassing the broken `alloc_if_none()` path.

3. **Spinlock bypass** (`ostd/src/mm/page_table/mod.rs`): On AArch64, `new_kernel_page_table()` uses `make_guard_unchecked()` instead of `lock()` to avoid `AtomicU8::swap` (exclusive-monitor instructions that fault on Cortex-A53 when page-table entries span both DRAM and peripherals).

4. **SimpleOnce fix** (`ostd/src/boot/mod.rs`): `call_once()` changed from `Ordering::Acquire` (LDARB) to `Ordering::Relaxed` (plain load) for the `initialized` flag check. Store changed from `Ordering::Release` (STLRB) to `Ordering::Relaxed` (plain store). Safe during single-core boot.

5. **meta_pages leak** (`ostd/src/mm/kspace/mod.rs`): On AArch64, `core::mem::forget(meta_pages)` prevents `Segment::drop` which iterates over all metadata pages with atomic ref-count decrements that fault on RPi3.

### What's Still Blocked (Persistent Crash)
The crash at `sync::init()` → `rcu::init()` → `RCU_MONITOR.call_once(RcuMonitor::new)`:
- **Symptom**: ESR=0x02000000, ELR=0x3AF610A8 (same address across ALL code paths)
- **x29** = 0x3 (corrupted FP), **x26/x28** = 0xd00dfeed (poison in some cases)
- **Consistency**: Same ELR regardless of which function is crashing — affects `alloc_frame_with`, `lock()`, `call_once()`, `Segment::drop`, and `sync::init()`
- **Hypothesis**: Cortex-A53 erratum/interaction where 1 GiB block entries in boot page tables (`boot_l3pt_high`, `boot_l3pt_linear`) covering both DRAM and peripheral MMIO (0x3F000000+) cause speculative accesses to the peripheral bus, corrupting saved x30 on the stack. Patching `boot_l3pt_high[0]` and `boot_l3pt_linear[0]` to use `boot_l2pt_gb0` (correct per-2MB attributes) did not resolve the crash.
- The crash is experienced as an IMMUTABLE pattern — all code paths crash at the same ELR with the same register state.

### Tools & Infrastructure Created
- `tools/build_mcp_server.py` — Unified build+deploy MCP server on port 8912 with tools: `build_kernel`, `convert_kernel`, `deploy_kernel`, `build_and_deploy`
- `tools/serial_mcp_server.py` — MCP HTTP server on port 8910 for reading RPi3 serial console
- `.reasonix/config.toml` — MCP plugin registrations for serial and build MCP servers

### Boot Log (last known state after fixes)
```
...
[kspace.W] start
[ek] empty_kernel start
[npt] after empty_kernel
[kspace.X] after new_kernel_page_table
[kspace.Y] after disable_preempt
[kspace.d] after meta mapping
[init.C] after kspace::init
"Synchronous Abort" handler, esr 0x02000000   ← in sync::init()
```

### Immediate Fix for Next Session
Add `core::mem::forget(meta_pages)` on AArch64 if not already present in `init_kernel_page_table()` — prevents `Segment::drop` of metadata pages which faults during loop.
Replace ALL `AtomicU8::load(Ordering::Acquire)` with `Ordering::Relaxed` in boot-critical paths.
Consider replacing boot 1 GiB block entries in ALL page tables (`boot_l3pt_high`, `boot_l3pt_linear`) with L2 table pointers to `boot_l2pt_gb0`.

### Files Modified This Session
- `ostd/src/arch/aarch64/boot/boot.S` — 1 GiB block → L2 table for boot_l3pt_high[0], boot_l3pt_linear[0] patching
- `ostd/src/mm/page_table/mod.rs` — root PT reservation, empty_kernel(), spinlock bypass
- `ostd/src/mm/kspace/mod.rs` — meta_pages leak, call_once → store_direct (reverted to call_once)
- `ostd/src/mm/frame/meta.rs` — frame_paddr() IN_BOOTSTRAP_CONTEXT fix
- `ostd/src/boot/mod.rs` — SimpleOnce::call_once relaxed ordering, store_direct method
- `ostd/src/mm/frame/allocator.rs` — spin loop workaround (unchanged from prev session)
- `tools/build_mcp_server.py` — NEW unified build+deploy MCP server
- `tools/serial_mcp_server.py` — serial reader MCP server
- `.reasonix/config.toml` — MCP registrations (serial, deploy, build)
- `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md` — this file
- `AGENTS.md` — updated workflow

### Git Log
```
41c7a709 T030: fix boot crash on RPi3 — generalized LDARB/LDXR workaround
405031d5 aarch64/npt: bypass PT spinlock on RPi3 — Cortex-A53 exclusive-monitor issue
2e9dc318 aarch64/once: bypass SimpleOnce::load(Acquire) — LDARB faults on RPi3
c1b4299e aarch64/boot: replace 1GiB block entry in boot_l3pt_high with L2 table
77a38d99 aarch64/meta: fix frame_paddr() during bootstrap on RPi3 (frame_paddr_base==0)
430725ac T030 session end: empty_kernel + root PT reservation work. Crash now in boot PT copy loop
5781f34e T030: all fixes applied except the root issue
```

### Next Session Action
Investigate the persistent x30 corruption at ELR=0x3AF610A8. The crash is pervasive across all function epilogues and appears to be a fundamental hardware interaction on Cortex-A53 RPi3. Possible approaches:
1. Replace remaining 1 GiB block entries in boot page tables with L2 table pointers (boot_l3pt_linear slot 0 on RPi3)
2. Use non-atomic (relaxed) operations throughout the boot path
3. Consider using a different page table layout during boot that avoids mixed-attribute block entries
4. Test QEMU regression to ensure fixes don't break QEMU virt boot
