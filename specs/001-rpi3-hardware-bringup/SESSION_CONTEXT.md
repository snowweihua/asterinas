# RPi3 Debug Session - 2026-07-22 (Session 4) — Compiler Epilogue Codegen Bug Found

## Current Task T030
Boot to shell with interactive command working.

## Current Status: Rust Compiler Codegen Bug in alloc_frame_with

### Root Cause Found
The crash at `ret` in `alloc_frame_with` is caused by a Rust nightly (2025-02-01) compiler epilogue codegen bug — returning `Err(Error::NoMemory)` from generic `fn alloc_frame_with<M: AnyFrameMeta>(&self, _metadata: M) -> Result<Frame<M>>` corrupts saved x30 on the stack.

### Evidence
1. **Spin loop works perfectly** — `loop { spin_loop(); }` causes NO crash (kernel hangs instead of crashing)
2. **ALL return approaches crash identically**: empty body, `#[inline(never)]`, `core::mem::forget`, inline asm manual return
3. **Canary statics NOT corrupted** — earlier "zeroed statics" was false positive from separate entry/exit block statics
4. **FP register UNCHANGED at exit** — proves corruption is only in saved x30 stack slot
5. **Crash signature consistent**: ESR=0x02000000, ELR=0xFFFFFFFFC900A8, x29=0x3, x26/x28=0xd00dfeed

### Current Workaround
`alloc_frame_with` uses `loop { core::hint::spin_loop(); }` to avoid the buggy epilogue. Kernel hangs at `kspace::init() → PageTableNode::alloc()`.

### What Works
- All boot stages up to and including `kspace::init()` start
- `pl011_puts()` works from inside `alloc_frame_with`

### What's Blocking
- Cannot return `Err(Error::NoMemory)` from `alloc_frame_with` — compiler epilogue bug
- `early_alloc()` consumed by `allocator::init()`
- Global frame allocator bypassed (`alloc` returns None, `add_free_memory` is no-op)

## Bypasses Active
- `FrameAllocator::alloc` → None; `add_free_memory` → no-op
- `alloc_frame_with` → spin loop (no return)
- `disable_local`/`enable_local` → no-op on AArch64

## Next Actions
**Option A**: Update Rust nightly in `rust-toolchain.toml` (newer than 2025-02-01)
**Option B**: Modify `PageTableNode::alloc()` to bypass `alloc_frame_with` — allocate directly
**Option C**: Restore allocator to let `alloc_frame_with` return `Ok` instead of `Err`

## Files Modified
- `ostd/src/mm/frame/allocator.rs` — bypass + canary experiments
- `ostd/src/arch/aarch64/boot/mod.rs` — pl011_puts_hex diagnostic
- `tools/serial_mcp_server.py`, `tools/deploy_mcp_server.py` — MCP servers

## Git Log (recent)
```
491b8d8f BREAKTHROUGH: compiler epilogue bug for generic Result<Frame<M>, Error> on AArch64
a6a5efe1 BREAKTHROUGH: identified Rust nightly compiler epilogue bug
4816a462 aarch64/diag: add core::mem::forget(_metadata)
e8e514e1 docs: update AGENTS.md with corrected stack canary findings
64225ca5 docs: update SESSION_CONTEXT with stack canary findings
82bc112c aarch64/diag: fix stack canary — use shared statics
```
