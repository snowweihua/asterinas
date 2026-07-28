# RPi3 Debug Session - 2026-07-28 (Session 10)

## Current Task T030
Boot to shell on RPi3 — persistent crash at `elr=0x3af610a8` (Instruction Abort).

## Session 10 Summary

### Slot0 Fix Verified Working
The slot0 L3 entry fix IS correctly applied:
- `l3table[0]` updated from `0x86003` → `0x80403` (PA=0x80000, VALID|TYPE|AF)
- `kpt_root[0]=0x80403` confirmed via TTBR1 read-back

### NEW Finding: Root Cause NOT Slot0 Mapping
The crash STILL happens at `elr=0x3af610a8` even WITH the slot0 fix.
This means the issue is NOT the PA mapping - it's something else.

### Crash Location Analysis
The crash happens in `invoke_ffi_init_funcs()` when calling the `aster_block` init:
```
[IFF slot=0xffff0000003dcac0 val=0xffff0000000ceacc]
[IFF.CALL]
```
- `__ostd_main` is at VA `0xffff0000001e1a30` (from nm)
- The init_array contains `0xffff0000000ceacc` which is `aster_block1_25__init___rust_ctor___ctor`

The aster_block init function:
```asm
ffff0000000ceacc:  adrp    x1, 0xffff0000003dc000
ffff0000000cead0:  add     x1, x1, #0xb90    ; x1 = 0xffff0000003dcb90
ffff0000000cead4:  ldp     x0, x8, [x1]       ; load from inventory
ffff0000000cead8:  ldr     x2, [x8, #24]     ; load fn ptr from vtable
ffff0000000ceadc:  br      x2                 ; jump to function
```

The crash occurs at `br x2` - the function pointer at offset 24 in the structure is INVALID.

### What's in the inventory?
At `0xffff0000003dcb90`: `d _ZN11aster_block1_6__init11__INVENTORY17h1fa541ee970bee5fE`

The inventory structure contains function pointers that aren't properly initialized, causing the crash.

### Files Modified (This Session)
- `ostd/src/mm/page_table/mod.rs`: Slot0 PTE fix (PA 0x80000 + AF flag)
- `ostd/src/mm/kspace/mod.rs`: Debug markers [akt.0b], [akt.0c]

### Git Log (Recent)
```
5df1e685 aarch64/rpi3: slot0 fix applied — crash in aster_block init (not PA mapping issue)
e93461c4 aarch64/rpi3: slot0 fix with AF flag — PTE verified correct but crash persists
ccc0e0d3 tools: remove old HTTP-based MCP servers and wrappers
b0bf5bd4 aarch64/rpi3: skip tlbi vmalle1 (hangs on RPi3) — boot reaches FFI init
```

### Next Steps
1. Investigate aster_block inventory initialization failure
2. OR bypass init_array and call main() directly
3. OR check if aster_block is designed to work on RPi3

### Boot Log (Current)
```
[slot0] FIX: updating l3table[0] from 0x0000000000086003 to 0x0000000000080403
[slot0] new_root[0]=0x0000000000084003
[akt.0b] TTBR1=0x000000000337b000
[akt.0c] kpt_root[0]=0x0000000000080403
[tlb.0]-[tlb.5] TLB flush (skipping tlbi)
[akt.1] after tlb_flush
[akt.2] skipping dismiss
[akt.3] after dismiss
[init.5a] after activate_kernel_page_table
[init.6] before IN_BOOTSTRAP_CONTEXT store
[init.7] before enable_local_irq
[init.8] before invoke_ffi_init_funcs
[IFF slot=0xffff0000003dcac0 val=0xffff0000000ceacc]
[IFF.CALL]
[CRASH elr=0x3af610a8 ESR=0x02000000]
```
