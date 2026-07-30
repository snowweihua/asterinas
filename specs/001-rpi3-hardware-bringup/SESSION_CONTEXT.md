# RPi3 Debug Session — 2026-07-30 (Session 14)

## Current Task
Resolve the deterministic "Synchronous Abort" (ESR `0x02000000`) crash at `0x3AF610A8` / `0xFFFFFFFFFC900A8` during frame allocator initialization.

## DEFINITIVE FINDING: VBAR_EL1 Handler NEVER Fires

We added a raw UART "SERR!" marker to `serr_current` (the EL1 SError handler). The marker **never appears** before the TF-A crash handler output. This proves:

> **The BCM2837 (RPi3 SoC) routes AXI bus errors / external aborts directly to EL3, bypassing EL1 exception handling entirely.**

This is a **hardware platform issue** — SCR_EL3.EA=0 does NOT prevent the SoC from routing bus errors to EL3. Our VBAR_EL1 handlers (synchronous, IRQ, FIQ, SError) are never invoked for this crash type.

## Session 14 Summary

### Experiments Completed (all crashed same way):
| Experiment | Result |
|-----------|--------|
| Skip coalescing loop | Crash moves to `push_front`, same address/registers |
| Skip `push_front` | Crash moves to `drop` path, same address/registers |
| Fix boot.S DRAM gap (0x3B4-0x3EF) | No effect |
| Remove ALL allocator debug prints | No effect |
| Double boot stack (256→512 KiB) | No effect |
| Mask SErrors (PSTATE.A=1) | **No effect** — crash still at same address |
| Memory bisection (only first 33 MB) | No effect — crash with same registers |
| **Raw UART SERR! marker in serr_current** | **Marker NOT PRINTED before crash** |

### Memory Range Tested
Free memory added: `0x43B000 + 0x21C5000` (4.3 MB to 38 MB, only 33.7 MB total).
The entire free range is well within DRAM center, far from boundaries.

### Consistent Crash Data (ALL experiments):
```
ELR: 0xFFFFFFFFFC900A8 / 0x3AF610A8
ESR: 0x02000000 (Unknown reason)
x16: 0x0260612C (DTB address range — always same)
x17: 0x00000008 (always same)
x19: 0x3AF4C440 (always same)
x20: 0x3B35C368 (always same)
x21: 0x3AF4C4B0 (always same)
x29: 0x00000003 (always same)
```

These register values NEVER CHANGE across ANY experiment. The crash is 100% deterministic.

### Disassembly Analysis
`insert_chunk` has `get_slot` inlined. The bootstrap path (IN_BOOTSTRAP_CONTEXT=true) computes PA pointers through TTBR0 identity mapping. The function accesses frame metadata via `frame_to_meta()` for the non-bootstrap path.

The build already uses `RUSTFLAGS=-C target-cpu=cortex-a53` (configured in build MCP server), so codegen target is correct.

### Crash Location
Always at `[pafm.split]` — after `get_with`, `borrow_mut`, `OnDemandGlobalLock::new` all succeed. Inside the `for_each` closure that calls `local_pool.insert_chunk(addr, order)`.

### Root Cause Hypothesis
The BCM2837 AXI interconnect generates bus errors for certain memory access patterns that are processed by the Cortex-A53 as external aborts routed directly to EL3. Possible triggers:
1. Speculative instruction fetch crossing into unmapped/non-existent memory
2. Exclusive-access instruction (LDXR/STXR) on memory with wrong attributes  
3. Access to a memory address that the interconnect considers invalid
4. Some timing-dependent bus transaction conflict

### Next Steps (Priority Order)
1. **Modify TF-A BL31** to forward SErrors to non-secure EL1 (requires TF-A rebuild)
2. **Replace buddy allocator** with a simpler bitmap allocator for RPi3 that avoids the problematic access patterns
3. **Add `dsb sy; isb` barriers** before every metadata access to serialize the bus
4. **Map ALL of DRAM (0x0-0x3F000000) as non-cacheable** during bootstrap to prevent speculative access
5. **Try building with `opt-level=1`** to change code generation for the insert_chunk function
