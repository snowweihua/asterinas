# RPi3 Debug Session — 2026-07-31 (Session 15)

## Current Task
Continue resolving the deterministic "Synchronous Abort" (ESR `0x02000000`) crash at `0x3AF610A8` / `0xFFFFFFFFFC900A8` during frame allocator initialization.

## Previous Session Findings (Session 14)

### DEFINITIVE FINDING: VBAR_EL1 Handler NEVER Fires
The BCM2837 (RPi3 SoC) routes AXI bus errors / external aborts directly to EL3, bypassing EL1 exception handling entirely.

### Memory Bisection Results
Free memory bisected to only first 33 MB (0x43B000 + 0x21C5000). Crash still occurs at same address/registers.

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

### Possible Root Causes
1. Speculative instruction fetch crossing into unmapped/non-existent memory
2. Exclusive-access instruction (LDXR/STXR) on memory with wrong attributes
3. Some timing-dependent bus transaction conflict

## Session 15 Actions

### EL2 Exception Handler Enhanced
Rewrote `el2_trap.S` with:
1. Proper UART polling (LSR bit 6 before TX)
2. Full register dump (ESR, ELR, SPSR, FAR, x0-x30)
3. 'X' marker for sync exception, 'S' for SError
4. Helper functions instead of macros to avoid label conflicts

### Build/Test Results
- Build succeeded
- System hangs at `[AFM] calling pools` - same location as before
- NO EL2 exception output seen - crash is happening at EL1, not EL2

### Key Observation
The system hangs at `[AFM] calling pools` and NO 'X' or 'S' EL2 exception markers are printed. This suggests:
1. The crash IS happening at EL1 (VBAR_EL1), not EL2
2. OR the EL2 handler IS catching it but the UART output isn't reaching the console
3. The crash happens inside `insert_chunk` / `add_free_memory`

## Next Steps (Priority Order)
1. **Verify EL1 exception handler** is working - add markers to `sync_exception_current`
2. **Add debug markers** around `insert_chunk` calls in frame allocator
3. **Check if x30 corruption** is the actual issue - look at the Cortex-A53 epilogue bug
4. **Try opt-level=1** to change code generation
5. **Add dsb/isb barriers** before metadata access

## Files Modified This Session
- `ostd/src/arch/aarch64/trap/el2_trap.S` - Enhanced EL2 handler with full register dump

(End of file - total 65 lines)
