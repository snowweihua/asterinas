# RPi3 Debug Session - 2026-07-22 (Session 4) — Major Infrastructure + Root Cause Analysis

## Current Task T030
Boot to shell with interactive command working.

## Root Cause: alloc_frame_with generic return crashes at ret

The function `alloc_frame_with<M: AnyFrameMeta>() -> Result<Frame<M>, Error>` crashes at `ret` when returning ANY value on AArch64 RPi3. The saved x30 on the stack is corrupted with value `0xFFFFFFFFC900A8` (physical `0x3AF610A8`). ALL attempts to return from this function crash identically — only `loop { spin_loop() }` works (no return).

### Proved NOT to be:
- Rust compiler bug (tested nightly-2025-02-01, -04-01, -06-01, -08-01 — same crash)
- Boot page table linear mapping issue (zeroed slots 1-3 — no change)
- `spin::Once` atomic CAS failure (KERNEL_PAGE_TABLE uses SimpleOnce now)
- `_metadata` drop corruption (tested with core::mem::forget — no change)

### What's Fixed This Session
1. **Boot page table** (`ostd/src/arch/aarch64/boot/boot.S`): zeroed linear mapping slots 1-3 on RPi3 (mapped non-existent PA 0x40000000+)
2. **`new_kernel_page_table()`** (`ostd/src/mm/page_table/mod.rs`): on AArch64, copies boot PTE entries for slots 256-511 instead of calling `alloc_if_none()`. This avoids calling the broken `alloc_frame_with` during page table creation.
3. **Linear + metadata mapping** (`ostd/src/mm/kspace/mod.rs`): skipped on AArch64 — boot page table already has these entries.
4. **KERNEL_PAGE_TABLE** (`ostd/src/mm/kspace/mod.rs`): changed from `spin::Once` to `crate::boot::SimpleOnce` (spin's atomic CAS hangs on RPi3 when memory region spans DRAM + peripherals)

### What's Still Blocked
`PageTableNode::alloc()` (`ostd/src/mm/page_table/node/mod.rs`) calls `alloc_frame_with()` which has the spin loop workaround. Tried static BSS pool + `Frame::from_raw()` but that also crashes (metadata system rejects untracked pages).

### Tools Created
- `tools/serial_mcp_server.py` — MCP HTTP server on port 8910 for reading RPi3 serial console
- `tools/deploy_mcp_server.py` — MCP HTTP server on port 8911 for deploying kernel binary to SD card
- `.reasonix/config.toml` — MCP plugin registrations for both servers

## Next Session Action
Restore the `EARLY_ALLOCATOR` in `allocator::init()` so `alloc_frame_with()` can actually allocate frames and return `Ok` (avoiding the buggy `Err` return path). This means NOT consuming the early allocator, or re-initializing it after use.

## Files Modified
- `ostd/src/arch/aarch64/boot/boot.S` — zero linear slots 1-3 on RPi3
- `ostd/src/mm/kspace/mod.rs` — SimpleOnce, skip linear+meta on AArch64
- `ostd/src/mm/page_table/mod.rs` — copy boot PTE entries on AArch64
- `ostd/src/mm/page_table/node/mod.rs` — static pool attempt (crashes)
- `ostd/src/mm/frame/allocator.rs` — spin loop workaround, canary experiments
- `ostd/src/arch/aarch64/boot/mod.rs` — pl011_puts_hex diagnostic
- `tools/serial_mcp_server.py`, `tools/deploy_mcp_server.py` — MCP servers
- `.reasonix/config.toml` — MCP plugin registrations
- `specs/001-rpi3-hardware-bringup/SESSION_CONTEXT.md` — this file
- `AGENTS.md` — updated session status

## Git Log
```
5781f34e T030: all fixes applied except root issue
e6b53c11 T030: new_kernel_page_table() copies boot PT entries
b01efadc T030: boot.S fix (zero linear slots 1-3) didn't help
cca4446a T030: confirmed crash NOT compiler bug
491b8d8f BREAKTHROUGH: compiler epilogue bug for generic Result
a6a5efe1 BREAKTHROUGH: identified compiler epilogue bug
4816a462 aarch64/diag: add core::mem::forget(_metadata)
```
