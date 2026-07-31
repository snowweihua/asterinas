# RPi3 Debug Session — 2026-07-31 (Session 16)

## Current Task
Fix RPi3 kernel panic during `kspace::init_kernel_page_table()` at metadata page mapping stage.

## Key Findings

### EL1 Exception Handler: FULLY WORKING ✅
- Deliberate exception (write to 0xFFFFFFF000000000) correctly triggers handler
- ESR=0x96000044 (Data Abort, Write, L1 translation fault) ✅
- ELR points to faulting instruction ✅
- FAR shows the unmapped VA ✅

### Current Crash Location
- Crashes inside `cursor_mut()` during metadata page mapping
- ESR=0x96000035 (Level 1 Address Size Fault)
- FAR=0x2c48ec4 (peripheral address - NOT a valid page table address)
- L0[511]=0 in the new KPT root

### Root Cause Hypothesis
The new KPT root is at PA 0x82000 (identity-mapped in boot page tables).
During `cursor_mut` → `lock_range` → `try_traverse_and_lock_subtree_root`:
1. Reads L0[511] = 0 (correct)
2. Tries to allocate a new L1 page table
3. The allocation or access corrupts something, causing peripheral address 0x2c48ec4 to be accessed

### Debug Output Sequence
```
[kspace.m4] before cursor_mut
[kspace.m4b] kpt_root_pa=0x0000000000082000
[kspace.m4c] L0[511]=0x0000000000000000
[EL1-SYNC] ESR=0x96000035 ELR=... FAR=0x2c48ec4
```

## Next Steps
1. Verify the early_allocator frame for KPT root (0x82000) is NOT in metadata range
2. Check if `paddr_to_vaddr(0x82000)` is properly mapped
3. Consider skipping metadata page mapping on RPi3 if it can't work
4. Investigate if peripheral address corruption is due to frame overlap

## Files Modified
- ostd/src/mm/kspace/mod.rs: Added debug markers [npt.X], [slot0.X], [kspace.mX]
- ostd/src/mm/page_table/mod.rs: Added debug markers, fixed unsafe warnings
- ostd/src/arch/aarch64/trap/mod.rs: Enhanced EL1 handler with full register dump
