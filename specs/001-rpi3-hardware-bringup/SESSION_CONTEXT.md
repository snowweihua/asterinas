# RPi3 Debug Session — 2026-07-31 (Session 15)

## Current Task
Continue resolving the deterministic "Synchronous Abort" (ESR `0x02000000`) crash at `0x3AF610A8` / `0xFFFFFFFFFC900A8` during frame allocator initialization.

## Previous Session Findings (Session 14)

### DEFINITIVE FINDING: VBAR_EL1 Handler NEVER Fires
The BCM2837 (RPi3 SoC) routes AXI bus errors / external aborts directly to EL3, bypassing EL1 exception handling entirely.

### Consistent Crash Data
```
ELR: 0xFFFFFFFFFC900A8 / 0x3AF610A8
ESR: 0x02000000 (Unknown reason)
x16: 0x0260612C (DTB address range)
x17: 0x00000008
x19: 0x3AF4C440
x20: 0x3B35C368
x21: 0x3AF4C4B0
x29: 0x00000003
```

## Session 15 Actions

### EL2 Exception Handler Enhanced
Rewrote `el2_trap.S` with:
1. Proper UART polling (LSR bit 6 before TX)
2. Full register dump (ESR, ELR, SPSR, FAR, x0-x30)
3. 'X' marker for sync exception, 'S' for SError
4. Helper functions instead of macros to avoid label conflicts

### EL1 Exception Handler Enhanced
Updated `trap/mod.rs` `sync_exception_current` with:
1. Print "[EL1-SYNC]" marker immediately on entry
2. Dump ESR, ELR, SPSR, LR and all x0-x30 registers
3. Halt instead of panic to avoid recursive exceptions

Updated `serr_current` with similar full register dump.

### Test Results
- Build succeeded
- System hangs at `[AFM] calling pools` - same location as before
- NO 'X' marker from EL2 handler
- NO '[EL1-SYNC]' marker from EL1 handler
- NO "Synchronous Abort" from TF-A

## Key Observation
The system hangs at `[AFM] calling pools` and:
- No 'X' (EL2 sync) or 'S' (EL2 serr) markers appear
- No '[EL1-SYNC]' from EL1 handler
- No TF-A "Synchronous Abort"

This suggests either:
1. Exception is happening at EL3 (TF-A level) but TF-A isn't printing
2. Exception IS happening at EL2 but UART output is lost/buggy
3. System is just very slow and hasn't crashed yet

## Files Modified This Session
- `ostd/src/arch/aarch64/trap/el2_trap.S` - Enhanced EL2 handler with full register dump
- `ostd/src/arch/aarch64/trap/mod.rs` - Enhanced EL1 handler with full register dump

## Next Steps (Priority Order)
1. **Check TF-A output** - verify if "Synchronous Abort" appears or not
2. **Add debug markers** inside `insert_chunk` to pinpoint exact crash location
3. **Verify VBAR_EL2 is set correctly** - check if `vector_table_el2` is actually being used
4. **Try early_println instead of raw_puts** - maybe UART output isn't working from exception context
5. **Add memory barriers** - `dsb sy; isb` before metadata access in frame allocator

(End of file - total 79 lines)
