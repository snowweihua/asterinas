# RPi3 Debug Session - 2026-07-21 (Session 3) — Updated after stack canary fix

## Current Task T030
Boot to shell with interactive command working.

## Current Status: Stack Slot Corruption (Not Stack Overflow)

**Key Serial Log Evidence** (from fixed canary with SHARED statics):
```
[fa.e] entry 0x000000003af4c380          ← FP register at entry
[fa.0] start
[fa] ret NoMemory                         ← bypass returns immediately
[fa.x] exit 0x000000003af4c380            ← FP register at exit (IDENTICAL!)
[fa.x] lr 0xffff00000030b0e8              ← current LR (changed by pl011_puts calls)
[fa.x] CORRUPT saved fp 0x000000003af4c380 ← static CANARY_FP (NOT zeroed!)
[fa.x] saved lr 0xffff000000308878         ← static CANARY_LR (original return address, correct!)
```

**What This Proves**:
1. **FP register is NOT corrupted** (0x3af4c380 == 0x3af4c380)
2. **Static variables are NOT corrupted** (CANARY_FP/LR retain entry values)
3. **The stack slot at [sp] IS corrupted** — the saved x30 is overwritten with `0xFFFFFFFFC900A8`
4. The false "CORRUPTED" from the BUGGY canary (separate per-block statics) was misleading

**Crash Mechanism** (confirmed):
1. Function prologue: `stp x29, x30, [sp, #-16]!` — saves FP=0x3af4c380, LR=0xffff000000308878 to stack
2. During function body: something writes 0xFFFFFFFFC900A8 to [sp] (where x30 was saved)
3. Function epilogue: `ldp x29, x30, [sp], #16` — loads x29=0x3, x30=0xFFFFFFFFC900A8
4. `ret` jumps to 0xFFFFFFFFC900A8 — unmapped address → Synchronous External Abort (ESR=0x02000000)

**Value Analysis**: 0xFFFFFFFFC900A8 (physical 0x3AF610A8 after relocation) is consistently the same across boots. It's near the stack region (~970 MB). This suggests a **pointer aliasing or stack buffer overflow** in a called function, writing to the specific stack slot of `alloc_frame_with`.

### Previous Session Note (deprecated)
The earlier hypothesis about `spin::Once` and SimpleOnce was already fixed. The current issue is stack slot corruption, NOT allocator logic.

## Progress Summary

### What Works Now
- `meta::init()` completes successfully
- `allocator::init()` completes (with `add_free_memory` no-op)
- `init_after_heap()` completes
- `kspace::init()` starts and reaches `alloc_frame_with`
- **Stack canary works correctly** (fixed shared statics)

### Current Bypasses (will need proper fix)
- `FrameAllocator::alloc` → returns `None` early
- `FrameAllocator::add_free_memory` → debug markers only
- `pools::add_free_memory` → `return;` (no-op)
- `alloc_frame_with` → returns `Err(NoMemory)` early
- `disable_local()` → no-op on AArch64
- `enable_local()` → no-op on AArch64

## Crash Analysis

### Consistent Crash Signature
- **ESR**: 0x02000000 (Synchronous External Abort)
- **Overwrite value**: 0xFFFFFFFFC900A8 (physical 0x3AF610A8)
- **Stack FP at entry**: 0x3AF4C380
- **Stack FP at exit**: 0x3AF4C380 (unchanged!)
- **Saved LR (static)**: 0xFFFF000000308878 (original return address, preserved in static!)

### Key Observations
1. FP register is correct — this is NOT a stack overflow or frame corruption
2. Static variables are fine — the memory zeroing hypothesis is WRONG
3. The overwrite is TARGETED to the specific stack slot of the saved x30
4. The overwrite value (0xFFFFFFFFC900A8) is consistent across boots
5. The crash is a Synchronous External Abort (bus error), not a page fault
6. The `ret` instruction tries to fetch from 0xFFFFFFFFC900A8 which is unmapped

### New Hypothesis
**Stack buffer overflow in a called function** — some function called from within `alloc_frame_with` (or the function body itself) has a stack buffer that overlaps with the saved x30 slot. The only functions called are `pl011_puts()` and `Layout::from_size_align()`. Since `pl011_puts` is heavily used elsewhere without issues, the most likely culprit is the compiler's epilogue code generation for `return Err(Error::NoMemory)` when the return type (`Result<Frame<M>, Error>`) involves dropping the `_metadata: M` parameter.

## Next Action
Investigate the epilogue/drop behavior: when `alloc_frame_with` returns `Err(NoMemory)`, the compiler needs to drop `_metadata: M` (the uninitialized frame metadata) and the partially-constructed `Frame<M>` in the `Err` path. This drop code might corrupt the stack. Try:
1. Making the function `#[inline(never)]` to force a clean stack frame
2. Explicitly dropping `_metadata` before the return
3. Checking if `core::mem::forget(_metadata)` before returning changes behavior

## Files Modified (current state)
- `ostd/src/mm/frame/allocator.rs` - Stack canary with shared statics + bypass
- `ostd/src/arch/aarch64/boot/mod.rs` - pl011_puts_hex diagnostic function
- `tools/serial_mcp_server.py` - MCP serial reader server
- `tools/deploy_mcp_server.py` - MCP deploy server

## Git Log (recent)
```
82bc112c aarch64/diag: fix stack canary — use shared statics so entry+exit blocks compare same variables
db7b7106 aarch64/diag: fix stack canary — use pl011_puts with utf8 decimal conversion instead of pl011_puts_hex
7648bcb7 aarch64/diag: fix inline asm — use direct register w3 instead of named operand with explicit reg
cd4846ed aarch64/diag: fix scope error in stack canary — use single static pair for both entry and exit
c10ac3e4 aarch64/diag: add stack canary and pl011_puts_hex to alloc_frame_with
52b7215d aarch64: bypass alloc_frame_with — crash at function return persists
745a155c aarch64: session 2026-07-21 final state — kernel reaches kspace::init
```